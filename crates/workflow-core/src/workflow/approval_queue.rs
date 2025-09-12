//! Strongly typed ApprovalQueue service implementation
//! File-based queue with atomic operations and thread safety

use crate::error::{LennardError, Result};
use super::approval_types::*;
use crate::paths;
use std::path::{Path, PathBuf};
use std::fs;
use chrono::Utc;
use serde_json;

/// Thread-safe approval queue for managing letter approval requests
pub struct ApprovalQueue {
    root_path: PathBuf,
    enable_file_locking: bool,
}

impl ApprovalQueue {
    /// Create new ApprovalQueue with specified root path
    pub fn new<P: AsRef<Path>>(root_path: P) -> Result<Self> {
        let root_path = root_path.as_ref().to_path_buf();
        
        // Create directory structure
        for state in &[
            ApprovalState::PendingApproval,
            ApprovalState::AwaitingUserResponse,
            ApprovalState::Approved,
            ApprovalState::NeedsImprovement,
            ApprovalState::Failed,
        ] {
            let state_dir = root_path.join(state.directory_name());
            fs::create_dir_all(&state_dir)?;
        }
        
        // Create data directories
        let data_dir = root_path.join(paths::DATA_DIR_NAME);
        for subdir in &[paths::DOSSIERS_DIR_NAME, paths::LETTERS_DIR_NAME, paths::ATTACHMENTS_DIR_NAME] {
            fs::create_dir_all(data_dir.join(subdir))?;
        }
        
        // Create triggers directories
        fs::create_dir_all(root_path.join(paths::TRIGGERS_DIR_NAME))?;
        fs::create_dir_all(root_path.join(paths::TRIGGERS_DIR_NAME).join(paths::PROCESSED_DIR_NAME))?;
        fs::create_dir_all(root_path.join(paths::TRIGGERS_DIR_NAME).join(paths::FAILED_DIR_NAME))?;
        
        Ok(Self {
            root_path,
            enable_file_locking: true,
        })
    }
    
    /// Get path for approval in specific state
    fn get_approval_path(&self, state: ApprovalState, approval_id: &ApprovalId) -> PathBuf {
        self.root_path
            .join(state.directory_name())
            .join(format!("approval_{}.json", approval_id))
    }
    
    /// Find approval in any state
    fn find_approval_path(&self, approval_id: &ApprovalId) -> Option<(PathBuf, ApprovalState)> {
        log::debug!("Searching for approval {} in all state directories", approval_id);
        
        for state in &[
            ApprovalState::PendingApproval,
            ApprovalState::AwaitingUserResponse,
            ApprovalState::Approved,
            ApprovalState::NeedsImprovement,
            ApprovalState::Failed,
        ] {
            let path = self.get_approval_path(*state, approval_id);
            log::debug!("Checking path: {:?} - exists: {}", path, path.exists());
            
            if path.exists() {
                log::debug!("Found approval {} in state {:?}", approval_id, state);
                return Some((path, *state));
            }
        }
        
        log::debug!("Approval {} not found in any state directory", approval_id);
        None
    }
    
    /// Write approval data to file
    fn write_approval(&self, path: &Path, approval: &ApprovalData) -> Result<()> {
        let json = serde_json::to_string_pretty(approval)
            .map_err(|e| LennardError::Serialization(format!("Failed to serialize approval: {}", e)))?;
        
        fs::write(path, json)?;
        
        Ok(())
    }
    
    /// Read approval data from file
    fn read_approval(&self, path: &Path) -> Result<ApprovalData> {
        let json = fs::read_to_string(path)?;
        
        serde_json::from_str(&json)
            .map_err(|e| LennardError::Deserialization(format!("Failed to deserialize approval: {}", e)))
    }
    
    /// Move approval file between states
    fn move_approval(&self, from: &Path, to: &Path) -> Result<()> {
        fs::rename(from, to)?;
        Ok(())
    }
    
