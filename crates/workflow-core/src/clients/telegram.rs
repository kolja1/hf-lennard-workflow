//! Telegram client for sending approval notifications

use crate::error::{LennardError, Result};
use crate::config::TelegramConfig;
use crate::workflow::approval_types::LetterContent;
use crate::types::ZohoContact;
use reqwest::{Client as HttpClient, multipart};
use serde_json::json;
use async_trait::async_trait;

// CRITICAL: Callback data prefixes MUST match what the Python Telegram bot expects
// The Python bot (hf-lennard-telegram-bot/src/bot.py) listens for these exact prefixes
// to handle approval callbacks. If these don't match, the bot will respond with
// "❓ Unbekannte Aktion" and the workflow will fail after user approval.
// 
// Python bot expects:
// - "approve_workflow_{id}" for approvals (NOT just "approve_{id}")
// - "reject_workflow_{id}" for rejections (NOT "change_{id}" or "reject_{id}")
//
// Any change here MUST be synchronized with the Python bot's callback handlers!
const APPROVE_CALLBACK_PREFIX: &str = "approve_workflow_";
const REJECT_CALLBACK_PREFIX: &str = "reject_workflow_";

/// Trait for Telegram client operations - allows for mocking in tests
#[async_trait]
pub trait TelegramClientTrait: Send + Sync {
    /// Send an approval request message to Telegram with action buttons (text only)
    async fn send_approval_request(
        &self,
        letter: &LetterContent,
        contact: &ZohoContact,
        approval_id: &str
    ) -> Result<()>;
    
    /// Send an error notification to Telegram
    async fn send_error_notification(
        &self,
        task_id: &str,
        contact_name: &str,
        company_name: &str,
        error_message: &str
    ) -> Result<()>;
    
    /// Send an approval request with PDF attachment to Telegram
    async fn send_approval_request_with_pdf(
        &self,
        letter: &LetterContent,
        contact: &ZohoContact,
        approval_id: &str,
        pdf_data: Vec<u8>
    ) -> Result<()>;
}

pub struct TelegramClient {
    bot_token: String,
    chat_id: String,
    http_client: HttpClient,
}

impl TelegramClient {
    pub fn new(config: TelegramConfig) -> Self {
        let http_client = HttpClient::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
            
        Self {
            bot_token: config.bot_token,
            chat_id: config.chat_id,
            http_client,
        }
    }
    
    /// Generate approval callback data that matches Python bot format
    /// Returns: "approve_workflow_{approval_id}"
    pub fn generate_approve_callback(approval_id: &str) -> String {
        format!("{}{}", APPROVE_CALLBACK_PREFIX, approval_id)
    }
    
    /// Generate reject callback data that matches Python bot format
    /// Returns: "reject_workflow_{approval_id}"
    pub fn generate_reject_callback(approval_id: &str) -> String {
        format!("{}{}", REJECT_CALLBACK_PREFIX, approval_id)
    }
    
    /// Escape special characters for Telegram HTML parse mode
    fn escape_html(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
}

#[async_trait]
impl TelegramClientTrait for TelegramClient {
    async fn send_approval_request(
        &self,
        letter: &LetterContent,
        contact: &ZohoContact,
        approval_id: &str
    ) -> Result<()> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);
        
        // Escape HTML special characters
        let escaped_name = Self::escape_html(&contact.full_name);
        let escaped_company = Self::escape_html(&letter.company_name);
        let escaped_subject = Self::escape_html(&letter.subject);
        let escaped_greeting = Self::escape_html(&letter.greeting);
        let escaped_body = Self::escape_html(&letter.body[..500.min(letter.body.len())]);
        
        // Format message text with letter preview using HTML
        let message = format!(
            "📬 <b>Neue Briefgenehmigung erforderlich</b>\n\n\
            🔖 <b>Approval ID:</b> <code>{}</code>\n\n\
            <b>Empfänger:</b> {}\n\
            <b>Firma:</b> {}\n\
            <b>Betreff:</b> {}\n\n\
            <b>Brief:</b>\n{}\n\n{}...",
            approval_id,
            escaped_name,
            escaped_company,
            escaped_subject,
            escaped_greeting,
            escaped_body
        );
        
        // Create inline keyboard with approval buttons
        let payload = json!({
            "chat_id": self.chat_id,
            "text": message,
            "parse_mode": "HTML",
            "reply_markup": {
                "inline_keyboard": [[
                    {"text": "✅ Genehmigen", "callback_data": Self::generate_approve_callback(approval_id)},
                    {"text": "❌ Ablehnen", "callback_data": Self::generate_reject_callback(approval_id)}
                ]]
            }
        });
        
