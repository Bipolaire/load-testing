//! Profils de charge et construction des seuils.

use crate::models::{Profile, RunOptions, Test};
use rust_i18n::t;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stage {
    pub duration_ms: u64,
    pub target: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutorPlan {
    ConstantVus {
        vus: u32,
        duration_ms: u64,
    },
    RampingVus {
        start_vus: u32,
        stages: Vec<Stage>,
    },
    ConstantArrival {
        rate: f64,
        duration_ms: u64,
        pre_allocated_vus: u32,
        max_vus: u32,
    },
    RampingArrival {
        start_rate: f64,
        stages: Vec<Stage>,
        pre_allocated_vus: u32,
        max_vus: u32,
    },
}

impl ExecutorPlan {
    pub fn total_duration_ms(&self) -> u64 {
        match self {
            ExecutorPlan::ConstantVus { duration_ms, .. } => *duration_ms,
            ExecutorPlan::RampingVus { stages, .. } => stages.iter().map(|s| s.duration_ms).sum(),
            ExecutorPlan::ConstantArrival { duration_ms, .. } => *duration_ms,
            ExecutorPlan::RampingArrival { stages, .. } => {
                stages.iter().map(|s| s.duration_ms).sum()
            }
        }
    }

    pub fn executor_name(&self) -> &'static str {
        match self {
            ExecutorPlan::ConstantVus { .. } => "constant-vus",
            ExecutorPlan::RampingVus { .. } => "ramping-vus",
            ExecutorPlan::ConstantArrival { .. } => "constant-arrival-rate",
            ExecutorPlan::RampingArrival { .. } => "ramping-arrival-rate",
        }
    }

    /// Nombre maximal de VUs simultanés (pour l'affichage et la série `vus`).
    pub fn peak_vus(&self) -> f64 {
        match self {
            ExecutorPlan::ConstantVus { vus, .. } => *vus as f64,
            ExecutorPlan::RampingVus { stages, .. } => {
                stages.iter().map(|s| s.target).fold(0.0, f64::max)
            }
            ExecutorPlan::ConstantArrival { max_vus, .. }
            | ExecutorPlan::RampingArrival { max_vus, .. } => *max_vus as f64,
        }
    }

    /// Unité de l'axe Y pour l'aperçu (VUs ou TPS).
    pub fn preview_unit(&self) -> &'static str {
        match self {
            ExecutorPlan::ConstantVus { .. } | ExecutorPlan::RampingVus { .. } => "VUs",
            ExecutorPlan::ConstantArrival { .. } | ExecutorPlan::RampingArrival { .. } => "TPS",
        }
    }

    /// Points (temps_ms, valeur) pour tracer la courbe planifiée.
    pub fn preview_points(&self, n: usize) -> Vec<(f64, f64)> {
        let n = n.max(2);
        let total = self.total_duration_ms().max(1) as f64;
        match self {
            ExecutorPlan::ConstantVus { vus, .. } => {
                let v = *vus as f64;
                (0..n)
                    .map(|i| {
                        let t = total * i as f64 / (n - 1) as f64;
                        (t, v)
                    })
                    .collect()
            }
            ExecutorPlan::ConstantArrival { rate, .. } => {
                let v = *rate;
                (0..n)
                    .map(|i| {
                        let t = total * i as f64 / (n - 1) as f64;
                        (t, v)
                    })
                    .collect()
            }
            ExecutorPlan::RampingVus {
                start_vus, stages, ..
            } => sample_ramping(*start_vus as f64, stages, total, n),
            ExecutorPlan::RampingArrival {
                start_rate, stages, ..
            } => sample_ramping(*start_rate, stages, total, n),
        }
    }
}

fn sample_ramping(start: f64, stages: &[Stage], total: f64, n: usize) -> Vec<(f64, f64)> {
    let mut points = Vec::with_capacity(n);
    for i in 0..n {
        let t = total * i as f64 / (n - 1) as f64;
        let mut elapsed = 0.0_f64;
        let mut current = start;
        let mut value = start;
        for stage in stages {
            let end = elapsed + stage.duration_ms as f64;
            if t <= end || (elapsed - end).abs() < f64::EPSILON {
                let span = stage.duration_ms.max(1) as f64;
                let ratio = ((t - elapsed) / span).clamp(0.0, 1.0);
                value = current + (stage.target - current) * ratio;
                break;
            }
            current = stage.target;
            value = stage.target;
            elapsed = end;
        }
        points.push((t, value));
    }
    points
}

