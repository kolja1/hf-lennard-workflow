//! PDF service client

use crate::config::PDFServiceConfig;
use crate::error::{LennardError, Result};
use crate::types::PDFTemplateData;
use crate::paths;
use reqwest::{Client as HttpClient, multipart};
use std::collections::HashMap;

pub struct PDFService {
    config: PDFServiceConfig,
    http_client: HttpClient,
}

impl PDFService {
    pub fn new(config: PDFServiceConfig) -> Self {
        let http_client = HttpClient::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");
            
        Self {
            config,
            http_client,
        }
    }
    
    /// Generate PDF from template and strongly typed data
    pub async fn generate_pdf_typed(&self, template_path: &str, data: &PDFTemplateData) -> Result<Vec<u8>> {
        let url = format!("{}/generate-pdf", self.config.base_url);
        
        // Build full path to template file
        let full_template_path = paths::templates_dir().join(template_path);
        
        // Read template file
        let template_bytes = std::fs::read(&full_template_path)
            .map_err(|e| LennardError::Config(
                format!("Failed to read template file {}: {}", full_template_path.display(), e)
            ))?;
        
        // Serialize typed data to JSON string - the serde annotations handle the German names
        let json_data = serde_json::to_string(data)
            .map_err(|e| LennardError::Config(
                format!("Failed to serialize PDF template data: {}", e)
            ))?;
        
        // Create multipart form
        let template_part = multipart::Part::bytes(template_bytes)
            .file_name(template_path.to_string())
            .mime_str("application/vnd.oasis.opendocument.text")?;
            
        let json_part = multipart::Part::text(json_data)
            .file_name("data.json")
            .mime_str("application/json")?;
            
        let form = multipart::Form::new()
            .part("odt_file", template_part)
            .part("json_data", json_part);
        
        // Send multipart request
        let response = self.http_client
            .post(&url)
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());

            // Check for page limit violation (HTTP 400 with specific error message)
            if status == reqwest::StatusCode::BAD_REQUEST {
                // Try to parse JSON error response
                if let Ok(json_error) = serde_json::from_str::<serde_json::Value>(&error_text) {
                    if let Some(detail) = json_error.get("detail").and_then(|d| d.as_str()) {
                        // Check if it's a page limit error
                        // Example: "Document exceeds one page limit (generated 2 pages). Please reduce content length."
                        if detail.contains("exceeds one page limit") || (detail.contains("exceeds") && detail.contains("page")) {
                            // Extract page count using regex
                            if let Some(captures) = regex::Regex::new(r"generated (\d+) pages")
                                .ok()
                                .and_then(|re| re.captures(detail))
                            {
                                if let Some(page_count_str) = captures.get(1) {
                                    if let Ok(page_count) = page_count_str.as_str().parse::<u32>() {
                                        return Err(LennardError::PageLimitExceeded {
                                            page_count,
                                            limit: 1,
                                            message: detail.to_string(),
                                        });
                                    }
                                }
                            }

                            // If we couldn't parse the page count, still return page limit error with default
                            return Err(LennardError::PageLimitExceeded {
                                page_count: 2, // Default assumption
                                limit: 1,
                                message: detail.to_string(),
                            });
                        }
                    }
                }
            }

            return Err(LennardError::ServiceUnavailable(
                format!("PDF service returned {} - {}", status, error_text)
            ));
        }

        let pdf_data = response.bytes().await?;
        Ok(pdf_data.to_vec())
    }

    /// Generate PDF from template and data (legacy HashMap interface)
    pub async fn generate_pdf(&self, template_path: &str, data: &HashMap<String, serde_json::Value>) -> Result<Vec<u8>> {
        let url = format!("{}/generate-pdf", self.config.base_url);
        
        // Build full path to template file
        let full_template_path = paths::templates_dir().join(template_path);
        
        // Read template file
        let template_bytes = std::fs::read(&full_template_path)
            .map_err(|e| LennardError::Config(
                format!("Failed to read template file {}: {}", full_template_path.display(), e)
            ))?;
        
        // Serialize data to JSON string
        let json_data = serde_json::to_string(data)
            .map_err(|e| LennardError::Config(
                format!("Failed to serialize JSON data: {}", e)
            ))?;
        
        // Create multipart form
        let template_part = multipart::Part::bytes(template_bytes)
            .file_name(template_path.to_string())
            .mime_str("application/vnd.oasis.opendocument.text")?;
            
        let json_part = multipart::Part::text(json_data)
            .file_name("data.json")
            .mime_str("application/json")?;
            
        let form = multipart::Form::new()
            .part("odt_file", template_part)
            .part("json_data", json_part);
        
        // Send multipart request
        let response = self.http_client
            .post(&url)
            .multipart(form)
            .send()
            .await?;
            
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(LennardError::ServiceUnavailable(
                format!("PDF service returned {} - {}", status, error_text)
            ));
        }
        
        let pdf_data = response.bytes().await?;
        Ok(pdf_data.to_vec())
    }
    
    /// Validate PDF data
    pub fn validate_pdf(&self, pdf_data: &[u8]) -> Result<bool> {
        // Check PDF magic bytes
        if pdf_data.len() < 4 {
            return Ok(false);
        }
        
        Ok(pdf_data.starts_with(b"%PDF"))
    }
    
    /// Check PDF service health
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.config.base_url);
        
        let response = self.http_client
            .get(&url)
            .send()
            .await;
            
        match response {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),  // Connection failed
        }
    }
}