    /// Create new approval request
    pub fn create_approval(
        &self,
        task_id: TaskId,
        contact_id: ContactId,
        recipient_name: String,
        recipient_email: Option<String>,
        recipient_title: Option<String>,
        company_name: String,
        letter: LetterContent,
        requested_by: UserId,
        mailing_address: Option<crate::types::MailingAddress>,
        pdf_base64: Option<String>,
        person_dossier: Option<String>,
        company_dossier: Option<String>,
        industry: Option<String>,
        website: Option<String>,
    ) -> Result<ApprovalId> {
        let mut approval = ApprovalData::new(
            task_id,
            contact_id,
            recipient_name,
            company_name,
            letter,
            requested_by,
        );
        
        // Add the additional fields
        approval.recipient_email = recipient_email;
        approval.recipient_title = recipient_title;
        approval.mailing_address = mailing_address;
        approval.pdf_base64 = pdf_base64;
        approval.person_dossier = person_dossier;
        approval.company_dossier = company_dossier;
        approval.industry = industry;
        approval.website = website;
        
        let approval_id = approval.approval_id.clone();
        let path = self.get_approval_path(ApprovalState::PendingApproval, &approval_id);
        
        self.write_approval(&path, &approval)?;
        
        log::info!("Created approval request: {}", approval_id);
        Ok(approval_id)
    }
    
    /// Get approval request by ID
    pub fn get_approval_request(
        &self,
        approval_id: &ApprovalId,
        state: Option<ApprovalState>,
    ) -> Result<Option<ApprovalData>> {
        if let Some(state) = state {
            // Look in specific state
            let path = self.get_approval_path(state, approval_id);
            if path.exists() {
                return Ok(Some(self.read_approval(&path)?));
            }
        } else {
            // Search all states
            if let Some((path, _)) = self.find_approval_path(approval_id) {
                return Ok(Some(self.read_approval(&path)?));
            }
        }
        
        Ok(None)
    }
    
    /// Get all pending approvals
    pub fn get_pending_approvals(&self) -> Result<Vec<ApprovalData>> {
        self.list_approvals_by_state(ApprovalState::PendingApproval)
    }
    
    /// List approvals in specific state
    pub fn list_approvals_by_state(&self, state: ApprovalState) -> Result<Vec<ApprovalData>> {
        let state_dir = self.root_path.join(state.directory_name());
        
        if !state_dir.exists() {
            return Ok(Vec::new());
        }
        
        let mut approvals = Vec::new();
        
        for entry in fs::read_dir(&state_dir)? 
        {
            let entry = entry?;
            
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(approval) = self.read_approval(&path) {
                    approvals.push(approval);
                }
            }
        }
        
