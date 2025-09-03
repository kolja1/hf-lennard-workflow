//! Lennard Core Library
//! 
//! Consolidated business logic for the Lennard marketing automation system.
//! Contains all service clients, data processing, and workflow logic.

pub mod config;
pub mod clients;
pub mod services;
pub mod workflow;
pub mod types;
pub mod error;
pub mod paths;

// Re-export main types for easy access
pub use config::LennardConfig;
pub use error::{LennardError, Result};

// Re-export all client types
pub use clients::{
    BaserowClient,
    ZohoClient, 
    DossierClient,
    LetterExpressClient,
    PDFService,
};

// Re-export service types
pub use services::{
    AddressExtractor,
    LetterGenerator,
    WorkflowProcessor,
};

// Re-export workflow types
pub use workflow::{
    WorkflowState,
    WorkflowData,
    LegacyApprovalData,
    // New strongly typed approval system
    ApprovalQueue,
    ApprovalId,
    WorkflowId,
    TaskId,
    ContactId,
    UserId,
    TelegramMessageId,
    TelegramChatId,
    ApprovalState,
    ApprovalData,
    approval_types::LetterContent as ApprovalLetterContent,
    LetterHistoryEntry,
    Feedback,
    HealthStatus,
    HealthCheckResult,
    StateCountMap,
    WorkflowTrigger,
};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        // Basic smoke test
        assert_eq!(2 + 2, 4);
    }
}