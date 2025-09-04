//! Workflow step traits for strongly-typed workflow processing

use async_trait::async_trait;
use crate::error::Result;
use crate::types::{ZohoContact, LinkedInProfile, MailingAddress};
use crate::clients::DossierResult;
use super::approval_types::{LetterContent, ApprovalState};
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
    
    /// Step 6: Request approval - requires letter, returns approval status
    /// Note: This may create long-running approval requests handled by ApprovalQueue
    async fn request_approval(&self, letter: &LetterContent, contact: &ZohoContact) -> Result<ApprovalState>;
    
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
}