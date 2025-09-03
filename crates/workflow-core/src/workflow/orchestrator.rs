//! Workflow orchestrator with strongly-typed steps

use super::traits::WorkflowSteps;
use super::approval_types::{WorkflowTrigger, ApprovalState};
use crate::error::{LennardError, Result};
use chrono::Utc;

/// Single orchestration component with hard-coded workflow steps
pub struct WorkflowOrchestrator<T: WorkflowSteps> {
    steps: T,
}

impl<T: WorkflowSteps> WorkflowOrchestrator<T> {
    pub fn new(steps: T) -> Self {
        Self { steps }
    }
    
    /// Process a workflow trigger through dynamic task loading and all 7 steps
    pub async fn process_workflow(&self, trigger: WorkflowTrigger) -> Result<WorkflowTrigger> {
        log::info!("Processing workflow trigger {} for up to {} tasks", 
                   trigger.trigger_id, trigger.max_tasks);
        
        // Load available tasks from Zoho CRM dynamically
        let available_tasks = self.steps.load_available_tasks(trigger.max_tasks).await?;
        
        if available_tasks.is_empty() {
            log::info!("No available tasks found for processing");
            return Ok(WorkflowTrigger {
                result: Some("No tasks available for processing".to_string()),
                processed: true,
                processed_at: Some(Utc::now()),
                ..trigger
            });
        }
        
        let mut processed_count = 0;
        let mut results = Vec::new();
        
        // Process each task through the 7-step workflow
        for task in available_tasks {
            log::info!("Processing task: {} - {}", task.id, task.subject);
            
            match self.process_single_task(&task).await {
                Ok(result) => {
                    processed_count += 1;
                    results.push(format!("✅ Task {}: {}", task.id, result));
                    log::info!("Successfully processed task {}: {}", task.id, result);
                }
                Err(e) => {
                    results.push(format!("❌ Task {}: {}", task.id, e));
                    log::error!("Failed to process task {}: {}", task.id, e);
                    
                    // Send error notification and update task status (no company name available at this level)
                    if let Err(notification_err) = self.handle_task_error(&task, &e, None).await {
                        log::error!("Failed to send error notification: {}", notification_err);
                    }
                }
            }
            
            // Respect max_tasks limit (defensive check)
            if processed_count >= trigger.max_tasks as usize {
                break;
            }
        }
        
        let final_result = if processed_count > 0 {
            format!("Processed {} tasks:\n{}", processed_count, results.join("\n"))
        } else {
            "No tasks were successfully processed".to_string()
        };
        
        Ok(WorkflowTrigger {
            result: Some(final_result),
            processed: true,
            processed_at: Some(Utc::now()),
            ..trigger
        })
    }
    
    /// Handle task error by sending notifications and updating status
    async fn handle_task_error(&self, task: &zoho_generated_types::TasksResponse, error: &LennardError, company_name: Option<&str>) -> Result<()> {
        // Extract contact name from task or use default
        let contact_name = task.who_id.as_ref()
            .and_then(|who| who.name.as_deref())
            .unwrap_or("Unknown Contact");
            
        // Use provided company name or default
        let company_name = company_name.unwrap_or("Unknown Company");
        
        let error_message = error.to_string();
        
        // Send Telegram notification
        if let Err(e) = self.steps.send_error_notification(
            &task.id,
            contact_name,
            company_name,
            &error_message
        ).await {
            log::error!("Failed to send Telegram error notification: {}", e);
        }
        
        // Update Zoho task status
        if let Err(e) = self.steps.update_task_error_status(&task.id, &error_message).await {
            log::error!("Failed to update Zoho task status: {}", e);
        }
        
        Ok(())
    }
    
