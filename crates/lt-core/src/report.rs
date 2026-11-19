//! Génération des rapports HTML/JSON/PDF d'une exécution.

use crate::i18n;
use crate::models::{Run, RunSeries, RunStatus, RunSummary, SeriesPoint};
use crate::profiles::format_duration;
use crate::report_pdf::{
    ACCENT, Align, CHART_PAD_Y, CHART_PLOT_H, CHART_PLOT_W, Cell, ChartSpec, Kpi, PdfBuilder,
    Rgb3, Tone,
};
use crate::storage::Workspace;
use rust_i18n::t;
use std::fmt::Write as _;

fn escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn fmt_ms(value: Option<f64>) -> String {
    match value {
        None => "–".to_string(),
        Some(value) if value >= 1000.0 => format!("{:.2} s", value / 1000.0),
        Some(value) => format!("{} ms", value.round() as i64),
    }
}

fn fmt_pct(value: Option<f64>) -> String {
    match value {
        None => "–".to_string(),
        Some(value) => format!("{:.1} %", value * 100.0),
    }
}

fn fmt_num(value: Option<f64>) -> String {
    match value {
        None => "–".to_string(),
        Some(value) => {
            let rounded = value.round();
            if (value - rounded).abs() < 0.005 {
                format!("{rounded:.0}")
            } else {
                format!("{value:.2}")
            }
        }
    }
}

/// Date locale : `JJ/MM/AAAA` en français, `MM/JJ/AAAA` sinon.
fn fmt_date(timestamp: i64) -> String {
    let format = if i18n::is_french() {
        "%d/%m/%Y %H:%M:%S"
    } else {
        "%m/%d/%Y %H:%M:%S"
    };
    chrono::DateTime::from_timestamp_millis(timestamp)
        .map(|date| date.with_timezone(&chrono::Local).format(format).to_string())
        .unwrap_or_default()
}

fn status_class(run: &Run) -> &'static str {
    match run.status {
        RunStatus::Passed => "ok",
        RunStatus::Failed | RunStatus::Error | RunStatus::Interrupted => "ko",
        _ => "warn",
    }
}

fn status_tone(run: &Run) -> Tone {
    match status_class(run) {
        "ok" => Tone::Ok,
        "ko" => Tone::Ko,
        _ => Tone::Warn,
    }
}

fn run_duration(run: &Run) -> String {
    run.duration_ms()
        .map(|ms| format_duration(ms as u64))
        .unwrap_or_else(|| "—".to_string())
}

fn run_options_line(run: &Run) -> String {
    let mut parts = Vec::new();
    if let Some(vus) = run.options.vus {
        parts.push(format!("vus = {vus}"));
    }
    if let Some(tps) = run.options.tps {
        parts.push(format!("tps = {tps}"));
    }
    if let Some(duration) = &run.options.duration {
        parts.push(format!("duration = {duration}"));
    }
    if let Some(iterations) = run.options.iterations {
        parts.push(format!("iterations = {iterations}"));
    }
    parts.join(" · ")
}

fn run_title(run: &Run) -> String {
    format!("{} · {}", run.test_name, run.profile.label())
}

// ---------------------------------------------------------------- KPIs

struct KpiItem {
    id: &'static str,
    label: String,
    value: String,
    metric: Option<&'static str>,
}

/// `Some(true)` si tous les seuils de la métrique sont respectés, `Some(false)`
/// si l'un d'eux ne l'est pas, `None` s'il n'y a pas de seuil sur cette métrique.
fn threshold_state(summary: &RunSummary, metric: &str) -> Option<bool> {
    let mut found = false;
    for threshold in summary.thresholds.iter().filter(|t| t.metric == metric) {
        if !threshold.ok {
            return Some(false);
        }
        found = true;
    }
    found.then_some(true)
}

fn kpi_items(summary: &RunSummary) -> Vec<KpiItem> {
    let item = |id, label: String, value: String, metric| KpiItem {
        id,
        label,
        value,
        metric,
    };
    vec![
        item(
            "requests",
            t!("report.kpi.requests").to_string(),
            fmt_num(summary.requests.map(|v| v as f64)),
            None,
        ),
        item(
            "rps",
            t!("report.kpi.rps").to_string(),
            format!("{} req/s", fmt_num(summary.rps)),
            None,
        ),
        item(
            "transactions",
            t!("report.kpi.transactions").to_string(),
            format!(
                "{} ({} /s)",
                fmt_num(summary.iterations.map(|v| v as f64)),
                fmt_num(summary.iterations_rate)
            ),
            None,
        ),
        item(
            "vus_max",
            t!("report.kpi.vus_max").to_string(),
            fmt_num(summary.vus_max),
            None,
        ),
        item(
            "avg",
            t!("report.kpi.avg").to_string(),
            fmt_ms(summary.avg),
            None,
        ),
        item(
            "med",
            t!("report.kpi.med").to_string(),
            fmt_ms(summary.med),
            None,
        ),
        item(
            "p95",
            t!("report.kpi.p95").to_string(),
            fmt_ms(summary.p95),
            Some("http_req_duration"),
        ),
        item(
            "p99",
            t!("report.kpi.p99").to_string(),
            fmt_ms(summary.p99),
            None,
        ),
        item(
            "max",
            t!("report.kpi.max").to_string(),
            fmt_ms(summary.max),
            None,
        ),
        item(
            "http_errors",
            t!("report.kpi.http_errors").to_string(),
            fmt_pct(summary.error_rate),
            Some("http_req_failed"),
        ),
        item(
            "checks_ok",
            t!("report.kpi.checks_ok").to_string(),
            fmt_pct(summary.checks_rate),
            Some("checks"),
        ),
        item(
            "dropped",
            t!("report.kpi.dropped").to_string(),
            fmt_num(summary.dropped.map(|v| v as f64)),
            Some("dropped_iterations"),
        ),
    ]
}

