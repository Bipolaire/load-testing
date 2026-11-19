//! Collecte des métriques (compteurs atomiques + histogrammes HDR).

use hdrhistogram::Histogram;
use lt_core::profiles::{ThresholdKind, ThresholdRule};
use lt_core::{CheckSummary, RunSummary, SeriesPoint, StepSummary, ThresholdResult};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

const MAX_US: u64 = 60_000_000;

fn new_histogram() -> Histogram<u64> {
    Histogram::new_with_bounds(1, MAX_US, 3).expect("bornes valides")
}

struct StepMetric {
    name: String,
    requests: AtomicU64,
    failures: AtomicU64,
    checks_passes: AtomicU64,
    checks_fails: AtomicU64,
    histogram: Mutex<Histogram<u64>>,
}

struct CheckMetric {
    name: String,
    step: usize,
    passes: AtomicU64,
    fails: AtomicU64,
}

pub struct Metrics {
    start: Instant,
    requests: AtomicU64,
    failures: AtomicU64,
    iterations: AtomicU64,
    dropped: AtomicU64,
    checks_passes: AtomicU64,
    checks_fails: AtomicU64,
    active_vus: AtomicU64,
    peak_vus: AtomicU64,
    histogram: Mutex<Histogram<u64>>,
    steps: Vec<StepMetric>,
    checks: Vec<CheckMetric>,
}

impl Metrics {
    /// `check_labels[step][i]` est le libellé complet du check `i` de l'étape.
    pub fn new(step_names: &[String], check_labels: &[Vec<String>]) -> Self {
        let steps: Vec<StepMetric> = step_names
            .iter()
            .map(|name| StepMetric {
                name: name.clone(),
                requests: AtomicU64::new(0),
                failures: AtomicU64::new(0),
                checks_passes: AtomicU64::new(0),
                checks_fails: AtomicU64::new(0),
                histogram: Mutex::new(new_histogram()),
            })
            .collect();
        let checks = check_labels
            .iter()
            .enumerate()
            .flat_map(|(step, labels)| {
                labels.iter().map(move |label| CheckMetric {
                    name: label.clone(),
                    step,
                    passes: AtomicU64::new(0),
                    fails: AtomicU64::new(0),
                })
            })
            .collect();
        Self {
            start: Instant::now(),
            requests: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            iterations: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            checks_passes: AtomicU64::new(0),
            checks_fails: AtomicU64::new(0),
            active_vus: AtomicU64::new(0),
            peak_vus: AtomicU64::new(0),
            histogram: Mutex::new(new_histogram()),
            steps,
            checks,
        }
    }

