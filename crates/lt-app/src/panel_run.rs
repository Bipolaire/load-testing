//! Onglet « Lancer » : choix du scénario, profil de charge, indicateurs temps
//! réel, graphiques et journal d'exécution.

use crate::app::{format_timestamp, status_color, AppView};
use crate::charts::{Bars, ProfilePreview, ProfileShape, Sparkline};
use crate::ui;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Context, Hsla, IntoElement, ParentElement, SharedString, Styled,
    Window, div, px, rgb,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::NumberInput;
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use gpui_component::resizable::{h_resizable, resizable_panel};
use gpui_component::spinner::Spinner;
use gpui_component::{ActiveTheme, Disableable, Side, Sizable, Size};
use lt_core::models::{Profile, Run, SeriesPoint};
use lt_core::profiles::{self, format_duration};
use rust_i18n::t;
use tokio_util::sync::CancellationToken;

/// État d'une exécution en cours ou terminée.
#[derive(Default)]
pub(crate) enum RunState {
    #[default]
    Idle,
    Starting,
    Running,
    Finished(Box<Run>),
    Failed(String),
}

impl RunState {
    pub(crate) fn is_running(&self) -> bool {
        matches!(self, RunState::Starting | RunState::Running)
    }
}

/// Tient l'état de l'exécution courante : journal, série temporelle et jeton
/// d'annulation.
#[derive(Default)]
pub(crate) struct RunController {
    pub(crate) state: RunState,
    pub(crate) logs: Vec<String>,
    pub(crate) series: Vec<SeriesPoint>,
    pub(crate) live: Option<Run>,
    pub(crate) log_scroll: gpui::ScrollHandle,
    /// Journal replié (seul l'en-tête reste visible).
    pub(crate) logs_collapsed: bool,
    pub(crate) cancel: Option<CancellationToken>,
    pub(crate) current: Option<Run>,
}

impl RunController {
    pub(crate) fn begin(&mut self, cancel: CancellationToken) {
        self.state = RunState::Starting;
        self.logs.clear();
        self.series.clear();
        self.live = None;
        self.current = None;
        self.cancel = Some(cancel);
    }

    pub(crate) fn is_running(&self) -> bool {
        self.state.is_running()
    }

    pub(crate) fn stop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
    }

    pub(crate) fn started(&mut self, run: Run) {
        self.state = RunState::Running;
        self.live = Some(run);
    }

    pub(crate) fn log(&mut self, message: String) {
        let line = format!(
            "{}  {}",
            chrono::Local::now().format("%H:%M:%S"),
            message
        );
        self.logs.push(line);
        if self.logs.len() > 4000 {
            self.logs.drain(0..1000);
        }
        self.log_scroll.scroll_to_bottom();
    }

    pub(crate) fn sample(&mut self, point: SeriesPoint) {
        self.series.push(point);
        if self.series.len() > 3600 {
            self.series.drain(0..600);
        }
    }

    pub(crate) fn finished(&mut self, run: Run) {
        self.state = RunState::Finished(Box::new(run.clone()));
        self.current = Some(run);
        self.cancel = None;
    }

    pub(crate) fn fatal(&mut self, error: String) {
        self.state = RunState::Failed(error);
        self.cancel = None;
    }

    pub(crate) fn last_sample(&self) -> Option<&SeriesPoint> {
        self.series.last()
    }

    pub(crate) fn elapsed_ms(&self) -> i64 {
        let started = self
            .live
            .as_ref()
            .map(|run| run.started_at)
            .or_else(|| self.current.as_ref().map(|run| run.started_at))
            .unwrap_or(0);
        if started == 0 {
            return 0;
        }
        (lt_core::template::now_ms() as i64 - started).max(0)
    }
}

// ---------------------------------------------------------------------------
// Rendu
// ---------------------------------------------------------------------------