fn scale_stages(stages: Vec<Stage>, target_total_ms: u64) -> Vec<Stage> {
    let current: u64 = stages.iter().map(|s| s.duration_ms).sum();
    if current == 0 || target_total_ms == 0 || current == target_total_ms {
        return stages;
    }
    let factor = target_total_ms as f64 / current as f64;
    let mut scaled: Vec<Stage> = stages
        .iter()
        .map(|stage| Stage {
            duration_ms: ((stage.duration_ms as f64) * factor).round().max(1.0) as u64,
            target: stage.target,
        })
        .collect();
    // Ajuste le dernier stage pour coller exactement à la durée cible.
    let sum: u64 = scaled.iter().map(|s| s.duration_ms).sum();
    if let Some(last) = scaled.last_mut() {
        if sum > target_total_ms {
            last.duration_ms = last.duration_ms.saturating_sub(sum - target_total_ms).max(1);
        } else if sum < target_total_ms {
            last.duration_ms += target_total_ms - sum;
        }
    }
    scaled
}

pub fn parse_duration_ms(input: &str) -> Result<u64, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err(t!("duration.error.empty").to_string());
    }
    let mut total = 0.0;
    let mut number = String::new();
    let mut consumed = false;
    let mut chars = input.chars().peekable();
    while let Some(character) = chars.next() {
        if character.is_ascii_digit() || character == '.' {
            number.push(character);
            continue;
        }
        let value: f64 = number
            .parse()
            .map_err(|_| t!("duration.error.invalid", input = input).to_string())?;
        number.clear();
        let mut unit = String::from(character);
        if character == 'm' && chars.peek() == Some(&'s') {
            unit.push(chars.next().unwrap_or('s'));
        }
        let factor = match unit.as_str() {
            "ms" => 1.0,
            "s" => 1000.0,
            "m" => 60_000.0,
            "h" => 3_600_000.0,
            _ => return Err(t!("duration.error.unknown_unit", unit = unit).to_string()),
        };
        total += value * factor;
        consumed = true;
    }
    if !number.is_empty() {
        return Err(t!("duration.error.missing_unit", input = input).to_string());
    }
    if !consumed {
        return Err(t!("duration.error.invalid_short", input = input).to_string());
    }
    Ok(total.round() as u64)
}

pub fn format_duration(ms: u64) -> String {
    let seconds = ms / 1000;
    if seconds >= 3600 {
        let hours = (seconds / 3600).to_string();
        let minutes = format!("{:02}", (seconds % 3600) / 60);
        t!("duration.format.hours", hours = hours, minutes = minutes).to_string()
    } else if seconds >= 60 {
        let minutes = (seconds / 60).to_string();
        let secs = format!("{:02}", seconds % 60);
        t!("duration.format.minutes", minutes = minutes, seconds = secs).to_string()
    } else if ms >= 1000 {
        t!("duration.format.seconds", seconds = seconds).to_string()
    } else {
        t!("duration.format.millis", ms = ms).to_string()
    }
}

fn peak_vus(profile: Profile, options: &RunOptions) -> u32 {
    if let Some(vus) = options.vus {
        return vus.max(1);
    }
    match profile {
        Profile::Smoke => 2,
        Profile::Load => 20,
        Profile::Stress | Profile::Spike => 100,
        Profile::Soak => 20,
        Profile::Tps => 50,
        Profile::Breakpoint => 100,
    }
}

fn target_tps(options: &RunOptions) -> f64 {
    options.tps.unwrap_or(10).max(1) as f64
}

fn duration_or(options: &RunOptions, default: &str) -> Result<u64, String> {
    match options.duration.as_deref() {
        Some(value) if !value.trim().is_empty() => parse_duration_ms(value),
        _ => parse_duration_ms(default),
    }
}

fn fraction(peak: u32, ratio: f64) -> f64 {
    ((peak as f64 * ratio).round() as u32).max(1) as f64
}

