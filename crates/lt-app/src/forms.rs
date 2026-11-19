//! Formulaires d'édition : scénarios, étapes et authentifications.
//!
//! Les champs sont des `InputState` GPUI ; le modèle (`Test`, `Auth`) est
//! reconstruit au moment de l'enregistrement (`draft`), ce qui évite de
//! synchroniser chaque frappe.

use gpui::{App, AppContext as _, Context, Entity, Window};
use gpui_component::input::InputState;
use lt_core::models::{
    Auth, AuthType, Check, CheckOp, Extract, Header, Step, StepType, Test, Thresholds, Variable,
};
use lt_core::Extra;
use rust_i18n::t;

/// Met à jour le texte indicatif d'un champ (changement de langue à chaud).
fn set_placeholder(
    state: &Entity<InputState>,
    text: impl Into<gpui::SharedString>,
    window: &mut Window,
    cx: &mut App,
) {
    let text = text.into();
    state.update(cx, |state, cx| state.set_placeholder(text, window, cx));
}

pub fn input<T: 'static>(
    window: &mut Window,
    cx: &mut Context<T>,
    value: &str,
    placeholder: &str,
) -> Entity<InputState> {
    cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(placeholder.to_string())
            .default_value(value.to_string())
    })
}

pub fn textarea<T: 'static>(
    window: &mut Window,
    cx: &mut Context<T>,
    value: &str,
    placeholder: &str,
    rows: usize,
) -> Entity<InputState> {
    cx.new(|cx| {
        InputState::new(window, cx)
            .multi_line(true)
            .rows(rows)
            .placeholder(placeholder.to_string())
            .default_value(value.to_string())
    })
}

pub fn secret_input<T: 'static>(
    window: &mut Window,
    cx: &mut Context<T>,
    value: &str,
    placeholder: &str,
) -> Entity<InputState> {
    cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(placeholder.to_string())
            .default_value(value.to_string())
            .masked(true)
    })
}

fn value(state: &Entity<InputState>, cx: &App) -> String {
    state.read(cx).value().to_string()
}

fn parse_number(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    text.replace(',', ".").parse::<f64>().ok()
}

fn parse_percent(text: &str) -> Option<f64> {
    parse_number(text).map(|value| (value / 100.0).clamp(0.0, 1.0))
}

fn format_number(value: Option<f64>) -> String {
    match value {
        None => String::new(),
        Some(value) => {
            let rounded = value.round();
            if (value - rounded).abs() < 0.005 {
                format!("{rounded:.0}")
            } else {
                format!("{value}")
            }
        }
    }
}

fn format_percent(value: Option<f64>) -> String {
    match value {
        None => String::new(),
        Some(value) => format_number(Some(value * 100.0)),
    }
}

// ---------------------------------------------------------------------------
// Conversions texte <-> structures
// ---------------------------------------------------------------------------

pub fn headers_to_text(headers: &[Header]) -> String {
    headers
        .iter()
        .map(|header| format!("{}: {}", header.key, header.value))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn parse_headers(text: &str) -> Vec<Header> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once(':')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            Some(Header {
                key: key.to_string(),
                value: value.trim().to_string(),
                extra: Extra::new(),
            })
        })
        .collect()
}

pub fn checks_to_text(checks: &[Check]) -> String {
    checks
        .iter()
        .map(|check| {
            if check.op.needs_value() {
                format!("{} | {} | {}", check.path, check.op.as_str(), check.value)
            } else {
                format!("{} | {}", check.path, check.op.as_str())
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn parse_checks(text: &str) -> Vec<Check> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let parts: Vec<&str> = line.split('|').map(str::trim).collect();
            match parts.len() {
                0 | 1 => None,
                2 => {
                    if let Some(op) = CheckOp::parse(parts[1]) {
                        Some(Check {
                            path: parts[0].to_string(),
                            op,
                            value: String::new(),
                            extra: Extra::new(),
                        })
                    } else {
                        Some(Check {
                            path: parts[0].to_string(),
                            op: CheckOp::Equals,
                            value: parts[1].to_string(),
                            extra: Extra::new(),
                        })
                    }
                }
                _ => Some(Check {
                    path: parts[0].to_string(),
                    op: CheckOp::parse(parts[1]).unwrap_or(CheckOp::Equals),
                    value: parts[2].to_string(),
                    extra: Extra::new(),
                }),
                }
        })
        .collect()
}

pub fn extract_to_text(extracts: &[Extract]) -> String {
    extracts
        .iter()
        .map(|extract| format!("{} | {}", extract.name, extract.path))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn parse_extracts(text: &str) -> Vec<Extract> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (name, path) = line.split_once('|')?;
            let name = name.trim();
            if name.is_empty() {
                return None;
            }
            Some(Extract {
                name: name.to_string(),
                path: path.trim().to_string(),
                extra: Extra::new(),
            })
        })
        .collect()
}