    /// Process a single task through the complete 7-step workflow
    async fn process_single_task(&self, task: &zoho_generated_types::TasksResponse) -> Result<String> {
        log::info!("Starting 7-step workflow for task: {}", task.id);
        
        // Step 1: Load contact - requires task, guaranteed to return contact
        let mut contact = match self.steps.load_contact(task).await {
            Ok(c) => c,
            Err(e) => {
                let error = LennardError::Workflow(format!("Step 1 (load contact) failed: {}", e));
                // Send error notification with task-level info (no company name available yet)
                self.handle_task_error(task, &error, None).await?;
                return Err(error);
            }
        };
        
        log::info!("Step 1: Loaded contact '{}'", contact.full_name);
        
        // Step 2: Load profile - requires contact, guaranteed to return profile
        let profile = self.steps.load_profile(&contact).await
            .map_err(|e| LennardError::Workflow(format!("Step 2 (load profile) failed: {}", e)))?;
        
        log::info!("Step 2: Loaded LinkedIn profile for '{}'", profile.full_name);
        
        // Step 3: Generate dossiers - requires profile and contact, returns extracted data
        let dossier_result = self.steps.generate_dossiers(&profile, &contact.id).await
            .map_err(|e| LennardError::Workflow(format!("Step 3 (generate dossiers) failed: {}", e)))?;
        
        // Use extracted company name (no fallback to task.what_id)
        let company_name = if !dossier_result.company_name.is_empty() {
            dossier_result.company_name.clone()
        } else {
            "Unknown Company".to_string()
        };
        
        log::info!("Step 3: Generated dossiers with company: {}", company_name);
        
        // Step 3.5: Update contact with extracted address if missing
        if contact.mailing_address.is_none() {
            if let Some(address) = dossier_result.mailing_address.clone() {
                log::info!("Step 3.5: Extracted mailing address for {}", contact.full_name);
                // Update the local contact object with the extracted address
                contact.mailing_address = Some(address.clone());
                // Also update Zoho contact with address for persistence
                self.steps.update_contact_address(&contact.id, &address).await?;
            }
        }
        
        // Step 4: Generate letter - requires contact and profile, guaranteed letter
        let letter = self.steps.generate_letter(&contact, &profile).await
            .map_err(|e| LennardError::Workflow(format!("Step 4 (generate letter) failed: {}", e)))?;
        
        log::info!("Step 4: Generated letter with subject '{}'", letter.subject);
        
        // Step 5: Request approval - requires letter, returns approval status
        // This may create a long-running approval request via ApprovalQueue
        let approval = match self.steps.request_approval(&letter, &contact).await {
            Ok(a) => a,
            Err(e) => {
                let error = LennardError::Workflow(format!("Step 5 (request approval) failed: {}", e));
                // Send error notification with full context using extracted company name
                if let Err(notify_err) = self.steps.send_error_notification(
                    &task.id,
                    &contact.full_name,
                    &company_name,
                    &error.to_string()
                ).await {
                    log::error!("Failed to send error notification: {}", notify_err);
                }
                
                // Update task status
                if let Err(status_err) = self.steps.update_task_error_status(&task.id, &error.to_string()).await {
                    log::error!("Failed to update task status: {}", status_err);
                }
                
                return Err(error);
            }
        };
        
        log::info!("Step 5: Approval status: {:?}", approval);
        
        // Step 6 & 7 only proceed if approved
        match approval {
            ApprovalState::Approved => {
                // Step 6: Send PDF - requires approved letter and contact
                let tracking_id = self.steps.send_pdf(&letter, &contact).await
                    .map_err(|e| LennardError::Workflow(format!("Step 6 (send PDF) failed: {}", e)))?;
                    
                log::info!("Step 6: Letter sent successfully, tracking: {}", tracking_id);
                    
                Ok(format!("Letter sent successfully, tracking: {}", tracking_id))
            }
            _ => {
                // Workflow paused - approval is pending or needs improvement
                let status_message = match approval {
                    ApprovalState::PendingApproval => "Approval request created, awaiting user response",
                    ApprovalState::AwaitingUserResponse => "Awaiting user response via Telegram",
                    ApprovalState::NeedsImprovement => "User requested letter improvements",
                    ApprovalState::Failed => "Approval process failed",
                    ApprovalState::Approved => unreachable!("Already handled above"),
                };
                
                Ok(status_message.to_string())
            }
        }
    }
}

