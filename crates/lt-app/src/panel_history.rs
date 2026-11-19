//! Onglet « Historique » : liste des exécutions et détail d'un rapport.

use crate::app::{format_timestamp, status_color, AppView};
use crate::ui;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Context, IntoElement, ParentElement, Styled, Window, div, px,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::Input;
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use gpui_component::resizable::{h_resizable, resizable_panel};
use gpui_component::{ActiveTheme, Sizable};
use lt_core::models::{Run, RunStatus};
use rust_i18n::t;
use std::path::Path;

impl AppView {
    pub(crate) fn render_history(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        h_resizable("history-layout")
            .child(
                resizable_panel()
                    .size(px(ui::SIDEBAR_WIDTH))
                    .size_range(px(ui::SIDEBAR_MIN_WIDTH)..px(ui::SIDEBAR_MAX_WIDTH))
                    .child(self.render_history_list(cx)),
            )
            .child(resizable_panel().child(self.render_history_detail(cx)))
            .into_any_element()
    }

    fn filtered_runs(&self, cx: &App) -> Vec<&Run> {
        let query = self.history_query.read(cx).value().trim().to_lowercase();
        self.runs
            .iter()
            .filter(|run| {
                if let Some(status) = self.history_status
                    && run.status != status
                {
                    return false;
                }
                if query.is_empty() {
                    return true;
                }
                run.test_name.to_lowercase().contains(&query)
                    || run.profile.label().to_lowercase().contains(&query)
                    || run.status.label().to_lowercase().contains(&query)
            })
            .collect()
    }

    fn render_history_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let filtered = self.filtered_runs(cx);
        let mut rows = div().flex().flex_col().gap_px().p_2();
        if self.runs.is_empty() {
            rows = rows.child(
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(ui::muted(t!("history.empty_list").to_string(), cx))
                    .child(ui::muted(t!("history.empty_list_hint").to_string(), cx)),
            );
        } else if filtered.is_empty() {
            rows = rows.child(
                div()
                    .p_3()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("history.no_match").to_string()),
            );
        }
        for run in filtered {
            let selected = self.selected_run.as_deref() == Some(run.id.as_str());
            let id = gpui::SharedString::from(format!("history-{}", run.id));
            let run_id = run.id.clone();
            rows = rows.child(
                div()
                    .id(id)
                    .cursor_pointer()
                    .rounded(cx.theme().radius)
                    .hover(|style| style.bg(cx.theme().list_hover))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.selected_run = Some(run_id.clone());
                        cx.notify();
                    }))
                    .child(crate::panel_run::run_row(run, selected, cx)),
            );
        }

        let status_label = match self.history_status {
            None => t!("common.all").to_string(),
            Some(status) => status.label(),
        };
        let weak = cx.entity().downgrade();
        let current = self.history_status;
        let status_menu = Button::new("history-status-filter")
            .small()
            .outline()
            .dropdown_caret(true)
            .label(status_label)
            .dropdown_menu(move |menu, _window, _cx| {
                let mut menu = menu.item({
                    let weak = weak.clone();
                    PopupMenuItem::new(t!("common.all_statuses").to_string())
                        .checked(current.is_none())
                        .on_click(move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.history_status = None;
                                cx.notify();
                            });
                        })
                });
                for status in [
                    RunStatus::Passed,
                    RunStatus::Failed,
                    RunStatus::Error,
                    RunStatus::Running,
                ] {
                    let weak = weak.clone();
                    menu = menu.item(
                        PopupMenuItem::new(status.label())
                            .checked(current == Some(status))
                            .on_click(move |_, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.history_status = Some(status);
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            });

        div()
            .flex()
            .flex_col()
            .size_full()
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(self.render_sidebar_nav(cx))
            .child(
                ui::sidebar_header_row(cx)
                    .child(ui::section_label(t!("history.header").to_string(), cx))
                    .child(ui::mono_muted(format!("{}", self.runs.len()), cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_none()
                    .items_center()
                    .gap_2()
                    .px_4()
                    .py_2()
                    .child(Input::new(&self.history_query).small().flex_1())
                    .child(status_menu),
            )
            .child(
                div()
                    .id("history-list")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .child(rows),
            )
            .into_any_element()
    }

    fn render_history_detail(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(run) = self
            .selected_run
            .as_ref()
            .and_then(|id| self.runs.iter().find(|run| &run.id == id))
            .cloned()
        else {
            return div()
                .flex()
                .flex_1()
                .size_full()
                .items_center()
                .justify_center()
                .child(ui::empty_state_rich(
                    Some("inbox"),
                    t!("history.empty.title").to_string(),
                    Some(t!("history.empty.subtitle").to_string()),
                    None,
                    cx,
                ))
                .into_any_element();
        };

        let options = &run.options;
        let mut meta = div().flex().flex_row().flex_wrap().gap_x_6().gap_y_2();
        meta = meta.child(meta_item(
            &t!("history.meta.profile"),
            run.profile.label(),
            cx,
        ));
        meta = meta.child(meta_item(
            &t!("history.meta.started"),
            format_timestamp(run.started_at),
            cx,
        ));
        meta = meta.child(meta_item(
            &t!("history.meta.duration"),
            run.duration_ms()
                .map(|ms| lt_core::profiles::format_duration(ms as u64))
                .unwrap_or_else(|| "—".to_string()),
            cx,
        ));
        if let Some(code) = run.exit_code {
            meta = meta.child(meta_item(
                &t!("history.meta.exit_code"),
                code.to_string(),
                cx,
            ));
        }
        if let Some(vus) = options.vus {
            meta = meta.child(meta_item(&t!("history.meta.vus"), vus.to_string(), cx));
        }
        if let Some(tps) = options.tps {
            meta = meta.child(meta_item(&t!("history.meta.tps"), tps.to_string(), cx));
        }
        if let Some(duration) = &options.duration {
            meta = meta.child(meta_item(
                &t!("history.meta.requested_duration"),
                duration.clone(),
                cx,
            ));
        }
        if let Some(iterations) = options.iterations {
            meta = meta.child(meta_item(
                &t!("history.meta.max_iterations"),
                iterations.to_string(),
                cx,
            ));
        }

        let delete = Button::new("history-delete")
            .small()
            .ghost()
            .icon(ui::icon("delete"))
            .label(t!("action.delete").to_string())
            .text_color(cx.theme().danger)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.request_delete_run(window, cx)
            }));

        let mut content = div()
            .id("history-detail")
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.))
            .min_h(px(0.))
            .overflow_y_scroll()
            .p_4()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_base()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .truncate()
                            .child(run.test_name.clone()),
                    )
                    .child(ui::chip(run.status.label(), status_color(run.status, cx)))
                    .child(delete),
            )
            .child(meta);

        if let Some(error) = &run.error {
            content = content.child(
                div()
                    .p_3()
                    .rounded(cx.theme().radius)
                    .bg(cx.theme().danger.opacity(0.1))
                    .border_1()
                    .border_color(cx.theme().danger.opacity(0.4))
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }

        content = content.child(summary_card(
            &run,
            &self.workspace.reports_dir(),
            cx,
        ));

        content.into_any_element()
    }
}

