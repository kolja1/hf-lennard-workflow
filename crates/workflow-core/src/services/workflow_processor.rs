//! Workflow processing service

use crate::error::{LennardError, Result};
use crate::types::{ZohoContact, LinkedInProfile, MailingAddress, PDFTemplateData};
use crate::workflow::approval_types::{LetterContent, ApprovalId};
use crate::clients::{ZohoClient, BaserowClient, DossierClient, DossierResult, LetterExpressClient, LetterServiceClient, PDFService, TelegramClientTrait};
use crate::clients::zoho::Authenticated;  // Import the authenticated state
use crate::services::AddressExtractor;
use crate::workflow::{WorkflowSteps, approval_types::ApprovalState, ApprovalQueue};
use std::sync::Arc;
use async_trait::async_trait;
use zoho_generated_types::TasksResponse;

// Constants for Zoho task filters
const TASK_SUBJECT_FILTER: &str = "Connect on LinkedIn";
const TASK_STATUS_FILTER: &str = "Nicht gestartet";  // German: "Not started"
const TASK_OWNER_ID: &str = "1294764000001730350";   // Lennard's Zoho user ID

pub struct WorkflowProcessor {
    zoho_client: Arc<ZohoClient<Authenticated>>,  // Type-safe authenticated client
    baserow_client: Arc<BaserowClient>,
    dossier_client: Arc<DossierClient>,
    letterexpress_client: Arc<LetterExpressClient>,
    pdf_service: Arc<PDFService>,
    _address_extractor: Arc<AddressExtractor>,
    letter_service: Arc<LetterServiceClient>,
    telegram_client: Arc<dyn TelegramClientTrait>,
    approval_queue: Arc<ApprovalQueue>,
}

impl WorkflowProcessor {
    pub fn new(
        zoho_client: Arc<ZohoClient<Authenticated>>,
        baserow_client: Arc<BaserowClient>,
        dossier_client: Arc<DossierClient>,
        letterexpress_client: Arc<LetterExpressClient>,
        pdf_service: Arc<PDFService>,
        address_extractor: Arc<AddressExtractor>,
        letter_service: Arc<LetterServiceClient>,
        telegram_client: Arc<dyn TelegramClientTrait>,
        approval_queue: Arc<ApprovalQueue>,
    ) -> Self {
        Self {
            zoho_client,
            baserow_client,
            dossier_client,
            letterexpress_client,
            pdf_service,
            _address_extractor: address_extractor,
            letter_service,
            telegram_client,
            approval_queue,
        }
    }
    
    /// Process a single workflow task (7-step workflow)
    pub async fn process_task(&self, task_id: &str) -> Result<()> {
        log::info!("Processing task: {}", task_id);
        
        // Step 1: Load task from Zoho CRM using single record endpoint
        let task = self.zoho_client.get_task_by_id(task_id).await?
            .ok_or_else(|| LennardError::Workflow(format!("Task {} not found", task_id)))?;
        
        log::info!("Step 1: Loaded task '{}'", task.subject);
        
        // Step 2: Fetch contact with LinkedIn ID
        // Extract contact_id from the who_id field (the associated contact)
        let contact_id = task.who_id
            .as_ref()
            .map(|who| who.id.clone())
            .ok_or_else(|| LennardError::Workflow("Task has no associated contact".to_string()))?;
            
        let mut contact = self.zoho_client.get_contact(&contact_id).await?
            .ok_or_else(|| LennardError::Workflow(format!("Contact {} not found", contact_id)))?;
        
        let linkedin_id = contact.linkedin_id.as_ref()
            .ok_or_else(|| LennardError::Workflow("Contact has no LinkedIn ID".to_string()))?;
            
        log::info!("Step 2: Fetched contact '{}' with LinkedIn ID '{}'", contact.full_name, linkedin_id);
        
        // Step 3: Load LinkedIn profile and generate dossiers if needed
        let profile = self.baserow_client.get_linkedin_profile(linkedin_id).await?
            .ok_or_else(|| LennardError::Workflow(format!("LinkedIn profile {} not found", linkedin_id)))?;
            
        // Generate dossiers and get extracted data
        let dossier_result = self.dossier_client.generate_and_get_dossiers(&serde_json::to_value(&profile)?, &contact_id).await?;
        
        log::info!("Step 3: Generated dossiers for profile '{}'", profile.full_name);
        log::info!("  - Extracted company name: {}", dossier_result.company_name);
        log::info!("  - Address found: {}", dossier_result.mailing_address.is_some());
        
        // Step 4: Extract and update mailing address if missing
        if contact.mailing_address.is_none() {
            if let Some(address) = &dossier_result.mailing_address {
                log::info!("Step 4: Updating contact with extracted mailing address");
                log::info!("  - Street: {}", address.street);
                log::info!("  - City: {}", address.city);
                log::info!("  - Postal Code: {}", address.postal_code);
                log::info!("  - Country: {}", address.country);
                
                // Update the local contact object with the extracted address
                contact.mailing_address = Some(address.clone());
                
                // Also update in Zoho CRM for persistence
                self.zoho_client.update_contact_address(&contact_id, address).await?;
                log::info!("Step 4: Successfully updated contact address in Zoho CRM");
            } else {
                log::info!("Step 4: No mailing address found in dossier");
            }
        } else {
            log::info!("Step 4: Contact already has mailing address, skipping update");
        }
        
        // Step 5: Generate letter content
        let _letter = self.letter_service.generate_letter(&contact, &profile, &dossier_result).await?;
        log::info!("Step 5: Generated letter content");
        
        // Step 6: Request approval via Telegram
        // This would be handled by the Python orchestrator for now
        log::info!("Step 6: Letter ready for approval");
        
        // Step 7: Send PDF via LetterExpress (when approved)
        // This would be triggered after approval
        log::info!("Step 7: Ready to send when approved");
        
        Ok(())
    }
}

