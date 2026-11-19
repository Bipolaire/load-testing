//! Modèles sérialisés dans le workspace. Les noms de champs JSON sont ceux de
//! l'ancienne plateforme (`camelCase`) afin de pouvoir importer un `data/` existant.

use rust_i18n::t;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

fn default_think_time() -> f64 {
    1.0
}

fn default_expected_status() -> String {
    "200".to_string()
}

fn default_soap_version() -> String {
    "1.1".to_string()
}

fn default_grant() -> String {
    "client_credentials".to_string()
}

fn default_header_name() -> String {
    "apikey".to_string()
}

/// Champs inconnus conservés tels quels (compatibilité ascendante).
pub type Extra = Map<String, Value>;

// ---------------------------------------------------------------------------
// Scénarios
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum StepType {
    #[default]
    Rest,
    Soap,
}

impl StepType {
    pub fn as_str(self) -> &'static str {
        match self {
            StepType::Rest => "rest",
            StepType::Soap => "soap",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum CheckOp {
    #[default]
    Equals,
    NotEquals,
    Contains,
    Exists,
    NotEmpty,
    IsArray,
    IsNumber,
    LessThan,
    GreaterThan,
}

impl CheckOp {
    pub fn as_str(self) -> &'static str {
        match self {
            CheckOp::Equals => "equals",
            CheckOp::NotEquals => "notEquals",
            CheckOp::Contains => "contains",
            CheckOp::Exists => "exists",
            CheckOp::NotEmpty => "notEmpty",
            CheckOp::IsArray => "isArray",
            CheckOp::IsNumber => "isNumber",
            CheckOp::LessThan => "lessThan",
            CheckOp::GreaterThan => "greaterThan",
        }
    }

    pub fn needs_value(self) -> bool {
        !matches!(
            self,
            CheckOp::Exists | CheckOp::NotEmpty | CheckOp::IsArray | CheckOp::IsNumber
        )
    }

    /// Analyse une opération écrite en anglais (`equals`, `notEquals`, …) ou
    /// avec un symbole (`=`, `!=`, `>`, …).
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "=" | "==" => return Some(CheckOp::Equals),
            "!=" | "<>" => return Some(CheckOp::NotEquals),
            "<" => return Some(CheckOp::LessThan),
            ">" => return Some(CheckOp::GreaterThan),
            "~" => return Some(CheckOp::Contains),
            _ => {}
        }
        let normalized: String = value
            .trim()
            .to_lowercase()
            .chars()
            .filter(|character| character.is_alphanumeric())
            .collect();
        Some(match normalized.as_str() {
            "equals" | "equal" | "egal" | "estegal" => CheckOp::Equals,
            "notequals" | "different" | "nestpasegal" => CheckOp::NotEquals,
            "contains" | "contient" => CheckOp::Contains,
            "exists" | "existe" => CheckOp::Exists,
            "notempty" | "nonvide" => CheckOp::NotEmpty,
            "isarray" | "estuneliste" | "liste" => CheckOp::IsArray,
            "isnumber" | "estunnombre" | "nombre" => CheckOp::IsNumber,
            "lessthan" | "inferieur" | "inferieura" => CheckOp::LessThan,
            "greaterthan" | "superieur" | "superieura" => CheckOp::GreaterThan,
            _ => return None,
        })
    }

    pub fn label(self) -> String {
        match self {
            CheckOp::Equals => t!("check.op.equals"),
            CheckOp::NotEquals => t!("check.op.not_equals"),
            CheckOp::Contains => t!("check.op.contains"),
            CheckOp::Exists => t!("check.op.exists"),
            CheckOp::NotEmpty => t!("check.op.not_empty"),
            CheckOp::IsArray => t!("check.op.is_array"),
            CheckOp::IsNumber => t!("check.op.is_number"),
            CheckOp::LessThan => t!("check.op.less_than"),
            CheckOp::GreaterThan => t!("check.op.greater_than"),
        }
        .to_string()
    }

    pub const ALL: [CheckOp; 9] = [
        CheckOp::Equals,
        CheckOp::NotEquals,
        CheckOp::Contains,
        CheckOp::Exists,
        CheckOp::NotEmpty,
        CheckOp::IsArray,
        CheckOp::IsNumber,
        CheckOp::LessThan,
        CheckOp::GreaterThan,
    ];
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Header {
    pub key: String,
    pub value: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Check {
    pub path: String,
    pub op: CheckOp,
    pub value: String,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Check {
    pub fn label(&self) -> String {
        if self.op.needs_value() {
            format!("{} {}", self.path, self.op.label())
        } else {
            format!("{} {} {}", self.path, self.op.label(), self.value)
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Extract {
    pub name: String,
    pub path: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Variable {
    pub name: String,
    pub values: Vec<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Thresholds {
    pub p95: Option<f64>,
    pub p99: Option<f64>,
    pub error_rate: Option<f64>,
    pub checks_rate: Option<f64>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            p95: Some(1000.0),
            p99: Some(2000.0),
            error_rate: Some(0.01),
            checks_rate: Some(0.99),
            extra: Extra::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Step {
    pub name: String,
    #[serde(rename = "type")]
    pub step_type: StepType,
    pub method: String,
    pub path: String,
    pub headers: Vec<Header>,
    pub body: String,
    pub expected_status: String,
    pub p95: Option<f64>,
    pub checks: Vec<Check>,
    pub extract: Vec<Extract>,
    pub stop_on_failure: bool,
    pub soap_version: String,
    pub soap_action: String,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for Step {
    fn default() -> Self {
        Self {
            name: String::new(),
            step_type: StepType::Rest,
            method: "GET".to_string(),
            path: "/".to_string(),
            headers: Vec::new(),
            body: String::new(),
            expected_status: default_expected_status(),
            p95: None,
            checks: Vec::new(),
            extract: Vec::new(),
            stop_on_failure: false,
            soap_version: default_soap_version(),
            soap_action: String::new(),
            extra: Extra::new(),
        }
    }
}

impl Step {
    pub fn is_soap(&self) -> bool {
        self.step_type == StepType::Soap
    }

    pub fn effective_method(&self) -> String {
        if self.is_soap() {
            "POST".to_string()
        } else {
            self.method.to_uppercase()
        }
    }

    pub fn effective_name(&self, index: usize) -> String {
        if self.name.trim().is_empty() {
            format!("etape-{}", index + 1)
        } else {
            self.name.clone()
        }
    }

    pub fn label(&self) -> String {
        if self.is_soap() {
            let op = self
                .soap_action
                .split(['/', '#']).rfind(|part| !part.is_empty())
                .unwrap_or(&self.path);
            format!("SOAP {op}")
        } else {
            format!("{} {}", self.method, self.path)
        }
    }

    pub fn expected_statuses(&self) -> Vec<u16> {
        self.expected_status
            .split(',')
            .filter_map(|s| s.trim().parse::<u16>().ok())
            .collect()
    }

    /// Modèle d'enveloppe SOAP proposé par l'éditeur.
    pub fn soap_template(&self) -> String {
        crate::xml::soap_template(&self.soap_version)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Test {
    pub id: String,
    pub name: String,
    pub description: String,
    pub folder: String,
    pub base_url: String,
    pub auth_id: String,
    pub dangerous: bool,
    #[serde(default = "default_think_time")]
    pub think_time: f64,
    pub variables: Vec<Variable>,
    pub thresholds: Thresholds,
    pub steps: Vec<Step>,
    pub insecure_tls: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for Test {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            description: String::new(),
            folder: String::new(),
            base_url: "https://".to_string(),
            auth_id: String::new(),
            dangerous: false,
            think_time: default_think_time(),
            variables: Vec::new(),
            thresholds: Thresholds::default(),
            steps: Vec::new(),
            insecure_tls: false,
            extra: Extra::new(),
        }
    }
}

impl Test {
    pub fn new_empty() -> Self {
        Self {
            id: String::new(),
            name: t!("scenario.default.name").to_string(),
            steps: vec![Step::default()],
            ..Self::default()
        }
    }

    pub fn demo() -> Self {
        Self {
            id: "demo-quickpizza".to_string(),
            name: t!("scenario.demo.name").to_string(),
            description: t!("scenario.demo.description").to_string(),
            base_url: "https://quickpizza.grafana.com".to_string(),
            steps: vec![Step {
                name: "home".to_string(),
                path: "/".to_string(),
                ..Step::default()
            }],
            ..Self::default()
        }
    }

    pub fn placeholder_vars(&self, vu: u64, iter: u64) -> std::collections::HashMap<String, String> {
        let mut vars = std::collections::HashMap::new();
        for variable in &self.variables {
            if !variable.values.is_empty() {
                let index = ((vu + iter) as usize) % variable.values.len();
                vars.insert(variable.name.clone(), variable.values[index].clone());
            }
        }
        vars
    }
}

// ---------------------------------------------------------------------------
// Authentifications
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum AuthType {
    #[default]
    None,
    OAuth2,
    Bearer,
    Apikey,
}

impl AuthType {
    pub fn as_str(self) -> &'static str {
        match self {
            AuthType::None => "none",
            AuthType::OAuth2 => "oauth2",
            AuthType::Bearer => "bearer",
            AuthType::Apikey => "apikey",
        }
    }

    pub fn label(self) -> String {
        match self {
            AuthType::None => t!("auth.type.none"),
            AuthType::OAuth2 => t!("auth.type.oauth2"),
            AuthType::Bearer => t!("auth.type.bearer"),
            AuthType::Apikey => t!("auth.type.apikey"),
        }
        .to_string()
    }

    pub const ALL: [AuthType; 4] =
        [AuthType::None, AuthType::OAuth2, AuthType::Bearer, AuthType::Apikey];
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Auth {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub auth_type: AuthType,
    pub token_url: String,
    #[serde(default = "default_grant")]
    pub grant: String,
    pub client_basic: String,
    pub client_id: String,
    pub client_secret: String,
    pub username: String,
    pub password: String,
    pub token: String,
    pub api_key: String,
    #[serde(default = "default_header_name")]
    pub header_name: String,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for Auth {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: t!("auth.default.name").to_string(),
            auth_type: AuthType::OAuth2,
            token_url: String::new(),
            grant: default_grant(),
            client_basic: String::new(),
            client_id: String::new(),
            client_secret: String::new(),
            username: String::new(),
            password: String::new(),
            token: String::new(),
            api_key: String::new(),
            header_name: default_header_name(),
            extra: Extra::new(),
        }
    }
}

impl Auth {
    pub const MASK: &'static str = "••••••";

    pub fn secret_fields() -> &'static [&'static str] {
        &["token", "apiKey", "clientBasic", "clientSecret", "password"]
    }

    pub fn masked(&self) -> Self {
        let mut out = self.clone();
        for field in Self::secret_fields() {
            match *field {
                "token" if !out.token.is_empty() => out.token = Self::MASK.to_string(),
                "apiKey" if !out.api_key.is_empty() => out.api_key = Self::MASK.to_string(),
                "clientBasic" if !out.client_basic.is_empty() => {
                    out.client_basic = Self::MASK.to_string()
                }
                "clientSecret" if !out.client_secret.is_empty() => {
                    out.client_secret = Self::MASK.to_string()
                }
                "password" if !out.password.is_empty() => out.password = Self::MASK.to_string(),
                _ => {}
            }
        }
        out
    }

    /// Les champs masqués (non modifiés par l'utilisateur) reprennent l'ancienne valeur.
    pub fn merge_secrets(self, previous: Option<&Auth>) -> Self {
        let Some(previous) = previous else {
            return self;
        };
        let mut out = self;
        if out.token == Self::MASK {
            out.token = previous.token.clone();
        }
        if out.api_key == Self::MASK {
            out.api_key = previous.api_key.clone();
        }
        if out.client_basic == Self::MASK {
            out.client_basic = previous.client_basic.clone();
        }
        if out.client_secret == Self::MASK {
            out.client_secret = previous.client_secret.clone();
        }
        if out.password == Self::MASK {
            out.password = previous.password.clone();
        }
        out
    }

    /// Valeur encodée à placer après « Basic » pour OAuth2.
    pub fn basic_credential(&self) -> String {
        use base64::Engine;
        if !self.client_basic.is_empty() {
            return self.client_basic.clone();
        }
        if self.client_id.is_empty() && self.client_secret.is_empty() {
            return String::new();
        }
        base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", self.client_id, self.client_secret))
    }
}

// ---------------------------------------------------------------------------
// Profils et exécutions
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    #[default]
    Smoke,
    Load,
    Stress,
    Spike,
    Soak,
    Tps,
    Breakpoint,
}

impl Profile {
    pub const ALL: [Profile; 7] = [
        Profile::Smoke,
        Profile::Load,
        Profile::Stress,
        Profile::Spike,
        Profile::Soak,
        Profile::Tps,
        Profile::Breakpoint,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Profile::Smoke => "smoke",
            Profile::Load => "load",
            Profile::Stress => "stress",
            Profile::Spike => "spike",
            Profile::Soak => "soak",
            Profile::Tps => "tps",
            Profile::Breakpoint => "breakpoint",
        }
    }

    /// Nom d'icône embarquée (`icons/profile-*.svg`).
    pub fn icon_name(self) -> &'static str {
        match self {
            Profile::Smoke => "profile-smoke",
            Profile::Load => "profile-load",
            Profile::Stress => "profile-stress",
            Profile::Spike => "profile-spike",
            Profile::Soak => "profile-soak",
            Profile::Tps => "profile-tps",
            Profile::Breakpoint => "profile-breakpoint",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "smoke" => Some(Profile::Smoke),
            "load" => Some(Profile::Load),
            "stress" => Some(Profile::Stress),
            "spike" => Some(Profile::Spike),
            "soak" => Some(Profile::Soak),
            "tps" => Some(Profile::Tps),
            "breakpoint" => Some(Profile::Breakpoint),
            _ => None,
        }
    }

    pub fn label(self) -> String {
        match self {
            Profile::Smoke => t!("profile.smoke.label"),
            Profile::Load => t!("profile.load.label"),
            Profile::Stress => t!("profile.stress.label"),
            Profile::Spike => t!("profile.spike.label"),
            Profile::Soak => t!("profile.soak.label"),
            Profile::Tps => t!("profile.tps.label"),
            Profile::Breakpoint => t!("profile.breakpoint.label"),
        }
        .to_string()
    }

    pub fn description(self) -> String {
        match self {
            Profile::Smoke => t!("profile.smoke.desc"),
            Profile::Load => t!("profile.load.desc"),
            Profile::Stress => t!("profile.stress.desc"),
            Profile::Spike => t!("profile.spike.desc"),
            Profile::Soak => t!("profile.soak.desc"),
            Profile::Tps => t!("profile.tps.desc"),
            Profile::Breakpoint => t!("profile.breakpoint.desc"),
        }
        .to_string()
    }

    pub fn defaults(self) -> String {
        match self {
            Profile::Smoke => t!("profile.smoke.defaults"),
            Profile::Load => t!("profile.load.defaults"),
            Profile::Stress => t!("profile.stress.defaults"),
            Profile::Spike => t!("profile.spike.defaults"),
            Profile::Soak => t!("profile.soak.defaults"),
            Profile::Tps => t!("profile.tps.defaults"),
            Profile::Breakpoint => t!("profile.breakpoint.defaults"),
        }
        .to_string()
    }

    pub fn family(self) -> ProfileFamily {
        match self {
            Profile::Smoke | Profile::Load | Profile::Stress | Profile::Spike | Profile::Soak => {
                ProfileFamily::Vu
            }
            Profile::Tps | Profile::Breakpoint => ProfileFamily::Rate,
        }
    }

    pub fn uses_tps(self) -> bool {
        self.family() == ProfileFamily::Rate
    }

    pub fn uses_duration(self) -> bool {
        true
    }

    pub fn uses_ramps(self) -> bool {
        matches!(self, Profile::Load)
    }

    pub fn shape(self) -> &'static str {
        match self {
            // Plateaux / rampes lisibles (évite les traits plats type « — »).
            Profile::Smoke => "M2,22 L10,22 L18,16 L82,16 L90,22 L98,22",
            Profile::Load => "M2,26 L22,8 L80,8 L98,26",
            Profile::Stress => "M2,26 L16,16 L42,16 L56,4 L84,4 L98,26",
            Profile::Spike => "M2,24 L20,24 L28,4 L70,4 L78,24 L98,24",
            Profile::Soak => "M2,24 L12,6 L88,6 L98,24",
            Profile::Tps => "M2,14 L98,14 M22,8 L22,20 M50,8 L50,20 M78,8 L78,20",
            Profile::Breakpoint => "M2,26 L78,4 M78,4 L78,14 M78,4 L64,4",
        }
    }

    /// Tooltip du sélecteur de profil (description + défauts).
    pub fn tooltip(self) -> String {
        format!("{} — {}", self.description(), self.defaults())
    }

    pub fn vus_label(self) -> String {
        match self {
            Profile::Smoke => t!("profile.smoke.vus_label"),
            Profile::Load => t!("profile.load.vus_label"),
            Profile::Stress => t!("profile.stress.vus_label"),
            Profile::Spike => t!("profile.spike.vus_label"),
            Profile::Soak => t!("profile.soak.vus_label"),
            Profile::Tps => t!("profile.tps.vus_label"),
            Profile::Breakpoint => t!("profile.breakpoint.vus_label"),
        }
        .to_string()
    }

    pub fn duration_label(self) -> String {
        match self {
            Profile::Smoke => t!("profile.smoke.duration_label"),
            Profile::Load => t!("profile.load.duration_label"),
            Profile::Stress => t!("profile.stress.duration_label"),
            Profile::Spike => t!("profile.spike.duration_label"),
            Profile::Soak => t!("profile.soak.duration_label"),
            Profile::Tps => t!("profile.tps.duration_label"),
            Profile::Breakpoint => t!("profile.breakpoint.duration_label"),
        }
        .to_string()
    }

    pub fn vus_tooltip(self) -> String {
        match self {
            Profile::Load => t!("profile.tooltip.vus_load"),
            Profile::Stress | Profile::Spike => t!("profile.tooltip.vus_peak"),
            Profile::Tps | Profile::Breakpoint => t!("profile.tooltip.vus_cap"),
            Profile::Smoke => t!("profile.tooltip.vus_smoke"),
            Profile::Soak => t!("profile.tooltip.vus_soak"),
        }
        .to_string()
    }

    pub fn duration_tooltip(self) -> String {
        match self {
            Profile::Load => t!("profile.tooltip.duration_plateau"),
            Profile::Stress | Profile::Spike => t!("profile.tooltip.duration_scaled"),
            Profile::Breakpoint => t!("profile.tooltip.duration_breakpoint"),
            Profile::Tps => t!("profile.tooltip.duration_tps"),
            Profile::Smoke | Profile::Soak => t!("profile.tooltip.duration_constant"),
        }
        .to_string()
    }

    pub fn tps_tooltip() -> String {
        t!("profile.tooltip.tps").to_string()
    }

    pub fn ramp_up_tooltip() -> String {
        t!("profile.tooltip.ramp_up").to_string()
    }

    pub fn ramp_down_tooltip() -> String {
        t!("profile.tooltip.ramp_down").to_string()
    }

    pub fn iterations_tooltip() -> String {
        t!("profile.tooltip.iterations").to_string()
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProfileFamily {
    Vu,
    Rate,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct RunOptions {
    pub vus: Option<u32>,
    pub tps: Option<u32>,
    pub duration: Option<String>,
    /// Montée progressive (profil Load), ex. `60s`.
    pub ramp_up: Option<String>,
    /// Descente progressive (profil Load), ex. `30s`.
    pub ramp_down: Option<String>,
    pub iterations: Option<u64>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    #[default]
    Running,
    Passed,
    Failed,
    Error,
    Stopped,
    Interrupted,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Running => "running",
            RunStatus::Passed => "passed",
            RunStatus::Failed => "failed",
            RunStatus::Error => "error",
            RunStatus::Stopped => "stopped",
            RunStatus::Interrupted => "interrupted",
        }
    }

    pub fn label(self) -> String {
        match self {
            RunStatus::Running => t!("run.status.running"),
            RunStatus::Passed => t!("run.status.passed"),
            RunStatus::Failed => t!("run.status.failed"),
            RunStatus::Error => t!("run.status.error"),
            RunStatus::Stopped => t!("run.status.stopped"),
            RunStatus::Interrupted => t!("run.status.interrupted"),
        }
        .to_string()
    }

    pub fn is_terminal(self) -> bool {
        self != RunStatus::Running
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ThresholdResult {
    pub metric: String,
    pub expr: String,
    pub ok: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CheckSummary {
    pub name: String,
    pub passes: u64,
    pub fails: u64,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct StepSummary {
    pub name: String,
    pub count: Option<u64>,
    pub rate: Option<f64>,
    pub avg: Option<f64>,
    pub p95: Option<f64>,
    pub p99: Option<f64>,
    pub max: Option<f64>,
    pub error_rate: Option<f64>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RunSummary {
    pub requests: Option<u64>,
    pub rps: Option<f64>,
    pub iterations: Option<u64>,
    pub iterations_rate: Option<f64>,
    pub avg: Option<f64>,
    pub med: Option<f64>,
    pub p90: Option<f64>,
    pub p95: Option<f64>,
    pub p99: Option<f64>,
    pub max: Option<f64>,
    pub error_rate: Option<f64>,
    pub checks_rate: Option<f64>,
    pub vus_max: Option<f64>,
    pub dropped: Option<u64>,
    pub thresholds: Vec<ThresholdResult>,
    pub checks: Vec<CheckSummary>,
    pub steps: Vec<StepSummary>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Run {
    pub id: String,
    pub name: String,
    pub test_id: String,
    pub test_name: String,
    pub profile: Profile,
    pub options: RunOptions,
    pub status: RunStatus,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub exit_code: Option<i32>,
    pub summary: Option<RunSummary>,
    pub reports: Vec<String>,
    pub error: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Run {
    pub fn duration_ms(&self) -> Option<i64> {
        self.ended_at.map(|end| (end - self.started_at).max(0))
    }
}

/// Point de série temporelle (1 point par seconde).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SeriesPoint {
    /// Secondes écoulées depuis le début du test.
    pub t: u64,
    pub vus: f64,
    pub rps: f64,
    pub p95: f64,
    pub error_rate: f64,
    pub checks_rate: f64,
    pub requests: u64,
    pub iterations: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RunSeries {
    pub run_id: String,
    pub points: Vec<SeriesPoint>,
}