fn pdf_kpis(summary: &RunSummary, only: Option<&[&str]>) -> Vec<Kpi> {
    kpi_items(summary)
        .into_iter()
        .filter(|item| only.is_none_or(|ids| ids.contains(&item.id)))
        .map(|item| Kpi {
            tone: match item.metric.and_then(|metric| threshold_state(summary, metric)) {
                Some(true) => Tone::Ok,
                Some(false) => Tone::Ko,
                None => Tone::Neutral,
            },
            label: item.label,
            value: item.value,
        })
        .collect()
}

fn check_rate(passes: u64, fails: u64) -> f64 {
    let total = passes + fails;
    if total == 0 {
        0.0
    } else {
        passes as f64 / total as f64
    }
}

// -------------------------------------------------------------- charts

/// Points d'une courbe mis à l'échelle dans une zone `width × height`
/// (origine en haut à gauche, axe y vers le bas).
pub(crate) struct ScaledPoints {
    pub coords: Vec<(f64, f64)>,
    pub min_v: f64,
    pub max_v: f64,
    pub max_t: f64,
    pub last: f64,
    pub plot_w: f64,
    pub plot_h: f64,
}

pub(crate) fn scale_points(
    points: &[(f64, f64)],
    width: f64,
    height: f64,
    pad_x: f64,
    pad_y: f64,
) -> Option<ScaledPoints> {
    if points.is_empty() {
        return None;
    }
    let max_t = points
        .iter()
        .map(|(t, _)| *t)
        .fold(1.0_f64, f64::max)
        .max(1.0);
    let mut min_v = points
        .iter()
        .map(|(_, v)| *v)
        .fold(f64::INFINITY, f64::min);
    let mut max_v = points
        .iter()
        .map(|(_, v)| *v)
        .fold(f64::NEG_INFINITY, f64::max);
    if !min_v.is_finite() {
        min_v = 0.0;
    }
    if !max_v.is_finite() {
        max_v = 1.0;
    }
    if (max_v - min_v).abs() < f64::EPSILON {
        max_v = min_v + 1.0;
    }
    let plot_w = width - pad_x * 2.0;
    let plot_h = height - pad_y * 2.0;
    let coords = points
        .iter()
        .map(|(t, value)| {
            (
                pad_x + (t / max_t) * plot_w,
                pad_y + plot_h - ((value - min_v) / (max_v - min_v)) * plot_h,
            )
        })
        .collect();
    Some(ScaledPoints {
        coords,
        min_v,
        max_v,
        max_t,
        last: points.last().map(|(_, value)| *value).unwrap_or(0.0),
        plot_w,
        plot_h,
    })
}

/// Libellés (dernier, max, min) d'un graphique.
fn chart_labels(unit: &str, percent: bool, scaled: &ScaledPoints) -> (String, String, String) {
    let format = |value: f64, percent_decimals: usize| {
        if percent {
            format!("{:.*} %", percent_decimals, value * 100.0)
        } else if unit == "ms" {
            fmt_ms(Some(value))
        } else {
            format!("{value:.0} {unit}").trim().to_string()
        }
    };
    (
        format(scaled.last, 1),
        format(scaled.max_v, 0),
        format(scaled.min_v, 0),
    )
}