/// Implementation of WorkflowSteps trait for the existing WorkflowProcessor
/// This connects the new strongly-typed trait system to the existing business logic
#[async_trait]
impl WorkflowSteps for WorkflowProcessor {
    async fn load_available_tasks(&self, max_count: u32) -> Result<Vec<TasksResponse>> {
        // Define task selection criteria for workflow processing
        // Note: These filters need to match actual Zoho task data
        // You may need to adjust based on your Zoho CRM configuration
        let filters = &[
            ("Subject", TASK_SUBJECT_FILTER),
            ("Status", TASK_STATUS_FILTER),
            ("Owner", TASK_OWNER_ID),
        ];
        
        log::info!("Loading up to {} available tasks with filters: {:?}", max_count, filters);
        
        let mut tasks = self.zoho_client.get_tasks(filters).await?;
        
        log::info!("Search API returned {} tasks (all should belong to Lennard)", tasks.len());
        
        // Sort by creation time (oldest first) - using optional created_time field
        tasks.sort_by(|a, b| {
            let a_time = a.created_time.as_deref().unwrap_or("");
            let b_time = b.created_time.as_deref().unwrap_or("");
            a_time.cmp(b_time)
        });
        
        // Limit to max_count
        tasks.truncate(max_count as usize);
        
        log::info!("Found {} available tasks for processing", tasks.len());
        for task in &tasks {
            let contact_info = task.who_id.as_ref()
                .map(|who| format!("{} ({})", who.id, who.name.as_deref().unwrap_or("Unknown")))
                .unwrap_or_else(|| "No contact".to_string());
            log::info!("  - Task {}: {} (Contact: {})", 
                      task.id, task.subject, contact_info);
        }
        
        Ok(tasks)
    }

    async fn load_task(&self, task_id: &str) -> Result<TasksResponse> {
        // Use the single record endpoint to get complete task data including Who_Id
        self.zoho_client.get_task_by_id(task_id).await?
            .ok_or_else(|| LennardError::Workflow(format!("Task {} not found", task_id)))
    }
    
    async fn load_contact(&self, task: &TasksResponse) -> Result<ZohoContact> {
        // Extract contact_id from the who_id field
        let contact_id = task.who_id
            .as_ref()
            .map(|who| who.id.clone())
            .ok_or_else(|| LennardError::Workflow("Task has no associated contact".to_string()))?;
            
        self.zoho_client.get_contact(&contact_id).await?
            .ok_or_else(|| LennardError::Workflow(format!("Contact {} not found", contact_id)))
    }
    
