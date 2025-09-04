//! Client modules for external services

pub mod baserow;
pub mod zoho;
pub mod dossier;
pub mod letterexpress;
pub mod letter_service;
pub mod pdf;
pub mod nango;
pub mod telegram;

// Re-export all client types
pub use baserow::BaserowClient;
pub use zoho::ZohoClient;
pub use dossier::{DossierClient, DossierResult};
pub use letterexpress::LetterExpressClient;
pub use letter_service::LetterServiceClient;
pub use pdf::PDFService;
pub use nango::NangoClient;
pub use telegram::TelegramClient;