fn line_chart(
    title: &str,
    unit: &str,
    points: &[(f64, f64)],
    color: &str,
    percent: bool,
) -> String {
    let width = 760.0;
    let height = 130.0;
    let pad_x = 10.0;
    let pad_y = 12.0;
    let Some(scaled) = scale_points(points, width, height, pad_x, pad_y) else {
        return format!(
            "<div class=\"chart\"><h3>{}</h3><div class=\"empty\">{}</div></div>",
            escape(title),
            escape(&t!("report.chart.no_data"))
        );
    };

    let mut path = String::new();
    for (index, (x, y)) in scaled.coords.iter().enumerate() {
        if index == 0 {
            let _ = write!(path, "M{x:.1},{y:.1}");
        } else {
            let _ = write!(path, " L{x:.1},{y:.1}");
        }
    }
    let base = pad_y + scaled.plot_h;
    let area = format!(
        "M{:.1},{:.1} L{} L{:.1},{:.1} Z",
        pad_x,
        base,
        path.trim_start_matches('M'),
        pad_x + scaled.plot_w,
        base
    );
    let (last_label, max_label, min_label) = chart_labels(unit, percent, &scaled);

    format!(
        "<div class=\"chart\"><div class=\"chart-head\"><h3>{title}</h3><span>{last_label}</span></div>\
         <svg viewBox=\"0 0 {width} {height}\" role=\"img\" aria-label=\"{title}\">\
         <line x1=\"{px}\" y1=\"{py}\" x2=\"{px2}\" y2=\"{py}\" class=\"grid\"/>\
         <line x1=\"{px}\" y1=\"{mid}\" x2=\"{px2}\" y2=\"{mid}\" class=\"grid\"/>\
         <line x1=\"{px}\" y1=\"{py2}\" x2=\"{px2}\" y2=\"{py2}\" class=\"grid\"/>\
         <path d=\"{area}\" fill=\"{color}\" opacity=\"0.12\"/>\
         <path d=\"{path}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"2\"/>\
         <text x=\"{px}\" y=\"{ty}\" class=\"axis\">{max_label}</text>\
         <text x=\"{px}\" y=\"{by}\" class=\"axis\">{min_label}</text>\
         </svg><div class=\"chart-foot\"><span>0 s</span><span>{duration}</span></div></div>",
        title = escape(title),
        px = pad_x,
        px2 = width - pad_x,
        py = pad_y,
        py2 = base,
        mid = pad_y + scaled.plot_h / 2.0,
        ty = pad_y + 4.0,
        by = base - 2.0,
        duration = format_duration((scaled.max_t * 1000.0) as u64),
        last_label = escape(&last_label),
        max_label = escape(&max_label),
        min_label = escape(&min_label),
    )
}

struct ChartDef {
    title: String,
    unit: &'static str,
    hex: &'static str,
    rgb: Rgb3,
    percent: bool,
    value: fn(&SeriesPoint) -> f64,
}

fn chart_defs() -> Vec<ChartDef> {
    vec![
        ChartDef {
            title: t!("report.chart.vus").to_string(),
            unit: "",
            hex: "#5b7fe0",
            rgb: (0.357, 0.498, 0.878),
            percent: false,
            value: |p| p.vus,
        },
        ChartDef {
            title: t!("report.chart.rps").to_string(),
            unit: "req/s",
            hex: "#5b4fe0",
            rgb: ACCENT,
            percent: false,
            value: |p| p.rps,
        },
        ChartDef {
            title: t!("report.chart.p95").to_string(),
            unit: "ms",
            hex: "#b7791f",
            rgb: (0.718, 0.475, 0.122),
            percent: false,
            value: |p| p.p95,
        },
        ChartDef {
            title: t!("report.chart.http_errors").to_string(),
            unit: "%",
            hex: "#d93838",
            rgb: (0.851, 0.220, 0.220),
            percent: true,
            value: |p| p.error_rate,
        },
    ]
}

fn series_points(series: &RunSeries, value: fn(&SeriesPoint) -> f64) -> Vec<(f64, f64)> {
    series
        .points
        .iter()
        .map(|point| (point.t as f64, value(point)))
        .collect()
}

/// Réduit le nombre de points (en gardant le dernier) pour alléger le PDF.
fn downsample(points: &[(f64, f64)], max: usize) -> Vec<(f64, f64)> {
    if points.len() <= max || max < 2 {
        return points.to_vec();
    }
    let stride = points.len().div_ceil(max);
    let mut sampled: Vec<(f64, f64)> = points.iter().step_by(stride).copied().collect();
    if let (Some(last), Some(sampled_last)) = (points.last(), sampled.last())
        && last != sampled_last
    {
        sampled.push(*last);
    }
    sampled
}

fn pdf_chart(def: &ChartDef, points: &[(f64, f64)]) -> ChartSpec {
    let sampled = downsample(points, 400);
    let scaled = scale_points(
        &sampled,
        CHART_PLOT_W as f64,
        CHART_PLOT_H as f64,
        0.0,
        CHART_PAD_Y as f64,
    );
    let mut spec = ChartSpec {
        title: def.title.clone(),
        last_label: String::new(),
        max_label: String::new(),
        min_label: String::new(),
        duration_label: String::new(),
        no_data: t!("report.chart.no_data").to_string(),
        color: def.rgb,
        coords: Vec::new(),
    };
    if let Some(scaled) = scaled {
        let (last, max, min) = chart_labels(def.unit, def.percent, &scaled);
        spec.last_label = last;
        spec.max_label = max;
        spec.min_label = min;
        spec.duration_label = format_duration((scaled.max_t * 1000.0) as u64);
        spec.coords = scaled.coords;
    }
    spec
}

// ---------------------------------------------------------------- HTML