    async fn load_profile(&self, contact: &ZohoContact) -> Result<LinkedInProfile> {
        let linkedin_id = contact.linkedin_id.as_ref()
            .ok_or_else(|| LennardError::Workflow("Contact has no LinkedIn ID".to_string()))?;
            
        self.baserow_client.get_linkedin_profile(linkedin_id).await?
            .ok_or_else(|| LennardError::Workflow(format!("LinkedIn profile {} not found", linkedin_id)))
    }
    
    async fn generate_dossiers(&self, profile: &LinkedInProfile, contact_id: &str) -> Result<crate::clients::DossierResult> {
        self.dossier_client.generate_and_get_dossiers(&serde_json::to_value(profile)?, contact_id).await
    }
    
    async fn update_contact_address(&self, contact_id: &str, address: &MailingAddress) -> Result<()> {
        self.zoho_client.update_contact_address(contact_id, address).await
    }
    
    async fn generate_letter(&self, contact: &ZohoContact, profile: &LinkedInProfile, dossier: &DossierResult) -> Result<LetterContent> {
        // Use the letter service which returns the correct LetterContent type
        self.letter_service.generate_letter(contact, profile, dossier).await
    }
    
    async fn approval_start(&self, task_id: &str, contact: &ZohoContact, letter: &LetterContent, dossier: &DossierResult) -> Result<ApprovalId> {
        use crate::workflow::approval_types::{TaskId, ContactId, UserId};
        use crate::types::PDFTemplateData;
        use base64::{Engine as _, engine::general_purpose};
        
        log::info!("Starting approval for task {} and contact {}", task_id, contact.full_name);
        
        // Get mailing address - REQUIRED for later PDF sending
        let mailing_address = contact.mailing_address.as_ref()
            .ok_or_else(|| LennardError::Workflow(
                format!("Contact {} has no mailing address", contact.full_name)
            ))?;
        
        // Generate PDF now so it's included in the approval
        log::info!("Generating PDF for approval");
        let pdf_template_data = PDFTemplateData::from_letter_and_address(letter, mailing_address);
        let pdf_data = self.pdf_service.generate_pdf_typed("letter_template.odt", &pdf_template_data).await
            .map_err(|e| {
                log::error!("Failed to generate PDF for approval: {}", e);
                LennardError::ServiceUnavailable(format!("PDF generation failed: {}", e))
            })?;
        
        log::info!("PDF generated successfully, {} bytes", pdf_data.len());
        
        // Create the required types
        let task_id = TaskId::new(task_id.to_string());
        let contact_id = ContactId::new(contact.id.clone());
        let user_id = UserId::new(1); // TODO: Get actual user ID from context
        
        // Use company name from dossier (more reliable than contact.company)
        let company_name = dossier.company_name.clone();
        
        // Extract email and title from person dossier
        let mut recipient_email = None;
        let mut recipient_title = None;
        
        // Parse the person dossier to extract email and title
        let person_dossier = &dossier.person_dossier_content;
        for line in person_dossier.lines() {
            if line.starts_with("**Email**:") || line.starts_with("**Email**: ") {
                recipient_email = Some(line.replace("**Email**:", "").replace("**Email**: ", "").trim().to_string());
            } else if line.starts_with("**Headline**:") || line.starts_with("**Headline**: ") {
                recipient_title = Some(line.replace("**Headline**:", "").replace("**Headline**: ", "").trim().to_string());
            }
        }
        
        // Extract industry and website from company dossier if available
        let mut industry = None;
        let mut website = None;
        
        let company_dossier = &dossier.company_dossier_content;
        for line in company_dossier.lines() {
            if line.starts_with("- **Industry**:") || line.starts_with("- **Industry**: ") {
                industry = Some(line.replace("- **Industry**:", "").replace("- **Industry**: ", "").trim().to_string());
            } else if line.starts_with("- **Website**:") || line.starts_with("- **Website**: ") {
                // Extract URL from markdown link if present
                if let Some(start) = line.find('[') {
                    if let Some(end) = line.find("](") {
                        website = Some(line[start+1..end].to_string());
                    }
                } else {
                    website = Some(line.replace("- **Website**:", "").replace("- **Website**: ", "").trim().to_string());
                }
            }
        }
        
        log::info!("Extracted metadata - Email: {:?}, Title: {:?}, Industry: {:?}, Website: {:?}", 
                  recipient_email, recipient_title, industry, website);
        
        // Create and persist the approval request with all necessary data
        let approval_id = self.approval_queue.create_approval(
            task_id,
            contact_id,
            contact.full_name.clone(),
            recipient_email,
            recipient_title,
            company_name,
            letter.clone(),
            user_id,
            Some(mailing_address.clone()),
            Some(general_purpose::STANDARD.encode(&pdf_data)),
            Some(dossier.person_dossier_content.clone()),
            Some(dossier.company_dossier_content.clone()),
            industry,
            website,
        )?;
        
        log::info!("Created approval with ID: {} (includes mailing address and PDF)", approval_id);
        Ok(approval_id)
    }
    
