//! Exécution ponctuelle d'une étape (« Tester la requête »).

use crate::auth::AuthProvider;
use crate::http::{self, CheckEvaluation, PreparedRequest, StepResponse};
use crate::EngineError;
use lt_core::{Auth, Test};
use rust_i18n::t;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct TryResult {
    pub request: PreparedRequest,
    pub response: StepResponse,
    pub checks: Vec<CheckEvaluation>,
    pub ok: bool,
}

impl TryResult {
    /// En-têtes de la requête avec les secrets masqués.
    pub fn display_headers(&self) -> Vec<(String, String)> {
        self.request
            .headers
            .iter()
            .map(|(key, value)| {
                let sensitive = key.eq_ignore_ascii_case("authorization")
                    || key.to_lowercase().contains("apikey")
                    || key.eq_ignore_ascii_case("x-api-key");
                if sensitive {
                    (key.clone(), "••••••".to_string())
                } else {
                    (key.clone(), value.clone())
                }
            })
            .collect()
    }
}

pub async fn try_request(
    test: &Test,
    step_index: usize,
    auth: Option<&Auth>,
) -> Result<TryResult, EngineError> {
    let step = test
        .steps
        .get(step_index)
        .ok_or_else(|| EngineError::Message(t!("engine.try.step_missing").to_string()))?;

    let client = http::build_client(test.insecure_tls, Duration::from_secs(60))?;
    let provider = AuthProvider::prepare(auth, test.insecure_tls)
        .await?
        .map(std::sync::Arc::new);
    let vars = test.placeholder_vars(1, 0);
    let request = http::prepare(
        test,
        step,
        step_index,
        &vars,
        1,
        0,
        provider.as_deref().map(|provider| &**provider),
    );
    let response = http::execute(&client, &request).await;
    let checks = http::evaluate(step, &response);
    let ok = checks.iter().all(|check| check.ok) && response.error.is_none();
    Ok(TryResult {
        request,
        response,
        checks,
        ok,
    })
}