    pub fn start(&self) -> Instant {
        self.start
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    pub fn record_request(&self, step: usize, duration_us: u64, failed: bool) {
        let duration_us = duration_us.clamp(1, MAX_US);
        self.requests.fetch_add(1, Ordering::Relaxed);
        let _ = self.histogram.lock().record(duration_us);
        if failed {
            self.failures.fetch_add(1, Ordering::Relaxed);
        }
        if let Some(step) = self.steps.get(step) {
            step.requests.fetch_add(1, Ordering::Relaxed);
            let _ = step.histogram.lock().record(duration_us);
            if failed {
                step.failures.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Enregistre les résultats des checks de l'étape (`true` = réussi).
    pub fn record_checks(&self, step: usize, results: &[bool]) {
        for (index, ok) in results.iter().enumerate() {
            let Some(check) = self
                .checks
                .iter()
                .filter(|check| check.step == step)
                .nth(index)
            else {
                continue;
            };
            if *ok {
                check.passes.fetch_add(1, Ordering::Relaxed);
                self.checks_passes.fetch_add(1, Ordering::Relaxed);
                if let Some(step) = self.steps.get(step) {
                    step.checks_passes.fetch_add(1, Ordering::Relaxed);
                }
            } else {
                check.fails.fetch_add(1, Ordering::Relaxed);
                self.checks_fails.fetch_add(1, Ordering::Relaxed);
                if let Some(step) = self.steps.get(step) {
                    step.checks_fails.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    pub fn iteration(&self) {
        self.iterations.fetch_add(1, Ordering::Relaxed);
    }

    pub fn iterations(&self) -> u64 {
        self.iterations.load(Ordering::Relaxed)
    }

    pub fn dropped_iteration(&self) {
        self.dropped.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_active_vus(&self, vus: u64) {
        self.active_vus.store(vus, Ordering::Relaxed);
        self.peak_vus.fetch_max(vus, Ordering::Relaxed);
    }

    pub fn enter_vu(&self) {
        let active = self.active_vus.fetch_add(1, Ordering::Relaxed) + 1;
        self.peak_vus.fetch_max(active, Ordering::Relaxed);
    }

    pub fn leave_vu(&self) {
        let mut current = self.active_vus.load(Ordering::Relaxed);
        while current > 0 {
            match self.active_vus.compare_exchange_weak(
                current,
                current - 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(observed) => current = observed,
            }
        }
    }

    pub fn snapshot(&self) -> SeriesPoint {
        let elapsed = self.elapsed_ms();
        let seconds = (elapsed as f64 / 1000.0).max(0.001);
        let requests = self.requests.load(Ordering::Relaxed);
        let failures = self.failures.load(Ordering::Relaxed);
        let checks_passes = self.checks_passes.load(Ordering::Relaxed);
        let checks_fails = self.checks_fails.load(Ordering::Relaxed);
        let checks_total = checks_passes + checks_fails;
        SeriesPoint {
            t: elapsed / 1000,
            vus: self.active_vus.load(Ordering::Relaxed) as f64,
            rps: requests as f64 / seconds,
            p95: percentile(&self.histogram.lock(), 0.95),
            error_rate: if requests == 0 {
                0.0
            } else {
                failures as f64 / requests as f64
            },
            checks_rate: if checks_total == 0 {
                1.0
            } else {
                checks_passes as f64 / checks_total as f64
            },
            requests,
            iterations: self.iterations.load(Ordering::Relaxed),
        }
    }

    pub fn summary(&self, rules: &[ThresholdRule]) -> RunSummary {
        let elapsed = self.elapsed_ms();
        let seconds = (elapsed as f64 / 1000.0).max(0.001);
        let requests = self.requests.load(Ordering::Relaxed);
        let failures = self.failures.load(Ordering::Relaxed);
        let checks_passes = self.checks_passes.load(Ordering::Relaxed);
        let checks_fails = self.checks_fails.load(Ordering::Relaxed);
        let checks_total = checks_passes + checks_fails;
        let error_rate = if requests == 0 {
            0.0
        } else {
            failures as f64 / requests as f64
        };
        let checks_rate = if checks_total == 0 {
            None
        } else {
            Some(checks_passes as f64 / checks_total as f64)
        };
        let p95 = percentile(&self.histogram.lock(), 0.95);
        let p99 = percentile(&self.histogram.lock(), 0.99);

        let steps: Vec<StepSummary> = self
            .steps
            .iter()
            .map(|step| {
                let count = step.requests.load(Ordering::Relaxed);
                let failures = step.failures.load(Ordering::Relaxed);
                let histogram = step.histogram.lock();
                StepSummary {
                    name: step.name.clone(),
                    count: Some(count),
                    rate: Some(count as f64 / seconds),
                    avg: mean(&histogram),
                    p95: Some(percentile(&histogram, 0.95)),
                    p99: Some(percentile(&histogram, 0.99)),
                    max: max(&histogram),
                    error_rate: if count == 0 {
                        Some(0.0)
                    } else {
                        Some(failures as f64 / count as f64)
                    },
                    ..StepSummary::default()
                }
            })
            .collect();

        let mut threshold_results = Vec::with_capacity(rules.len());
        for rule in rules {
            let actual = match rule.kind {
                ThresholdKind::DurationP95 => Some(p95),
                ThresholdKind::DurationP99 => Some(p99),
                ThresholdKind::ErrorRate => Some(error_rate),
                ThresholdKind::ChecksRate => checks_rate,
                ThresholdKind::DroppedIterations => {
                    Some(self.dropped.load(Ordering::Relaxed) as f64)
                }
                ThresholdKind::StepP95 => step_percentile(&self.steps, &rule.metric, 0.95),
            };
            let ok = match rule.kind {
                ThresholdKind::DurationP95 | ThresholdKind::StepP95 => {
                    actual.map(|value| value < rule.limit).unwrap_or(false)
                }
                ThresholdKind::DurationP99 => actual.map(|value| value < rule.limit).unwrap_or(false),
                ThresholdKind::ErrorRate => actual.map(|value| value < rule.limit).unwrap_or(false),
                ThresholdKind::ChecksRate => actual.map(|value| value > rule.limit).unwrap_or(false),
                ThresholdKind::DroppedIterations => {
                    actual.map(|value| value == 0.0).unwrap_or(true)
                }
            };
            threshold_results.push(ThresholdResult {
                metric: rule.metric.clone(),
                expr: rule.expr.clone(),
                ok,
                ..ThresholdResult::default()
            });
        }

        let checks = self
            .checks
            .iter()
            .map(|check| CheckSummary {
                name: check.name.clone(),
                passes: check.passes.load(Ordering::Relaxed),
                fails: check.fails.load(Ordering::Relaxed),
                ..CheckSummary::default()
            })
            .filter(|check| check.passes + check.fails > 0)
            .collect::<Vec<_>>();

        let histogram = self.histogram.lock();
        RunSummary {
            requests: Some(requests),
            rps: Some(requests as f64 / seconds),
            iterations: Some(self.iterations.load(Ordering::Relaxed)),
            iterations_rate: Some(self.iterations.load(Ordering::Relaxed) as f64 / seconds),
            avg: mean(&histogram),
            med: if histogram.is_empty() {
                None
            } else {
                Some(histogram.value_at_quantile(0.5) as f64 / 1000.0)
            },
            p90: Some(percentile(&histogram, 0.90)),
            p95: Some(p95),
            p99: Some(p99),
            max: max(&histogram),
            error_rate: Some(error_rate),
            checks_rate,
            vus_max: Some(self.peak_vus.load(Ordering::Relaxed) as f64),
            dropped: Some(self.dropped.load(Ordering::Relaxed)),
            thresholds: threshold_results,
            checks,
            steps,
            ..RunSummary::default()
        }
    }

    /// Pour l'affichage live : p95, taux d'erreur et VUs actifs courants.
    pub fn current(&self) -> (f64, f64, f64) {
        let requests = self.requests.load(Ordering::Relaxed);
        let failures = self.failures.load(Ordering::Relaxed);
        (
            percentile(&self.histogram.lock(), 0.95),
            if requests == 0 {
                0.0
            } else {
                failures as f64 / requests as f64
            },
            self.active_vus.load(Ordering::Relaxed) as f64,
        )
    }
}

fn step_percentile(steps: &[StepMetric], metric: &str, quantile: f64) -> Option<f64> {
    let name = metric
        .strip_prefix("http_req_duration{name:")?
        .strip_suffix('}')?;
    let step = steps.iter().find(|step| step.name == name)?;
    let histogram = step.histogram.lock();
    if histogram.is_empty() {
        None
    } else {
        Some(histogram.value_at_quantile(quantile) as f64 / 1000.0)
    }
}

/// Histogrammes enregistrés en microsecondes, restitués en millisecondes.
fn percentile(histogram: &Histogram<u64>, quantile: f64) -> f64 {
    if histogram.is_empty() {
        0.0
    } else {
        histogram.value_at_quantile(quantile) as f64 / 1000.0
    }
}

fn mean(histogram: &Histogram<u64>) -> Option<f64> {
    if histogram.is_empty() {
        None
    } else {
        Some(histogram.mean() / 1000.0)
    }
}

fn max(histogram: &Histogram<u64>) -> Option<f64> {
    if histogram.is_empty() {
        None
    } else {
        Some(histogram.max() as f64 / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lt_core::Profile;

    #[test]
    fn computes_summary_and_thresholds() {
        let metrics = Metrics::new(
            &["home".to_string()],
            &[vec!["home : HTTP 200".to_string()]],
        );
        for value in 1..=100 {
            metrics.record_request(0, value * 1000, value > 95);
            metrics.record_checks(0, &[value <= 95]);
        }
        metrics.iteration();
        metrics.set_active_vus(5);
        let mut test = lt_core::Test::default();
        test.steps.push(lt_core::Step {
            name: "home".into(),
            p95: Some(200.0),
            ..lt_core::Step::default()
        });
        let rules = lt_core::profiles::build_thresholds(&test, Profile::Load);
        let summary = metrics.summary(&rules);
        assert_eq!(summary.requests, Some(100));
        assert_eq!(summary.steps[0].name, "home");
        assert!(summary.p95.unwrap_or(0.0) > 90.0);
        assert_eq!(summary.vus_max, Some(5.0));
        assert_eq!(summary.checks.len(), 1);
        assert_eq!(summary.checks[0].passes, 95);
        assert_eq!(summary.checks[0].fails, 5);
    }
}