    async fn request_approval(&self, approval_id: &ApprovalId, letter: &LetterContent, contact: &ZohoContact) -> Result<ApprovalState> {
        use base64::{Engine as _, engine::general_purpose};
        
        // Use the existing approval ID that was already persisted
        let approval_id_str = approval_id.to_string();
        
        log::info!("Sending approval request {} for letter to {}", approval_id_str, contact.full_name);
        
        // Get the approval to retrieve the PDF that was already generated
        let approval_data = self.approval_queue.get_approval_request(approval_id, None)?
            .ok_or_else(|| LennardError::Workflow(
                format!("Approval {} not found", approval_id_str)
            ))?;
        
        // Decode the PDF from base64
        let pdf_data = approval_data.pdf_base64
            .ok_or_else(|| LennardError::Workflow("Approval has no PDF data".to_string()))
            .and_then(|base64| {
                general_purpose::STANDARD.decode(&base64)
                    .map_err(|e| LennardError::Workflow(format!("Failed to decode PDF: {}", e)))
            })?;
        
        log::info!("Retrieved PDF from approval, {} bytes", pdf_data.len());
        
        // Send approval request to Telegram with PDF
        self.telegram_client
            .send_approval_request_with_pdf(letter, contact, &approval_id_str, pdf_data)
            .await?;
        
        log::info!("Telegram approval message with PDF sent for approval_id: {}", approval_id_str);
        
        // Transition the approval to AwaitingUserResponse state now that it's been sent to Telegram
        // This is critical for the gRPC handler to find the approval when the user responds
        self.approval_queue.mark_as_awaiting_response(approval_id)?;
        
        log::info!("Transitioned approval {} to AwaitingUserResponse state", approval_id_str);
        
        // Return the updated state
        Ok(ApprovalState::AwaitingUserResponse)
    }
    
