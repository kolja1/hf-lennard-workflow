//! Shared types for the workflow engine

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Represents a workflow trigger request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTrigger {
    pub trigger_id: String,
    pub requested_by: i64,
    pub requested_at: DateTime<Utc>,
    pub max_tasks: u32,
    pub dry_run: bool,
}

impl WorkflowTrigger {
    /// Create a new workflow trigger
    pub fn new(requested_by: i64, max_tasks: u32) -> Self {
        Self {
            trigger_id: Uuid::new_v4().to_string(),
            requested_by,
            requested_at: Utc::now(),
            max_tasks,
            dry_run: false,
        }
    }
}

/// Workflow processing state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkflowStatus {
    Pending,
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

/// Workflow state information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowState {
    pub workflow_id: String,
    pub status: WorkflowStatus,
    pub task_results: Vec<TaskResult>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub error_message: Option<String>,
    pub total_tasks: u32,
    pub completed_tasks: u32,
}

/// Result of a single task processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub contact_name: String,
    pub company_name: String,
    pub success: bool,
    pub tracking_id: Option<String>,
    pub error_message: Option<String>,
    pub processed_at: DateTime<Utc>,
}

/// Approval request data
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub approval_id: String,
    pub workflow_id: String,
    pub contact_name: String,
    pub company_name: String,
    pub letter_content: String,
    pub pdf_content: Vec<u8>,
    pub pdf_filename: String,
    pub pdf_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub task_id: String,
    pub contact_id: String,
    pub tracking_info: Option<String>,
}

/// Approval decision
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ApprovalDecision {
    Approved,
    Rejected,
    NeedsRevision,
}

/// Approval response
#[derive(Debug, Clone)]
pub struct ApprovalResponse {
    pub approval_id: String,
    pub decision: ApprovalDecision,
    pub feedback: Option<String>,
    pub revised_letter_content: Option<String>,
    pub decided_at: DateTime<Utc>,
    pub decided_by: i64,
}

/// Approval state
#[derive(Debug, Clone, PartialEq)]
pub enum ApprovalState {
    Pending,
    Approved,
    Rejected,
    Expired,
    InRevision,
}

/// Mailing address
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailingAddress {
    pub street: String,
    pub city: String,
    pub state: Option<String>,
    pub postal_code: String,
    pub country: String,
}

/// Contact information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub id: String,
    pub full_name: String,
    pub first_name: String,
    pub last_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub company: Option<String>,
    pub linkedin_id: Option<String>,
    pub mailing_address: Option<MailingAddress>,
}

/// LinkedIn profile information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedInProfile {
    pub profile_id: String,
    pub profile_url: String,
    pub full_name: String,
    pub headline: Option<String>,
    pub location: Option<String>,
    pub company: Option<String>,
    pub raw_data: serde_json::Value,
}

/// Letter content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LetterContent {
    pub subject: String,
    pub greeting: String,
    pub body: String,
    pub closing: String,
    pub sender_name: String,
    pub recipient_name: String,
    pub company_name: String,
}

/// Dossier generation result
#[derive(Debug, Clone)]
pub struct DossierResult {
    pub person_dossier: String,
    pub company_dossier: String,
    pub company_name: String,
    pub mailing_address: Option<MailingAddress>,
}

/// Custom error type for workflow operations
#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("Configuration error: {0}")]
    Configuration(String),
    
    #[error("External service error: {0}")]
    ExternalService(String),
    
    #[error("Validation error: {0}")]
    Validation(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Internal error: {0}")]
    Internal(String),
    
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    
    #[error("Timeout: {0}")]
    Timeout(String),
    
    #[error("Network error: {0}")]
    Network(String),
}

pub type Result<T> = std::result::Result<T, WorkflowError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_trigger_creation() {
        let trigger = WorkflowTrigger::new(12345, 5);
        assert_eq!(trigger.requested_by, 12345);
        assert_eq!(trigger.max_tasks, 5);
        assert!(!trigger.dry_run);
        assert!(!trigger.trigger_id.is_empty());
    }

    #[test]
    fn test_workflow_status_equality() {
        assert_eq!(WorkflowStatus::Pending, WorkflowStatus::Pending);
        assert_ne!(WorkflowStatus::Running, WorkflowStatus::Completed);
    }
}