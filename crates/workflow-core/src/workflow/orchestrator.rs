//! Workflow orchestrator with strongly-typed steps

use super::traits::WorkflowSteps;
use super::approval_types::WorkflowTrigger;
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

        // CRITICAL: Mark task as "In Progress" IMMEDIATELY to prevent duplicate execution
        // This must happen before any long-running operations (dossier, letter, PDF generation)
        self.steps.mark_task_in_progress(&task.id).await
            .map_err(|e| LennardError::Workflow(format!("Failed to mark task as in progress: {}", e)))?;

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
                // Double-check the address is valid before using it
                if address.is_valid() {
                    log::info!("Step 3.5: Extracted valid mailing address for {}", contact.full_name);
                    // Update the local contact object with the extracted address
                    contact.mailing_address = Some(address.clone());
                    // Also update Zoho contact with address for persistence
                    self.steps.update_contact_address(&contact.id, &address).await?;
                } else {
                    return Err(LennardError::Workflow(format!(
                        "Extracted address for {} is invalid (empty fields). Cannot proceed without valid recipient address.",
                        contact.full_name
                    )));
                }
            } else {
                return Err(LennardError::Workflow(format!(
                    "Failed to extract mailing address for {}. Cannot proceed without recipient address. Please verify the company website is accessible.",
                    contact.full_name
                )));
            }
        } else {
            // Validate existing address
            if let Some(ref addr) = contact.mailing_address {
                if !addr.is_valid() {
                    return Err(LennardError::Workflow(format!(
                        "Contact {} has invalid mailing address (empty fields). Cannot proceed.",
                        contact.full_name
                    )));
                }
            }
        }
        
        // Step 4: Generate letter - requires contact, profile and dossier, guaranteed letter
        let letter = self.steps.generate_letter(&contact, &profile, &dossier_result).await
            .map_err(|e| LennardError::Workflow(format!("Step 4 (generate letter) failed: {}", e)))?;
        
        log::info!("Step 4: Generated letter with subject '{}'", letter.subject);
        
        // Step 5a: Start approval - creates and persists the approval request
        let approval_id = match self.steps.approval_start(&task.id, &contact, &profile, &letter, &dossier_result).await {
            Ok(id) => id,
            Err(e) => {
                let error = LennardError::Workflow(format!("Step 5a (approval start) failed: {}", e));
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
        
        log::info!("Step 5a: Created approval with ID: {}", approval_id);
        
        // Step 5b: Request approval - sends the notification for the persisted approval
        let approval_state = match self.steps.request_approval(&approval_id, &letter, &contact).await {
            Ok(a) => a,
            Err(e) => {
                let error = LennardError::Workflow(format!("Step 5b (request approval) failed: {}", e));
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
        
        log::info!("Step 5b: Approval status: {:?}", approval_state);
        
        // IMPORTANT: Workflow STOPS here and waits for user approval
        // The workflow will be continued via continue_after_approval() when the user approves
        // We should NEVER immediately proceed to Step 6 here
        
        Ok("Awaiting user response via Telegram".to_string())
    }
    
    /// Process improvement request - generate an improved letter based on feedback
    pub async fn process_improvement_request(
        &self, 
        approval_data: &super::approval_types::ApprovalData,
        feedback: &str
    ) -> Result<super::approval_types::ApprovalData> {
        use crate::workflow::approval_types::{ApprovalState, LetterHistoryEntry};
        use chrono::Utc;
        use base64::Engine;
        
        log::info!("Processing improvement request for approval {}", approval_data.approval_id);
        log::info!("Feedback: {}", feedback);
        
        // Create a new iteration of the letter using the feedback
        let mut improved_approval = approval_data.clone();
        
        // Generate improved letter using the letter service with feedback
        let improved_letter = self.steps.generate_improved_letter(
            approval_data,
            feedback
        ).await?;
        
        log::info!("Generated improved letter for approval {}", approval_data.approval_id);
        
        // Add current letter to history before updating
        let history_entry = LetterHistoryEntry {
            iteration: improved_approval.letter_history.len() as u32 + 1,
            content: approval_data.current_letter.clone(),
            feedback: approval_data.letter_history.last()
                .and_then(|e| e.feedback.clone()),
            created_at: Utc::now(),
        };
        improved_approval.letter_history.push(history_entry);
        
        // Update with improved letter
        improved_approval.current_letter = improved_letter;
        improved_approval.state = ApprovalState::PendingApproval;
        improved_approval.updated_at = Utc::now();
        
        // Generate new PDF for the improved letter using the stored mailing address
        let mailing_address = improved_approval.mailing_address.as_ref()
            .ok_or_else(|| LennardError::Workflow("Missing mailing address in approval data".to_string()))?;
        let pdf_bytes = self.steps.generate_pdf_with_address(&improved_approval.current_letter, mailing_address).await?;
        improved_approval.pdf_base64 = Some(base64::engine::general_purpose::STANDARD.encode(&pdf_bytes));
        
        // Send the improved letter to Telegram for re-approval
        log::info!("Sending improved letter to Telegram for approval {}", improved_approval.approval_id);
        self.steps.send_improved_approval_to_telegram(&improved_approval).await?;
        
        // Set state to awaiting response since we sent to Telegram
        improved_approval.state = ApprovalState::AwaitingUserResponse;
        
        // Log that the approval update is ready
        self.steps.request_approval_update(
            &improved_approval.approval_id.to_string(),
            improved_approval.letter_history.len()
        ).await?;
        
        Ok(improved_approval)
    }
    
    /// Continue workflow after approval - complete Step 6 (send PDF via LetterExpress)
    pub async fn continue_after_approval(&self, approval_data: &super::approval_types::ApprovalData) -> Result<String> {
        use base64::{Engine as _, engine::general_purpose};
        
        log::info!("Continuing workflow after approval for task: {}", approval_data.task_id);
        
        // The approval contains everything we need:
        // - The approved letter content
        // - The mailing address 
        // - The PDF (base64 encoded)
        
        // Check we have the required data
        let mailing_address = approval_data.mailing_address.as_ref()
            .ok_or_else(|| LennardError::Workflow("Approval missing mailing address".to_string()))?;
        
        let pdf_base64 = approval_data.pdf_base64.as_ref()
            .ok_or_else(|| LennardError::Workflow("Approval missing PDF data".to_string()))?;
        
        // Decode the approved PDF - we will use THIS PDF (not regenerate it)
        let pdf_data = general_purpose::STANDARD.decode(pdf_base64)
            .map_err(|e| LennardError::Workflow(format!("Failed to decode PDF: {}", e)))?;

        // Step 6: Send approved PDF via LetterExpress using the binary method
        // IMPORTANT: We use the EXACT PDF that was approved, not a regenerated one
        // This prevents page limit violations if the regenerated PDF differs from approved
        log::info!("Sending approved PDF via LetterExpress (NOT regenerating)");

        let tracking_id = self.steps.send_pdf_binary(pdf_data, mailing_address).await
            .map_err(|e| LennardError::Workflow(format!("Step 6 (send approved PDF) failed: {}", e)))?;
            
        log::info!("Step 6: Letter sent successfully after approval, tracking: {}", tracking_id);
        
        // Update task status in Zoho to mark as completed and attach the letter
        let task_id = approval_data.task_id.to_string();
        let success_message = format!("Brief erfolgreich versendet. Tracking: {}", tracking_id);
        
        // First, attach the PDF to the task
        let pdf_data = general_purpose::STANDARD.decode(pdf_base64)
            .map_err(|e| LennardError::Workflow(format!("Failed to decode PDF for attachment: {}", e)))?;
        
        let pdf_filename = format!("Brief_{}.pdf", approval_data.contact_id);
        if let Err(e) = self.steps.attach_file_to_task(&task_id, pdf_data, &pdf_filename).await {
            log::error!("Failed to attach PDF to Zoho task: {}", e);
            // Don't fail the whole workflow if attachment fails
        } else {
            log::info!("Attached PDF letter to Zoho task {}", task_id);
        }
        
        // Then update the status to completed
        if let Err(e) = self.steps.update_task_completed_status(&task_id, &success_message).await {
            log::error!("Failed to update Zoho task status to completed: {}", e);
            // Don't fail the whole workflow if status update fails
        } else {
            log::info!("Updated Zoho task {} status to 'Done'", task_id);
        }

        // Create follow-up task for the contact
        match self.steps.create_follow_up_task(approval_data.contact_id.as_str(), &task_id).await {
            Ok(follow_up_task_id) => {
                log::info!("Created follow-up task {} for contact {}", follow_up_task_id, approval_data.contact_id);
            }
            Err(e) => {
                log::error!("Failed to create follow-up task for contact {}: {}", approval_data.contact_id, e);
                // Don't fail the whole workflow if follow-up task creation fails
            }
        }

        Ok(format!("Letter sent successfully after approval, tracking: {}", tracking_id))
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
                person_dossier_content: "Mock person dossier content".to_string(),
                company_dossier_content: "Mock company dossier content".to_string(),
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
        
        async fn approval_start(&self, _task_id: &str, _contact: &ZohoContact, _profile: &LinkedInProfile, _letter: &LetterContent, _dossier: &DossierResult) -> Result<ApprovalId> {
            if self.should_fail_at_step == Some("approval_start") {
                return Err(LennardError::ServiceUnavailable("Approval start failed".to_string()));
            }
            // Return a mock approval ID
            Ok(ApprovalId::new())
        }
        
        async fn request_approval(&self, _approval_id: &ApprovalId, _letter: &LetterContent, _contact: &ZohoContact) -> Result<ApprovalState> {
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