    async fn send_pdf(&self, letter: &LetterContent, contact: &ZohoContact) -> Result<String> {
        use crate::types::{LetterExpressRequest, MailingAddress, PrintColor, PrintMode, ShippingType, PDFTemplateData};
        use crate::paths::{pdfs_dir, letterexpress_logs_dir};
        use std::fs;
        
        // Get mailing address from contact - REQUIRED
        let recipient_address = contact.mailing_address.as_ref()
            .ok_or_else(|| LennardError::Workflow(
                format!("Contact {} has no mailing address. Address must be extracted from company dossier first.", contact.full_name)
            ))?;
        
        // Create strongly typed PDF data
        let pdf_template_data = PDFTemplateData::from_letter_and_address(letter, recipient_address);
        
        // Generate PDF using template with strongly typed data
        let pdf_data = self.pdf_service.generate_pdf_typed("letter_template.odt", &pdf_template_data).await?;
        
        // Save PDF locally first (for backup and debugging)
        let pdf_dir = pdfs_dir();
        fs::create_dir_all(&pdf_dir).ok();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let pdf_filename = format!("letter_{}_{}.pdf", 
            contact.id.replace(":", "_"),
            timestamp
        );
        let pdf_path = pdf_dir.join(&pdf_filename);
        if let Err(e) = fs::write(&pdf_path, &pdf_data) {
            log::warn!("Failed to save PDF locally: {}", e);
        } else {
            log::info!("PDF saved locally at: {:?}", pdf_path);
        }
        
        // Create sender address (placeholder)
        let sender_address = MailingAddress {
            street: "Example Street 123".to_string(),
            city: "Example City".to_string(),
            state: Some("Example State".to_string()),
            postal_code: "12345".to_string(),
            country: "Germany".to_string(),
        };
        
        // Create LetterExpress request
        let request = LetterExpressRequest {
            pdf_data,
            recipient_address: recipient_address.clone(),
            sender_address,
            color: PrintColor::Color,
            mode: PrintMode::Duplex,
            shipping: ShippingType::Standard,
        };
        
        // Try to send via LetterExpress with detailed error handling
        log::info!("Attempting to send letter via LetterExpress for contact: {} ({})", 
            contact.full_name, contact.id);
        
        match self.letterexpress_client.send_letter(&request).await {
            Ok(tracking_id) => {
                log::info!("Successfully sent letter via LetterExpress. Tracking ID: {}", tracking_id);
                Ok(tracking_id)
            },
            Err(e) => {
                // Log detailed error
                log::error!("LetterExpress API failed for contact {} ({}): {}", 
                    contact.full_name, contact.id, e);
                
                // Log to error file
                let error_log_dir = letterexpress_logs_dir();
                fs::create_dir_all(&error_log_dir).ok();
                let error_timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let error_log_file = error_log_dir.join(format!("error_{}_{}.log",
                    contact.id.replace(":", "_"),
                    error_timestamp
                ));
                let error_details = format!(
                    "LetterExpress Error Log\n\
                    ========================\n\
                    Timestamp: {}\n\
                    Contact: {} (ID: {})\n\
                    Company: {}\n\
                    Recipient Address:\n  {}\n  {}, {} {}\n  {}\n\
                    Error: {}\n\
                    PDF saved at: {:?}\n",
                    error_timestamp,
                    contact.full_name,
                    contact.id,
                    contact.company.as_deref().unwrap_or("Unknown"),
                    recipient_address.street,
                    recipient_address.city,
                    recipient_address.state.as_deref().unwrap_or(""),
                    recipient_address.postal_code,
                    recipient_address.country,
                    e,
                    pdf_path
                );
                fs::write(&error_log_file, &error_details).ok();
                log::info!("LetterExpress error details saved to: {:?}", error_log_file);
                
                // Note: LetterExpress error details are logged to file and will be included in the
                // main error message that gets sent to Telegram through the normal error flow
                
                // Return error with helpful context
                Err(LennardError::ServiceUnavailable(format!(
                    "LetterExpress service failed: {}. PDF was generated and saved locally at {}. Please check the LetterExpress credentials or send manually.",
                    e, pdf_filename
                )))
            }
        }
    }
    
    async fn send_error_notification(
        &self,
        task_id: &str,
        contact_name: &str,
        company_name: &str,
        error_message: &str
    ) -> Result<()> {
        self.telegram_client
            .send_error_notification(task_id, contact_name, company_name, error_message)
            .await
    }
    
    async fn update_task_error_status(
        &self,
        task_id: &str,
        error_message: &str
    ) -> Result<()> {
        // Update Zoho task status to "Warten auf Andere" (Waiting for Others)
        let error_description = format!("Workflow failed: {}", error_message);
        self.zoho_client
            .update_task_status(task_id, "Warten auf Andere", &error_description)
            .await
    }
    
    async fn update_task_completed_status(
        &self,
        task_id: &str,
        success_message: &str
    ) -> Result<()> {
        // Update Zoho task status to "Abgeschlossen" (Completed)
        self.zoho_client
            .update_task_status(task_id, "Abgeschlossen", success_message)
            .await
    }
    
    async fn attach_file_to_task(
        &self,
        task_id: &str,
        file_data: Vec<u8>,
        filename: &str
    ) -> Result<()> {
        // Attach file to Zoho task
        self.zoho_client
            .attach_file_to_task(task_id, file_data, filename)
            .await
    }
    
    async fn generate_improved_letter(
        &self,
        approval_data: &crate::workflow::approval_types::ApprovalData,
        feedback: &str
    ) -> Result<LetterContent> {
        log::info!("Generating improved letter based on feedback for {} at {}", 
                  approval_data.recipient_name, approval_data.company_name);
        
        // Use the letter service to generate an improved version with full context
        let improved_letter = self.letter_service
            .generate_improved_letter_with_approval(approval_data, feedback)
            .await?;
            
        Ok(improved_letter)
    }
    
    async fn generate_pdf_with_address(&self, letter: &LetterContent, address: &MailingAddress) -> Result<Vec<u8>> {
        log::info!("Generating PDF for letter with subject: {}", letter.subject);
        
        // Create PDF template data with the actual mailing address
        let pdf_template_data = PDFTemplateData::from_letter_and_address(letter, address);
        
        // Generate PDF using existing service method
        let pdf_bytes = self.pdf_service
            .generate_pdf_typed("letter_template.odt", &pdf_template_data)
            .await?;
            
        Ok(pdf_bytes)
    }
    
