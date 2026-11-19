//! Construction et exécution des requêtes HTTP/SOAP.

use crate::EngineError;
use crate::auth::AuthProvider;
use lt_core::checks::{eval_check, get_path};
use lt_core::{Auth, CheckOp, Step, Test};
use rust_i18n::t;
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct PreparedRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StepResponse {
    pub status: Option<u16>,
    pub duration_ms: f64,
    pub body_text: String,
    pub json: Option<Value>,
    pub is_xml: bool,
    pub content_type: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CheckEvaluation {
    pub label: String,
    pub ok: bool,
    pub actual: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct StepExecution {
    pub response: StepResponse,
    pub checks: Vec<CheckEvaluation>,
    pub ok: bool,
}

impl StepExecution {
    pub fn json(&self) -> Option<&Value> {
        self.response.json.as_ref()
    }
}

pub fn build_client(insecure_tls: bool, timeout: Duration) -> Result<reqwest::Client, EngineError> {
    Ok(reqwest::Client::builder()
        .timeout(timeout)
        .cookie_store(true)
        .danger_accept_invalid_certs(insecure_tls)
        .build()?)
}

fn set_header(headers: &mut Vec<(String, String)>, key: &str, value: String) {
    if let Some(existing) = headers
        .iter_mut()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
    {
        existing.1 = value;
    } else {
        headers.push((key.to_string(), value));
    }
}

fn has_header(headers: &[(String, String)], key: &str) -> bool {
    headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case(key))
}

pub fn prepare(
    test: &Test,
    step: &Step,
    index: usize,
    vars: &HashMap<String, String>,
    vu: u64,
    iter: u64,
    auth: Option<&AuthProvider>,
) -> PreparedRequest {
    let soap = step.is_soap();
    let mut headers: Vec<(String, String)> = if soap {
        lt_core::xml::soap_headers(
            &step.soap_version,
            &lt_core::template::render(&step.soap_action, vars, vu, iter),
        )
    } else {
        vec![("Accept".to_string(), "application/json".to_string())]
    };
    for header in &step.headers {
        if header.key.trim().is_empty() {
            continue;
        }
        set_header(
            &mut headers,
            header.key.trim(),
            lt_core::template::render(&header.value, vars, vu, iter),
        );
    }
    if let Some(auth) = auth {
        for (key, value) in auth.headers() {
            set_header(&mut headers, &key, value);
        }
    }

    let method = step.effective_method();
    let body = if matches!(method.as_str(), "GET" | "HEAD") || step.body.is_empty() {
        None
    } else {
        Some(lt_core::template::render(&step.body, vars, vu, iter))
    };
    if body.is_some() && !has_header(&headers, "content-type") {
        set_header(&mut headers, "Content-Type", "application/json".to_string());
    }

    let path = lt_core::template::render(&step.path, vars, vu, iter);
    let url = if path.starts_with("http://") || path.starts_with("https://") {
        path
    } else {
        format!("{}{}", test.base_url.trim_end_matches('/'), path)
    };

    let _ = index;
    PreparedRequest {
        method,
        url,
        headers,
        body,
    }
}

fn op_symbol(op: CheckOp) -> &'static str {
    match op {
        CheckOp::Equals => "=",
        CheckOp::NotEquals => "≠",
        CheckOp::Contains => "contient",
        CheckOp::Exists => "existe",
        CheckOp::NotEmpty => "non vide",
        CheckOp::IsArray => "est une liste",
        CheckOp::IsNumber => "est un nombre",
        CheckOp::LessThan => "<",
        CheckOp::GreaterThan => ">",
    }
}

pub fn check_labels(step: &Step, index: usize) -> Vec<String> {
    let tag = step.effective_name(index);
    let expected = step.expected_statuses();
    let expected_label = if expected.is_empty() {
        "200".to_string()
    } else {
        expected
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join("/")
    };
    let mut labels = vec![format!("{tag} : HTTP {expected_label}")];
    if step.is_soap() {
        labels.push(t!("engine.http.no_soap_fault", tag = tag).to_string());
    }
    for check in &step.checks {
        if check.path.trim().is_empty() && check.op != CheckOp::Exists {
            continue;
        }
        let suffix = if check.op.needs_value() {
            format!(" {} {}", op_symbol(check.op), check.value)
        } else {
            format!(" {}", op_symbol(check.op))
        };
        labels.push(format!("{tag} : {}{suffix}", check.path));
    }
    labels
}