pub fn build_plan(profile: Profile, options: &RunOptions) -> Result<ExecutorPlan, String> {
    let peak = peak_vus(profile, options);
    let tps = target_tps(options);
    Ok(match profile {
        Profile::Smoke => ExecutorPlan::ConstantVus {
            vus: peak,
            duration_ms: duration_or(options, "1m")?,
        },
        Profile::Load => {
            let ramp_up = duration_or_field(options.ramp_up.as_deref(), "60s")?;
            let ramp_down = duration_or_field(options.ramp_down.as_deref(), "30s")?;
            ExecutorPlan::RampingVus {
                start_vus: 0,
                stages: vec![
                    Stage {
                        duration_ms: ramp_up,
                        target: peak as f64,
                    },
                    Stage {
                        duration_ms: duration_or(options, "3m")?,
                        target: peak as f64,
                    },
                    Stage {
                        duration_ms: ramp_down,
                        target: 0.0,
                    },
                ],
            }
        }
        Profile::Stress => {
            let stages = vec![
                Stage {
                    duration_ms: 60_000,
                    target: fraction(peak, 0.5),
                },
                Stage {
                    duration_ms: 120_000,
                    target: fraction(peak, 0.5),
                },
                Stage {
                    duration_ms: 60_000,
                    target: peak as f64,
                },
                Stage {
                    duration_ms: 120_000,
                    target: peak as f64,
                },
                Stage {
                    duration_ms: 60_000,
                    target: 0.0,
                },
            ];
            let stages = scale_optional(stages, options, "7m")?;
            ExecutorPlan::RampingVus {
                start_vus: 0,
                stages,
            }
        }
        Profile::Spike => {
            let stages = vec![
                Stage {
                    duration_ms: 10_000,
                    target: fraction(peak, 0.05),
                },
                Stage {
                    duration_ms: 10_000,
                    target: peak as f64,
                },
                Stage {
                    duration_ms: 60_000,
                    target: peak as f64,
                },
                Stage {
                    duration_ms: 10_000,
                    target: fraction(peak, 0.05),
                },
                Stage {
                    duration_ms: 30_000,
                    target: 0.0,
                },
            ];
            let stages = scale_optional(stages, options, "2m")?;
            ExecutorPlan::RampingVus {
                start_vus: 0,
                stages,
            }
        }
        Profile::Soak => ExecutorPlan::ConstantVus {
            vus: peak,
            duration_ms: duration_or(options, "30m")?,
        },
        Profile::Tps => ExecutorPlan::ConstantArrival {
            rate: tps,
            duration_ms: duration_or(options, "5m")?,
            pre_allocated_vus: ((tps * 3.0).ceil() as u32).max(10),
            max_vus: options
                .vus
                .unwrap_or_else(|| ((tps * 10.0).ceil() as u32).max(50)),
        },
        Profile::Breakpoint => ExecutorPlan::RampingArrival {
            start_rate: 1.0,
            stages: vec![Stage {
                duration_ms: duration_or(options, "10m")?,
                target: tps,
            }],
            pre_allocated_vus: (tps.ceil() as u32).max(10),
            max_vus: options
                .vus
                .unwrap_or_else(|| ((tps * 10.0).ceil() as u32).max(100)),
        },
    })
}

fn duration_or_field(value: Option<&str>, default: &str) -> Result<u64, String> {
    match value {
        Some(value) if !value.trim().is_empty() => parse_duration_ms(value),
        _ => parse_duration_ms(default),
    }
}