pub fn variables_to_text(variables: &[Variable]) -> String {
    variables
        .iter()
        .map(|variable| format!("{} | {}", variable.name, variable.values.join(", ")))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn parse_variables(text: &str) -> Vec<Variable> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (name, values) = line.split_once('|')?;
            let name = name.trim();
            if name.is_empty() {
                return None;
            }
            Some(Variable {
                name: name.to_string(),
                values: values
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .collect(),
                extra: Extra::new(),
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Étape
// ---------------------------------------------------------------------------

pub struct StepForm {
    pub name: Entity<InputState>,
    pub path: Entity<InputState>,
    pub expected_status: Entity<InputState>,
    pub headers: Entity<InputState>,
    pub body: Entity<InputState>,
    pub checks: Entity<InputState>,
    pub extract: Entity<InputState>,
    pub p95: Entity<InputState>,
    pub soap_action: Entity<InputState>,
    pub method: String,
    pub step_type: StepType,
    pub soap_version: String,
    pub stop_on_failure: bool,
    pub extra: Extra,
}

impl StepForm {
    pub fn new<T: 'static>(
        step: &Step,
        index: usize,
        window: &mut Window,
        cx: &mut Context<T>,
    ) -> Self {
        Self {
            name: input(
                window,
                cx,
                &step.effective_name(index),
                &t!("placeholder.step_name"),
            ),
            path: input(window, cx, &step.path, &t!("placeholder.step_path")),
            expected_status: input(window, cx, &step.expected_status, "200"),
            headers: textarea(
                window,
                cx,
                &headers_to_text(&step.headers),
                &t!("placeholder.headers"),
                3,
            ),
            body: textarea(window, cx, &step.body, &t!("placeholder.body"), 6),
            checks: textarea(
                window,
                cx,
                &checks_to_text(&step.checks),
                &t!("placeholder.checks"),
                3,
            ),
            extract: textarea(
                window,
                cx,
                &extract_to_text(&step.extract),
                &t!("placeholder.extract"),
                2,
            ),
            p95: input(
                window,
                cx,
                &format_number(step.p95),
                &t!("placeholder.step_p95"),
            ),
            soap_action: input(
                window,
                cx,
                &step.soap_action,
                &t!("placeholder.soap_action"),
            ),
            method: step.method.to_uppercase(),
            step_type: step.step_type,
            soap_version: if step.soap_version.is_empty() {
                "1.1".to_string()
            } else {
                step.soap_version.clone()
            },
            stop_on_failure: step.stop_on_failure,
            extra: step.extra.clone(),
        }
    }

    /// Réapplique les textes indicatifs dans la langue courante.
    pub fn refresh_placeholders(&self, window: &mut Window, cx: &mut App) {
        set_placeholder(&self.name, t!("placeholder.step_name").to_string(), window, cx);
        set_placeholder(&self.path, t!("placeholder.step_path").to_string(), window, cx);
        set_placeholder(&self.headers, t!("placeholder.headers").to_string(), window, cx);
        set_placeholder(&self.body, t!("placeholder.body").to_string(), window, cx);
        set_placeholder(&self.checks, t!("placeholder.checks").to_string(), window, cx);
        set_placeholder(&self.extract, t!("placeholder.extract").to_string(), window, cx);
        set_placeholder(&self.p95, t!("placeholder.step_p95").to_string(), window, cx);
        set_placeholder(
            &self.soap_action,
            t!("placeholder.soap_action").to_string(),
            window,
            cx,
        );
    }

    pub fn to_step(&self, index: usize, cx: &App) -> Step {
        let name = value(&self.name, cx);
        let name = if name.trim().is_empty() {
            format!("etape-{}", index + 1)
        } else {
            name
        };
        Step {
            name,
            step_type: self.step_type,
            method: self.method.to_uppercase(),
            path: value(&self.path, cx),
            headers: parse_headers(&value(&self.headers, cx)),
            body: value(&self.body, cx),
            expected_status: value(&self.expected_status, cx),
            p95: parse_number(&value(&self.p95, cx)),
            checks: parse_checks(&value(&self.checks, cx)),
            extract: parse_extracts(&value(&self.extract, cx)),
            stop_on_failure: self.stop_on_failure,
            soap_version: self.soap_version.clone(),
            soap_action: value(&self.soap_action, cx),
            extra: self.extra.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Scénario
// ---------------------------------------------------------------------------

pub struct ScenarioForm {
    pub id: String,
    pub name: Entity<InputState>,
    pub folder: Entity<InputState>,
    pub base_url: Entity<InputState>,
    pub description: Entity<InputState>,
    pub think_time: Entity<InputState>,
    pub variables: Entity<InputState>,
    pub curl: Entity<InputState>,
    pub p95: Entity<InputState>,
    pub p99: Entity<InputState>,
    pub error_rate: Entity<InputState>,
    pub checks_rate: Entity<InputState>,
    pub insecure_tls: bool,
    pub dangerous: bool,
    pub auth_id: String,
    pub steps: Vec<StepForm>,
    pub selected_step: usize,
    pub extra: Extra,
}

impl ScenarioForm {
    pub fn new<T: 'static>(test: &Test, window: &mut Window, cx: &mut Context<T>) -> Self {
        let steps = test
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| StepForm::new(step, index, window, cx))
            .collect();
        Self {
            id: test.id.clone(),
            name: input(window, cx, &test.name, &t!("placeholder.scenario_name")),
            folder: input(window, cx, &test.folder, &t!("placeholder.folder")),
            base_url: input(window, cx, &test.base_url, &t!("placeholder.base_url")),
            description: textarea(
                window,
                cx,
                &test.description,
                &t!("placeholder.description"),
                2,
            ),
            think_time: input(
                window,
                cx,
                &format_number(Some(test.think_time)),
                &t!("placeholder.think_time"),
            ),
            variables: textarea(
                window,
                cx,
                &variables_to_text(&test.variables),
                &t!("placeholder.variables"),
                2,
            ),
            curl: textarea(window, cx, "", &t!("placeholder.curl"), 3),
            p95: input(
                window,
                cx,
                &format_number(test.thresholds.p95),
                &t!("placeholder.threshold_p95"),
            ),
            p99: input(
                window,
                cx,
                &format_number(test.thresholds.p99),
                &t!("placeholder.threshold_p99"),
            ),
            error_rate: input(
                window,
                cx,
                &format_percent(test.thresholds.error_rate),
                &t!("placeholder.threshold_errors"),
            ),
            checks_rate: input(
                window,
                cx,
                &format_percent(test.thresholds.checks_rate),
                &t!("placeholder.threshold_checks"),
            ),
            insecure_tls: test.insecure_tls,
            dangerous: test.dangerous,
            auth_id: test.auth_id.clone(),
            steps,
            selected_step: 0,
            extra: test.extra.clone(),
        }
    }

    pub fn selected_step(&self) -> Option<&StepForm> {
        self.steps.get(self.selected_step)
    }

    /// Réapplique les textes indicatifs dans la langue courante.
    pub fn refresh_placeholders(&self, window: &mut Window, cx: &mut App) {
        set_placeholder(&self.name, t!("placeholder.scenario_name").to_string(), window, cx);
        set_placeholder(&self.folder, t!("placeholder.folder").to_string(), window, cx);
        set_placeholder(&self.base_url, t!("placeholder.base_url").to_string(), window, cx);
        set_placeholder(
            &self.description,
            t!("placeholder.description").to_string(),
            window,
            cx,
        );
        set_placeholder(
            &self.think_time,
            t!("placeholder.think_time").to_string(),
            window,
            cx,
        );
        set_placeholder(
            &self.variables,
            t!("placeholder.variables").to_string(),
            window,
            cx,
        );
        set_placeholder(&self.curl, t!("placeholder.curl").to_string(), window, cx);
        set_placeholder(
            &self.p95,
            t!("placeholder.threshold_p95").to_string(),
            window,
            cx,
        );
        set_placeholder(
            &self.p99,
            t!("placeholder.threshold_p99").to_string(),
            window,
            cx,
        );
        set_placeholder(
            &self.error_rate,
            t!("placeholder.threshold_errors").to_string(),
            window,
            cx,
        );
        set_placeholder(
            &self.checks_rate,
            t!("placeholder.threshold_checks").to_string(),
            window,
            cx,
        );
        for step in &self.steps {
            step.refresh_placeholders(window, cx);
        }
    }

    pub fn draft(&self, cx: &App) -> Test {
        Test {
            id: self.id.clone(),
            name: value(&self.name, cx),
            description: value(&self.description, cx),
            folder: lt_core::folders::norm_folder(&value(&self.folder, cx)),
            base_url: value(&self.base_url, cx),
            auth_id: self.auth_id.clone(),
            dangerous: self.dangerous,
            think_time: parse_number(&value(&self.think_time, cx)).unwrap_or(1.0),
            variables: parse_variables(&value(&self.variables, cx)),
            thresholds: Thresholds {
                p95: parse_number(&value(&self.p95, cx)),
                p99: parse_number(&value(&self.p99, cx)),
                error_rate: parse_percent(&value(&self.error_rate, cx)),
                checks_rate: parse_percent(&value(&self.checks_rate, cx)),
                extra: Extra::new(),
            },
            steps: self
                .steps
                .iter()
                .enumerate()
                .map(|(index, step)| step.to_step(index, cx))
                .collect(),
            insecure_tls: self.insecure_tls,
            extra: self.extra.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Authentification
// ---------------------------------------------------------------------------

pub struct AuthForm {
    pub id: String,
    pub name: Entity<InputState>,
    pub token_url: Entity<InputState>,
    pub client_basic: Entity<InputState>,
    pub client_id: Entity<InputState>,
    pub client_secret: Entity<InputState>,
    pub username: Entity<InputState>,
    pub password: Entity<InputState>,
    pub token: Entity<InputState>,
    pub api_key: Entity<InputState>,
    pub header_name: Entity<InputState>,
    pub auth_type: AuthType,
    pub grant: String,
    pub extra: Extra,
}

impl AuthForm {
    pub fn new<T: 'static>(auth: &Auth, window: &mut Window, cx: &mut Context<T>) -> Self {
        let display = auth.masked();
        Self {
            id: auth.id.clone(),
            name: input(window, cx, &auth.name, &t!("placeholder.auth_name")),
            token_url: input(window, cx, &auth.token_url, &t!("placeholder.token_url")),
            client_basic: secret_input(
                window,
                cx,
                &display.client_basic,
                &t!("placeholder.client_basic"),
            ),
            client_id: input(window, cx, &auth.client_id, "client_id"),
            client_secret: secret_input(window, cx, &display.client_secret, "client_secret"),
            username: input(window, cx, &auth.username, &t!("placeholder.username")),
            password: secret_input(window, cx, &display.password, &t!("placeholder.password")),
            token: secret_input(window, cx, &display.token, &t!("placeholder.bearer_token")),
            api_key: secret_input(window, cx, &display.api_key, &t!("placeholder.api_key")),
            header_name: input(window, cx, &auth.header_name, "apikey"),
            auth_type: auth.auth_type,
            grant: if auth.grant.is_empty() {
                "client_credentials".to_string()
            } else {
                auth.grant.clone()
            },
            extra: auth.extra.clone(),
        }
    }

    /// Réapplique les textes indicatifs dans la langue courante.
    pub fn refresh_placeholders(&self, window: &mut Window, cx: &mut App) {
        set_placeholder(&self.name, t!("placeholder.auth_name").to_string(), window, cx);
        set_placeholder(&self.token_url, t!("placeholder.token_url").to_string(), window, cx);
        set_placeholder(
            &self.client_basic,
            t!("placeholder.client_basic").to_string(),
            window,
            cx,
        );
        set_placeholder(&self.username, t!("placeholder.username").to_string(), window, cx);
        set_placeholder(&self.password, t!("placeholder.password").to_string(), window, cx);
        set_placeholder(
            &self.token,
            t!("placeholder.bearer_token").to_string(),
            window,
            cx,
        );
        set_placeholder(&self.api_key, t!("placeholder.api_key").to_string(), window, cx);
    }

    pub fn draft(&self, cx: &App, previous: Option<&Auth>) -> Auth {
        let auth = Auth {
            id: self.id.clone(),
            name: value(&self.name, cx),
            auth_type: self.auth_type,
            token_url: value(&self.token_url, cx),
            grant: self.grant.clone(),
            client_basic: value(&self.client_basic, cx),
            client_id: value(&self.client_id, cx),
            client_secret: value(&self.client_secret, cx),
            username: value(&self.username, cx),
            password: value(&self.password, cx),
            token: value(&self.token, cx),
            api_key: value(&self.api_key, cx),
            header_name: value(&self.header_name, cx),
            extra: self.extra.clone(),
        };
        auth.merge_secrets(previous)
    }
}
