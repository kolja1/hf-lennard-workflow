//! Letter generation service using AI

use crate::error::{LennardError, Result};
use crate::types::{LetterContent, LinkedInProfile, ZohoContact};
use crate::config::OpenAIConfig;
use reqwest::Client as HttpClient;
use serde_json::json;

pub struct LetterGenerator {
    config: OpenAIConfig,
    http_client: HttpClient,
}

impl LetterGenerator {
    pub fn new(config: OpenAIConfig) -> Self {
        let http_client = HttpClient::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");
            
        Self {
            config,
            http_client,
        }
    }
    
    /// Generate personalized letter content
    pub async fn generate_letter(
        &self,
        contact: &ZohoContact,
        profile: &LinkedInProfile,
        company_info: Option<&str>,
    ) -> Result<LetterContent> {
        let prompt = self.build_prompt(contact, profile, company_info);
        
        let api_url = self.config.base_url.as_ref()
            .map(|url| format!("{}/chat/completions", url))
            .unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_string());
        
        let response = self.http_client
            .post(&api_url)
            .bearer_auth(&self.config.api_key)
            .json(&json!({
                "model": self.config.model,
                "messages": [
                    {
                        "role": "system",
                        "content": "You are a professional business letter writer. Generate personalized, engaging business letters in German."
                    },
                    {
                        "role": "user", 
                        "content": prompt
                    }
                ],
                "max_tokens": 1000,
                "temperature": 0.7
            }))
            .send()
            .await?;
            
        if !response.status().is_success() {
            return Err(LennardError::ServiceUnavailable(
                format!("OpenAI API returned {}", response.status())
            ));
        }
        
        let result: serde_json::Value = response.json().await?;
        
        let content = result["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| LennardError::Processing("No content in OpenAI response".to_string()))?;
        
        self.parse_letter_content(content, contact)
    }
    
    fn build_prompt(&self, contact: &ZohoContact, profile: &LinkedInProfile, company_info: Option<&str>) -> String {
        format!(
            "Generate a personalized business letter for:\n\
            Name: {}\n\
            Company: {}\n\
            LinkedIn Profile: {}\n\
            {}\n\
            \n\
            The letter should be professional, engaging, and personalized based on the provided information.",
            contact.full_name,
            contact.company.as_ref().unwrap_or(&"Unknown".to_string()),
            profile.headline.as_ref().unwrap_or(&"Professional".to_string()),
            company_info.unwrap_or("")
        )
    }
    
    fn parse_letter_content(&self, content: &str, contact: &ZohoContact) -> Result<LetterContent> {
        // Simplified parsing - real implementation would use more sophisticated parsing
        
        Ok(LetterContent {
            subject: "Geschäftliche Anfrage".to_string(), // Default subject
            greeting: format!("Sehr geehrte Damen und Herren,"),
            body: content.to_string(),
            sender_name: "Ihr Lennard Team".to_string(),
            recipient_name: contact.full_name.clone(),
            company_name: contact.company.clone().unwrap_or_else(|| "Unbekannt".to_string()),
        })
    }
}