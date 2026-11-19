//! Domaine de l'application de tests de charge : modèles, stockage JSON,
//! profils, checks, SOAP/XML, import curl et rapports.

rust_i18n::i18n!("locales", fallback = "en");

pub mod checks;
pub mod curl;
pub mod folders;
pub mod i18n;
pub mod models;
pub mod profiles;
pub mod report;
pub(crate) mod report_pdf;
pub mod storage;
pub mod summary;
pub mod template;
pub mod xml;

pub use models::*;
pub use storage::Workspace;