/// Carte de synthèse d'une exécution : indicateurs, seuils, étapes et rapports.
pub(crate) fn summary_card(run: &Run, reports_dir: &Path, cx: &App) -> AnyElement {
    let Some(summary) = &run.summary else {
        return ui::surface(cx)
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .child(ui::section_label(t!("report.reports").to_string(), cx))
            .child(report_buttons(run, reports_dir, cx))
            .into_any_element();
    };

    let danger = cx.theme().danger;
    let success = cx.theme().success;

    let mut kpis = div().flex().flex_row().flex_wrap().gap_2();
    kpis = kpis.child(ui::kpi(
        t!("metric.requests").to_string(),
        format_num(summary.requests.map(|value| value as f64)),
        None,
        cx,
    ));
    kpis = kpis.child(ui::kpi(
        t!("metric.throughput").to_string(),
        format!("{} req/s", format_num(summary.rps)),
        None,
        cx,
    ));
    kpis = kpis.child(ui::kpi(t!("metric.average").to_string(), format_ms(summary.avg), None, cx));
    kpis = kpis.child(ui::kpi(t!("metric.median").to_string(), format_ms(summary.med), None, cx));
    kpis = kpis.child(ui::kpi("p90", format_ms(summary.p90), None, cx));
    kpis = kpis.child(ui::kpi("p95", format_ms(summary.p95), None, cx));
    kpis = kpis.child(ui::kpi("p99", format_ms(summary.p99), None, cx));
    kpis = kpis.child(ui::kpi(t!("metric.max").to_string(), format_ms(summary.max), None, cx));
    kpis = kpis.child(ui::kpi(
        t!("metric.errors").to_string(),
        format_pct(summary.error_rate),
        Some(danger),
        cx,
    ));
    kpis = kpis.child(ui::kpi(
        t!("metric.checks").to_string(),
        format_pct(summary.checks_rate),
        Some(success),
        cx,
    ));
    kpis = kpis.child(ui::kpi(
        t!("metric.transactions").to_string(),
        format_num(summary.iterations.map(|value| value as f64)),
        None,
        cx,
    ));
    kpis = kpis.child(ui::kpi(
        t!("metric.vus_max").to_string(),
        format_num(summary.vus_max),
        None,
        cx,
    ));
    if let Some(dropped) = summary.dropped
        && dropped > 0
    {
        kpis = kpis.child(ui::kpi(
            t!("metric.dropped").to_string(),
            dropped.to_string(),
            Some(danger),
            cx,
        ));
    }

    let mut body = ui::surface(cx)
        .flex()
        .flex_col()
        .gap_3()
        .p_3()
        .child(ui::section_label(t!("report.summary").to_string(), cx))
        .child(kpis);

    if !summary.thresholds.is_empty() {
        let mut thresholds = div().flex().flex_col().gap_2();
        for threshold in &summary.thresholds {
            thresholds = thresholds.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .child(
                                div()
                                    .text_color(if threshold.ok {
                                        success
                                    } else {
                                        danger
                                    })
                                    .child(if threshold.ok { "✓" } else { "✗" }),
                            )
                            .child(
                                div()
                                    .text_color(cx.theme().foreground)
                                    .child(format!("{} {}", threshold.metric, threshold.expr)),
                            ),
                    )
                    .child(ui::progress_bar(
                        if threshold.ok { 1.0 } else { 0.35 },
                        threshold.ok,
                        cx,
                    )),
            );
        }
        body = body.child(ui::divider(cx)).child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(ui::section_label(t!("report.thresholds").to_string(), cx))
                .child(thresholds),
        );
    }

    if !summary.steps.is_empty() {
        let widths = [px(200.), px(72.), px(80.), px(80.), px(80.), px(72.)];
        let mut table = div()
            .flex()
            .flex_col()
            .gap_0p5()
            .p_2()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background);
        table = table.child(table_row(
            [
                t!("report.column.step").to_string(),
                t!("report.column.requests").to_string(),
                t!("report.column.average").to_string(),
                "p95".to_string(),
                "p99".to_string(),
                t!("report.column.errors").to_string(),
            ],
            widths,
            cx,
            true,
        ));
        for (index, step) in summary.steps.iter().enumerate() {
            let err = step.error_rate.unwrap_or(0.0);
            let err_color = if err <= 0.01 {
                Some(success)
            } else if err <= 0.05 {
                Some(cx.theme().warning)
            } else {
                Some(danger)
            };
            let row = table_row_colored(
                [
                    step.name.clone(),
                    format_num(step.count.map(|value| value as f64)),
                    format_ms(step.avg),
                    format_ms(step.p95),
                    format_ms(step.p99),
                    format_pct(step.error_rate),
                ],
                widths,
                err_color,
                cx,
                false,
            );
            table = table.child(
                div()
                    .when(index % 2 == 1, |el| el.bg(cx.theme().background.opacity(0.4)))
                    .child(row),
            );
        }
        body = body.child(ui::divider(cx)).child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(ui::section_label(t!("report.steps").to_string(), cx))
                .child(table),
        );
    }

    if !summary.checks.is_empty() {
        let mut checks = div().flex().flex_col().gap_1();
        for check in &summary.checks {
            let total = check.passes + check.fails;
            let rate = if total == 0 {
                0.0
            } else {
                check.passes as f64 / total as f64
            };
            checks = checks.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .child(
                                div()
                                    .text_color(if rate >= 1.0 { success } else { danger })
                                    .child(format!("{:.0} %", rate * 100.0)),
                            )
                            .child(
                                div()
                                    .text_color(cx.theme().foreground)
                                    .child(format!("{} ({} / {})", check.name, check.passes, total)),
                            ),
                    )
                    .child(ui::progress_bar(rate, rate >= 0.99, cx)),
            );
        }
        body = body.child(ui::divider(cx)).child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(ui::section_label(t!("report.checks").to_string(), cx))
                .child(checks),
        );
    }

    body.child(ui::divider(cx))
        .child(ui::section_label(t!("report.reports").to_string(), cx))
        .child(report_buttons(run, reports_dir, cx))
        .into_any_element()
}

