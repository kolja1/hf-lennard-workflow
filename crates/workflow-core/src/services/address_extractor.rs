//! Address extraction service

use crate::error::Result;
use crate::types::MailingAddress;
use crate::config::OpenAIConfig;
use regex::Regex;

pub struct AddressExtractor {
    _config: OpenAIConfig,
    postal_code_regex: Regex,
}

impl AddressExtractor {
    pub fn new(config: OpenAIConfig) -> Self {
        let postal_code_regex = Regex::new(r"\b\d{5}\b")
            .expect("Failed to compile postal code regex");
            
        Self {
            _config: config,
            postal_code_regex,
        }
    }
    
    /// Extract address from text (simplified implementation)
    pub fn extract_address(&self, text: &str) -> Result<Option<MailingAddress>> {
        // This is a simplified implementation
        // Real implementation would use more sophisticated parsing
        
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() < 3 {
            return Ok(None);
        }
        
        // Look for postal code
        let postal_code = self.postal_code_regex.find(text)
            .map(|m| m.as_str().to_string());
            
        if let Some(code) = postal_code {
            // Simple heuristic: assume structure is street, city+code, country
            return Ok(Some(MailingAddress {
                street: lines[0].to_string(),
                city: "Unknown".to_string(), // Would parse from text in real implementation
                state: None,
                postal_code: code,
                country: "Germany".to_string(),
            }));
        }
        
        Ok(None)
    }
    
    /// Validate German postal code
    pub fn validate_german_postal_code(&self, code: &str) -> bool {
        self.postal_code_regex.is_match(code)
    }
}

impl Default for AddressExtractor {
    fn default() -> Self {
        let default_config = OpenAIConfig {
            api_key: String::new(),
            model: "gpt-3.5-turbo".to_string(),
            base_url: None,
        };
        Self::new(default_config)
    }
}