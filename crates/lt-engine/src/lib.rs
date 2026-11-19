//! Moteur de test de charge natif (Tokio + reqwest).
//!
//! Le moteur exécute un scénario selon un profil de charge, collecte les
//! métriques, écrit les rapports et met à jour l'historique du workspace.

rust_i18n::i18n!("locales", fallback = "en");

pub mod auth;
pub mod engine;
pub mod http;
pub mod metrics;
pub mod try_run;

pub use engine::{Logger, RunConfig, RunEvent, execute};
pub use try_run::{TryResult, try_request};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