        let response = self.http_client
            .post(&url)
            .json(&payload)
            .send()
            .await?;
            
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LennardError::ServiceUnavailable(
                format!("Telegram API error: {}", error_text)
            ));
        }
        
        log::info!("Telegram approval message sent for approval_id: {}", approval_id);
        Ok(())
    }
    
    async fn send_error_notification(
        &self,
        task_id: &str,
        contact_name: &str,
        company_name: &str,
        error_message: &str
    ) -> Result<()> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);
        
        // Escape HTML special characters
        let escaped_contact = Self::escape_html(contact_name);
        let escaped_company = Self::escape_html(company_name);
        let escaped_error = Self::escape_html(error_message);
        
        // Format error message using HTML formatting
        let message = format!(
            "❌ <b>Workflow fehlgeschlagen!</b>\n\n\
            👤 <b>Kontakt:</b> {}\n\
            🏢 <b>Firma:</b> {}\n\
            📋 <b>Task ID:</b> {}...\n\
            ❗ <b>Fehler:</b> {}\n\
            ⏰ <b>Zeit:</b> {}\n\n\
            Bitte prüfen Sie die Task in Zoho CRM (Status: Warten auf Andere).",
            escaped_contact,
            escaped_company,
            &task_id[..8.min(task_id.len())],
            escaped_error,
            chrono::Local::now().format("%H:%M:%S")
        );
        
        // Send as plain message without buttons using HTML parse mode
        let payload = json!({
            "chat_id": self.chat_id,
            "text": message,
            "parse_mode": "HTML"
        });
        
        let response = self.http_client
            .post(&url)
            .json(&payload)
            .send()
            .await?;
            
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LennardError::ServiceUnavailable(
                format!("Telegram API error: {}", error_text)
            ));
        }
        
        log::info!("Telegram error notification sent for task: {}", task_id);
        Ok(())
    }
    
    async fn send_approval_request_with_pdf(
        &self,
        letter: &LetterContent,
        contact: &ZohoContact,
        approval_id: &str,
        pdf_data: Vec<u8>
    ) -> Result<()> {
        let url = format!("https://api.telegram.org/bot{}/sendDocument", self.bot_token);
        
        // Escape HTML special characters
        let escaped_name = Self::escape_html(&contact.full_name);
        let escaped_company = Self::escape_html(&letter.company_name);
        let escaped_subject = Self::escape_html(&letter.subject);
        
        // Create concise caption for the PDF using HTML
        let caption = format!(
            "📬 <b>Neue Briefgenehmigung erforderlich</b>\n\n\
            🔖 <b>Approval ID:</b> <code>{}</code>\n\n\
            <b>Empfänger:</b> {}\n\
            <b>Firma:</b> {}\n\
            <b>Betreff:</b> {}\n\n\
            Bitte prüfen Sie den angehängten Brief.",
            approval_id,
            escaped_name,
            escaped_company,
            escaped_subject
        );
        
        // Create inline keyboard with approval buttons
        let reply_markup = json!({
            "inline_keyboard": [[
                {"text": "✅ Genehmigen", "callback_data": Self::generate_approve_callback(approval_id)},
                {"text": "❌ Ablehnen", "callback_data": Self::generate_reject_callback(approval_id)}
            ]]
        });
        
        // Create multipart form
        let filename = format!("Brief_{}_{}.pdf", 
            contact.full_name.replace(" ", "_"),
            chrono::Utc::now().format("%Y%m%d_%H%M%S")
        );
        
        let part = multipart::Part::bytes(pdf_data)
            .file_name(filename)
            .mime_str("application/pdf")?;
            
        let form = multipart::Form::new()
            .text("chat_id", self.chat_id.clone())
            .text("caption", caption)
            .text("parse_mode", "HTML")
            .text("reply_markup", reply_markup.to_string())
            .part("document", part);
        
        let response = self.http_client
            .post(&url)
            .multipart(form)
            .send()
            .await?;
            
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LennardError::ServiceUnavailable(
                format!("Telegram API error: {}", error_text)
            ));
        }
        
        log::info!("Telegram approval message with PDF sent for approval_id: {}", approval_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_callback_generation_matches_python_bot_format() {
        // Test that the callback functions generate the correct format
        // that the Python Telegram bot expects
        let approval_id = "test-123-456";
        
        // Test approve callback
        let approve_callback = TelegramClient::generate_approve_callback(approval_id);
        assert_eq!(approve_callback, "approve_workflow_test-123-456");
        assert!(approve_callback.starts_with("approve_workflow_"));
        
        // Test reject callback  
        let reject_callback = TelegramClient::generate_reject_callback(approval_id);
        assert_eq!(reject_callback, "reject_workflow_test-123-456");
        assert!(reject_callback.starts_with("reject_workflow_"));
    }
    
    #[test]
    fn test_callback_generation_with_uuid() {
        // Test with a real UUID format like we use in production
        let approval_id = "2b4282ed-3bab-4aac-b7ab-320ecd461518";
        
        let approve_callback = TelegramClient::generate_approve_callback(approval_id);
        assert_eq!(approve_callback, "approve_workflow_2b4282ed-3bab-4aac-b7ab-320ecd461518");
        
        let reject_callback = TelegramClient::generate_reject_callback(approval_id);
        assert_eq!(reject_callback, "reject_workflow_2b4282ed-3bab-4aac-b7ab-320ecd461518");
    }
}