// Tests temporarily disabled while migrating from ZohoTask to TasksResponse
// TODO: Update tests to use generated TasksResponse type
/*
#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use crate::types::*;
    use crate::error::LennardError;
    use chrono::Utc;
    
    struct MockWorkflowSteps {
        tasks_to_return: Vec<ZohoTask>,
        should_fail_at_step: Option<&'static str>,
        approval_state: ApprovalState,
    }
    
    impl MockWorkflowSteps {
        fn new_with_tasks(tasks: Vec<ZohoTask>) -> Self {
            Self {
                tasks_to_return: tasks,
                should_fail_at_step: None,
                approval_state: ApprovalState::Approved,
            }
        }
        
        fn new_empty() -> Self {
            Self {
                tasks_to_return: vec![],
                should_fail_at_step: None,
                approval_state: ApprovalState::Approved,
            }
        }
        
        fn with_failure_at(mut self, step: &'static str) -> Self {
            self.should_fail_at_step = Some(step);
            self
        }
        
        fn with_approval_state(mut self, state: ApprovalState) -> Self {
            self.approval_state = state;
            self
        }
    }
    
    #[async_trait]
    impl WorkflowSteps for MockWorkflowSteps {
        async fn load_available_tasks(&self, _max_count: u32) -> Result<Vec<ZohoTask>> {
            if self.should_fail_at_step == Some("load_available_tasks") {
                return Err(LennardError::ServiceUnavailable("Test error".to_string()));
            }
            Ok(self.tasks_to_return.clone())
        }
        
        async fn load_task(&self, task_id: &str) -> Result<ZohoTask> {
            self.tasks_to_return
                .iter()
                .find(|t| t.id == task_id)
                .cloned()
                .ok_or_else(|| LennardError::NotFound("Task not found".to_string()))
        }
        
        async fn load_contact(&self, task: &ZohoTask) -> Result<ZohoContact> {
            if self.should_fail_at_step == Some("load_contact") {
                return Err(LennardError::ServiceUnavailable("Contact load failed".to_string()));
            }
            
            Ok(ZohoContact {
                id: task.contact_id.clone().unwrap_or_else(|| "mock-contact-123".to_string()),
                full_name: "Mock Contact".to_string(),
                email: Some("mock@example.com".to_string()),
                phone: None,
                company: Some("Mock Company".to_string()),
                linkedin_id: Some("mock-linkedin-123".to_string()),
                mailing_address: None,
            })
        }
        
        async fn load_profile(&self, _contact: &ZohoContact) -> Result<LinkedInProfile> {
            if self.should_fail_at_step == Some("load_profile") {
                return Err(LennardError::ServiceUnavailable("Profile load failed".to_string()));
            }
            
            Ok(LinkedInProfile {
                profile_id: "mock-profile-123".to_string(),
                profile_url: "https://linkedin.com/in/mock".to_string(),
                full_name: "Mock LinkedIn Profile".to_string(),
                headline: Some("Mock Professional".to_string()),
                location: Some("Test City".to_string()),
                company: Some("Mock Company".to_string()),
                raw_data: std::collections::HashMap::new(),
            })
        }
        
        async fn generate_dossiers(&self, _profile: &LinkedInProfile, _contact_id: &str) -> Result<crate::clients::DossierResult> {
            if self.should_fail_at_step == Some("generate_dossiers") {
                return Err(LennardError::ServiceUnavailable("Dossier generation failed".to_string()));
            }
            Ok(crate::clients::DossierResult {
                person_dossier: "Mock person dossier content".to_string(),
                company_dossier: "Mock company dossier content".to_string(),
                company_name: "Mock Company".to_string(),
                mailing_address: Some(crate::types::MailingAddress {
                    street: "123 Mock Street".to_string(),
                    city: "Mock City".to_string(),
                    state: Some("Mock State".to_string()),
                    postal_code: "12345".to_string(),
                    country: "Germany".to_string(),
                }),
            })
        }
        
        async fn generate_letter(&self, _contact: &ZohoContact, _profile: &LinkedInProfile) -> Result<LetterContent> {
            if self.should_fail_at_step == Some("generate_letter") {
                return Err(LennardError::ServiceUnavailable("Letter generation failed".to_string()));
            }
            
            Ok(LetterContent {
                subject: "Mock Letter Subject".to_string(),
                greeting: "Dear Mock Contact".to_string(),
                body: "This is a mock letter body.".to_string(),
                sender_name: "Mock Sender".to_string(),
                recipient_name: "Mock Contact".to_string(),
                company_name: "Mock Company".to_string(),
            })
        }
        
        async fn request_approval(&self, _letter: &LetterContent, _contact: &ZohoContact) -> Result<ApprovalState> {
            if self.should_fail_at_step == Some("request_approval") {
                return Err(LennardError::ServiceUnavailable("Approval request failed".to_string()));
            }
            Ok(self.approval_state)
        }
        
        async fn send_pdf(&self, _letter: &LetterContent, _contact: &ZohoContact) -> Result<String> {
            if self.should_fail_at_step == Some("send_pdf") {
                return Err(LennardError::ServiceUnavailable("PDF sending failed".to_string()));
            }
            Ok("mock-tracking-123".to_string())
        }
        
        async fn send_error_notification(
            &self,
            _task_id: &str,
            _contact_name: &str,
            _company_name: &str,
            _error_message: &str
        ) -> Result<()> {
            Ok(())
        }
        
        async fn update_task_error_status(
            &self,
            _task_id: &str,
            _error_message: &str
        ) -> Result<()> {
            Ok(())
        }
    }
    
    #[tokio::test]
    async fn test_workflow_orchestrator_no_tasks() {
        let mock_steps = MockWorkflowSteps::new_empty();
        let orchestrator = WorkflowOrchestrator::new(mock_steps);
        
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
        
        let result = orchestrator.process_workflow(trigger).await.unwrap();
        
        assert!(result.processed);
        assert!(result.result.as_ref().unwrap().contains("No tasks available"));
    }
    
    #[tokio::test]
    async fn test_workflow_orchestrator_with_approved_task() {
        let test_task = ZohoTask {
            id: "task-123".to_string(),
            subject: "Test Task".to_string(),
            description: Some("Test task description".to_string()),
            status: "Not Started".to_string(),
            contact_id: Some("contact-456".to_string()),
            created_time: Utc::now().to_rfc3339(),
        };
        
        let mock_steps = MockWorkflowSteps::new_with_tasks(vec![test_task])
            .with_approval_state(ApprovalState::Approved);
        let orchestrator = WorkflowOrchestrator::new(mock_steps);
        
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
        
        let result = orchestrator.process_workflow(trigger).await.unwrap();
        
        assert!(result.processed);
        let result_msg = result.result.unwrap();
        assert!(result_msg.contains("Processed 1 tasks"));
        assert!(result_msg.contains("Letter sent successfully"));
        assert!(result_msg.contains("mock-tracking-123"));
    }
    
    #[tokio::test]
    async fn test_workflow_orchestrator_with_pending_approval() {
        let test_task = ZohoTask {
            id: "task-456".to_string(),
            subject: "Test Task Pending".to_string(),
            description: Some("Test pending task description".to_string()),
            status: "Not Started".to_string(),
            contact_id: Some("contact-789".to_string()),
            created_time: Utc::now().to_rfc3339(),
        };
        
        let mock_steps = MockWorkflowSteps::new_with_tasks(vec![test_task])
            .with_approval_state(ApprovalState::PendingApproval);
        let orchestrator = WorkflowOrchestrator::new(mock_steps);
        
        let trigger = WorkflowTrigger {
            trigger_id: "test-456".to_string(),
            requested_by: UserId::new(67890),
            requested_at: Utc::now(),
            max_tasks: 1,
            dry_run: false,
            processed: false,
            processed_at: None,
            result: None,
        };
        
        let result = orchestrator.process_workflow(trigger).await.unwrap();
        
        assert!(result.processed);
        let result_msg = result.result.unwrap();
        assert!(result_msg.contains("Processed 1 tasks"));
        assert!(result_msg.contains("Approval request created"));
    }
    
    #[tokio::test]
    async fn test_workflow_orchestrator_step_failure() {
        let test_task = ZohoTask {
            id: "task-789".to_string(),
            subject: "Test Task Fail".to_string(),
            description: Some("Test failing task description".to_string()),
            status: "Not Started".to_string(),
            contact_id: Some("contact-101".to_string()),
            created_time: Utc::now().to_rfc3339(),
        };
        
        let mock_steps = MockWorkflowSteps::new_with_tasks(vec![test_task])
            .with_failure_at("load_contact");
        let orchestrator = WorkflowOrchestrator::new(mock_steps);
        
        let trigger = WorkflowTrigger {
            trigger_id: "test-789".to_string(),
            requested_by: UserId::new(11111),
            requested_at: Utc::now(),
            max_tasks: 1,
            dry_run: false,
            processed: false,
            processed_at: None,
            result: None,
        };
        
        let result = orchestrator.process_workflow(trigger).await.unwrap();
        
        assert!(result.processed);
        let result_msg = result.result.unwrap();
        assert!(result_msg.contains("No tasks were successfully processed"));
        // Error details should be in the logs, not in the final result message
    }
    
    #[tokio::test]
    async fn test_workflow_orchestrator_multiple_tasks() {
        let test_tasks = vec![
            ZohoTask {
                id: "task-001".to_string(),
                subject: "Test Task 1".to_string(),
                description: Some("Test task 1 description".to_string()),
                status: "Not Started".to_string(),
                contact_id: Some("contact-001".to_string()),
                created_time: Utc::now().to_rfc3339(),
            },
            ZohoTask {
                id: "task-002".to_string(),
                subject: "Test Task 2".to_string(),
                description: Some("Test task 2 description".to_string()),
                status: "Not Started".to_string(),
                contact_id: Some("contact-002".to_string()),
                created_time: Utc::now().to_rfc3339(),
            },
        ];
        
        let mock_steps = MockWorkflowSteps::new_with_tasks(test_tasks)
            .with_approval_state(ApprovalState::Approved);
        let orchestrator = WorkflowOrchestrator::new(mock_steps);
        
        let trigger = WorkflowTrigger {
            trigger_id: "test-multi".to_string(),
            requested_by: UserId::new(22222),
            requested_at: Utc::now(),
            max_tasks: 2,
            dry_run: false,
            processed: false,
            processed_at: None,
            result: None,
        };
        
        let result = orchestrator.process_workflow(trigger).await.unwrap();
        
        assert!(result.processed);
        let result_msg = result.result.unwrap();
        assert!(result_msg.contains("Processed 2 tasks"));
        assert!(result_msg.contains("task-001"));
        assert!(result_msg.contains("task-002"));
    }
}
*/