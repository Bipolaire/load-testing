//! Authentification des requêtes (Bearer, clé d'API, OAuth2).

use crate::EngineError;
use lt_core::Auth;
use rust_i18n::t;
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio_util::sync::CancellationToken;

const REFRESH_MARGIN_MS: i64 = 60_000;

#[derive(Clone, Debug)]
struct TokenState {
    access_token: String,
    expires_at_ms: i64,
}

pub struct AuthProvider {
    auth: Auth,
    token: RwLock<Option<TokenState>>,
    client: reqwest::Client,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

impl AuthProvider {
    pub async fn prepare(
        auth: Option<&Auth>,
        insecure_tls: bool,
    ) -> Result<Option<Arc<Self>>, EngineError> {
        let Some(auth) = auth else {
            return Ok(None);
        };
        if auth.auth_type == lt_core::AuthType::None {
            return Ok(None);
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .danger_accept_invalid_certs(insecure_tls)
            .build()?;
        let provider = Arc::new(Self {
            auth: auth.clone(),
            token: RwLock::new(None),
            client,
        });
        if provider.auth.auth_type == lt_core::AuthType::OAuth2 {
            let token = provider.fetch_token().await?;
            *provider.token.write() = Some(token);
        }
        Ok(Some(provider))
    }

    pub fn headers(&self) -> Vec<(String, String)> {
        match self.auth.auth_type {
            lt_core::AuthType::None => Vec::new(),
            lt_core::AuthType::Bearer => {
                if self.auth.token.is_empty() {
                    Vec::new()
                } else {
                    vec![(
                        "Authorization".to_string(),
                        format!("Bearer {}", self.auth.token),
                    )]
                }
            }
            lt_core::AuthType::Apikey => {
                if self.auth.api_key.is_empty() {
                    Vec::new()
                } else {
                    let header = if self.auth.header_name.is_empty() {
                        "apikey"
                    } else {
                        &self.auth.header_name
                    };
                    vec![(header.to_string(), self.auth.api_key.clone())]
                }
            }
            lt_core::AuthType::OAuth2 => self
                .token
                .read()
                .as_ref()
                .map(|token| {
                    vec![(
                        "Authorization".to_string(),
                        format!("Bearer {}", token.access_token),
                    )]
                })
                .unwrap_or_default(),
        }
    }

    async fn fetch_token(&self) -> Result<TokenState, EngineError> {
        let basic = self.auth.basic_credential();
        if basic.is_empty() {
            return Err(EngineError::Message(
                t!("engine.auth.missing_basic").to_string(),
            ));
        }
        let mut form: Vec<(&str, String)> = vec![(
            "grant_type",
            if self.auth.grant.is_empty() {
                "client_credentials".to_string()
            } else {
                self.auth.grant.clone()
            },
        )];
        if self.auth.grant == "password" {
            form.push(("username", self.auth.username.clone()));
            form.push(("password", self.auth.password.clone()));
        }
        let response = self
            .client
            .post(&self.auth.token_url)
            .header("Authorization", format!("Basic {basic}"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&form)
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(EngineError::Message(
                t!(
                    "engine.auth.token_failed",
                    status = status,
                    body = truncate(&text, 300)
                )
                .to_string(),
            ));
        }
        let value: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
            EngineError::Message(
                t!("engine.auth.invalid_json", error = error).to_string(),
            )
        })?;
        let token = value
            .get("access_token")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                EngineError::Message(
                    t!(
                        "engine.auth.no_access_token",
                        body = truncate(&text, 300)
                    )
                    .to_string(),
                )
            })?;
        let expires_in = value
            .get("expires_in")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(3600);
        Ok(TokenState {
            access_token: token.to_string(),
            expires_at_ms: now_ms() + expires_in * 1000,
        })
    }

    /// Renouvelle le jeton en arrière-plan avant son expiration.
    pub fn start_refresh(
        self: &Arc<Self>,
        cancel: CancellationToken,
        logger: crate::engine::Logger,
    ) {
        if self.auth.auth_type != lt_core::AuthType::OAuth2 {
            return;
        }
        let provider = Arc::clone(self);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(15));
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = ticker.tick() => {
                        let needs_refresh = provider
                            .token
                            .read()
                            .as_ref()
                            .map(|token| now_ms() > token.expires_at_ms - REFRESH_MARGIN_MS)
                            .unwrap_or(true);
                        if needs_refresh {
                            match provider.fetch_token().await {
                                Ok(token) => {
                                    *provider.token.write() = Some(token);
                                    logger.log(t!("engine.auth.token_refreshed").to_string());
                                }
                                Err(error) => {
                                    logger.log(
                                        t!("engine.auth.refresh_failed", error = error).to_string(),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    pub fn description(&self) -> String {
        match self.auth.auth_type {
            lt_core::AuthType::OAuth2 => format!("OAuth2 ({})", self.auth.grant),
            lt_core::AuthType::Bearer => t!("engine.auth.desc.bearer").to_string(),
            lt_core::AuthType::Apikey => {
                let header = if self.auth.header_name.is_empty() {
                    "apikey"
                } else {
                    self.auth.header_name.as_str()
                };
                t!("engine.auth.desc.apikey", header = header).to_string()
            }
            lt_core::AuthType::None => t!("engine.auth.desc.none").to_string(),
        }
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        text.chars().take(max).collect()
    }
}
