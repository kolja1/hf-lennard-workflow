//! Workflow management module

pub mod approval_types;
pub mod approval_queue;
pub mod approval_watcher;
pub mod traits;
pub mod orchestrator;

pub use approval_types::*;
pub use approval_queue::ApprovalQueue;
pub use approval_watcher::ApprovalWatcher;
pub use traits::WorkflowSteps;
pub use orchestrator::WorkflowOrchestrator;

// Keep existing WorkflowState for compatibility during migration
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowState {
    pub workflow_id: String,
    pub task_id: String,
    pub contact_id: String,
    pub state: WorkflowStatus,
    pub data: WorkflowData,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkflowStatus {
    Pending,
    Processing,
    WaitingApproval,
    Approved,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowData {
    pub recipient_name: String,
    pub company_name: String,
    pub letter_content: Option<LetterContent>,
    pub approval_data: Option<LegacyApprovalData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyApprovalData {
    pub approval_id: String,
    pub requested_at: String,
    pub requested_by: String,
    pub status: LegacyApprovalStatus,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LegacyApprovalStatus {
    Pending,
    Approved,
    Rejected(String),
}

impl WorkflowState {
    pub fn new(task_id: String, contact_id: String, recipient_name: String, company_name: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        
        Self {
            workflow_id: Uuid::new_v4().to_string(),
            task_id,
            contact_id,
            state: WorkflowStatus::Pending,
            data: WorkflowData {
                recipient_name,
                company_name,
                letter_content: None,
                approval_data: None,
            },
            created_at: now.clone(),
            updated_at: now,
        }
    }
    
    pub fn update_status(&mut self, status: WorkflowStatus) {
        self.state = status;
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }
}