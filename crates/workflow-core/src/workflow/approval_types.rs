//! Strongly typed approval system types
//! No string-based state management - everything is strongly typed

use serde::{Deserialize, Serialize};
use std::fmt;
use chrono::{DateTime, Utc};

/// Strongly typed ApprovalId
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ApprovalId(String);

impl ApprovalId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
    
    pub fn from_string(s: &str) -> Result<Self, String> {
        // Validate UUID format
        uuid::Uuid::parse_str(s)
            .map(|_| Self(s.to_string()))
            .map_err(|e| format!("Invalid ApprovalId format: {}", e))
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ApprovalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Strongly typed WorkflowId
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkflowId(String);

impl WorkflowId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
    
    pub fn from_string(s: &str) -> Result<Self, String> {
        uuid::Uuid::parse_str(s)
            .map(|_| Self(s.to_string()))
            .map_err(|e| format!("Invalid WorkflowId format: {}", e))
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for WorkflowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Strongly typed TaskId from Zoho
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskId(String);

impl TaskId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Strongly typed ContactId from Zoho
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactId(String);

impl ContactId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Strongly typed UserId (Telegram user ID)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserId(i64);

impl UserId {
    pub fn new(id: i64) -> Self {
        Self(id)
    }
    
    pub fn value(&self) -> i64 {
        self.0
    }
}

/// Strongly typed TelegramMessageId
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelegramMessageId(i64);

impl TelegramMessageId {
    pub fn new(id: i64) -> Self {
        Self(id)
    }
    
    pub fn value(&self) -> i64 {
        self.0
    }
}

/// Strongly typed TelegramChatId
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelegramChatId(String);

impl TelegramChatId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Strongly typed ApprovalState enum - no strings!
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApprovalState {
    PendingApproval,
    AwaitingUserResponse,
    Approved,
    NeedsImprovement,
    Failed,
}

impl ApprovalState {
    /// Get directory name for file storage
    pub fn directory_name(&self) -> &'static str {
        match self {
            Self::PendingApproval => "pending_approval",
            Self::AwaitingUserResponse => "awaiting_response",
            Self::Approved => "approved",
            Self::NeedsImprovement => "needs_improvement",
            Self::Failed => "failed",
        }
    }
}

// Use the main LetterContent type from types.rs
pub use crate::types::LetterContent;

/// User feedback structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    pub text: String,
    pub provided_by: UserId,
    pub provided_at: DateTime<Utc>,
}

/// Letter history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LetterHistoryEntry {
    pub iteration: u32,
    pub content: LetterContent,
    pub feedback: Option<Feedback>,
    pub created_at: DateTime<Utc>,
}

/// Main approval data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalData {
    pub approval_id: ApprovalId,
    pub workflow_id: WorkflowId,
    pub task_id: TaskId,
    pub contact_id: ContactId,
    pub state: ApprovalState,
    pub recipient_name: String,
    pub company_name: String,
    pub current_letter: LetterContent,
    pub letter_history: Vec<LetterHistoryEntry>,
    pub requested_at: DateTime<Utc>,
    pub requested_by: UserId,
    pub telegram_message_id: Option<TelegramMessageId>,
    pub telegram_chat_id: Option<TelegramChatId>,
    pub updated_at: DateTime<Utc>,
}

impl ApprovalData {
    /// Create new approval request
    pub fn new(
        task_id: TaskId,
        contact_id: ContactId,
        recipient_name: String,
        company_name: String,
        letter: LetterContent,
        requested_by: UserId,
    ) -> Self {
        let now = Utc::now();
        let approval_id = ApprovalId::new();
        let workflow_id = WorkflowId::new();
        
        let first_entry = LetterHistoryEntry {
            iteration: 1,
            content: letter.clone(),
            feedback: None,
            created_at: now,
        };
        
        Self {
            approval_id,
            workflow_id,
            task_id,
            contact_id,
            state: ApprovalState::PendingApproval,
            recipient_name,
            company_name,
            current_letter: letter,
            letter_history: vec![first_entry],
            requested_at: now,
            requested_by,
            telegram_message_id: None,
            telegram_chat_id: None,
            updated_at: now,
        }
    }
    
    /// Get current iteration number
    pub fn current_iteration(&self) -> u32 {
        self.letter_history.len() as u32
    }
    
    /// Add feedback to current iteration
    pub fn add_feedback(&mut self, feedback_text: String, user_id: UserId) {
        let feedback = Feedback {
            text: feedback_text,
            provided_by: user_id,
            provided_at: Utc::now(),
        };
        
        if let Some(entry) = self.letter_history.last_mut() {
            entry.feedback = Some(feedback);
        }
        
        self.state = ApprovalState::NeedsImprovement;
        self.updated_at = Utc::now();
    }
    
    /// Add improved letter as new iteration
    pub fn add_improved_letter(&mut self, improved_letter: LetterContent) {
        let new_entry = LetterHistoryEntry {
            iteration: self.current_iteration() + 1,
            content: improved_letter.clone(),
            feedback: None,
            created_at: Utc::now(),
        };
        
        self.letter_history.push(new_entry);
        self.current_letter = improved_letter;
        self.state = ApprovalState::PendingApproval;
        self.updated_at = Utc::now();
        
        // Reset Telegram tracking
        self.telegram_message_id = None;
        self.telegram_chat_id = None;
    }
    
