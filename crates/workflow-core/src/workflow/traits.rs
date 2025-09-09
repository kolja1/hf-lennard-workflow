//! Workflow step traits for strongly-typed workflow processing

use async_trait::async_trait;
use crate::error::Result;
use crate::types::{ZohoContact, LinkedInProfile, MailingAddress};
use crate::clients::DossierResult;
use super::approval_types::{LetterContent, ApprovalState, ApprovalId};
use zoho_generated_types::TasksResponse;

/// Trait defining the individual workflow steps with strongly-typed parameters
/// 
/// Each step has explicit, required parameters - no optional context objects.
/// This enables compile-time safety and easy mocking for tests.
#[async_trait]
pub trait WorkflowSteps: Send + Sync {
    /// Load available tasks for processing - returns up to max_count tasks
    /// This is the primary entry point for dynamic task selection
    async fn load_available_tasks(&self, max_count: u32) -> Result<Vec<TasksResponse>>;
    
    /// Step 1: Load task by ID - returns required TasksResponse (for single task processing)
    async fn load_task(&self, task_id: &str) -> Result<TasksResponse>;
    
    /// Step 2: Load contact - requires TasksResponse, returns required ZohoContact
    async fn load_contact(&self, task: &TasksResponse) -> Result<ZohoContact>;
    
    /// Step 3: Load profile - requires ZohoContact, returns required LinkedInProfile
    async fn load_profile(&self, contact: &ZohoContact) -> Result<LinkedInProfile>;
    
    /// Step 4: Generate dossiers - requires profile and contact, returns extracted data
    async fn generate_dossiers(&self, profile: &LinkedInProfile, contact_id: &str) -> Result<DossierResult>;
    
    /// Step 4.5: Update contact address in Zoho CRM
    async fn update_contact_address(&self, contact_id: &str, address: &MailingAddress) -> Result<()>;
    
    /// Step 5: Generate letter - requires contact, profile and dossier, returns required LetterContent
    async fn generate_letter(&self, contact: &ZohoContact, profile: &LinkedInProfile, dossier: &DossierResult) -> Result<LetterContent>;
    
    /// Step 6a: Start approval - creates and persists the approval request, returns approval ID
    async fn approval_start(&self, task_id: &str, contact: &ZohoContact, letter: &LetterContent, dossier: &DossierResult) -> Result<ApprovalId>;
    
    /// Step 6b: Request approval - sends the approval request notification, returns approval status
    /// Note: This sends the notification for an already-created approval
    async fn request_approval(&self, approval_id: &ApprovalId, letter: &LetterContent, contact: &ZohoContact) -> Result<ApprovalState>;
    
    /// Step 7: Send PDF - requires approved letter and contact, returns tracking ID
    async fn send_pdf(&self, letter: &LetterContent, contact: &ZohoContact) -> Result<String>;
    
    /// Send error notification via Telegram
    async fn send_error_notification(
        &self,
        task_id: &str,
        contact_name: &str,
        company_name: &str,
        error_message: &str
    ) -> Result<()>;
    
    /// Update Zoho task status with error
    async fn update_task_error_status(
        &self,
        task_id: &str,
        error_message: &str
    ) -> Result<()>;
    
    /// Update Zoho task status to completed
    async fn update_task_completed_status(
        &self,
        task_id: &str,
        success_message: &str
    ) -> Result<()>;
    
    /// Attach file to Zoho task
    async fn attach_file_to_task(
        &self,
        task_id: &str,
        file_data: Vec<u8>,
        filename: &str
    ) -> Result<()>;
    
    /// Generate improved letter based on feedback
    async fn generate_improved_letter(
        &self,
        approval_data: &super::approval_types::ApprovalData,
        feedback: &str
    ) -> Result<LetterContent>;
    
    /// Generate PDF from letter content with mailing address
    async fn generate_pdf_with_address(&self, letter: &LetterContent, address: &MailingAddress) -> Result<Vec<u8>>;
    
    /// Log that an approval update is ready for re-review
    async fn request_approval_update(
        &self,
        approval_id: &str,
        iteration_count: usize
    ) -> Result<()>;
    
    /// Send improved approval to Telegram (after revision)
    async fn send_improved_approval_to_telegram(
        &self,
        approval_data: &super::approval_types::ApprovalData
    ) -> Result<()>;
}