pub async fn execute(
    client: &reqwest::Client,
    prepared: &PreparedRequest,
) -> StepResponse {
    let method = match reqwest::Method::from_bytes(prepared.method.as_bytes()) {
        Ok(method) => method,
        Err(_) => {
            return StepResponse {
                status: None,
                duration_ms: 0.0,
                body_text: String::new(),
                json: None,
                is_xml: false,
                content_type: String::new(),
                error: Some(
                    t!("engine.http.invalid_method", method = prepared.method).to_string(),
                ),
            };
        }
    };
    let mut headers = reqwest::header::HeaderMap::new();
    for (key, value) in &prepared.headers {
        let name = match reqwest::header::HeaderName::from_bytes(key.as_bytes()) {
            Ok(name) => name,
            Err(_) => continue,
        };
        let value = match reqwest::header::HeaderValue::from_str(value) {
            Ok(value) => value,
            Err(_) => continue,
        };
        headers.insert(name, value);
    }

    let started = Instant::now();
    let mut request = client.request(method, &prepared.url).headers(headers);
    if let Some(body) = &prepared.body {
        request = request.body(body.clone());
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            return StepResponse {
                status: None,
                duration_ms: started.elapsed().as_secs_f64() * 1000.0,
                body_text: String::new(),
                json: None,
                is_xml: false,
                content_type: String::new(),
                error: Some(error.to_string()),
            };
        }
    };
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = response.bytes().await;
    let duration_ms = started.elapsed().as_secs_f64() * 1000.0;
    let body_text = match body {
        Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
        Err(error) => {
            return StepResponse {
                status: Some(status),
                duration_ms,
                body_text: String::new(),
                json: None,
                is_xml: false,
                content_type,
                error: Some(error.to_string()),
            };
        }
    };

    let mut json = None;
    let mut is_xml = false;
    let looks_xml = content_type.to_lowercase().contains("xml");
    if looks_xml && !body_text.trim().is_empty()
        && let Ok(parsed) = lt_core::xml::parse_xml(&body_text) {
            json = Some(parsed);
            is_xml = true;
        }
    if json.is_none() && !body_text.trim().is_empty()
        && let Ok(parsed) = serde_json::from_str::<Value>(&body_text) {
            json = Some(parsed);
        }

    StepResponse {
        status: Some(status),
        duration_ms,
        body_text,
        json,
        is_xml,
        content_type,
        error: None,
    }
}

pub fn evaluate(step: &Step, response: &StepResponse) -> Vec<CheckEvaluation> {
    let expected = step.expected_statuses();
    let expected = if expected.is_empty() {
        vec![200]
    } else {
        expected
    };
    let status_ok = response
        .status
        .map(|status| expected.contains(&status))
        .unwrap_or(false);
    let mut checks = vec![CheckEvaluation {
        label: format!("HTTP {}", expected.iter().map(u16::to_string).collect::<Vec<_>>().join("/")),
        ok: status_ok,
        actual: response.status.map(Value::from),
    }];
    if step.is_soap() {
        let fault = response.json.as_ref().and_then(lt_core::xml::soap_fault);
        checks.push(CheckEvaluation {
            label: t!("engine.http.no_soap_fault_label").to_string(),
            ok: response.json.is_some() && fault.is_none(),
            actual: fault.map(Value::String),
        });
    }
    for check in &step.checks {
        if check.path.trim().is_empty() && check.op != CheckOp::Exists {
            continue;
        }
        let actual = response
            .json
            .as_ref()
            .and_then(|json| get_path(json, &check.path))
            .cloned();
        let ok = response
            .json
            .as_ref()
            .map(|json| eval_check(check, json))
            .unwrap_or(false);
        checks.push(CheckEvaluation {
            label: check.label(),
            ok,
            actual,
        });
    }
    checks
}

/// Exécute une étape : préparation (gabarit, auth, URL), appel HTTP puis checks.
#[allow(clippy::too_many_arguments)]
pub async fn execute_step(
    client: &reqwest::Client,
    test: &Test,
    step: &Step,
    index: usize,
    vars: &HashMap<String, String>,
    vu: u64,
    iter: u64,
    auth: Option<&AuthProvider>,
) -> StepExecution {
    let prepared = prepare(test, step, index, vars, vu, iter, auth);
    let response = execute(client, &prepared).await;
    let checks = evaluate(step, &response);
    let ok = checks.iter().all(|check| check.ok);
    StepExecution {
        response,
        checks,
        ok,
    }
}

pub fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(max).collect();
        out.push('…');
        out
    }
}

/// Construit les en-têtes affichés dans l'essai de requête, sans les secrets.
pub fn redacted_headers(auth: Option<&Auth>, prepared: &PreparedRequest) -> Vec<(String, String)> {
    let _ = auth;
    prepared
        .headers
        .iter()
        .map(|(key, value)| {
            let sensitive = key.eq_ignore_ascii_case("authorization")
                || key.eq_ignore_ascii_case("apikey");
            if sensitive {
                (key.clone(), "••••••".to_string())
            } else {
                (key.clone(), value.clone())
            }
        })
        .collect()
}