    /// Mark as sent to Telegram
    pub fn mark_sent_to_telegram(&mut self, message_id: TelegramMessageId, chat_id: TelegramChatId) {
        self.telegram_message_id = Some(message_id);
        self.telegram_chat_id = Some(chat_id);
        self.state = ApprovalState::AwaitingUserResponse;
        self.updated_at = Utc::now();
    }
    
    /// Mark as approved by user
    pub fn mark_approved(&mut self) {
        self.state = ApprovalState::Approved;
        self.updated_at = Utc::now();
    }
    
    /// Mark as failed with error
    pub fn mark_failed(&mut self) {
        self.state = ApprovalState::Failed;
        self.updated_at = Utc::now();
    }
}

/// Health check status enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// State count map for statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateCountMap {
    counts: std::collections::HashMap<ApprovalState, usize>,
}

impl StateCountMap {
    pub fn new() -> Self {
        Self {
            counts: std::collections::HashMap::new(),
        }
    }
    
    pub fn increment(&mut self, state: ApprovalState) {
        *self.counts.entry(state).or_insert(0) += 1;
    }
    
    pub fn get(&self, state: ApprovalState) -> usize {
        self.counts.get(&state).copied().unwrap_or(0)
    }
    
    pub fn total(&self) -> usize {
        self.counts.values().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    
    #[test]
    fn test_workflow_trigger_serialization() {
        let trigger = WorkflowTrigger {
            trigger_id: "test-123".to_string(),
            requested_by: UserId::new(12345),
            requested_at: Utc::now(),
            max_tasks: 1,
            dry_run: false,
            processed: false,
            processed_at: None,
            result: None,
        };
        
        // Test serialization
        let json = serde_json::to_string(&trigger).unwrap();
        assert!(!json.contains("task_id")); // Should NOT contain task_id
        assert!(json.contains("trigger_id"));
        assert!(json.contains("test-123"));
        assert!(json.contains("max_tasks"));
        
        // Test deserialization
        let deserialized: WorkflowTrigger = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.trigger_id, "test-123");
        assert_eq!(deserialized.max_tasks, 1);
        assert_eq!(deserialized.requested_by.value(), 12345);
        assert!(!deserialized.dry_run);
        assert!(!deserialized.processed);
    }
    
    #[test]
    fn test_workflow_trigger_deserialization_backwards_compatibility() {
        // Test that old format with task_id field is handled gracefully
        let json_with_task_id = r#"{
            "trigger_id": "test-456",
            "task_id": "should_be_ignored",
            "requested_by": 67890,
            "requested_at": "2025-01-01T00:00:00Z",
            "max_tasks": 2,
            "dry_run": true,
            "processed": true,
            "processed_at": "2025-01-01T01:00:00Z",
            "result": "test result"
        }"#;
        
        // This should succeed even with extra task_id field (serde ignores unknown fields by default)
        let result: Result<WorkflowTrigger, _> = serde_json::from_str(json_with_task_id);
        assert!(result.is_ok());
        
        let trigger = result.unwrap();
        assert_eq!(trigger.trigger_id, "test-456");
        assert_eq!(trigger.max_tasks, 2);
        assert!(trigger.dry_run);
        assert!(trigger.processed);
    }
    
    #[test]
    fn test_approval_data_creation() {
        let task_id = TaskId::new("task-123".to_string());
        let contact_id = ContactId::new("contact-456".to_string());
        let letter = LetterContent {
            subject: "Test Subject".to_string(),
            greeting: "Dear Test".to_string(),
            body: "Test body content".to_string(),
            sender_name: "Test Sender".to_string(),
            recipient_name: "John Doe".to_string(),
            company_name: "Test Company".to_string(),
        };
        let user_id = UserId::new(12345);
        
        let approval = ApprovalData::new(
            task_id,
            contact_id,
            "John Doe".to_string(),
            "Test Company".to_string(),
            letter,
            user_id,
        );
        
        assert_eq!(approval.state, ApprovalState::PendingApproval);
        assert_eq!(approval.recipient_name, "John Doe");
        assert_eq!(approval.company_name, "Test Company");
        assert_eq!(approval.current_iteration(), 1);
        assert_eq!(approval.letter_history.len(), 1);
        assert!(approval.telegram_message_id.is_none());
    }
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub status: HealthStatus,
    pub counts: StateCountMap,
    pub total_workflows: usize,
    pub root_path: std::path::PathBuf,
    pub file_locking_enabled: bool,
    pub last_check: DateTime<Utc>,
}

/// Workflow trigger for batch processing - generic trigger without specific task ID
/// The workflow processor will dynamically load available tasks based on max_tasks limit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTrigger {
    pub trigger_id: String,
    pub requested_by: UserId,
    pub requested_at: DateTime<Utc>,
    pub max_tasks: u32,
    pub dry_run: bool,
    pub processed: bool,
    pub processed_at: Option<DateTime<Utc>>,
    pub result: Option<String>,
}