mod tests {

    #[test]
    fn test_pdf_service_creation() {
        use crate::config::PDFServiceConfig;
        use super::PDFService;
        
        let config = PDFServiceConfig {
            base_url: "http://localhost:8000".to_string(),
        };
        
        let service = PDFService::new(config.clone());
        assert_eq!(service.config.base_url, "http://localhost:8000");
    }

    #[test]
    fn test_validate_pdf_with_valid_data() {
        use crate::config::PDFServiceConfig;
        use super::PDFService;
        
        let config = PDFServiceConfig {
            base_url: "http://localhost:8000".to_string(),
        };
        let service = PDFService::new(config);
        
        // Valid PDF starts with %PDF
        let valid_pdf = b"%PDF-1.4\n...rest of pdf...";
        assert!(service.validate_pdf(valid_pdf).unwrap());
    }

    #[test]
    fn test_validate_pdf_with_invalid_data() {
        use crate::config::PDFServiceConfig;
        use super::PDFService;
        
        let config = PDFServiceConfig {
            base_url: "http://localhost:8000".to_string(),
        };
        let service = PDFService::new(config);
        
        // Invalid PDF data
        let invalid_pdf = b"Not a PDF file";
        assert!(!service.validate_pdf(invalid_pdf).unwrap());
    }

    #[test]
    fn test_validate_pdf_with_empty_data() {
        use crate::config::PDFServiceConfig;
        use super::PDFService;
        
        let config = PDFServiceConfig {
            base_url: "http://localhost:8000".to_string(),
        };
        let service = PDFService::new(config);
        
        // Empty data
        let empty_data = b"";
        assert!(!service.validate_pdf(empty_data).unwrap());
    }

    #[test]
    fn test_validate_pdf_with_short_data() {
        use crate::config::PDFServiceConfig;
        use super::PDFService;
        
        let config = PDFServiceConfig {
            base_url: "http://localhost:8000".to_string(),
        };
        let service = PDFService::new(config);
        
        // Too short to be a PDF
        let short_data = b"AB";
        assert!(!service.validate_pdf(short_data).unwrap());
    }

    #[test]
    fn test_pdf_template_data_serialization_for_service() {
        use crate::types::{PDFTemplateData, LetterContent, MailingAddress};
        
        // Create test data
        let letter = LetterContent {
            subject: "Test Subject".to_string(),
            greeting: "Sehr geehrter Herr Test".to_string(),
            body: "Test body".to_string(),
            sender_name: "Sender".to_string(),
            recipient_name: "Recipient".to_string(),
            company_name: "Company".to_string(),
        };
        
        let address = MailingAddress {
            street: "Test Street 123".to_string(),
            city: "Berlin".to_string(),
            state: None,
            postal_code: "10115".to_string(),
            country: "Germany".to_string(),
        };
        
        let pdf_data = PDFTemplateData::from_letter_and_address(&letter, &address);
        
        // Serialize to JSON as the service would
        let json = serde_json::to_string(&pdf_data).unwrap();
        
        // Verify it contains the German bookmark names
        assert!(json.contains("\"Betreff\":"));
        assert!(json.contains("\"Anrede\":"));
        assert!(json.contains("\"Brieftext\":"));
        assert!(json.contains("\"Sender-Name\":"));
        assert!(json.contains("\"Reciepient\":"));
        assert!(json.contains("\"Street-1\":"));
        assert!(json.contains("\"City\":"));
        assert!(json.contains("\"PLZ\":"));
        assert!(json.contains("\"Country\":"));
    }
}