fn scale_optional(
    stages: Vec<Stage>,
    options: &RunOptions,
    default: &str,
) -> Result<Vec<Stage>, String> {
    let target = duration_or(options, default)?;
    Ok(scale_stages(stages, target))
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThresholdRule {
    pub metric: String,
    pub expr: String,
    /// En profil breakpoint, k6 interrompt le test dès que le seuil casse.
    pub abort_on_fail: bool,
    pub delay_abort: Option<Duration>,
    pub kind: ThresholdKind,
    pub limit: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThresholdKind {
    DurationP95,
    DurationP99,
    ErrorRate,
    ChecksRate,
    DroppedIterations,
    StepP95,
}

impl ThresholdRule {
    pub fn display_metric(&self) -> &str {
        &self.metric
    }
}

pub fn build_thresholds(test: &Test, profile: Profile) -> Vec<ThresholdRule> {
    let mut rules = Vec::new();
    let breakpoint = profile == Profile::Breakpoint;
    let abort = |rule: &mut ThresholdRule| {
        if breakpoint {
            rule.abort_on_fail = true;
            rule.delay_abort = Some(Duration::from_secs(30));
        }
    };

    if let Some(p95) = test.thresholds.p95 {
        let mut rule = ThresholdRule {
            metric: "http_req_duration".to_string(),
            expr: format!("p(95)<{p95}"),
            abort_on_fail: false,
            delay_abort: None,
            kind: ThresholdKind::DurationP95,
            limit: p95,
        };
        abort(&mut rule);
        rules.push(rule);
    }
    if let Some(p99) = test.thresholds.p99 {
        rules.push(ThresholdRule {
            metric: "http_req_duration".to_string(),
            expr: format!("p(99)<{p99}"),
            abort_on_fail: false,
            delay_abort: None,
            kind: ThresholdKind::DurationP99,
            limit: p99,
        });
    }
    if let Some(error_rate) = test.thresholds.error_rate {
        let mut rule = ThresholdRule {
            metric: "http_req_failed".to_string(),
            expr: format!("rate<{error_rate}"),
            abort_on_fail: false,
            delay_abort: None,
            kind: ThresholdKind::ErrorRate,
            limit: error_rate,
        };
        abort(&mut rule);
        rules.push(rule);
    }
    if let Some(checks_rate) = test.thresholds.checks_rate {
        rules.push(ThresholdRule {
            metric: "checks".to_string(),
            expr: format!("rate>{checks_rate}"),
            abort_on_fail: false,
            delay_abort: None,
            kind: ThresholdKind::ChecksRate,
            limit: checks_rate,
        });
    }
    if profile == Profile::Tps {
        rules.push(ThresholdRule {
            metric: "dropped_iterations".to_string(),
            expr: "count==0".to_string(),
            abort_on_fail: false,
            delay_abort: None,
            kind: ThresholdKind::DroppedIterations,
            limit: 0.0,
        });
    }
    for (index, step) in test.steps.iter().enumerate() {
        if let Some(p95) = step.p95 {
            let name = step.effective_name(index);
            rules.push(ThresholdRule {
                metric: format!("http_req_duration{{name:{name}}}"),
                expr: format!("p(95)<{p95}"),
                abort_on_fail: false,
                delay_abort: None,
                kind: ThresholdKind::StepP95,
                limit: p95,
            });
        }
    }
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(parse_duration_ms("30s").unwrap(), 30_000);
        assert_eq!(parse_duration_ms("5m").unwrap(), 300_000);
        assert_eq!(parse_duration_ms("1h30m").unwrap(), 5_400_000);
        assert_eq!(parse_duration_ms("250ms").unwrap(), 250);
        assert!(parse_duration_ms("5").is_err());
        assert!(parse_duration_ms("5x").is_err());
    }

    #[test]
    fn plans_have_expected_shapes() {
        let options = RunOptions::default();
        let plan = build_plan(Profile::Smoke, &options).unwrap();
        assert_eq!(
            plan,
            ExecutorPlan::ConstantVus {
                vus: 2,
                duration_ms: 60_000
            }
        );
        let plan = build_plan(
            Profile::Load,
            &RunOptions {
                vus: Some(50),
                duration: Some("2m".into()),
                ..RunOptions::default()
            },
        )
        .unwrap();
        assert_eq!(plan.total_duration_ms(), 60_000 + 120_000 + 30_000);
        assert_eq!(plan.peak_vus(), 50.0);
        let plan = build_plan(
            Profile::Load,
            &RunOptions {
                vus: Some(40),
                duration: Some("1m".into()),
                ramp_up: Some("20s".into()),
                ramp_down: Some("10s".into()),
                ..RunOptions::default()
            },
        )
        .unwrap();
        assert_eq!(plan.total_duration_ms(), 20_000 + 60_000 + 10_000);
        let plan = build_plan(
            Profile::Stress,
            &RunOptions {
                duration: Some("14m".into()),
                ..RunOptions::default()
            },
        )
        .unwrap();
        assert_eq!(plan.total_duration_ms(), 14 * 60_000);
        let plan = build_plan(
            Profile::Spike,
            &RunOptions {
                duration: Some("4m".into()),
                ..RunOptions::default()
            },
        )
        .unwrap();
        assert_eq!(plan.total_duration_ms(), 4 * 60_000);
        let plan = build_plan(
            Profile::Tps,
            &RunOptions {
                tps: Some(25),
                ..RunOptions::default()
            },
        )
        .unwrap();
        assert_eq!(plan.executor_name(), "constant-arrival-rate");
        let preview = plan.preview_points(16);
        assert_eq!(preview.len(), 16);
        assert!(preview.iter().all(|(_, v)| *v > 0.0));
    }

    #[test]
    fn thresholds_include_steps_and_abort() {
        let mut test = Test::default();
        test.steps.push(crate::models::Step {
            p95: Some(800.0),
            ..crate::models::Step::default()
        });
        let rules = build_thresholds(&test, Profile::Breakpoint);
        assert!(rules.iter().any(|rule| rule.kind == ThresholdKind::StepP95));
        assert!(rules
            .iter()
            .filter(|rule| rule.kind == ThresholdKind::DurationP95)
            .all(|rule| rule.abort_on_fail));
    }
}