impl AppView {
    pub(crate) fn render_run(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(test) = self.run_test().cloned() else {
            let cta = Button::new("goto-scenarios")
                .primary()
                .label(t!("action.create_scenario").to_string())
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.set_tab(crate::app::TabKind::Scenarios, window, cx);
                }));
            return h_resizable("run-layout-empty")
                .child(
                    resizable_panel()
                        .size(px(ui::SIDEBAR_WIDTH))
                        .size_range(px(ui::SIDEBAR_MIN_WIDTH)..px(ui::SIDEBAR_MAX_WIDTH))
                        .child(self.render_run_sidebar(cx)),
                )
                .child(
                    resizable_panel().child(
                        div()
                            .flex()
                            .size_full()
                            .items_center()
                            .justify_center()
                            .child(ui::empty_state_rich(
                                Some("play"),
                                t!("run.empty.title").to_string(),
                                Some(t!("run.empty.subtitle").to_string()),
                                Some(cta),
                                cx,
                            )),
                    ),
                )
                .into_any_element();
        };

        h_resizable("run-layout")
            .child(
                resizable_panel()
                    .size(px(ui::SIDEBAR_WIDTH))
                    .size_range(px(ui::SIDEBAR_MIN_WIDTH)..px(ui::SIDEBAR_MAX_WIDTH))
                    .child(self.render_run_sidebar(cx)),
            )
            .child(
                resizable_panel().child(
                    div()
                        .flex()
                        .flex_col()
                        .size_full()
                        .min_w(px(0.))
                        .min_h(px(0.))
                        .child(
                            div()
                                .id("run-main")
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_h(px(0.))
                                .overflow_y_scroll()
                                .p_4()
                                .gap_4()
                                .child(self.render_run_heading(&test, cx))
                                .child(self.render_run_config(cx))
                                .child(self.render_run_kpis(cx))
                                .child(self.render_run_charts(cx))
                                .child(self.render_run_logs(cx))
                                .children(
                                    self.run
                                        .current
                                        .clone()
                                        .filter(|run| run.summary.is_some())
                                        .map(|run| {
                                            crate::panel_history::summary_card(
                                                &run,
                                                &self.workspace.reports_dir(),
                                                cx,
                                            )
                                        }),
                                ),
                        )
                        .child(self.render_run_launch_bar(&test, cx)),
                ),
            )
            .into_any_element()
    }

    /// Barre de commande en bas de la colonne : résumé du profil + Lancer / Arrêter.
    fn render_run_launch_bar(
        &self,
        test: &lt_core::Test,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let running = self.run.is_running();
        let starting = matches!(self.run.state, RunState::Starting);

        // Les champs peuvent avoir été modifiés dans l'éditeur sans enregistrement :
        // on se base sur le brouillon quand le formulaire correspond au scénario à lancer.
        let (steps_empty, base_empty, dangerous) = if let Some(form) = self
            .scenario
            .as_ref()
            .filter(|f| Some(f.id.as_str()) == self.run_test.as_deref())
        {
            let draft = form.draft(cx);
            (
                draft.steps.is_empty(),
                draft.base_url.trim().is_empty(),
                draft.dangerous,
            )
        } else {
            (
                test.steps.is_empty(),
                test.base_url.trim().is_empty(),
                test.dangerous,
            )
        };

        let action = if running {
            Button::new("run-stop")
                .danger()
                .large()
                .icon(ui::icon("square"))
                .label(t!("action.stop").to_string())
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.stop_run(window, cx)
                }))
        } else {
            Button::new("run-start")
                .primary()
                .large()
                .icon(ui::icon("play"))
                .label(if dangerous {
                    t!("action.start_dangerous").to_string()
                } else {
                    t!("action.start").to_string()
                })
                .disabled(steps_empty || base_empty)
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.start_run(window, cx)
                }))
        };

        let (status, status_color) = match &self.run.state {
            RunState::Idle => (t!("status.ready").to_string(), cx.theme().muted_foreground),
            RunState::Starting => (t!("status.starting").to_string(), cx.theme().info),
            RunState::Running => (
                t!(
                    "status.running",
                    duration =
                        lt_core::profiles::format_duration(self.run.elapsed_ms().max(0) as u64)
                )
                .to_string(),
                cx.theme().info,
            ),
            RunState::Finished(run) => (
                format!(
                    "{} · {}",
                    run.status.label(),
                    run.duration_ms()
                        .map(|ms| lt_core::profiles::format_duration(ms as u64))
                        .unwrap_or_else(|| "—".to_string())
                ),
                status_color(run.status, cx),
            ),
            RunState::Failed(error) => (
                t!("status.failed", error = error).to_string(),
                cx.theme().danger,
            ),
        };

        // Résumé du profil en pastilles mono.
        let mut summary = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_1p5()
            .child(mono_chip(self.profile.label(), cx));
        if self.profile.uses_tps() {
            summary = summary.child(mono_chip(
                format!("{} TPS", self.tps.read(cx).value().trim()),
                cx,
            ));
        } else {
            summary = summary.child(mono_chip(
                format!("{} VUs", self.vus.read(cx).value().trim()),
                cx,
            ));
        }
        if self.profile.uses_duration() {
            let duration = self.duration.read(cx).value().trim().to_string();
            if !duration.is_empty() {
                summary = summary.child(mono_chip(duration, cx));
            }
        }

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .px_4()
            .py_2p5()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(summary)
            .when(dangerous, |el| {
                el.child(ui::chip(t!("chip.dangerous").to_string(), cx.theme().danger))
            })
            .child(div().flex_1())
            .when(starting, |el| el.child(Spinner::new().small()))
            .child(ui::status_pill(status, status_color))
            .child(action)
            .into_any_element()
    }

    fn render_run_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div().flex().flex_col().gap_px().p_2();
        let mut last_folder: Option<String> = None;
        for test in &self.tests {
            if last_folder.as_deref() != Some(test.folder.as_str()) {
                let label = if test.folder.is_empty() {
                    t!("common.no_folder").to_uppercase()
                } else {
                    lt_core::folders::folder_label(&test.folder).to_uppercase()
                };
                list = list.child(
                    div()
                        .px_2()
                        .pt_2()
                        .pb_1()
                        .child(ui::section_label(label, cx)),
                );
                last_folder = Some(test.folder.clone());
            }
            let selected = self.run_test.as_deref() == Some(test.id.as_str());
            let id = SharedString::from(format!("run-pick-{}", test.id));
            let test_id = test.id.clone();
            list = list.child(
                ui::list_row(id, selected, cx)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.run_test = Some(test_id.clone());
                        this.save_settings();
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .text_sm()
                            .font_weight(if selected {
                                gpui::FontWeight::MEDIUM
                            } else {
                                gpui::FontWeight::NORMAL
                            })
                            .text_color(if selected {
                                cx.theme().foreground
                            } else {
                                cx.theme().muted_foreground
                            })
                            .child(test.name.clone()),
                    )
                    .when(test.dangerous, |el| {
                        el.child(
                            div()
                                .size(px(6.))
                                .flex_none()
                                .rounded_full()
                                .bg(cx.theme().danger),
                        )
                    })
                    .child(ui::mono_muted(format!("{}", test.steps.len()), cx)),
            );
        }

        div()
            .id("run-sidebar")
            .flex()
            .flex_col()
            .size_full()
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(self.render_sidebar_nav(cx))
            .child(ui::sidebar_header(t!("run.scenarios_header").to_string(), None, cx))
            .child(
                div()
                    .id("run-sidebar-scroll")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .child(list),
            )
            .into_any_element()
    }

    fn render_run_heading(&self, test: &lt_core::Test, cx: &mut Context<Self>) -> AnyElement {
        let folder = if test.folder.is_empty() {
            t!("common.no_folder").to_string()
        } else {
            lt_core::folders::folder_label(&test.folder)
        };
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
                    .child(
                        div()
                            .text_base()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(test.name.clone()),
                    )
                    .child(mono_chip(folder, cx))
                    .when(test.dangerous, |el| {
                        el.child(ui::chip(t!("chip.dangerous").to_string(), cx.theme().danger))
                    }),
            )
            .child(
                div()
                    .truncate()
                    .child(ui::mono_muted(test.base_url.clone(), cx)),
            )
            .into_any_element()
    }

    fn render_run_config(&self, cx: &mut Context<Self>) -> AnyElement {
        let rate_profile = self.profile.uses_tps();
        let options_state = self.run_options(cx);
        let plan = profiles::build_plan(self.profile, &options_state).ok();
        let preview_unit = plan
            .as_ref()
            .map(|plan| plan.preview_unit())
            .unwrap_or("VUs");
        let preview_points = plan
            .as_ref()
            .map(|plan| plan.preview_points(64))
            .unwrap_or_default();
        let preview_duration = plan
            .as_ref()
            .map(|plan| format_duration(plan.total_duration_ms()))
            .unwrap_or_else(|| "—".into());
        let preview_peak = plan
            .as_ref()
            .map(|plan| {
                if plan.preview_unit() == "TPS" {
                    let peak = preview_points
                        .iter()
                        .map(|(_, v)| *v)
                        .fold(0.0_f64, f64::max);
                    format!("{peak:.0} TPS")
                } else {
                    format!("{:.0} VUs", plan.peak_vus())
                }
            })
            .unwrap_or_else(|| "—".into());

        let vus_tooltip = self.profile.vus_tooltip();
        let mut options = div().flex().flex_row().flex_wrap().gap_3();
        if rate_profile {
            let tps_tooltip = Profile::tps_tooltip();
            options = options.child(number_field(
                &t!("run.field.tps"),
                Some(tps_tooltip.as_str()),
                &self.tps,
                cx,
            ));
            options = options.child(number_field(
                &t!("run.field.max_vus"),
                Some(vus_tooltip.as_str()),
                &self.vus,
                cx,
            ));
        } else {
            let vus_label = self.profile.vus_label();
            options = options.child(number_field(
                &vus_label,
                Some(vus_tooltip.as_str()),
                &self.vus,
                cx,
            ));
        }
        if self.profile.uses_duration() {
            let duration_label = self.profile.duration_label();
            let duration_tooltip = self.profile.duration_tooltip();
            options = options.child(ui::field_with_tooltip(
                &duration_label,
                Some(duration_tooltip.as_str()),
                &self.duration,
                cx,
            ));
        }
        if self.profile.uses_ramps() {
            let ramp_up_tooltip = Profile::ramp_up_tooltip();
            let ramp_down_tooltip = Profile::ramp_down_tooltip();
            options = options.child(ui::field_with_tooltip(
                &t!("run.field.ramp_up"),
                Some(ramp_up_tooltip.as_str()),
                &self.ramp_up,
                cx,
            ));
            options = options.child(ui::field_with_tooltip(
                &t!("run.field.ramp_down"),
                Some(ramp_down_tooltip.as_str()),
                &self.ramp_down,
                cx,
            ));
        }
        let iterations_tooltip = Profile::iterations_tooltip();
        options = options.child(ui::field_with_tooltip(
            &t!("run.field.max_iterations"),
            Some(iterations_tooltip.as_str()),
            &self.iterations,
            cx,
        ));

        let weak = cx.entity().downgrade();
        let current = self.profile;
        let accent = cx.theme().primary;
        let profile_menu = Button::new("profile-select")
            .small()
            .outline()
            .icon(ui::icon(self.profile.icon_name()))
            .label(self.profile.label())
            .tooltip(self.profile.tooltip())
            .dropdown_menu(move |menu, _window, _cx| {
                // Picto dans le contenu (pas `.icon()` xsmall du menu) pour rester
                // lisible. Check à droite.
                let mut menu = menu.check_side(Side::Right);
                for profile in Profile::ALL {
                    let weak = weak.clone();
                    menu = menu.item(
                        PopupMenuItem::element(move |_window, cx| {
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(
                                    ui::icon(profile.icon_name())
                                        .with_size(Size::Large)
                                        .text_color(cx.theme().primary),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_0p5()
                                        .child(profile.label())
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(profile.description()),
                                        ),
                                )
                        })
                        .checked(current == profile)
                        .on_click(move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.profile = profile;
                                this.save_settings();
                                cx.notify();
                            });
                        }),
                    );
                }
                menu
            });

        ui::surface(cx)
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(ui::section_label(t!("run.load_profile").to_string(), cx))
                    .child(profile_menu)
                    .child(
                        ProfileShape::new(self.profile.shape(), accent).size(72.0, 22.0),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.profile.description()),
                    ),
            )
            .child(ui::mono_muted(
                t!("run.defaults", defaults = self.profile.defaults()).to_string(),
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded(cx.theme().radius)
                    .bg(cx.theme().background)
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(ui::section_label(t!("run.plan_preview").to_string(), cx))
                            .child(ui::mono_muted(
                                format!("{preview_peak} · {preview_duration} · {preview_unit}"),
                                cx,
                            )),
                    )
                    .child(
                        ProfilePreview::new(preview_points, accent, cx.theme().border)
                            .into_any_element(),
                    ),
            )
            .child(options)
            .into_any_element()
    }

    fn render_run_kpis(&self, cx: &mut Context<Self>) -> AnyElement {
        let sample = self.run.last_sample().cloned();
        let summary = self.run.current.as_ref().and_then(|run| run.summary.clone());

        let (requests, rps, p95, p99, error_rate, checks_rate, vus, iterations) = match (
            sample.as_ref(),
            summary.as_ref(),
        ) {
            (_, Some(summary)) => (
                format_num(summary.requests.map(|value| value as f64)),
                format_num(summary.rps),
                format_ms(summary.p95),
                format_ms(summary.p99),
                format_pct(summary.error_rate),
                format_pct(summary.checks_rate),
                format_num(summary.vus_max),
                format_num(summary.iterations.map(|value| value as f64)),
            ),
            (Some(sample), None) => (
                sample.requests.to_string(),
                format!("{:.1}", sample.rps),
                format_ms(Some(sample.p95)),
                "—".to_string(),
                format_pct(Some(sample.error_rate)),
                format_pct(Some(sample.checks_rate)),
                format!("{:.0}", sample.vus),
                sample.iterations.to_string(),
            ),
            (None, None) => (
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
            ),
        };

        let danger = cx.theme().danger;
        let success = cx.theme().success;
        let primary = cx.theme().primary;

        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_2()
            .child(ui::stat(
                t!("metric.requests").to_string(),
                requests,
                None::<SharedString>,
                Some(primary),
                cx,
            ))
            .child(ui::stat(t!("metric.throughput").to_string(), rps, Some("req/s"), None, cx))
            .child(ui::stat("p95", p95, None::<SharedString>, None, cx))
            .child(ui::stat("p99", p99, None::<SharedString>, None, cx))
            .child(ui::stat(
                t!("metric.errors").to_string(),
                error_rate,
                None::<SharedString>,
                Some(danger),
                cx,
            ))
            .child(ui::stat(
                t!("metric.checks").to_string(),
                checks_rate,
                None::<SharedString>,
                Some(success),
                cx,
            ))
            .child(ui::stat(t!("metric.vus").to_string(), vus, None::<SharedString>, None, cx))
            .child(ui::stat(
                t!("metric.transactions").to_string(),
                iterations,
                None::<SharedString>,
                None,
                cx,
            ))
            .into_any_element()
    }

    fn render_run_charts(&self, cx: &mut Context<Self>) -> AnyElement {
        let points: Vec<&SeriesPoint> = self
            .run
            .series
            .iter()
            .rev()
            .take(180)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let info = cx.theme().info;
        let warning = cx.theme().warning;
        let danger = cx.theme().danger;
        let primary = cx.theme().chart_1;

        if points.is_empty() {
            let waiting = t!("chart.waiting").to_string();
            return div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_3()
                .child(chart_card(
                    t!("chart.throughput").to_string(),
                    "—".to_string(),
                    format!("req/s · {waiting}"),
                    empty_chart_placeholder(primary, cx),
                    primary,
                    cx,
                ))
                .child(chart_card(
                    t!("chart.latency_p95").to_string(),
                    "—".to_string(),
                    format!("ms · {waiting}"),
                    empty_chart_placeholder(warning, cx),
                    warning,
                    cx,
                ))
                .child(chart_card(
                    t!("chart.errors").to_string(),
                    "—".to_string(),
                    format!("% · {waiting}"),
                    empty_chart_placeholder(danger, cx),
                    danger,
                    cx,
                ))
                .child(chart_card(
                    t!("chart.vus").to_string(),
                    "—".to_string(),
                    waiting,
                    empty_chart_placeholder(info, cx),
                    info,
                    cx,
                ))
                .into_any_element();
        }
        let rps: Vec<f64> = points.iter().map(|point| point.rps).collect();
        let p95: Vec<f64> = points.iter().map(|point| point.p95).collect();
        let errors: Vec<f64> = points.iter().map(|point| point.error_rate * 100.0).collect();
        let vus: Vec<f64> = points.iter().map(|point| point.vus).collect();
        let last_rps = *rps.last().unwrap_or(&0.0);
        let last_p95 = *p95.last().unwrap_or(&0.0);
        let last_err = *errors.last().unwrap_or(&0.0);
        let last_vus = *vus.last().unwrap_or(&0.0);
        let min_rps = rps.iter().copied().fold(f64::INFINITY, f64::min);
        let max_rps = rps.iter().copied().fold(0.0, f64::max);
        let min_p95 = p95.iter().copied().fold(f64::INFINITY, f64::min);
        let max_p95 = p95.iter().copied().fold(0.0, f64::max);
        let min_err = errors.iter().copied().fold(f64::INFINITY, f64::min);
        let max_err = errors.iter().copied().fold(0.0, f64::max);
        let min_vus = vus.iter().copied().fold(f64::INFINITY, f64::min);
        let max_vus = vus.iter().copied().fold(0.0, f64::max);

        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_3()
            .child(chart_card(
                t!("chart.throughput").to_string(),
                format!("{last_rps:.1}"),
                format!(
                    "req/s · {}",
                    t!(
                        "chart.min_max",
                        min = format!("{min_rps:.1}"),
                        max = format!("{max_rps:.1}")
                    )
                ),
                Bars::new(rps, primary).into_any_element(),
                primary,
                cx,
            ))
            .child(chart_card(
                t!("chart.latency_p95").to_string(),
                format!("{last_p95:.0}"),
                format!(
                    "ms · {}",
                    t!(
                        "chart.min_max",
                        min = format!("{min_p95:.0}"),
                        max = format!("{max_p95:.0}")
                    )
                ),
                Sparkline::new(p95, warning, cx.theme().border).into_any_element(),
                warning,
                cx,
            ))
            .child(chart_card(
                t!("chart.errors").to_string(),
                format!("{last_err:.1}%"),
                t!(
                    "chart.min_max",
                    min = format!("{min_err:.1}%"),
                    max = format!("{max_err:.1}%")
                )
                .to_string(),
                Sparkline::new(errors, danger, cx.theme().border).into_any_element(),
                danger,
                cx,
            ))
            .child(chart_card(
                t!("chart.vus").to_string(),
                format!("{last_vus:.0}"),
                t!(
                    "chart.min_max",
                    min = format!("{min_vus:.0}"),
                    max = format!("{max_vus:.0}")
                )
                .to_string(),
                Sparkline::new(vus, info, cx.theme().border).into_any_element(),
                info,
                cx,
            ))
            .into_any_element()
    }

    fn render_run_logs(&self, cx: &mut Context<Self>) -> AnyElement {
        // Le défilement est (re)demandé au rendu : à l'ajout d'une ligne, la mise en page
        // n'est pas encore à jour, donc on recolle en bas une fois les lignes construites.
        if self.run.is_running() && !self.run.logs_collapsed {
            self.run.log_scroll.scroll_to_bottom();
        }

        let collapsed = self.run.logs_collapsed;
        let term_bg = terminal_bg(cx);

        let mut lines = div()
            .flex()
            .flex_col()
            .gap_px()
            .p_3()
            .font_family(cx.theme().mono_font_family.clone())
            .text_xs();
        for line in &self.run.logs {
            let color = if line.contains("Erreur")
                || line.contains("Error")
                || line.contains("Impossible")
                || line.contains("Could not")
                || line.contains("Seuil non respecté")
                || line.contains("Threshold not met")
                || line.contains("Arrêt automatique")
                || line.contains("Automatic stop")
                || line.contains("failed")
            {
                cx.theme().danger
            } else if line.contains("Terminé") || line.contains("Finished") {
                cx.theme().success
            } else {
                cx.theme().muted_foreground
            };
            lines = lines.child(
                div()
                    .whitespace_nowrap()
                    .text_color(color)
                    .child(line.clone()),
            );
        }
        if self.run.logs.is_empty() {
            lines = lines.child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("run.log_empty").to_string()),
            );
        }

        let header = div()
            .id("run-logs-toggle")
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(32.))
            .px_3()
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().muted.opacity(0.4)))
            .when(!collapsed, |el| {
                el.border_b_1().border_color(cx.theme().border)
            })
            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.run.logs_collapsed = !this.run.logs_collapsed;
                cx.notify();
            }))
            .child(
                ui::icon(if collapsed { "chevron-right" } else { "chevron-down" })
                    .with_size(Size::XSmall)
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                ui::icon("terminal")
                    .with_size(Size::Small)
                    .text_color(cx.theme().muted_foreground),
            )
            .child(ui::section_label(t!("run.log").to_string(), cx))
            .child(div().flex_1())
            .child(ui::mono_muted(
                t!("run.log_lines", count = self.run.logs.len()).to_string(),
                cx,
            ));

        div()
            .flex()
            .flex_col()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .bg(term_bg)
            .overflow_hidden()
            .child(header)
            .when(!collapsed, |el| {
                el.child(
                    div()
                        .id("run-logs")
                        .h(px(220.))
                        .overflow_y_scroll()
                        .track_scroll(&self.run.log_scroll)
                        .child(lines),
                )
            })
            .into_any_element()
    }
}