pub fn render_html(run: &Run, summary: &RunSummary, series: &RunSeries) -> String {
    let status = run.status.label();
    let class = status_class(run);
    let options = run_options_line(run);
    let duration = run_duration(run);

    let kpis = kpi_items(summary)
        .into_iter()
        .map(|item| {
            let state = match item.metric.and_then(|metric| threshold_state(summary, metric)) {
                Some(true) => "ok",
                Some(false) => "ko",
                None => "",
            };
            format!(
                "<div class=\"kpi {state}\"><div class=\"l\">{}</div><div class=\"v\">{}</div></div>",
                escape(&item.label),
                escape(&item.value)
            )
        })
        .collect::<String>();

    let steps_rows = summary
        .steps
        .iter()
        .map(|step| {
            format!(
                "<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
                escape(&step.name),
                fmt_num(step.count.map(|value| value as f64)),
                fmt_num(step.rate),
                fmt_ms(step.avg),
                fmt_ms(step.p95),
                fmt_ms(step.p99),
                fmt_ms(step.max),
                fmt_pct(step.error_rate),
            )
        })
        .collect::<String>();

    let checks_rows = summary
        .checks
        .iter()
        .map(|check| {
            let rate = check_rate(check.passes, check.fails);
            format!(
                "<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\"><span class=\"badge {}\">{}</span></td></tr>",
                escape(&check.name),
                check.passes,
                check.fails,
                if check.fails > 0 { "ko" } else { "ok" },
                fmt_pct(Some(rate)),
            )
        })
        .collect::<String>();

    let threshold_ok = t!("report.threshold.ok").to_string();
    let threshold_ko = t!("report.threshold.ko").to_string();
    let thresholds_rows = summary
        .thresholds
        .iter()
        .map(|threshold| {
            format!(
                "<tr><td class=\"mono\">{}</td><td class=\"mono\">{}</td><td><span class=\"badge {}\">{}</span></td></tr>",
                escape(&threshold.metric),
                escape(&threshold.expr),
                if threshold.ok { "ok" } else { "ko" },
                escape(if threshold.ok { &threshold_ok } else { &threshold_ko }),
            )
        })
        .collect::<String>();

    let charts = if series.points.is_empty() {
        String::new()
    } else {
        let body = chart_defs()
            .iter()
            .map(|def| {
                line_chart(
                    &def.title,
                    def.unit,
                    &series_points(series, def.value),
                    def.hex,
                    def.percent,
                )
            })
            .collect::<String>();
        format!(
            "<section class=\"card\"><h2>{}</h2><div class=\"charts\">{body}</div></section>",
            escape(&t!("report.section.evolution"))
        )
    };

    let title = run_title(run);
    let meta = format!(
        "{} · {} · {}: {}{}",
        run.profile.label(),
        fmt_date(run.started_at),
        t!("report.meta.duration"),
        duration,
        if options.is_empty() {
            String::new()
        } else {
            format!(" · {options}")
        }
    );
    let detail = |label: String, value: &str| {
        format!(
            "<p class=\"mono\">{} = {}</p>",
            escape(&label.to_lowercase()),
            escape(value)
        )
    };
    let details = format!(
        "{}{}{}{}",
        detail(t!("report.details.run").to_string(), &run.id),
        detail(t!("report.details.profile").to_string(), &run.profile.label()),
        detail(t!("report.meta.started").to_string(), &fmt_date(run.started_at)),
        detail(t!("report.meta.status").to_string(), &status),
    );

    let thresholds = if summary.thresholds.is_empty() {
        format!("<div class=\"empty\">{}</div>", escape(&t!("report.empty.thresholds")))
    } else {
        format!(
            "<table><tr><th>{}</th><th>{}</th><th>{}</th></tr>{thresholds_rows}</table>",
            escape(&t!("report.table.metric")),
            escape(&t!("report.table.target")),
            escape(&t!("report.table.result")),
        )
    };
    let steps = if summary.steps.is_empty() {
        format!("<div class=\"empty\">{}</div>", escape(&t!("report.empty.steps")))
    } else {
        let head = [
            ("report.table.step", false),
            ("report.table.requests", true),
            ("report.table.rps", true),
            ("report.table.avg", true),
            ("report.table.p95", true),
            ("report.table.p99", true),
            ("report.table.max", true),
            ("report.table.errors", true),
        ]
        .iter()
        .map(|(key, num)| {
            format!(
                "<th{}>{}</th>",
                if *num { " class=\"num\"" } else { "" },
                escape(&t!(*key))
            )
        })
        .collect::<String>();
        format!("<table><tr>{head}</tr>{steps_rows}</table>")
    };
    let checks = if summary.checks.is_empty() {
        format!("<div class=\"empty\">{}</div>", escape(&t!("report.empty.checks")))
    } else {
        format!(
            "<table><tr><th>{}</th><th class=\"num\">{}</th><th class=\"num\">{}</th><th class=\"num\">{}</th></tr>{checks_rows}</table>",
            escape(&t!("report.table.check")),
            escape(&t!("report.table.passes")),
            escape(&t!("report.table.fails")),
            escape(&t!("report.table.rate")),
        )
    };

    format!(
        r#"<!doctype html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
:root {{ color-scheme: light dark; --bg:#f5f6f8; --panel:#fff; --panel-2:#f0f2f5; --border:#dfe3e8; --text:#1d2330; --muted:#677085; --accent:#5b4fe0; --ok:#1f9d55; --ok-bg:#e5f6ec; --ko:#d93838; --ko-bg:#fde8e8; --warn:#b7791f; --warn-bg:#fdf3e1; }}
@media (prefers-color-scheme: dark) {{ :root {{ --bg:#12141a; --panel:#1a1d25; --panel-2:#222631; --border:#2e3340; --text:#e6e8ee; --muted:#9098ab; --accent:#8a80ff; --ok:#4cc38a; --ok-bg:#173326; --ko:#f07171; --ko-bg:#3a1d1f; --warn:#e0a84a; --warn-bg:#3a2e17; }} }}
* {{ box-sizing: border-box; }}
body {{ margin:0; background:var(--bg); color:var(--text); font:14px/1.5 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; }}
main {{ max-width: 1000px; margin: 0 auto; padding: 24px 16px 60px; }}
header {{ background:var(--panel); border-bottom:1px solid var(--border); padding:18px 16px; }}
header .inner {{ max-width:1000px; margin:0 auto; display:flex; align-items:center; gap:12px; flex-wrap:wrap; }}
h1 {{ font-size:20px; margin:0; }} h2 {{ font-size:17px; margin:0 0 14px; }} h3 {{ font-size:13px; margin:0; color:var(--muted); }}
.card {{ background:var(--panel); border:1px solid var(--border); border-radius:12px; padding:18px; margin-bottom:16px; }}
.kpis {{ display:grid; grid-template-columns:repeat(auto-fill,minmax(150px,1fr)); gap:10px; }}
.kpi {{ background:var(--panel-2); border-radius:10px; padding:10px 12px; }}
.kpi .l {{ font-size:12px; color:var(--muted); }} .kpi .v {{ font-size:20px; font-weight:700; font-variant-numeric:tabular-nums; }}
.kpi.ok .v {{ color:var(--ok); }} .kpi.ko .v {{ color:var(--ko); }}
.badge {{ display:inline-block; font-size:11px; font-weight:700; padding:2px 8px; border-radius:99px; }}
.badge.ok {{ background:var(--ok-bg); color:var(--ok); }} .badge.ko {{ background:var(--ko-bg); color:var(--ko); }}
.badge.warn {{ background:var(--warn-bg); color:var(--warn); }}
table {{ width:100%; border-collapse:collapse; font-size:13px; }}
th,td {{ text-align:left; padding:8px; border-bottom:1px solid var(--border); }}
th {{ color:var(--muted); font-size:12px; font-weight:600; }} td.num,th.num {{ text-align:right; font-variant-numeric:tabular-nums; }}
.mono {{ font-family:ui-monospace, Consolas, monospace; font-size:12px; }}
.charts {{ display:grid; grid-template-columns:1fr 1fr; gap:14px; }}
.chart {{ background:var(--panel-2); border-radius:10px; padding:10px 12px; }}
.chart-head {{ display:flex; justify-content:space-between; align-items:baseline; margin-bottom:6px; }} .chart-head span {{ font-weight:700; font-variant-numeric:tabular-nums; }}
.chart-foot {{ display:flex; justify-content:space-between; color:var(--muted); font-size:11px; }}
svg {{ width:100%; height:130px; display:block; }}
.grid {{ stroke:var(--border); stroke-width:1; }} .axis {{ fill:var(--muted); font-size:10px; }}
.empty {{ color:var(--muted); padding:20px 0; text-align:center; }}
@media (max-width: 760px) {{ .charts {{ grid-template-columns:1fr; }} }}
</style>
</head>
<body>
<header><div class="inner"><h1>{title}</h1><span class="badge {class}">{status}</span><span style="color:var(--muted)">{meta}</span></div></header>
<main>
<section class="card"><div class="kpis">
{kpis}
</div></section>
{charts}
<section class="card"><h2>{h_thresholds}</h2>{thresholds}</section>
<section class="card"><h2>{h_steps}</h2>{steps}</section>
<section class="card"><h2>{h_checks}</h2>{checks}</section>
<section class="card"><h2>{h_details}</h2>{details}</section>
</main>
</body>
</html>"#,
        lang = escape(&i18n::language()),
        title = escape(&title),
        class = class,
        status = escape(&status),
        meta = escape(&meta),
        h_thresholds = escape(&t!("report.section.thresholds")),
        h_steps = escape(&t!("report.section.by_step")),
        h_checks = escape(&t!("report.section.checks")),
        h_details = escape(&t!("report.section.details")),
    )
}

// ----------------------------------------------------------------- PDF

fn footer_label(run: &Run) -> String {
    format!("{} · {}", t!("report.app_name"), run.id)
}

fn meta_line(run: &Run) -> String {
    let options = run_options_line(run);
    let mut meta = format!(
        "{}: {} · {}: {}",
        t!("report.meta.started"),
        fmt_date(run.started_at),
        t!("report.meta.duration"),
        run_duration(run),
    );
    if !options.is_empty() {
        meta.push_str(" · ");
        meta.push_str(&options);
    }
    meta
}

fn write_header(pdf: &mut PdfBuilder, run: &Run) {
    pdf.header(
        &run_title(run),
        &run.status.label().to_uppercase(),
        status_tone(run),
        &meta_line(run),
    );
}

fn write_thresholds_section(pdf: &mut PdfBuilder, summary: &RunSummary) {
    pdf.heading(
        &t!("report.section.thresholds"),
        if summary.thresholds.is_empty() { 6.0 } else { 20.0 },
    );
    if summary.thresholds.is_empty() {
        pdf.muted_line(&t!("report.empty.thresholds"));
        return;
    }
    let headers = vec![
        t!("report.table.metric").to_string(),
        t!("report.table.target").to_string(),
        t!("report.table.result").to_string(),
    ];
    let rows: Vec<Vec<Cell>> = summary
        .thresholds
        .iter()
        .map(|threshold| {
            vec![
                Cell::text(threshold.metric.clone()),
                Cell::text(threshold.expr.clone()),
                if threshold.ok {
                    Cell::badge(t!("report.threshold.ok").to_string(), Tone::Ok)
                } else {
                    Cell::badge(t!("report.threshold.ko").to_string(), Tone::Ko)
                },
            ]
        })
        .collect();
    pdf.table(
        &headers,
        &rows,
        &[4.0, 4.0, 2.2],
        &[Align::Left, Align::Left, Align::Left],
    );
}

fn write_steps_section(pdf: &mut PdfBuilder, summary: &RunSummary) {
    pdf.heading(
        &t!("report.section.by_step"),
        if summary.steps.is_empty() { 6.0 } else { 20.0 },
    );
    if summary.steps.is_empty() {
        pdf.muted_line(&t!("report.empty.steps"));
        return;
    }
    let headers: Vec<String> = [
        "report.table.step",
        "report.table.requests",
        "report.table.rps",
        "report.table.avg",
        "report.table.p95",
        "report.table.p99",
        "report.table.max",
        "report.table.errors",
    ]
    .iter()
    .map(|key| t!(*key).to_string())
    .collect();
    let rows: Vec<Vec<Cell>> = summary
        .steps
        .iter()
        .map(|step| {
            vec![
                Cell::text(step.name.clone()),
                Cell::text(fmt_num(step.count.map(|value| value as f64))),
                Cell::text(fmt_num(step.rate)),
                Cell::text(fmt_ms(step.avg)),
                Cell::text(fmt_ms(step.p95)),
                Cell::text(fmt_ms(step.p99)),
                Cell::text(fmt_ms(step.max)),
                Cell::text(fmt_pct(step.error_rate)),
            ]
        })
        .collect();
    let mut aligns = vec![Align::Right; 8];
    aligns[0] = Align::Left;
    pdf.table(
        &headers,
        &rows,
        &[4.2, 1.9, 1.4, 1.5, 1.5, 1.5, 1.5, 1.6],
        &aligns,
    );
}

fn write_checks_section(pdf: &mut PdfBuilder, summary: &RunSummary) {
    pdf.heading(
        &t!("report.section.checks"),
        if summary.checks.is_empty() { 6.0 } else { 20.0 },
    );
    if summary.checks.is_empty() {
        pdf.muted_line(&t!("report.empty.checks"));
        return;
    }
    let headers: Vec<String> = [
        "report.table.check",
        "report.table.passes",
        "report.table.fails",
        "report.table.rate",
    ]
    .iter()
    .map(|key| t!(*key).to_string())
    .collect();
    let rows: Vec<Vec<Cell>> = summary
        .checks
        .iter()
        .map(|check| {
            let rate = check_rate(check.passes, check.fails);
            vec![
                Cell::text(check.name.clone()),
                Cell::text(check.passes.to_string()),
                Cell::text(check.fails.to_string()),
                Cell::badge(
                    fmt_pct(Some(rate)),
                    if check.fails > 0 { Tone::Ko } else { Tone::Ok },
                ),
            ]
        })
        .collect();
    pdf.table(
        &headers,
        &rows,
        &[6.0, 1.6, 1.6, 1.8],
        &[Align::Left, Align::Right, Align::Right, Align::Right],
    );
}

/// Synthèse exécutive PDF (verdict, KPIs clés, seuils).
pub fn render_executive_pdf(run: &Run, summary: &RunSummary) -> Vec<u8> {
    let mut pdf = PdfBuilder::new(
        &t!(
            "report.pdf.executive_title",
            test = run.test_name,
            profile = run.profile.label()
        ),
        &footer_label(run),
    );
    write_header(&mut pdf, run);

    let tone = status_tone(run);
    let verdict = match run.status {
        RunStatus::Passed => t!("report.pdf.verdict_passed").to_string(),
        _ if tone == Tone::Ko => t!("report.pdf.verdict_failed").to_string(),
        status => status.label().to_uppercase(),
    };
    let total = summary.thresholds.len();
    let failed = summary.thresholds.iter().filter(|t| !t.ok).count();
    let sentence = if failed > 0 {
        t!("report.pdf.summary_failed", ko = failed, total = total).to_string()
    } else if total > 0 && run.status == RunStatus::Passed {
        t!("report.pdf.summary_passed", total = total).to_string()
    } else if total == 0 && run.status == RunStatus::Passed {
        t!("report.pdf.summary_no_thresholds").to_string()
    } else {
        t!("report.pdf.summary_status", status = run.status.label()).to_string()
    };
    pdf.verdict(&verdict, tone, &sentence);

    pdf.heading(&t!("report.section.kpis"), 38.0);
    pdf.kpi_grid(&pdf_kpis(
        summary,
        Some(&[
            "requests",
            "rps",
            "vus_max",
            "p95",
            "p99",
            "http_errors",
            "checks_ok",
            "dropped",
        ]),
    ));
    write_thresholds_section(&mut pdf, summary);
    pdf.finish()
}

/// Rapport PDF complet (KPIs, graphiques, seuils, étapes, checks).
pub fn render_summary_pdf(run: &Run, summary: &RunSummary, series: &RunSeries) -> Vec<u8> {
    let mut pdf = PdfBuilder::new(
        &t!(
            "report.pdf.full_title",
            test = run.test_name,
            profile = run.profile.label()
        ),
        &footer_label(run),
    );
    write_header(&mut pdf, run);

    pdf.heading(&t!("report.section.kpis"), 38.0);
    pdf.kpi_grid(&pdf_kpis(summary, None));

    if !series.points.is_empty() {
        pdf.heading(&t!("report.section.evolution"), 50.0);
        let charts: Vec<ChartSpec> = chart_defs()
            .iter()
            .map(|def| pdf_chart(def, &series_points(series, def.value)))
            .collect();
        pdf.charts_grid(&charts);
    }

    write_thresholds_section(&mut pdf, summary);
    write_steps_section(&mut pdf, summary);
    write_checks_section(&mut pdf, summary);

    pdf.heading(&t!("report.section.details"), 24.0);
    pdf.key_value(&t!("report.details.run"), &run.id);
    pdf.key_value(&t!("report.details.profile"), &run.profile.label());
    pdf.key_value(&t!("report.meta.started"), &fmt_date(run.started_at));
    pdf.key_value(&t!("report.meta.status"), &run.status.label());
    pdf.finish()
}

/// Écrit JSON/HTML/PDF/series dans `reports/<run.name>/`, puis renvoie les
/// chemins relatifs (à stocker dans `Run.reports`).
pub fn write_reports(
    workspace: &Workspace,
    run: &Run,
    summary: &RunSummary,
    series: &RunSeries,
) -> Result<Vec<String>, std::io::Error> {
    let dir = workspace.run_reports_dir(&run.name);
    std::fs::create_dir_all(&dir)?;

    let summary_json = "summary.json";
    let summary_html = "summary.html";
    let summary_pdf = "summary.pdf";
    let executive_pdf = "executive.pdf";
    let series_json = "series.json";

    std::fs::write(
        dir.join(summary_json),
        serde_json::to_string_pretty(summary).unwrap_or_else(|_| "{}".to_string()),
    )?;
    std::fs::write(dir.join(summary_html), render_html(run, summary, series))?;
    std::fs::write(
        dir.join(summary_pdf),
        render_summary_pdf(run, summary, series),
    )?;
    std::fs::write(dir.join(executive_pdf), render_executive_pdf(run, summary))?;
    std::fs::write(
        dir.join(series_json),
        serde_json::to_string_pretty(series).unwrap_or_else(|_| "{}".to_string()),
    )?;

    Ok(vec![
        format!("{}/{}", run.name, summary_json),
        format!("{}/{}", run.name, summary_html),
        format!("{}/{}", run.name, summary_pdf),
        format!("{}/{}", run.name, executive_pdf),
        format!("{}/{}", run.name, series_json),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        CheckSummary, Profile, Run, RunOptions, RunSeries, RunStatus, RunSummary, SeriesPoint,
        StepSummary, ThresholdResult,
    };

    fn sample_run() -> (Run, RunSummary) {
        let summary = RunSummary {
            requests: Some(120),
            rps: Some(40.0),
            iterations: Some(40),
            iterations_rate: Some(13.3),
            avg: Some(85.0),
            med: Some(70.0),
            p95: Some(140.0),
            p99: Some(180.0),
            max: Some(220.0),
            error_rate: Some(0.01),
            checks_rate: Some(0.99),
            vus_max: Some(5.0),
            dropped: Some(0),
            thresholds: vec![ThresholdResult {
                metric: "http_req_duration".into(),
                expr: "p(95)<500".into(),
                ok: true,
                ..Default::default()
            }],
            checks: vec![CheckSummary {
                name: "status 200".into(),
                passes: 119,
                fails: 1,
                ..Default::default()
            }],
            steps: vec![StepSummary {
                name: "ping".into(),
                count: Some(120),
                rate: Some(40.0),
                avg: Some(85.0),
                p95: Some(140.0),
                p99: Some(180.0),
                max: Some(220.0),
                error_rate: Some(0.01),
                ..Default::default()
            }],
            ..Default::default()
        };
        let run = Run {
            id: "run-1".into(),
            name: "demo-smoke-20260101-120000".into(),
            test_id: "t1".into(),
            test_name: "Demo".into(),
            profile: Profile::Smoke,
            options: RunOptions {
                vus: Some(5),
                duration: Some("30s".into()),
                ..Default::default()
            },
            status: RunStatus::Passed,
            started_at: 1_704_067_200_000,
            ended_at: Some(1_704_067_230_000),
            summary: Some(summary.clone()),
            ..Default::default()
        };
        (run, summary)
    }

    fn sample_series() -> RunSeries {
        RunSeries {
            points: (0..30)
                .map(|t| SeriesPoint {
                    t,
                    vus: 5.0,
                    rps: 40.0 + (t % 5) as f64,
                    p95: 120.0 + (t * 3) as f64,
                    error_rate: if t > 20 { 0.02 } else { 0.0 },
                    checks_rate: 1.0,
                    requests: t * 40,
                    iterations: t * 13,
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn pdfs_start_with_magic_header() {
        let (run, summary) = sample_run();
        let series = sample_series();
        let executive = render_executive_pdf(&run, &summary);
        let full = render_summary_pdf(&run, &summary, &series);
        assert!(executive.starts_with(b"%PDF"));
        assert!(full.starts_with(b"%PDF"));
        assert!(full.len() > executive.len());
    }

    #[test]
    fn write_reports_uses_per_run_folder() {
        let dir = std::env::temp_dir().join(format!("lt-reports-{}", crate::storage::new_id()));
        let workspace = Workspace::open(&dir).unwrap();
        let (run, summary) = sample_run();
        let files = write_reports(&workspace, &run, &summary, &sample_series()).unwrap();
        assert!(files.iter().all(|f| f.starts_with(&format!("{}/", run.name))));
        let run_dir = workspace.run_reports_dir(&run.name);
        assert!(run_dir.join("summary.html").is_file());
        assert!(run_dir.join("summary.pdf").is_file());
        assert!(run_dir.join("executive.pdf").is_file());
        assert!(run_dir.join("summary.json").is_file());
        assert!(run_dir.join("series.json").is_file());
        assert_eq!(workspace.report_files(&run.name), files);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pdf_without_series_and_many_rows_paginates() {
        let (run, mut summary) = sample_run();
        let step = summary.steps[0].clone();
        summary.steps = (0..120)
            .map(|i| StepSummary {
                name: format!("GET /api/v1/some/very/long/path/that/needs/wrapping/{i}"),
                ..step.clone()
            })
            .collect();
        let short = render_summary_pdf(&run, &sample_run().1, &RunSeries::default());
        let long = render_summary_pdf(&run, &summary, &RunSeries::default());
        assert!(short.starts_with(b"%PDF"));
        assert!(long.len() > short.len());
    }

    #[test]
    fn html_uses_active_language() {
        let (run, summary) = sample_run();
        let html = render_html(&run, &summary, &sample_series());
        assert!(html.contains("<html lang=\""));
        assert!(html.contains("<svg"));
    }

    #[test]
    fn scale_points_handles_flat_series() {
        let scaled = scale_points(&[(0.0, 3.0), (10.0, 3.0)], 100.0, 50.0, 5.0, 5.0).unwrap();
        assert!(scaled.max_v > scaled.min_v);
        assert_eq!(scaled.coords.len(), 2);
        assert!(scale_points(&[], 100.0, 50.0, 5.0, 5.0).is_none());
    }

    #[test]
    fn pdfs_render_in_both_languages() {
        let (run, summary) = sample_run();
        let series = sample_series();
        let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../reports/i18n-samples");
        let _ = std::fs::create_dir_all(&out);

        crate::i18n::set_language("en");
        let en_exec = render_executive_pdf(&run, &summary);
        let en_full = render_summary_pdf(&run, &summary, &series);
        std::fs::write(out.join("sample-en-executive.pdf"), &en_exec).unwrap();
        std::fs::write(out.join("sample-en-summary.pdf"), &en_full).unwrap();

        crate::i18n::set_language("fr");
        let fr_exec = render_executive_pdf(&run, &summary);
        let fr_full = render_summary_pdf(&run, &summary, &series);
        std::fs::write(out.join("sample-fr-executive.pdf"), &fr_exec).unwrap();
        std::fs::write(out.join("sample-fr-summary.pdf"), &fr_full).unwrap();

        assert!(en_exec.starts_with(b"%PDF"));
        assert!(fr_exec.starts_with(b"%PDF"));
        assert_ne!(en_full, fr_full);
        crate::i18n::set_language("en");
    }
}
