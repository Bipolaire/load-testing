//! Fenêtre principale : état de l'application, onglets et actions globales.

use crate::confirm;
use crate::forms::{AuthForm, ScenarioForm, StepForm};
use crate::panel_run::RunController;
use crate::ui;
use gpui::prelude::*;
use gpui::{
    actions, AnyElement, App, ClickEvent, Context, Entity, Hsla, IntoElement, ParentElement,
    SharedString, Styled, Subscription, WeakEntity, Window, div, img, px,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{InputEvent, InputState, NumberInputEvent, StepAction};
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use gpui_component::tab::{Tab, TabBar};
use gpui_component::TitleBar;
use gpui_component::{ActiveTheme, Root, Sizable, StyledExt, Theme, ThemeMode};

actions!(
    load_testing,
    [
        TabRun,
        TabScenarios,
        TabAuth,
        TabHistory,
        Save,
        StartRun,
        StopRun,
        NewItem,
    ]
);
use lt_core::models::{Auth, Profile, Run, RunOptions, RunStatus, Step, StepType, Test};
use lt_core::profiles::parse_duration_ms;
use lt_core::storage::{self, Settings, Workspace};
use lt_engine::engine::{RunConfig, RunEvent};
use rust_i18n::t;
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ScenarioEditorTab {
    #[default]
    General,
    Steps,
    Advanced,
    Curl,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TabKind {
    Run,
    Scenarios,
    Auth,
    History,
}

impl TabKind {
    pub fn index(self) -> usize {
        match self {
            TabKind::Run => 0,
            TabKind::Scenarios => 1,
            TabKind::Auth => 2,
            TabKind::History => 3,
        }
    }

    pub fn from_index(index: usize) -> Self {
        match index {
            1 => TabKind::Scenarios,
            2 => TabKind::Auth,
            3 => TabKind::History,
            _ => TabKind::Run,
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key {
            "scenarios" => TabKind::Scenarios,
            "auth" => TabKind::Auth,
            "history" => TabKind::History,
            _ => TabKind::Run,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            TabKind::Run => "run",
            TabKind::Scenarios => "scenarios",
            TabKind::Auth => "auth",
            TabKind::History => "history",
        }
    }
}

pub struct AppView {
    pub(crate) workspace: Workspace,
    pub(crate) settings: Settings,
    pub(crate) tests: Vec<Test>,
    pub(crate) folders: Vec<String>,
    pub(crate) auths: Vec<Auth>,
    pub(crate) runs: Vec<Run>,
    pub(crate) tab: TabKind,
    pub(crate) selected_test: Option<String>,
    pub(crate) scenario: Option<ScenarioForm>,
    pub(crate) scenario_baseline: Option<Test>,
    pub(crate) scenario_tab: ScenarioEditorTab,
    pub(crate) selected_auth: Option<String>,
    pub(crate) auth_form: Option<AuthForm>,
    pub(crate) auth_baseline: Option<Auth>,
    pub(crate) run_test: Option<String>,
    pub(crate) profile: Profile,
    pub(crate) vus: Entity<InputState>,
    pub(crate) tps: Entity<InputState>,
    pub(crate) duration: Entity<InputState>,
    pub(crate) ramp_up: Entity<InputState>,
    pub(crate) ramp_down: Entity<InputState>,
    pub(crate) iterations: Entity<InputState>,
    pub(crate) run: RunController,
    pub(crate) selected_run: Option<String>,
    pub(crate) history_query: Entity<InputState>,
    pub(crate) history_status: Option<RunStatus>,
    pub(crate) collapsed: Vec<String>,
    pub(crate) new_folder: Entity<InputState>,
    pub(crate) theme: String,
    pub(crate) language: String,
    pub(crate) try_result: Option<Result<lt_engine::TryResult, String>>,
    pub(crate) try_running: bool,
    pub(crate) try_body_scroll: gpui::ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl AppView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = Settings::load();
        let root = settings
            .workspace
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(storage::default_workspace);
        let workspace = Workspace::open(&root).unwrap_or_else(|_| {
            let fallback = storage::default_workspace();
            Workspace::open(&fallback).expect(&t!("error.default_workspace"))
        });

        let vus = forms_input(window, cx, "20", &t!("placeholder.vus"));
        let tps = forms_input(window, cx, "10", &t!("placeholder.tps"));
        let duration = forms_input(window, cx, "", &t!("placeholder.duration"));
        let ramp_up = forms_input(window, cx, "60s", &t!("placeholder.ramp_up"));
        let ramp_down = forms_input(window, cx, "30s", &t!("placeholder.ramp_down"));
        let iterations = forms_input(window, cx, "", &t!("placeholder.iterations"));
        let new_folder = forms_input(window, cx, "", &t!("placeholder.new_folder"));
        let history_query = forms_input(window, cx, "", &t!("placeholder.search"));

        let profile = settings.profile;
        let mut subscriptions = Vec::new();
        for input in [&vus, &tps, &iterations] {
            subscriptions.push(cx.subscribe_in(input, window, {
                move |_this, input, event: &NumberInputEvent, window, cx| {
                    let NumberInputEvent::Step(action) = event;
                    input.update(cx, |state, cx| {
                        let raw = state.value();
                        let value = raw.trim().parse::<f64>().unwrap_or(0.0);
                        let next = match action {
                            StepAction::Increment => (value + 1.0).max(0.0),
                            StepAction::Decrement => (value - 1.0).max(0.0),
                        };
                        let text = if (next - next.round()).abs() < 0.005 {
                            format!("{:.0}", next)
                        } else {
                            format!("{next}")
                        };
                        state.set_value(SharedString::from(text), window, cx);
                    });
                }
            }));
        }
        subscriptions.push(cx.subscribe_in(&new_folder, window, {
            move |this, _input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.add_folder(window, cx);
                }
            }
        }));

        let mut view = Self {
            workspace,
            theme: settings.theme.clone(),
            language: lt_core::i18n::normalize(&settings.language).to_string(),
            tab: TabKind::from_key(&settings.last_tab),
            selected_test: None,
            scenario: None,
            scenario_baseline: None,
            scenario_tab: ScenarioEditorTab::General,
            selected_auth: None,
            auth_form: None,
            auth_baseline: None,
            run_test: None,
            profile,
            vus,
            tps,
            duration,
            ramp_up,
            ramp_down,
            iterations,
            run: RunController::default(),
            selected_run: None,
            history_query,
            history_status: None,
            collapsed: settings.collapsed_folders.clone(),
            new_folder,
            try_result: None,
            try_running: false,
            try_body_scroll: gpui::ScrollHandle::default(),
            tests: Vec::new(),
            folders: Vec::new(),
            auths: Vec::new(),
            runs: Vec::new(),
            settings,
            _subscriptions: subscriptions,
        };
        view.reload();
        let first = view
            .tests
            .iter()
            .find(|test| test.id == view.settings.last_test_id)
            .map(|test| test.id.clone())
            .or_else(|| view.tests.first().map(|test| test.id.clone()));
        if let Some(id) = first {
            view.run_test = Some(id.clone());
            view.select_test(&id, window, cx);
        }
        view
    }

    pub(crate) fn reload(&mut self) {
        self.tests = self.workspace.load_tests();
        self.folders = lt_core::storage::all_folders(&self.workspace);
        self.auths = self.workspace.load_auth();
        let mut runs = self.workspace.load_runs();
        runs.sort_by_key(|run| std::cmp::Reverse(run.started_at));
        self.runs = runs;
    }

    pub(crate) fn save_settings(&mut self) {
        self.save_settings_with_window(None);
    }

    pub(crate) fn save_settings_with_window(&mut self, window: Option<&Window>) {
        self.settings.workspace = Some(self.workspace.root().to_string_lossy().to_string());
        self.settings.last_tab = self.tab.key().to_string();
        self.settings.last_test_id = self.selected_test.clone().unwrap_or_default();
        self.settings.profile = self.profile;
        self.settings.theme = self.theme.clone();
        self.settings.language = self.language.clone();
        self.settings.collapsed_folders = self.collapsed.clone();
        if let Some(window) = window {
            let bounds = window.bounds();
            self.settings.window_width = Some(f32::from(bounds.size.width));
            self.settings.window_height = Some(f32::from(bounds.size.height));
        }
        let _ = self.settings.save();
    }

    pub(crate) fn notify(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        text: impl Into<gpui::SharedString>,
        error: bool,
    ) {
        ui::notify(window, cx, text, error);
    }

    pub(crate) fn set_tab(&mut self, tab: TabKind, window: &mut Window, cx: &mut Context<Self>) {
        self.tab = tab;
        self.save_settings_with_window(Some(window));
        cx.notify();
    }

    pub(crate) fn is_scenario_dirty(&self, cx: &App) -> bool {
        let Some(form) = self.scenario.as_ref() else {
            return false;
        };
        let Some(baseline) = self.scenario_baseline.as_ref() else {
            return true;
        };
        &form.draft(cx) != baseline
    }

    pub(crate) fn is_auth_dirty(&self, cx: &App) -> bool {
        let Some(form) = self.auth_form.as_ref() else {
            return false;
        };
        let Some(baseline) = self.auth_baseline.as_ref() else {
            return true;
        };
        let previous = self.auths.iter().find(|auth| auth.id == form.id);
        &form.draft(cx, previous) != baseline
    }

    pub(crate) fn request_select_auth(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_auth.as_deref() == Some(id) {
            return;
        }
        if self.is_auth_dirty(cx) {
            let id = id.to_string();
            let weak = cx.entity().downgrade();
            confirm::confirm_discard(window, cx, move |window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.select_auth(&id, window, cx);
                });
            });
            return;
        }
        self.select_auth(id, window, cx);
    }

    pub(crate) fn request_select_test(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_test.as_deref() == Some(id) {
            return;
        }
        if self.is_scenario_dirty(cx) {
            let id = id.to_string();
            let weak = cx.entity().downgrade();
            confirm::confirm_discard(window, cx, move |window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.select_test(&id, window, cx);
                });
            });
            return;
        }
        self.select_test(id, window, cx);
    }

    pub(crate) fn test_by_id(&self, id: &str) -> Option<&Test> {
        self.tests.iter().find(|test| test.id == id)
    }

    pub(crate) fn selected_test(&self) -> Option<&Test> {
        self.selected_test
            .as_ref()
            .and_then(|id| self.test_by_id(id))
    }

    pub(crate) fn run_test(&self) -> Option<&Test> {
        self.run_test.as_ref().and_then(|id| self.test_by_id(id))
    }

    pub(crate) fn select_test(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(test) = self.test_by_id(id).cloned() else {
            return;
        };
        self.selected_test = Some(id.to_string());
        let form = ScenarioForm::new(&test, window, cx);
        self.scenario_baseline = Some(form.draft(cx));
        self.scenario = Some(form);
        self.scenario_tab = ScenarioEditorTab::General;
        self.try_result = None;
        self.run_test = Some(id.to_string());
        self.save_settings_with_window(Some(window));
        cx.notify();
    }

    pub(crate) fn request_delete_test(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(test) = self.selected_test().cloned() else {
            return;
        };
        let id = test.id.clone();
        let name = test.name.clone();
        let weak = cx.entity().downgrade();
        confirm::confirm_delete(
            window,
            cx,
            name,
            t!("confirm.irreversible").to_string(),
            move |window, cx| {
                let _ = weak.update(cx, |this, cx| this.delete_test(&id, window, cx));
            },
        );
    }

    pub(crate) fn request_delete_auth(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(auth) = self
            .selected_auth
            .as_ref()
            .and_then(|id| self.auths.iter().find(|auth| &auth.id == id))
            .cloned()
        else {
            return;
        };
        let usage = self
            .tests
            .iter()
            .filter(|test| test.auth_id == auth.id)
            .count();
        let detail = if usage > 0 {
            t!("confirm.auth_in_use", count = usage).to_string()
        } else {
            t!("confirm.irreversible").to_string()
        };
        let id = auth.id.clone();
        let name = auth.name.clone();
        let weak = cx.entity().downgrade();
        confirm::confirm_delete(window, cx, name, detail, move |window, cx| {
            let _ = weak.update(cx, |this, cx| this.delete_auth(&id, window, cx));
        });
    }

    pub(crate) fn request_delete_run(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(run) = self
            .selected_run
            .as_ref()
            .and_then(|id| self.runs.iter().find(|run| &run.id == id))
            .cloned()
        else {
            return;
        };
        let id = run.id.clone();
        let name = run.test_name.clone();
        let weak = cx.entity().downgrade();
        confirm::confirm_delete(
            window,
            cx,
            name,
            t!("confirm.run_reports").to_string(),
            move |window, cx| {
                let _ = weak.update(cx, |this, cx| this.delete_run(&id, window, cx));
            },
        );
    }

    pub(crate) fn request_delete_step(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label = self
            .scenario
            .as_ref()
            .and_then(|form| form.steps.get(index))
            .map(|step| {
                let name = step.name.read(cx).value().trim().to_string();
                if name.is_empty() {
                    t!("step.default_name", index = index + 1).to_string()
                } else {
                    name
                }
            })
            .unwrap_or_else(|| t!("step.default_name", index = index + 1).to_string());
        let weak = cx.entity().downgrade();
        confirm::confirm_delete(
            window,
            cx,
            label,
            t!("confirm.step_removed").to_string(),
            move |_window, cx| {
                let _ = weak.update(cx, |this, cx| this.delete_step(index, cx));
            },
        );
    }

    pub(crate) fn select_auth(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(auth) = self.auths.iter().find(|auth| auth.id == id).cloned() else {
            return;
        };
        self.selected_auth = Some(id.to_string());
        let form = AuthForm::new(&auth, window, cx);
        let previous = self.auths.iter().find(|item| item.id == auth.id);
        self.auth_baseline = Some(form.draft(cx, previous));
        self.auth_form = Some(form);
        cx.notify();
    }

    pub(crate) fn new_test(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut test = Test::new_empty();
        test.id = lt_core::storage::new_id();
        let base = test.name.clone();
        let mut index = 2;
        while self.tests.iter().any(|existing| existing.name == test.name) {
            test.name = format!("{base} {index}");
            index += 1;
        }
        if let Some(current) = self.selected_test().and_then(|test| {
            (!test.folder.is_empty()).then_some(test.folder.clone())
        }) {
            test.folder = current;
        }
        self.tests.push(test.clone());
        if let Err(error) = self.workspace.save_tests(&self.tests) {
            self.notify(window, cx, t!("toast.save_failed", error = error).to_string(), true);
            return;
        }
        self.reload();
        self.select_test(&test.id, window, cx);
        self.notify(window, cx, t!("toast.scenario.created").to_string(), false);
    }

    pub(crate) fn duplicate_test(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut test) = self.selected_test().cloned() else {
            return;
        };
        test.id = lt_core::storage::new_id();
        test.name = t!("scenario.copy_name", name = test.name).to_string();
        self.tests.push(test.clone());
        if let Err(error) = self.workspace.save_tests(&self.tests) {
            self.notify(window, cx, t!("toast.save_failed", error = error).to_string(), true);
            return;
        }
        self.reload();
        self.select_test(&test.id, window, cx);
        self.notify(window, cx, t!("toast.scenario.duplicated").to_string(), false);
    }

    pub(crate) fn delete_test(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.tests.retain(|test| test.id != id);
        if let Err(error) = self.workspace.save_tests(&self.tests) {
            self.notify(window, cx, t!("toast.delete_failed", error = error).to_string(), true);
            return;
        }
        self.reload();
        let next = self.tests.first().map(|test| test.id.clone());
        if let Some(next) = next {
            self.select_test(&next, window, cx);
        } else {
            self.selected_test = None;
            self.scenario = None;
            self.scenario_baseline = None;
        }
        self.notify(window, cx, t!("toast.scenario.deleted").to_string(), false);
    }

    pub(crate) fn save_scenario(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.scenario.as_ref() else {
            return;
        };
        let draft = form.draft(cx);
        if draft.name.trim().is_empty() {
            self.notify(
                window,
                cx,
                t!("validation.scenario_name_required").to_string(),
                true,
            );
            return;
        }
        if let Some(existing) = self.tests.iter_mut().find(|test| test.id == draft.id) {
            let id = draft.id.clone();
            *existing = draft.clone();
            if let Err(error) = self.workspace.save_tests(&self.tests) {
                self.notify(window, cx, t!("toast.save_failed", error = error).to_string(), true);
                return;
            }
            self.reload();
            self.selected_test = Some(id.clone());
            self.scenario_baseline = Some(draft);
        }
        self.notify(window, cx, t!("toast.scenario.saved").to_string(), false);
        cx.notify();
    }

    // -----------------------------------------------------------------------
    // Édition des étapes
    // -----------------------------------------------------------------------

    pub(crate) fn add_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.scenario.as_mut() else {
            return;
        };
        let index = form.steps.len();
        let step = Step {
            name: format!("etape-{}", index + 1),
            ..Step::default()
        };
        form.steps.push(StepForm::new(&step, index, window, cx));
        form.selected_step = index;
        self.try_result = None;
        cx.notify();
    }

    pub(crate) fn duplicate_step(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.scenario.as_ref() else {
            return;
        };
        let Some(step) = form.steps.get(index) else {
            return;
        };
        let mut draft = step.to_step(index, cx);
        draft.name = t!("scenario.copy_name", name = draft.name).to_string();
        let position = (index + 1).min(form.steps.len());
        let Some(form) = self.scenario.as_mut() else {
            return;
        };
        form.steps
            .insert(position, StepForm::new(&draft, position, window, cx));
        form.selected_step = position;
        self.try_result = None;
        cx.notify();
    }

    pub(crate) fn delete_step(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(form) = self.scenario.as_mut() else {
            return;
        };
        if index >= form.steps.len() {
            return;
        }
        form.steps.remove(index);
        if form.selected_step >= form.steps.len() {
            form.selected_step = form.steps.len().saturating_sub(1);
        }
        self.try_result = None;
        cx.notify();
    }

    pub(crate) fn set_step_type(
        &mut self,
        index: usize,
        step_type: StepType,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.scenario.as_mut() else {
            return;
        };
        let Some(step) = form.steps.get_mut(index) else {
            return;
        };
        step.step_type = step_type;
        if step_type == StepType::Soap {
            step.method = "POST".to_string();
            if step.body.read(cx).value().trim().is_empty() {
                let template = lt_core::xml::soap_template(&step.soap_version);
                step.body
                    .update(cx, |state, cx| state.set_value(template, window, cx));
            }
        }
        cx.notify();
    }

    pub(crate) fn set_soap_version(
        &mut self,
        index: usize,
        version: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.scenario.as_mut() else {
            return;
        };
        let Some(step) = form.steps.get_mut(index) else {
            return;
        };
        let previous = step.soap_version.clone();
        if previous == version {
            return;
        }
        let old_template = lt_core::xml::soap_template(&previous);
        let current_body = step.body.read(cx).value().to_string();
        step.soap_version = version.to_string();
        if current_body.trim().is_empty() || current_body.trim() == old_template.trim() {
            let template = lt_core::xml::soap_template(version);
            step.body
                .update(cx, |state, cx| state.set_value(template, window, cx));
        }
        cx.notify();
    }

    pub(crate) fn import_curl(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.scenario.as_ref() else {
            return;
        };
        let command = form.curl.read(cx).value().to_string();
        if command.trim().is_empty() {
            self.notify(
                window,
                cx,
                t!("validation.curl_required").to_string(),
                true,
            );
            return;
        }
        let parsed = match lt_core::curl::parse_curl(&command) {
            Ok(parsed) => parsed,
            Err(error) => {
                self.notify(
                    window,
                    cx,
                    t!("error.curl_invalid", error = error).to_string(),
                    true,
                );
                return;
            }
        };
        let base = form.base_url.read(cx).value().trim().to_string();
        let Some(form) = self.scenario.as_mut() else {
            return;
        };
        if base.is_empty() || base == "https://" || base == "http://" {
            let parsed_base = parsed.base_url.clone();
            form.base_url
                .update(cx, |state, cx| state.set_value(parsed_base.clone(), window, cx));
        }
        let index = form.steps.len();
        form.steps
            .push(StepForm::new(&parsed.step, index, window, cx));
        form.selected_step = index;
        form.curl
            .update(cx, |state, cx| state.set_value("", window, cx));
        let notes = parsed.notes.len();
        self.notify(
            window,
            cx,
            if notes > 0 {
                t!("toast.curl.imported_notes", step = index + 1, notes = notes).to_string()
            } else {
                t!("toast.curl.imported", step = index + 1).to_string()
            },
            false,
        );
        self.scenario_tab = ScenarioEditorTab::Steps;
        cx.notify();
    }

    pub(crate) fn try_step(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.try_running {
            return;
        }
        let Some(form) = self.scenario.as_ref() else {
            return;
        };
        let index = form.selected_step;
        if index >= form.steps.len() {
            return;
        }
        let test = form.draft(cx);
        let auth = self
            .auths
            .iter()
            .find(|auth| auth.id == test.auth_id)
            .cloned();
        self.try_running = true;
        self.try_result = None;

        let (sender, receiver) = tokio::sync::oneshot::channel();
        std::thread::Builder::new()
            .name("lt-try".to_string())
            .spawn(move || {
                let result = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime
                        .block_on(lt_engine::try_request(&test, index, auth.as_ref()))
                        .map_err(|error| error.to_string()),
                    Err(error) => Err(t!("error.runtime_unavailable", error = error).to_string()),
                };
                let _ = sender.send(result);
            })
            .ok();

        let weak = cx.entity().downgrade();
        cx.spawn(async move |_this, cx| {
            let result = receiver.await.unwrap_or_else(|_| {
                Err(t!("error.try_failed").to_string())
            });
            let _ = weak.update(cx, |this, cx| {
                this.try_result = Some(result);
                this.try_running = false;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    // -----------------------------------------------------------------------
    // Authentifications
    // -----------------------------------------------------------------------

    pub(crate) fn new_auth(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let auth = crate::panel_auth::new_auth(&self.auths);
        self.auths.push(auth.clone());
        if let Err(error) = self.workspace.save_auth(&self.auths) {
            self.notify(window, cx, t!("toast.save_failed", error = error).to_string(), true);
            return;
        }
        self.reload();
        self.select_auth(&auth.id, window, cx);
        self.notify(window, cx, t!("toast.auth.created").to_string(), false);
    }

    pub(crate) fn save_auth(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.auth_form.as_ref() else {
            return;
        };
        let previous = self
            .auths
            .iter()
            .find(|auth| auth.id == form.id)
            .cloned();
        let draft = form.draft(cx, previous.as_ref());
        if draft.name.trim().is_empty() {
            self.notify(
                window,
                cx,
                t!("validation.auth_name_required").to_string(),
                true,
            );
            return;
        }
        let draft_id = draft.id.clone();
        match self.auths.iter_mut().find(|auth| auth.id == draft.id) {
            Some(existing) => *existing = draft,
            None => self.auths.push(draft),
        }
        if let Err(error) = self.workspace.save_auth(&self.auths) {
            self.notify(window, cx, t!("toast.save_failed", error = error).to_string(), true);
            return;
        }
        self.reload();
        self.select_auth(&draft_id, window, cx);
        if let Some(form) = self.auth_form.as_ref() {
            let previous = self.auths.iter().find(|auth| auth.id == form.id);
            self.auth_baseline = Some(form.draft(cx, previous));
        }
        self.notify(window, cx, t!("toast.auth.saved").to_string(), false);
    }

    pub(crate) fn delete_auth(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = id.to_string();
        self.auths.retain(|auth| auth.id != id);
        if let Err(error) = self.workspace.save_auth(&self.auths) {
            self.notify(window, cx, t!("toast.delete_failed", error = error).to_string(), true);
            return;
        }
        let mut tests = self.workspace.load_tests();
        let mut changed = false;
        for test in tests.iter_mut() {
            if test.auth_id == id {
                test.auth_id.clear();
                changed = true;
            }
        }
        if changed {
            let _ = self.workspace.save_tests(&tests);
        }
        if let Some(form) = self.scenario.as_mut()
            && form.auth_id == id
        {
            form.auth_id.clear();
        }
        self.reload();
        self.selected_auth = None;
        self.auth_form = None;
        self.auth_baseline = None;
        self.notify(window, cx, t!("toast.auth.deleted").to_string(), false);
        cx.notify();
    }

    // -----------------------------------------------------------------------
    // Historique
    // -----------------------------------------------------------------------

    pub(crate) fn delete_run(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut runs = self.workspace.load_runs();
        let Some(run) = runs.iter().find(|run| run.id == id).cloned() else {
            return;
        };
        runs.retain(|run| run.id != id);
        if let Err(error) = self.workspace.save_runs(&runs) {
            self.notify(window, cx, t!("toast.delete_failed", error = error).to_string(), true);
            return;
        }
        // Nouveau layout : dossier reports/<run.name>/ ; ancien : fichiers à plat.
        let run_dir = self.workspace.run_reports_dir(&run.name);
        if run_dir.is_dir() {
            let _ = std::fs::remove_dir_all(&run_dir);
        }
        for name in &run.reports {
            let _ = std::fs::remove_file(self.workspace.reports_dir().join(name));
        }
        let _ = std::fs::remove_file(self.workspace.run_config_path(&run.name));
        let _ = std::fs::remove_file(self.workspace.run_log_path(&run.name));
        self.reload();
        self.selected_run = None;
        self.notify(window, cx, t!("toast.run.deleted").to_string(), false);
        cx.notify();
    }

    pub(crate) fn add_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.new_folder.read(cx).value().to_string();
        let name = lt_core::folders::norm_folder(&name);
        if name.is_empty() {
            self.notify(
                window,
                cx,
                t!("validation.folder_name_required").to_string(),
                true,
            );
            return;
        }
        let mut folders = self.workspace.load_folders();
        if !folders.contains(&name) {
            folders.push(name.clone());
            folders.sort();
            if let Err(error) = self.workspace.save_folders(&folders) {
                self.notify(
                    window,
                    cx,
                    t!("toast.folder.save_failed", error = error).to_string(),
                    true,
                );
                return;
            }
        }
        self.new_folder
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.reload();
        self.notify(
            window,
            cx,
            t!("toast.folder.added", name = name).to_string(),
            false,
        );
        cx.notify();
    }

    pub(crate) fn apply_theme(&mut self, mode: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.theme = mode.to_string();
        match mode {
            "light" => Theme::change(ThemeMode::Light, Some(window), cx),
            "dark" => Theme::change(ThemeMode::Dark, Some(window), cx),
            _ => Theme::sync_system_appearance(Some(window), cx),
        }
        ui::apply_theme(cx);
        self.save_settings_with_window(Some(window));
        cx.notify();
    }

    /// Change la langue de l'interface (et de `lt-core` / `gpui-component`) à chaud.
    pub(crate) fn apply_language(
        &mut self,
        lang: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let lang = lt_core::i18n::normalize(lang).to_string();
        self.language = lang.clone();
        lt_core::i18n::set_language(&lang);
        gpui_component::set_locale(&lang);
        self.refresh_placeholders(window, cx);
        self.save_settings_with_window(Some(window));
        cx.notify();
    }

    /// Réapplique les textes indicatifs des champs déjà créés.
    fn refresh_placeholders(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let fields = [
            (&self.vus, t!("placeholder.vus").to_string()),
            (&self.tps, t!("placeholder.tps").to_string()),
            (&self.duration, t!("placeholder.duration").to_string()),
            (&self.ramp_up, t!("placeholder.ramp_up").to_string()),
            (&self.ramp_down, t!("placeholder.ramp_down").to_string()),
            (&self.iterations, t!("placeholder.iterations").to_string()),
            (&self.new_folder, t!("placeholder.new_folder").to_string()),
            (&self.history_query, t!("placeholder.search").to_string()),
        ];
        for (state, text) in fields {
            state.update(cx, |state, cx| state.set_placeholder(text, window, cx));
        }
        if let Some(form) = self.scenario.as_ref() {
            form.refresh_placeholders(window, cx);
        }
        if let Some(form) = self.auth_form.as_ref() {
            form.refresh_placeholders(window, cx);
        }
    }

    pub(crate) fn action_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.tab {
            TabKind::Scenarios => self.save_scenario(window, cx),
            TabKind::Auth => self.save_auth(window, cx),
            _ => {}
        }
    }

    pub(crate) fn action_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.tab {
            TabKind::Scenarios | TabKind::Run => self.new_test(window, cx),
            TabKind::Auth => self.new_auth(window, cx),
            TabKind::History => {}
        }
    }

    pub(crate) fn open_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        match Workspace::open(&path) {
            Ok(workspace) => {
                self.workspace = workspace;
                self.settings.workspace = Some(path.to_string_lossy().to_string());
                self.reload();
                self.selected_test = None;
                self.scenario = None;
                self.scenario_baseline = None;
                self.run = RunController::default();
                self.selected_auth = None;
                self.auth_form = None;
                self.auth_baseline = None;
                let first = self.tests.first().map(|test| test.id.clone());
                if let Some(id) = first {
                    self.select_test(&id, window, cx);
                }
                self.notify(
                    window,
                    cx,
                    t!("toast.workspace", path = path.display().to_string()).to_string(),
                    false,
                );
                self.save_settings();
            }
            Err(error) => {
                self.notify(window, cx, t!("toast.open_failed", error = error).to_string(), true)
            }
        }
        cx.notify();
    }

    pub(crate) fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reload();
        if let Some(id) = self.selected_test.clone()
            && self.tests.iter().any(|test| test.id == id)
        {
            self.selected_test = Some(id);
        }
        self.notify(window, cx, t!("toast.refreshed").to_string(), false);
        cx.notify();
    }

    pub(crate) fn request_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        self.import_from(source, false, window, cx);
    }

    pub(crate) fn import_from(
        &mut self,
        source: PathBuf,
        force: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match Workspace::import_from(&source, self.workspace.root(), force) {
            Ok(report) => {
                self.reload();
                let first = self.tests.first().map(|test| test.id.clone());
                if let Some(id) = first {
                    self.select_test(&id, window, cx);
                }
                self.notify(
                    window,
                    cx,
                    t!(
                        "toast.import.done",
                        files = report.files,
                        runs = report.runs,
                        backfilled = report.backfilled
                    )
                    .to_string(),
                    false,
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && !force => {
                let label = source.display().to_string();
                let weak = cx.entity().downgrade();
                let source_clone = source.clone();
                confirm::confirm_import(window, cx, label, move |window, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.import_from(source_clone.clone(), true, window, cx);
                    });
                });
            }
            Err(error) => {
                self.notify(window, cx, t!("toast.import.failed", error = error).to_string(), true)
            }
        }
        cx.notify();
    }

    pub(crate) fn run_options(&self, cx: &App) -> RunOptions {
        let number = |state: &Entity<InputState>| -> Option<u32> {
            state
                .read(cx)
                .value()
                .trim()
                .parse::<u32>()
                .ok()
                .filter(|value| *value > 0)
        };
        let text = |state: &Entity<InputState>| -> Option<String> {
            let value = state.read(cx).value().trim().to_string();
            (!value.is_empty()).then_some(value)
        };
        RunOptions {
            vus: number(&self.vus),
            tps: number(&self.tps),
            duration: text(&self.duration),
            ramp_up: text(&self.ramp_up),
            ramp_down: text(&self.ramp_down),
            iterations: text(&self.iterations).and_then(|value| value.parse::<u64>().ok()),
            ..RunOptions::default()
        }
    }

    pub(crate) fn start_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.run.is_running() {
            return;
        }
        let Some(test) = self.run_test().cloned() else {
            self.notify(
                window,
                cx,
                t!("validation.select_scenario").to_string(),
                true,
            );
            cx.notify();
            return;
        };
        if test.steps.is_empty() {
            self.notify(window, cx, t!("validation.no_steps").to_string(), true);
            cx.notify();
            return;
        }
        let duration = self.duration.read(cx).value().trim().to_string();
        if !duration.is_empty()
            && let Err(error) = parse_duration_ms(&duration)
        {
            self.notify(window, cx, error, true);
            cx.notify();
            return;
        }

        let test = match self.scenario.as_ref() {
            Some(form) if form.id == test.id => form.draft(cx),
            _ => test,
        };
        if test.dangerous {
            let options = self.run_options(cx);
            let summary = t!(
                "dialog.dangerous.summary",
                name = test.name,
                url = test.base_url,
                profile = self.profile.label(),
                vus = options
                    .vus
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "—".to_string()),
                tps = options
                    .tps
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "—".to_string()),
                duration = options
                    .duration
                    .clone()
                    .unwrap_or_else(|| "—".to_string()),
            )
            .to_string();
            let weak = cx.entity().downgrade();
            confirm::confirm_dangerous_run(window, cx, summary, move |window, cx| {
                let _ = weak.update(cx, |this, cx| this.start_run_confirmed(window, cx));
            });
            return;
        }
        self.start_run_confirmed(window, cx);
    }

    pub(crate) fn start_run_confirmed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.run.is_running() {
            return;
        }
        let Some(test) = self.run_test().cloned() else {
            return;
        };
        let test = match self.scenario.as_ref() {
            Some(form) if form.id == test.id => form.draft(cx),
            _ => test,
        };
        let auth = self
            .auths
            .iter()
            .find(|auth| auth.id == test.auth_id)
            .cloned();
        let mut config = RunConfig::new(self.workspace.clone(), test, self.profile);
        config.options = self.run_options(cx);
        config.auth = auth;

        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<RunEvent>();
        let reporter: lt_engine::engine::Reporter = Arc::new(move |event: RunEvent| {
            let _ = sender.send(event);
        });
        let cancel = CancellationToken::new();

        self.run.begin(cancel.clone());
        spawn_engine(config, reporter, cancel);
        let weak = cx.entity().downgrade();
        let window_handle = window.window_handle();
        cx.spawn(async move |_this, cx| {
            receive_events(weak, window_handle, receiver, cx).await;
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn stop_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.run.stop();
        self.notify(window, cx, t!("toast.stop_requested").to_string(), false);
        cx.notify();
    }

    pub(crate) fn on_run_event(
        &mut self,
        event: RunEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            RunEvent::Started { run } => {
                self.run.started(run);
                self.selected_run = None;
            }
            RunEvent::Log(line) => {
                self.run.log(line);
            }
            RunEvent::Sample(point) => {
                self.run.sample(point);
            }
            RunEvent::Finished { run } => {
                self.run.finished(run.clone());
                self.reload();
                self.selected_run = Some(run.id);
                self.notify(
                    window,
                    cx,
                    t!("toast.run.finished", status = run.status.label()).to_string(),
                    run.status != lt_core::RunStatus::Passed,
                );
            }
            RunEvent::Fatal(error) => {
                self.run.fatal(error.clone());
                self.notify(
                    window,
                    cx,
                    t!("toast.run.engine_error", error = error).to_string(),
                    true,
                );
            }
        }
        cx.notify();
    }

    fn render_title_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let entity: WeakEntity<AppView> = cx.entity().downgrade();
        let theme_icon = match self.theme.as_str() {
            "light" => "sun",
            "dark" => "moon",
            _ => "settings",
        };
        let current_theme = self.theme.clone();
        let theme_menu = Button::new("theme")
            .small()
            .ghost()
            .icon(ui::icon(theme_icon))
            .tooltip(t!("toolbar.theme").to_string())
            .dropdown_menu({
                let weak = entity.clone();
                move |menu, _window, _cx| {
                    let mut menu = menu;
                    for (label, value) in [
                        (t!("theme.light").to_string(), "light"),
                        (t!("theme.dark").to_string(), "dark"),
                        (t!("theme.system").to_string(), "auto"),
                    ] {
                        let weak = weak.clone();
                        let value = value.to_string();
                        menu = menu.item(
                            PopupMenuItem::new(label)
                                .checked(current_theme == value)
                                .on_click(move |_, window, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.apply_theme(&value, window, cx);
                                    });
                                }),
                        );
                    }
                    menu
                }
            });

        let current_language = self.language.clone();
        let language_menu = Button::new("language")
            .small()
            .ghost()
            .icon(ui::icon("globe"))
            .label(current_language.to_uppercase())
            .tooltip(t!("toolbar.language").to_string())
            .dropdown_menu({
                let weak = entity.clone();
                move |menu, _window, _cx| {
                    let mut menu = menu;
                    for (label, value) in [("English", "en"), ("Français", "fr")] {
                        let weak = weak.clone();
                        menu = menu.item(
                            PopupMenuItem::new(label)
                                .checked(current_language == value)
                                .on_click(move |_, window, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.apply_language(value, window, cx);
                                    });
                                }),
                        );
                    }
                    menu
                }
            });

        // Les onglets principaux ne sont plus ici : la title bar transparente
        // (macOS / Windows) capture les clics pour le drag de fenêtre.
        TitleBar::new()
            .child(
                div()
                    .h_flex()
                    .w_full()
                    .items_center()
                    .gap_3()
                    .pr_3()
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .child(img(crate::assets::APP_ICON).size(px(18.)).flex_none())
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(t!("app.title").to_string()),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_1()
                            .child(
                                ui::icon_toolbar_button(
                                    "refresh",
                                    "refresh",
                                    &t!("toolbar.refresh"),
                                )
                                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                    this.refresh(window, cx)
                                })),
                            )
                            .child(
                                ui::icon_toolbar_button(
                                    "open-workspace",
                                    "folder",
                                    &t!("toolbar.open_workspace"),
                                )
                                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                    this.open_workspace(window, cx)
                                })),
                            )
                            .child(
                                ui::icon_toolbar_button(
                                    "import",
                                    "external-link",
                                    &t!("toolbar.import"),
                                )
                                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                    this.request_import(window, cx)
                                })),
                            )
                            .child(language_menu)
                            .child(theme_menu),
                    ),
            )
            .into_any_element()
    }

    /// Navigation principale — en haut de la sidebar (hors title bar, donc cliquable).
    pub(crate) fn render_sidebar_nav(&self, cx: &mut Context<Self>) -> AnyElement {
        let entity: WeakEntity<AppView> = cx.entity().downgrade();
        let selected = self.tab.index();
        div()
            .id("sidebar-nav")
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .w_full()
            .h(px(ui::HEADER_HEIGHT))
            .px_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                TabBar::new("main-tabs")
                    .w_full()
                    .segmented()
                    .selected_index(selected)
                    .child(
                        Tab::new()
                            .icon(ui::icon("rocket"))
                            .label(t!("tab.run").to_string()),
                    )
                    .child(
                        Tab::new()
                            .icon(ui::icon("list"))
                            .label(t!("tab.scenarios").to_string()),
                    )
                    .child(
                        Tab::new()
                            .icon(ui::icon("key-round"))
                            .label(t!("tab.auth").to_string()),
                    )
                    .child(
                        Tab::new()
                            .icon(ui::icon("history"))
                            .label(t!("tab.history").to_string()),
                    )
                    .on_click(move |index, window, cx| {
                        let tab = TabKind::from_index(*index);
                        let _ = entity.update(cx, |this, cx| this.set_tab(tab, window, cx));
                    }),
            )
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let (status, color) = match &self.run.state {
            crate::panel_run::RunState::Idle => {
                (t!("status.ready").to_string(), cx.theme().muted_foreground)
            }
            crate::panel_run::RunState::Starting => {
                (t!("status.starting").to_string(), cx.theme().info)
            }
            crate::panel_run::RunState::Running => (
                t!(
                    "status.running",
                    duration =
                        lt_core::profiles::format_duration(self.run.elapsed_ms().max(0) as u64)
                )
                .to_string(),
                cx.theme().info,
            ),
            crate::panel_run::RunState::Finished(run) => (
                t!("status.last_test", status = run.status.label()).to_string(),
                status_color(run.status, cx),
            ),
            crate::panel_run::RunState::Failed(error) => (
                t!("status.failed", error = error).to_string(),
                cx.theme().danger,
            ),
        };
        let path = self.workspace.root().display().to_string();
        let short_path = if path.chars().count() > 48 {
            format!(
                "…{}",
                path.chars()
                    .rev()
                    .take(46)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
            )
        } else {
            path
        };
        div()
            .h_flex()
            .w_full()
            .h(px(24.))
            .px_3()
            .items_center()
            .gap_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(ui::chip(status, color))
            .child(div().flex_1())
            .child(ui::mono_muted(
                t!(
                    "statusbar.counts",
                    scenarios = self.tests.len(),
                    runs = self.runs.len()
                )
                .to_string(),
                cx,
            ))
            .child(ui::mono_muted(short_path, cx))
            .into_any_element()
    }

    pub(crate) fn render_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match self.tab {
            TabKind::Run => self.render_run(window, cx),
            TabKind::Scenarios => self.render_scenarios(window, cx),
            TabKind::Auth => self.render_auth(window, cx),
            TabKind::History => self.render_history(window, cx),
        }
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("app-root")
            .relative()
            .key_context("LoadTesting")
            .on_action(cx.listener(|this, _: &TabRun, window, cx| {
                this.set_tab(TabKind::Run, window, cx);
            }))
            .on_action(cx.listener(|this, _: &TabScenarios, window, cx| {
                this.set_tab(TabKind::Scenarios, window, cx);
            }))
            .on_action(cx.listener(|this, _: &TabAuth, window, cx| {
                this.set_tab(TabKind::Auth, window, cx);
            }))
            .on_action(cx.listener(|this, _: &TabHistory, window, cx| {
                this.set_tab(TabKind::History, window, cx);
            }))
            .on_action(cx.listener(|this, _: &Save, window, cx| {
                this.action_save(window, cx);
            }))
            .on_action(cx.listener(|this, _: &StartRun, window, cx| {
                this.start_run(window, cx);
            }))
            .on_action(cx.listener(|this, _: &StopRun, window, cx| {
                this.stop_run(window, cx);
            }))
            .on_action(cx.listener(|this, _: &NewItem, window, cx| {
                this.action_new(window, cx);
            }))
            .v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_sm()
            .child(self.render_title_bar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .child(self.render_panel(window, cx)),
            )
            .child(self.render_footer(cx))
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

