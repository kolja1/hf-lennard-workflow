//! Workflow processing service

use crate::error::{LennardError, Result};
use crate::types::{ZohoContact, LinkedInProfile, MailingAddress};
use crate::workflow::approval_types::LetterContent;
use crate::clients::{ZohoClient, BaserowClient, DossierClient, DossierResult, LetterExpressClient, LetterServiceClient, PDFService, TelegramClient};
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
    telegram_client: Arc<TelegramClient>,
    _approval_queue: Arc<ApprovalQueue>,
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
        telegram_client: Arc<TelegramClient>,
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
            _approval_queue: approval_queue,
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
    
    async fn request_approval(&self, letter: &LetterContent, contact: &ZohoContact) -> Result<ApprovalState> {
        use crate::types::PDFTemplateData;
        
        // Generate a unique approval ID
        let approval_id = uuid::Uuid::new_v4().to_string();
        
        log::info!("Creating approval request {} for letter to {}", approval_id, contact.full_name);
        
        // Get mailing address from contact
        let mailing_address = contact.mailing_address.as_ref()
            .ok_or_else(|| LennardError::Workflow(
                format!("Contact {} has no mailing address", contact.full_name)
            ))?;
        
        // Create strongly typed PDF data
        let pdf_template_data = PDFTemplateData::from_letter_and_address(letter, mailing_address);
        
        log::info!("Generating PDF for approval request {}", approval_id);
        let pdf_data = self.pdf_service.generate_pdf_typed("letter_template.odt", &pdf_template_data).await
            .map_err(|e| {
                log::error!("Failed to generate PDF for approval: {}", e);
                LennardError::ServiceUnavailable(format!("PDF generation failed: {}", e))
            })?;
        
        log::info!("PDF generated successfully, {} bytes", pdf_data.len());
        
        // Send approval request to Telegram with PDF
        self.telegram_client
            .send_approval_request_with_pdf(letter, contact, &approval_id, pdf_data)
            .await?;
        
        log::info!("Telegram approval message with PDF sent for approval_id: {}", approval_id);
        
        // Return pending approval state
        // The Python bot will handle the callback and update the approval state
        Ok(ApprovalState::PendingApproval)
    }
    
    async fn send_pdf(&self, letter: &LetterContent, contact: &ZohoContact) -> Result<String> {
        use crate::types::{LetterExpressRequest, MailingAddress, PrintColor, PrintMode, ShippingType, PDFTemplateData};
        
        // Get mailing address from contact - REQUIRED
        let recipient_address = contact.mailing_address.as_ref()
            .ok_or_else(|| LennardError::Workflow(
                format!("Contact {} has no mailing address. Address must be extracted from company dossier first.", contact.full_name)
            ))?;
        
        // Create strongly typed PDF data
        let pdf_template_data = PDFTemplateData::from_letter_and_address(letter, recipient_address);
        
        // Generate PDF using template with strongly typed data
        let pdf_data = self.pdf_service.generate_pdf_typed("letter_template.odt", &pdf_template_data).await?;
        
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
        
        // Send via LetterExpress
        self.letterexpress_client.send_letter(&request).await
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