    async fn request_approval_update(
        &self,
        approval_id: &str,
        iteration_count: usize
    ) -> Result<()> {
        log::info!("Approval {} ready for iteration {} review", approval_id, iteration_count);
        
        // The actual Telegram notification will be sent when the ApprovalWatcher
        // processes the file from the pending_approval directory
        log::info!(
            "Approval {} has been improved and will be requeued for review (iteration {})",
            approval_id, iteration_count
        );
        
        Ok(())
    }
    
    async fn send_improved_approval_to_telegram(
        &self,
        approval_data: &super::super::workflow::approval_types::ApprovalData
    ) -> Result<()> {
        use base64::{Engine as _, engine::general_purpose};
        
        log::info!("Sending improved letter to Telegram for approval {}", approval_data.approval_id);
        
        // Decode the PDF from base64
        let pdf_data = approval_data.pdf_base64.as_ref()
            .ok_or_else(|| LennardError::Workflow("Approval has no PDF data".to_string()))
            .and_then(|base64| {
                general_purpose::STANDARD.decode(base64)
                    .map_err(|e| LennardError::Workflow(format!("Failed to decode PDF: {}", e)))
            })?;
        
        log::info!("Decoded PDF from approval, {} bytes", pdf_data.len());
        
        // Create a minimal ZohoContact from approval data for the Telegram API
        let contact = ZohoContact {
            id: approval_data.contact_id.to_string(),
            full_name: approval_data.recipient_name.clone(),
            email: None,
            phone: None,
            company: Some(approval_data.company_name.clone()),
            linkedin_id: None,
            mailing_address: approval_data.mailing_address.clone(),
        };
        
        // Send approval request to Telegram with PDF
        let approval_id_str = approval_data.approval_id.to_string();
        
        self.telegram_client
            .send_approval_request_with_pdf(
                &approval_data.current_letter,
                &contact,
                &approval_id_str,
                pdf_data
            )
            .await?;
        
        log::info!("Improved letter sent to Telegram for approval_id: {}", approval_id_str);
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zoho_filter_constants() {
        // Verify that the filter constants have the correct values
        assert_eq!(TASK_SUBJECT_FILTER, "Connect on LinkedIn", 
            "Subject filter must be 'Connect on LinkedIn' with capital I");
        
        assert_eq!(TASK_STATUS_FILTER, "Nicht gestartet", 
            "Status filter must be in German: 'Nicht gestartet'");
        
        assert_eq!(TASK_OWNER_ID, "1294764000001730350", 
            "Owner ID must be Lennard's Zoho user ID");
    }

    #[test]
    fn test_filter_values_match_expected() {
        // Create the filters array as used in load_available_tasks
        let filters = &[
            ("Subject", TASK_SUBJECT_FILTER),
            ("Status", TASK_STATUS_FILTER),
            ("Owner", TASK_OWNER_ID),
        ];

        // Verify the filter structure and values
        assert_eq!(filters.len(), 3, "Must have exactly 3 filters");
        
        assert_eq!(filters[0], ("Subject", "Connect on LinkedIn"),
            "First filter must be Subject='Connect on LinkedIn'");
        
        assert_eq!(filters[1], ("Status", "Nicht gestartet"),
            "Second filter must be Status='Nicht gestartet' (German)");
        
        assert_eq!(filters[2], ("Owner", "1294764000001730350"),
            "Third filter must be Owner with Lennard's ID");
    }

    #[test]
    fn test_no_english_status_in_filters() {
        // Ensure we're not using English status
        assert_ne!(TASK_STATUS_FILTER, "Not Started", 
            "Status filter must NOT be in English");
        
        assert_ne!(TASK_STATUS_FILTER, "Not started", 
            "Status filter must NOT be in English (lowercase)");
    }

    #[test]
    fn test_linkedin_capitalization() {
        // Ensure LinkedIn has correct capitalization
        assert!(TASK_SUBJECT_FILTER.contains("LinkedIn"), 
            "Subject must contain 'LinkedIn' with capital I");
        
        assert!(!TASK_SUBJECT_FILTER.contains("Linkedin"), 
            "Subject must NOT contain 'Linkedin' with lowercase i");
    }
    
}