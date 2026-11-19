//! Résumés d'exécution : structure normalisée et conversion des anciens
//! rapports k6 (pour l'import de l'historique existant).

use crate::models::{CheckSummary, RunSummary, StepSummary, ThresholdResult};
use serde_json::Value;

pub fn from_legacy_k6(data: &Value) -> RunSummary {
    let metrics = data.get("metrics").and_then(Value::as_object);
    let value = |metric: &str, key: &str| -> Option<&Value> {
        metrics?.get(metric)?.get("values")?.get(key)
    };
    let number = |metric: &str, key: &str| -> Option<f64> { value(metric, key)?.as_f64() };
    let count = |metric: &str, key: &str| -> Option<u64> { value(metric, key)?.as_u64() };

    let mut thresholds = Vec::new();
    let mut steps: std::collections::BTreeMap<String, StepSummary> = Default::default();
    if let Some(metrics) = metrics {
        for (metric, definition) in metrics {
            if let Some(thresholds_map) = definition.get("thresholds").and_then(Value::as_object) {
                for (expr, result) in thresholds_map {
                    let neutral = ["max>=0", "rate>=0", "count>=0"]
                        .iter()
                        .any(|suffix| expr.ends_with(suffix));
                    if !neutral {
                        thresholds.push(ThresholdResult {
                            metric: metric.clone(),
                            expr: expr.clone(),
                            ok: result.get("ok").and_then(Value::as_bool).unwrap_or(false),
                            ..ThresholdResult::default()
                        });
                    }
                }
            }
            if let Some(rest) = metric.strip_prefix("http_req_duration{name:")
                && let Some(name) = rest.strip_suffix('}') {
                    let entry = steps.entry(name.to_string()).or_insert_with(|| StepSummary {
                        name: name.to_string(),
                        ..StepSummary::default()
                    });
                    entry.avg = number(metric, "avg");
                    entry.p95 = number(metric, "p(95)");
                    entry.p99 = number(metric, "p(99)");
                    entry.max = number(metric, "max");
                }
            if let Some(rest) = metric.strip_prefix("http_req_failed{name:")
                && let Some(name) = rest.strip_suffix('}') {
                    steps
                        .entry(name.to_string())
                        .or_insert_with(|| StepSummary {
                            name: name.to_string(),
                            ..StepSummary::default()
                        })
                        .error_rate = number(metric, "rate");
                }
            if let Some(rest) = metric.strip_prefix("http_reqs{name:")
                && let Some(name) = rest.strip_suffix('}') {
                    let entry = steps.entry(name.to_string()).or_insert_with(|| StepSummary {
                        name: name.to_string(),
                        ..StepSummary::default()
                    });
                    entry.count = count(metric, "count");
                    entry.rate = number(metric, "rate");
                }
        }
    }

    let mut checks = Vec::new();
    fn walk(group: &Value, checks: &mut Vec<CheckSummary>) {
        if let Some(list) = group.get("checks").and_then(Value::as_array) {
            for check in list {
                checks.push(CheckSummary {
                    name: check
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    passes: check.get("passes").and_then(Value::as_u64).unwrap_or(0),
                    fails: check.get("fails").and_then(Value::as_u64).unwrap_or(0),
                    ..CheckSummary::default()
                });
            }
        }
        if let Some(groups) = group.get("groups").and_then(Value::as_array) {
            for child in groups {
                walk(child, checks);
            }
        }
    }
    if let Some(root) = data.get("root_group") {
        walk(root, &mut checks);
    }

    RunSummary {
        requests: count("http_reqs", "count"),
        rps: number("http_reqs", "rate"),
        iterations: count("iterations", "count"),
        iterations_rate: number("iterations", "rate"),
        avg: number("http_req_duration", "avg"),
        med: number("http_req_duration", "med"),
        p90: number("http_req_duration", "p(90)"),
        p95: number("http_req_duration", "p(95)"),
        p99: number("http_req_duration", "p(99)"),
        max: number("http_req_duration", "max"),
        error_rate: number("http_req_failed", "rate"),
        checks_rate: number("checks", "rate"),
        vus_max: number("vus_max", "max").or_else(|| number("vus_max", "value")),
        dropped: count("dropped_iterations", "count"),
        thresholds,
        checks,
        steps: steps.into_values().collect(),
        ..RunSummary::default()
    }
}

impl RunSummary {
    pub fn thresholds_ok(&self) -> bool {
        self.thresholds.iter().all(|threshold| threshold.ok)
    }

    pub fn failed_thresholds(&self) -> impl Iterator<Item = &ThresholdResult> {
        self.thresholds.iter().filter(|threshold| !threshold.ok)
    }

    pub fn check_totals(&self) -> (u64, u64) {
        self.checks.iter().fold((0, 0), |(passes, fails), check| {
            (passes + check.passes, fails + check.fails)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn converts_legacy_summary() {
        let raw = json!({
            "root_group": {
                "groups": [{
                    "name": "step",
                    "checks": [{"name": "step : HTTP 200", "passes": 9, "fails": 1}]
                }],
                "checks": []
            },
            "metrics": {
                "http_reqs": {"values": {"count": 10, "rate": 2.5}},
                "http_req_duration": {"values": {"avg": 100.0, "p(95)": 250.0, "p(99)": 300.0, "max": 400.0}},
                "http_req_failed": {"values": {"rate": 0.1}},
                "checks": {"values": {"rate": 0.9}},
                "vus_max": {"values": {"max": 5}},
                "http_req_duration{name:home}": {"values": {"avg": 90.0, "p(95)": 200.0}},
                "http_req_failed{name:home}": {"values": {"rate": 0.0}},
                "http_reqs{name:home}": {"values": {"count": 10, "rate": 2.5}}
            }
        });
        let summary = from_legacy_k6(&raw);
        assert_eq!(summary.requests, Some(10));
        assert_eq!(summary.p95, Some(250.0));
        assert_eq!(summary.steps.len(), 1);
        assert_eq!(summary.steps[0].name, "home");
        assert_eq!(summary.checks.len(), 1);
    }
}