fn report_buttons(run: &Run, reports_dir: &Path, cx: &App) -> AnyElement {
    let html = run
        .reports
        .iter()
        .find(|name| name.ends_with("summary.html") || name.ends_with(".html"))
        .or_else(|| run.reports.first());
    let html_path = html.map(|name| reports_dir.join(name));
    let pdf = run
        .reports
        .iter()
        .find(|name| name.ends_with("summary.pdf") || name.ends_with("-summary.pdf"))
        .or_else(|| {
            run.reports.iter().find(|name| {
                name.ends_with(".pdf")
                    && !name.ends_with("executive.pdf")
                    && !name.ends_with("-executive.pdf")
            })
        });
    let pdf_path = pdf.map(|name| reports_dir.join(name));
    let executive = run.reports.iter().find(|name| {
        name.ends_with("executive.pdf") || name.ends_with("-executive.pdf")
    });
    let executive_path = executive.map(|name| reports_dir.join(name));
    // Ouvre le dossier de l'exécution s'il existe, sinon reports/.
    let run_folder = reports_dir.join(&run.name);
    let folder = if run_folder.is_dir() {
        run_folder
    } else {
        reports_dir.to_path_buf()
    };

    let mut row = div().flex().flex_row().flex_wrap().items_center().gap_2();
    let mut has_report = false;
    if let Some(path) = html_path {
        has_report = true;
        let label = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| t!("report.default_name").to_string());
        row = row.child(
            Button::new(gpui::SharedString::from(format!("open-html-{}", run.id)))
                .small()
                .primary()
                .label(t!("action.open_html").to_string())
                .on_click(move |_: &ClickEvent, _window, _cx| {
                    ui::open_path(&path);
                }),
        );
        row = row.child(ui::muted(label, cx));
    }
    if let Some(path) = pdf_path {
        has_report = true;
        row = row.child(
            Button::new(gpui::SharedString::from(format!("open-pdf-{}", run.id)))
                .small()
                .label(t!("action.open_pdf").to_string())
                .on_click(move |_: &ClickEvent, _window, _cx| {
                    ui::open_path(&path);
                }),
        );
    }
    if let Some(path) = executive_path {
        has_report = true;
        row = row.child(
            Button::new(gpui::SharedString::from(format!(
                "open-executive-{}",
                run.id
            )))
            .small()
            .label(t!("action.open_executive").to_string())
            .on_click(move |_: &ClickEvent, _window, _cx| {
                ui::open_path(&path);
            }),
        );
    }
    if !has_report {
        row = row.child(ui::muted(t!("report.no_report").to_string(), cx));
    }
    row = row.child(
        Button::new(gpui::SharedString::from(format!(
            "open-folder-{}",
            run.id
        )))
        .small()
        .ghost()
        .label(t!("action.open_reports_folder").to_string())
        .on_click(move |_: &ClickEvent, _window, _cx| {
            ui::open_path(&folder);
        }),
    );
    row.into_any_element()
}

