//! Telegram client for sending approval notifications

use crate::error::{LennardError, Result};
use crate::config::TelegramConfig;
use crate::workflow::approval_types::LetterContent;
use crate::types::ZohoContact;
use reqwest::{Client as HttpClient, multipart};
use serde_json::json;

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
    
    /// Escape special characters for Telegram HTML parse mode
    fn escape_html(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
    
    /// Send an approval request message to Telegram with action buttons (text only)
    pub async fn send_approval_request(
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
            <b>Empfänger:</b> {}\n\
            <b>Firma:</b> {}\n\
            <b>Betreff:</b> {}\n\n\
            <b>Brief:</b>\n{}\n\n{}...",
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
                    {"text": "✅ Genehmigen", "callback_data": format!("approve_{}", approval_id)},
                    {"text": "📝 Änderungen anfordern", "callback_data": format!("change_{}", approval_id)}
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
    
    /// Send an error notification to Telegram
    pub async fn send_error_notification(
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
    
    /// Send an approval request with PDF attachment to Telegram
    pub async fn send_approval_request_with_pdf(
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
            <b>Empfänger:</b> {}\n\
            <b>Firma:</b> {}\n\
            <b>Betreff:</b> {}\n\n\
            Bitte prüfen Sie den angehängten Brief.",
            escaped_name,
            escaped_company,
            escaped_subject
        );
        
        // Create inline keyboard with approval buttons
        let reply_markup = json!({
            "inline_keyboard": [[
                {"text": "✅ Genehmigen", "callback_data": format!("approve_{}", approval_id)},
                {"text": "📝 Änderungen anfordern", "callback_data": format!("change_{}", approval_id)}
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