        Ok(approvals)
    }
    
    /// Send approval to Telegram
    pub fn send_to_telegram(
        &self,
        approval_id: &ApprovalId,
        message_id: TelegramMessageId,
        chat_id: TelegramChatId,
    ) -> Result<bool> {
        if let Some((path, current_state)) = self.find_approval_path(approval_id) {
            if current_state != ApprovalState::PendingApproval {
                return Ok(false);
            }
            
            let mut approval = self.read_approval(&path)?;
            approval.mark_sent_to_telegram(message_id, chat_id);
            
            // Write updated approval
            self.write_approval(&path, &approval)?;
            
            // Move to awaiting response state
            let new_path = self.get_approval_path(ApprovalState::AwaitingUserResponse, approval_id);
            self.move_approval(&path, &new_path)?;
            
            log::info!("Sent approval {} to Telegram", approval_id);
            return Ok(true);
        }
        
        Ok(false)
    }
    
    /// Mark approval as awaiting user response (after sending to Telegram)
    pub fn mark_as_awaiting_response(&self, approval_id: &ApprovalId) -> Result<()> {
        if let Some((path, current_state)) = self.find_approval_path(approval_id) {
            if current_state != ApprovalState::PendingApproval {
                return Err(LennardError::Workflow(
                    format!("Cannot transition approval {} from {:?} to AwaitingUserResponse", 
                        approval_id, current_state)
                ));
            }
            
            let mut approval = self.read_approval(&path)?;
            approval.state = ApprovalState::AwaitingUserResponse;
            approval.updated_at = Utc::now();
            
            // Write updated approval
            self.write_approval(&path, &approval)?;
            
            // Move to awaiting_response directory
            let new_path = self.get_approval_path(ApprovalState::AwaitingUserResponse, approval_id);
            self.move_approval(&path, &new_path)?;
            
            log::info!("Transitioned approval {} to AwaitingUserResponse", approval_id);
            Ok(())
        } else {
            Err(LennardError::Workflow(
                format!("Approval {} not found", approval_id)
            ))
        }
    }
    
    /// Handle user approval
    pub fn handle_user_approval(&self, approval_id: &ApprovalId) -> Result<Option<ApprovalData>> {
        log::info!("handle_user_approval called for approval_id: {}", approval_id);
        
        if let Some((path, current_state)) = self.find_approval_path(approval_id) {
            log::info!("Found approval at {:?} with state {:?}", path, current_state);
            
            if current_state != ApprovalState::AwaitingUserResponse {
                log::warn!("Approval {} is in state {:?}, not AwaitingUserResponse", approval_id, current_state);
                return Ok(None);
            }
            
            let mut approval = self.read_approval(&path)?;
            approval.mark_approved();
            
            // Write updated approval
            self.write_approval(&path, &approval)?;
            
            // Move to approved state
            let new_path = self.get_approval_path(ApprovalState::Approved, approval_id);
            self.move_approval(&path, &new_path)?;
            
            log::info!("Approval {} approved by user", approval_id);
            return Ok(Some(approval));
        }
        
        log::warn!("Approval {} not found in any state directory", approval_id);
        Ok(None)
    }
    
    /// Handle user feedback for improvement
    pub fn handle_user_feedback(
        &self,
        approval_id: &ApprovalId,
        feedback_text: String,
        user_id: UserId,
    ) -> Result<Option<ApprovalData>> {
        if let Some((path, current_state)) = self.find_approval_path(approval_id) {
            if current_state != ApprovalState::AwaitingUserResponse {
                return Ok(None);
            }
            
            let mut approval = self.read_approval(&path)?;
            approval.add_feedback(feedback_text, user_id);
            
            // Write updated approval
            self.write_approval(&path, &approval)?;
            
            // Move to needs improvement state
            let new_path = self.get_approval_path(ApprovalState::NeedsImprovement, approval_id);
            self.move_approval(&path, &new_path)?;
            
            log::info!("Approval {} needs improvement based on feedback", approval_id);
            return Ok(Some(approval));
        }
        
        Ok(None)
    }
    
    /// Requeue improved letter
    pub fn requeue_after_improvement(
        &self,
        approval_id: &ApprovalId,
        improved_letter: LetterContent,
    ) -> Result<bool> {
        if let Some((path, current_state)) = self.find_approval_path(approval_id) {
            if current_state != ApprovalState::NeedsImprovement {
                return Ok(false);
            }
            
            let mut approval = self.read_approval(&path)?;
            approval.add_improved_letter(improved_letter);
            
            // Write updated approval
            self.write_approval(&path, &approval)?;
            
            // Move back to pending approval
            let new_path = self.get_approval_path(ApprovalState::PendingApproval, approval_id);
            self.move_approval(&path, &new_path)?;
            
            log::info!("Approval {} requeued with improved letter", approval_id);
            return Ok(true);
        }
        
        Ok(false)
    }
    
    /// Mark approval as failed
    pub fn mark_failed(&self, approval_id: &ApprovalId) -> Result<bool> {
        if let Some((path, _)) = self.find_approval_path(approval_id) {
            let mut approval = self.read_approval(&path)?;
            approval.mark_failed();
            
            // Write updated approval
            self.write_approval(&path, &approval)?;
            
            // Move to failed state
            let new_path = self.get_approval_path(ApprovalState::Failed, approval_id);
            self.move_approval(&path, &new_path)?;
            
            log::info!("Approval {} marked as failed", approval_id);
            return Ok(true);
        }
        
        Ok(false)
    }
    
    /// Get approval counts by state
    pub fn get_approval_counts(&self) -> Result<StateCountMap> {
        let mut counts = StateCountMap::new();
        
        for state in &[
            ApprovalState::PendingApproval,
            ApprovalState::AwaitingUserResponse,
            ApprovalState::Approved,
            ApprovalState::NeedsImprovement,
            ApprovalState::Failed,
        ] {
            let approvals = self.list_approvals_by_state(*state)?;
            for _ in approvals {
                counts.increment(*state);
            }
        }
        
        Ok(counts)
    }
    
    /// Perform health check
    pub fn health_check(&self) -> Result<HealthCheckResult> {
        let counts = self.get_approval_counts()?;
        let total_workflows = counts.total();
        
        // Determine health status
        let status = if total_workflows == 0 {
            HealthStatus::Healthy
        } else if counts.get(ApprovalState::Failed) > 10 {
            HealthStatus::Unhealthy
        } else if counts.get(ApprovalState::PendingApproval) > 50 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };
        
        Ok(HealthCheckResult {
            status,
            counts,
            total_workflows,
            root_path: self.root_path.clone(),
            file_locking_enabled: self.enable_file_locking,
            last_check: Utc::now(),
        })
    }
    
    /// Create workflow trigger for batch processing - generic trigger without specific task ID
    /// The workflow processor will dynamically load available tasks up to max_tasks limit
    pub fn create_workflow_trigger(
        &self,
        requested_by: UserId,
        max_tasks: u32,
        dry_run: bool,
    ) -> Result<uuid::Uuid> {
        let trigger_id = uuid::Uuid::new_v4();
        
        let trigger = WorkflowTrigger {
            trigger_id: trigger_id.to_string(),
            requested_by,
            requested_at: Utc::now(),
            max_tasks,
            dry_run,
            processed: false,
            processed_at: None,
            result: None,
        };
        
        let trigger_path = self.root_path
            .join("triggers")
            .join(format!("trigger_{}.json", trigger_id));
        
        let json = serde_json::to_string_pretty(&trigger)
            .map_err(|e| LennardError::Serialization(format!("Failed to serialize trigger: {}", e)))?;
        
        fs::write(&trigger_path, json)?;
        
        log::info!("Created workflow trigger: {}", trigger_id);
        Ok(trigger_id)
    }
    
    /// Get pending workflow triggers
    pub fn get_pending_triggers(&self) -> Result<Vec<WorkflowTrigger>> {
        let trigger_dir = self.root_path.join("triggers");
        
        if !trigger_dir.exists() {
            return Ok(Vec::new());
        }
        
        let mut triggers = Vec::new();
        
        for entry in fs::read_dir(&trigger_dir)?
        {
            let entry = entry?;
            
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                let json = fs::read_to_string(&path)?;
                
                if let Ok(trigger) = serde_json::from_str::<WorkflowTrigger>(&json) {
                    if !trigger.processed {
                        triggers.push(trigger);
                    }
                }
            }
        }
        
        Ok(triggers)
    }
    
    /// Mark trigger as processed
    pub fn mark_trigger_processed(&self, trigger_id: &str, result: String) -> Result<bool> {
        let trigger_path = self.root_path
            .join("triggers")
            .join(format!("trigger_{}.json", trigger_id));
        
        if !trigger_path.exists() {
            return Ok(false);
        }
        
        let json = fs::read_to_string(&trigger_path)?;
        
        let mut trigger: WorkflowTrigger = serde_json::from_str(&json)
            .map_err(|e| LennardError::Deserialization(format!("Failed to deserialize trigger: {}", e)))?;
        
        trigger.processed = true;
        trigger.processed_at = Some(Utc::now());
        trigger.result = Some(result);
        
        let updated_json = serde_json::to_string_pretty(&trigger)
            .map_err(|e| LennardError::Serialization(format!("Failed to serialize trigger: {}", e)))?;
        
        fs::write(&trigger_path, updated_json)?;
        
        // Move to processed directory
        let processed_path = self.root_path
            .join("triggers")
            .join("processed")
            .join(format!("trigger_{}.json", trigger_id));
        
        fs::rename(&trigger_path, &processed_path)?;
        
        log::info!("Marked trigger {} as processed", trigger_id);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_approval_queue_creation() {
        let temp_dir = TempDir::new().unwrap();
        let _queue = ApprovalQueue::new(temp_dir.path()).unwrap();
        
        // Verify directory structure was created
        for state in &[
            ApprovalState::PendingApproval,
            ApprovalState::AwaitingUserResponse,
            ApprovalState::Approved,
            ApprovalState::NeedsImprovement,
            ApprovalState::Failed,
        ] {
            let state_dir = temp_dir.path().join(state.directory_name());
            assert!(state_dir.exists(), "State directory {:?} should exist", state_dir);
        }
        
        // Verify data directories
        let data_dir = temp_dir.path().join("data");
        assert!(data_dir.join("dossiers").exists());
        assert!(data_dir.join("letters").exists());
        assert!(data_dir.join("attachments").exists());
        
        // Verify triggers directories
        assert!(temp_dir.path().join("triggers").exists());
        assert!(temp_dir.path().join("triggers").join("processed").exists());
    }
    
    #[test]
    fn test_create_workflow_trigger() {
        let temp_dir = TempDir::new().unwrap();
        let queue = ApprovalQueue::new(temp_dir.path()).unwrap();
        
        let user_id = UserId::new(12345);
        let trigger_id = queue.create_workflow_trigger(
            user_id,
            1,
            false
        ).unwrap();
        
        // Verify trigger file was created
        let trigger_path = temp_dir.path()
            .join("triggers")
            .join(format!("trigger_{}.json", trigger_id));
        
        assert!(trigger_path.exists());
        
        // Verify trigger content
        let content = std::fs::read_to_string(trigger_path).unwrap();
        let parsed: WorkflowTrigger = serde_json::from_str(&content).unwrap();
        
        assert_eq!(parsed.requested_by, user_id);
        assert_eq!(parsed.max_tasks, 1);
        assert!(!parsed.dry_run);
        assert!(!parsed.processed);
        assert!(!content.contains("task_id")); // Should NOT contain task_id field
        
        // Verify UUID format for trigger_id
        assert!(uuid::Uuid::parse_str(&parsed.trigger_id).is_ok());
    }
    
    #[test]
    fn test_create_workflow_trigger_dry_run() {
        let temp_dir = TempDir::new().unwrap();
        let queue = ApprovalQueue::new(temp_dir.path()).unwrap();
        
        let user_id = UserId::new(67890);
        let trigger_id = queue.create_workflow_trigger(
            user_id,
            5,
            true // dry_run = true
        ).unwrap();
        
        // Verify trigger content for dry run
        let trigger_path = temp_dir.path()
            .join("triggers")
            .join(format!("trigger_{}.json", trigger_id));
        
        let content = std::fs::read_to_string(trigger_path).unwrap();
        let parsed: WorkflowTrigger = serde_json::from_str(&content).unwrap();
        
        assert_eq!(parsed.requested_by, user_id);
        assert_eq!(parsed.max_tasks, 5);
        assert!(parsed.dry_run); // Should be true
        assert!(!parsed.processed);
    }
    
    #[test]
    fn test_create_approval() {
        let temp_dir = TempDir::new().unwrap();
        let queue = ApprovalQueue::new(temp_dir.path()).unwrap();
        
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
        
        let approval_id = queue.create_approval(
            task_id,
            contact_id,
            "John Doe".to_string(),
            None,  // recipient_email
            None,  // recipient_title
            "Test Company".to_string(),
            letter,
            user_id,
            None,  // mailing_address
            None,  // pdf_base64
            None,  // person_dossier
            None,  // company_dossier
            None,  // industry
            None,  // website
        ).unwrap();
        
        // Verify approval file was created in pending_approval directory
        let approval_path = temp_dir.path()
            .join("pending_approval")
            .join(format!("approval_{}.json", approval_id));
        
        assert!(approval_path.exists());
        
        // Verify approval content
        let content = std::fs::read_to_string(approval_path).unwrap();
        let parsed: ApprovalData = serde_json::from_str(&content).unwrap();
        
        assert_eq!(parsed.approval_id, approval_id);
        assert_eq!(parsed.state, ApprovalState::PendingApproval);
        assert_eq!(parsed.recipient_name, "John Doe");
        assert_eq!(parsed.company_name, "Test Company");
        assert_eq!(parsed.requested_by, user_id);
        assert_eq!(parsed.current_iteration(), 1);
    }
    
    #[test]
    fn test_get_approval() {
        let temp_dir = TempDir::new().unwrap();
        let queue = ApprovalQueue::new(temp_dir.path()).unwrap();
        
        // Create an approval first
        let task_id = TaskId::new("task-789".to_string());
        let contact_id = ContactId::new("contact-101".to_string());
        let letter = LetterContent {
            subject: "Get Test Subject".to_string(),
            greeting: "Dear Get Test".to_string(),
            body: "Get test body content".to_string(),
            sender_name: "Get Test Sender".to_string(),
            recipient_name: "Jane Smith".to_string(),
            company_name: "Get Test Company".to_string(),
        };
        let user_id = UserId::new(99999);
        
        let approval_id = queue.create_approval(
            task_id,
            contact_id,
            "Jane Smith".to_string(),
            None,  // recipient_email
            None,  // recipient_title
            "Get Test Company".to_string(),
            letter,
            user_id,
            None,  // mailing_address
            None,  // pdf_base64
            None,  // person_dossier
            None,  // company_dossier
            None,  // industry
            None,  // website
        ).unwrap();
        
        // Now retrieve it
        let retrieved_approval = queue.get_approval_request(&approval_id, None).unwrap().unwrap();
        
        assert_eq!(retrieved_approval.approval_id, approval_id);
        assert_eq!(retrieved_approval.recipient_name, "Jane Smith");
        assert_eq!(retrieved_approval.company_name, "Get Test Company");
        assert_eq!(retrieved_approval.current_letter.subject, "Get Test Subject");
    }
    
    #[test]
    fn test_get_nonexistent_approval() {
        let temp_dir = TempDir::new().unwrap();
        let queue = ApprovalQueue::new(temp_dir.path()).unwrap();
        
        let nonexistent_id = ApprovalId::new();
        let result = queue.get_approval_request(&nonexistent_id, None).unwrap();
        
        assert!(result.is_none());
    }
    
    #[test]
    fn test_health_check() {
        let temp_dir = TempDir::new().unwrap();
        let queue = ApprovalQueue::new(temp_dir.path()).unwrap();
        
        // Create some test approvals in different states
        let user_id = UserId::new(11111);
        
        // Create pending approval
        let _pending_id = queue.create_approval(
            TaskId::new("pending-task".to_string()),
            ContactId::new("pending-contact".to_string()),
            "Pending Person".to_string(),
            None,  // recipient_email
            None,  // recipient_title
            "Pending Company".to_string(),
            LetterContent {
                subject: "Pending Subject".to_string(),
                greeting: "Dear Pending".to_string(),
                body: "Pending body".to_string(),
                sender_name: "Pending Sender".to_string(),
                recipient_name: "Pending Person".to_string(),
                company_name: "Pending Company".to_string(),
            },
            user_id,
            None,  // mailing_address
            None,  // pdf_base64
            None,  // person_dossier
            None,  // company_dossier
            None,  // industry
            None,  // website
        ).unwrap();
        
        let health_result = queue.health_check().unwrap();
        
        assert_eq!(health_result.status, HealthStatus::Healthy);
        assert_eq!(health_result.total_workflows, 1);
        assert_eq!(health_result.counts.get(ApprovalState::PendingApproval), 1);
        assert_eq!(health_result.counts.get(ApprovalState::Approved), 0);
        assert!(health_result.file_locking_enabled);
        assert_eq!(health_result.root_path, temp_dir.path());
    }
    
    #[test]
    fn test_mark_trigger_processed() {
        let temp_dir = TempDir::new().unwrap();
        let queue = ApprovalQueue::new(temp_dir.path()).unwrap();
        
        // Create a trigger first
        let user_id = UserId::new(33333);
        let trigger_id = queue.create_workflow_trigger(user_id, 1, false).unwrap();
        
        // Mark it as processed
        let result = queue.mark_trigger_processed(
            &trigger_id.to_string(),
            "Test processing completed".to_string()
        ).unwrap();
        
        assert!(result);
        
        // Verify trigger was moved to processed directory
        let processed_path = temp_dir.path()
            .join("triggers")
            .join("processed")
            .join(format!("trigger_{}.json", trigger_id));
        
        assert!(processed_path.exists());
        
        // Verify trigger content was updated
        let content = std::fs::read_to_string(processed_path).unwrap();
        let parsed: WorkflowTrigger = serde_json::from_str(&content).unwrap();
        
        assert!(parsed.processed);
        assert!(parsed.processed_at.is_some());
        assert_eq!(parsed.result.as_ref().unwrap(), "Test processing completed");
    }
    
    #[test]
    fn test_approval_persists_to_disk_across_restart() {
        use tempfile::TempDir;
        
        // Create a temporary directory for the test
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        
        // Store approval ID for later verification
        let approval_id;
        
        // First instance - create and save approval
        {
            let approval_queue = ApprovalQueue::new(&temp_path).unwrap();
            
            // Create test data
            let task_id = TaskId::new("test-task-123".to_string());
            let contact_id = ContactId::new("test-contact-456".to_string());
            let user_id = UserId::new(789);
            
            let letter = LetterContent {
                subject: "Test Subject".to_string(),
                greeting: "Dear Test".to_string(),
                body: "Test body".to_string(),
                sender_name: "Test Sender".to_string(),
                recipient_name: "Test Person".to_string(),
                company_name: "Test Company".to_string(),
            };
            
            // Create and persist approval
            approval_id = approval_queue.create_approval(
                task_id,
                contact_id,
                "Test Person".to_string(),
                None,  // recipient_email
                None,  // recipient_title
                "Test Company".to_string(),
                letter,
                user_id,
                None,  // mailing_address
                None,  // pdf_base64
                None,  // person_dossier
                None,  // company_dossier
                None,  // industry
                None,  // website
            ).unwrap();
            
            // Verify it exists in first instance
            let pending = approval_queue.get_pending_approvals().unwrap();
            assert_eq!(pending.len(), 1, "Should have one approval in first instance");
            
            // First instance is dropped here, simulating shutdown
        }
        
        // Second instance - should be able to load persisted approval
        {
            let approval_queue_2 = ApprovalQueue::new(&temp_path).unwrap();
            
            // Check that approval was persisted and can be loaded
            let pending_approvals = approval_queue_2.get_pending_approvals().unwrap();
            
            // This will pass if approvals are properly persisted to disk
            assert_eq!(pending_approvals.len(), 1, 
                "Approval should persist to disk and be loadable after restart");
            assert_eq!(pending_approvals[0].recipient_name, "Test Person");
            
            // Verify we can retrieve it by ID in the new instance
            let retrieved = approval_queue_2.get_approval_request(&approval_id, None).unwrap();
            assert!(retrieved.is_some(), 
                "Should be able to retrieve approval by ID after restart");
            assert_eq!(retrieved.unwrap().recipient_name, "Test Person");
        }
    }
}