fn meta_item(label: &str, value: String, cx: &App) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(gpui::SharedString::from(label.to_string())),
        )
        .child(
            div()
                .font_family(cx.theme().mono_font_family.clone())
                .text_xs()
                .text_color(cx.theme().foreground)
                .child(value),
        )
        .into_any_element()
}

fn table_row(
    cells: [String; 6],
    widths: [gpui::Pixels; 6],
    cx: &App,
    header: bool,
) -> AnyElement {
    table_row_colored(cells, widths, None, cx, header)
}

fn table_row_colored(
    cells: [String; 6],
    widths: [gpui::Pixels; 6],
    last_color: Option<gpui::Hsla>,
    cx: &App,
    header: bool,
) -> AnyElement {
    let mut row = div().flex().flex_row().items_center().gap_2().py_1();
    for (index, cell) in cells.iter().enumerate() {
        let is_last = index + 1 == cells.len();
        row = row.child(
            div()
                .w(widths[index])
                .truncate()
                .text_xs()
                .when(!header && index > 0, |el| {
                    el.font_family(cx.theme().mono_font_family.clone())
                })
                .when(header, |el| {
                    el.font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(cx.theme().muted_foreground)
                })
                .when(!header, |el| {
                    el.text_color(if is_last {
                        last_color.unwrap_or(cx.theme().foreground)
                    } else {
                        cx.theme().foreground
                    })
                })
                .child(gpui::SharedString::from(cell.clone())),
        );
    }
    row.into_any_element()
}

fn format_ms(value: Option<f64>) -> String {
    match value {
        None => "—".to_string(),
        Some(value) if value >= 1000.0 => format!("{:.2} s", value / 1000.0),
        Some(value) => format!("{:.0} ms", value),
    }
}

fn format_pct(value: Option<f64>) -> String {
    match value {
        None => "—".to_string(),
        Some(value) => format!("{:.2} %", value * 100.0),
    }
}

fn format_num(value: Option<f64>) -> String {
    match value {
        None => "—".to_string(),
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