/// Fond « terminal » : quasi noir en thème sombre, surface neutre sinon.
pub(crate) fn terminal_bg(cx: &App) -> Hsla {
    if cx.theme().is_dark() {
        rgb(0x0A0A0C).into()
    } else {
        cx.theme().muted
    }
}

/// Petite pastille mono neutre (résumé de profil, dossier…).
fn mono_chip(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .flex_none()
        .px_1p5()
        .py(px(2.))
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().muted.opacity(0.5))
        .font_family(cx.theme().mono_font_family.clone())
        .text_xs()
        .line_height(px(16.))
        .text_color(cx.theme().foreground)
        .child(text.into())
}

fn chart_card(
    title: String,
    current: String,
    subtitle: String,
    chart: AnyElement,
    accent: Hsla,
    cx: &App,
) -> AnyElement {
    ui::surface(cx)
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(240.))
        .gap_2()
        .p_3()
        .child(
            div()
                .flex()
                .flex_row()
                .items_start()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .flex_1()
                        .min_w(px(0.))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(div().size(px(6.)).rounded_full().bg(accent))
                                .child(ui::section_label(title, cx)),
                        )
                        .child(ui::mono_muted(subtitle, cx)),
                )
                .child(
                    div()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_lg()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(cx.theme().foreground)
                        .child(current),
                ),
        )
        .child(chart)
        .into_any_element()
}