// ---------------------------------------------------------------------------
// Utilitaires
// ---------------------------------------------------------------------------

fn forms_input<T: 'static>(
    window: &mut Window,
    cx: &mut Context<T>,
    value: &str,
    placeholder: &str,
) -> Entity<InputState> {
    crate::forms::input(window, cx, value, placeholder)
}

fn spawn_engine(config: RunConfig, reporter: lt_engine::engine::Reporter, cancel: CancellationToken) {
    std::thread::Builder::new()
        .name("lt-engine".to_string())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            let Ok(runtime) = runtime else {
                reporter(RunEvent::Fatal(
                    t!("error.runtime_start").to_string(),
                ));
                return;
            };
            if let Err(error) = runtime.block_on(lt_engine::execute(
                config,
                Some(reporter.clone()),
                cancel,
            )) {
                reporter(RunEvent::Fatal(error.to_string()));
            }
        })
        .ok();
}

async fn receive_events(
    weak: WeakEntity<AppView>,
    window_handle: gpui::AnyWindowHandle,
    mut receiver: tokio::sync::mpsc::UnboundedReceiver<RunEvent>,
    cx: &mut gpui::AsyncApp,
) {
    while let Some(event) = receiver.recv().await {
        let stop = matches!(event, RunEvent::Finished { .. } | RunEvent::Fatal(_));
        let updated = window_handle.update(cx, |_, window, cx| {
            weak.update(cx, |this, cx| this.on_run_event(event, window, cx))
        });
        if updated.is_err() || stop {
            break;
        }
    }
}

pub fn format_timestamp(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp_millis(timestamp)
        .map(|date| {
            date.with_timezone(&chrono::Local)
                .format("%d/%m/%Y %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "—".to_string())
}

pub fn status_color(status: lt_core::RunStatus, cx: &App) -> Hsla {
    match status {
        lt_core::RunStatus::Passed => cx.theme().success,
        lt_core::RunStatus::Failed | lt_core::RunStatus::Error => cx.theme().danger,
        lt_core::RunStatus::Running => cx.theme().info,
        _ => cx.theme().warning,
    }
}