fn number_field(
    label: &str,
    tooltip: Option<&str>,
    input: &gpui::Entity<gpui_component::input::InputState>,
    cx: &App,
) -> AnyElement {
    ui::labeled(label, tooltip, NumberInput::new(input).w_full(), cx)
}

fn empty_chart_placeholder(accent: Hsla, cx: &App) -> AnyElement {
    div()
        .h(px(160.))
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .justify_end()
        .p_2()
        .rounded(cx.theme().radius)
        .bg(cx.theme().background)
        .child(
            div()
                .flex()
                .flex_row()
                .items_end()
                .gap_1()
                .h(px(88.))
                .children((0..12).map(|i| {
                    let h = 12.0 + ((i * 17) % 48) as f32;
                    let alpha = 0.10 + ((i % 5) as f32) * 0.04;
                    div()
                        .flex_1()
                        .h(px(h))
                        .rounded(cx.theme().radius)
                        .bg(accent.opacity(alpha))
                })),
        )
        .child(ui::muted(t!("chart.empty_hint").to_string(), cx))
        .into_any_element()
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

/// Rendu d'une entrée d'historique dense : point de statut, nom, date, durée en mono.
pub(crate) fn run_row(
    run: &Run,
    selected: bool,
    cx: &App,
) -> AnyElement {
    let color = status_color(run.status, cx);
    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .px_2()
        .py_1p5()
        .rounded(cx.theme().radius)
        .when(selected, |el| {
            el.bg(cx.theme().list_active)
                .border_l_2()
                .border_color(cx.theme().primary)
                .pl(px(6.))
        })
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(div().size(px(6.)).flex_none().rounded_full().bg(color))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .truncate()
                        .text_sm()
                        .child(run.test_name.clone()),
                )
                .child(ui::mono_muted(
                    run.duration_ms()
                        .map(|ms| lt_core::profiles::format_duration(ms as u64))
                        .unwrap_or_else(|| "—".to_string()),
                    cx,
                )),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .pl(px(14.))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format_timestamp(run.started_at)),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .text_xs()
                        .text_color(color)
                        .child(run.status.label()),
                ),
        )
        .into_any_element()
}
