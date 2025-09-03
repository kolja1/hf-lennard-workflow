//! LetterExpress client for sending physical mail

use crate::config::LetterExpressConfig;
use crate::error::{LennardError, Result};
use crate::types::LetterExpressRequest;
use reqwest::Client as HttpClient;
use serde_json;
use base64::{Engine as _, engine::general_purpose};
use log::{debug, info, warn, error};

pub struct LetterExpressClient {
    config: LetterExpressConfig,
    http_client: HttpClient,
}

impl LetterExpressClient {
    pub fn new(config: LetterExpressConfig) -> Self {
        let http_client = HttpClient::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");
            
        Self {
            config,
            http_client,
        }
    }
    
    /// Send letter via LetterExpress
    pub async fn send_letter(&self, request: &LetterExpressRequest) -> Result<String> {
        let url = format!("{}/setJob", self.config.base_url);
        
        // LetterExpress API v3 requires JSON body with auth
        // Note: For actual implementation, PDF needs to be base64 encoded
        let pdf_base64 = general_purpose::STANDARD.encode(&request.pdf_data);
        
        let request_body = serde_json::json!({
            "auth": {
                "username": self.config.username,
                "apikey": self.config.api_key,
                "mode": self.config.mode
            },
            "letter": {
                "base64_file": pdf_base64,
                "base64_checksum": "",  // Optional MD5 checksum
                "specification": {
                    "color": if request.color == crate::types::PrintColor::Color { 1 } else { 0 },
                    "mode": if request.mode == crate::types::PrintMode::Duplex { "duplex" } else { "simplex" },
                    "ship": if request.shipping == crate::types::ShippingType::Standard { "national" } else { "international" }
                }
            }
        });
        
        let response = self.http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;
            
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(LennardError::ServiceUnavailable(
                format!("LetterExpress returned error: {}", error_text)
            ));
        }
        
        let result: serde_json::Value = response.json().await?;
        
        // The response should contain a job_id or similar identifier
        Ok(result["job_id"].as_str()
            .or_else(|| result["id"].as_str())
            .or_else(|| result["jid"].as_str())
            .unwrap_or("unknown")
            .to_string())
    }
    
    /// Test connection to LetterExpress service
    pub async fn test_connection(&self) -> Result<bool> {
        // Use /v3/balance endpoint with JSON body auth
        let url = format!("{}/balance", self.config.base_url);
        
        // LetterExpress requires auth in JSON body, not headers
        let auth_body = serde_json::json!({
            "auth": {
                "username": self.config.username,
                "apikey": self.config.api_key,
                "mode": self.config.mode
            }
        });
        
        debug!("Testing LetterExpress connection to: {}", url);
        debug!("Using auth mode: {}", self.config.mode);
        
        let response = self.http_client
            .get(&url)  // GET request with JSON body
            .header("Content-Type", "application/json")
            .json(&auth_body)
            .send()
            .await;
            
        match response {
            Ok(resp) => {
                let status = resp.status();
                debug!("LetterExpress response status: {}", status);
                
                if status.is_success() {
                    // Try to parse response to verify it's valid
                    let body_text = resp.text().await.unwrap_or_else(|e| {
                        warn!("Failed to read response body: {}", e);
                        "Failed to read body".to_string()
                    });
                    
                    debug!("Response body: {}", body_text);
                    
                    if let Ok(body) = serde_json::from_str::<serde_json::Value>(&body_text) {
                        // Check if we got a valid balance response
                        // Response format: {"status":200,"message":"OK","data":{"balance":94.11,"currency":"EUR"}}
                        let has_balance = body.get("status").and_then(|s| s.as_i64()) == Some(200) &&
                                        body.get("data").and_then(|d| d.get("balance")).is_some();
                        
                        if has_balance {
                            if let Some(balance) = body.get("data").and_then(|d| d.get("balance")) {
                                info!("LetterExpress authentication successful - balance: {} EUR", balance);
                            }
                        } else {
                            warn!("LetterExpress response missing expected fields. Response: {}", body_text);
                        }
                        
                        Ok(has_balance)
                    } else {
                        error!("Failed to parse LetterExpress JSON response: {}", body_text);
                        Ok(false)
                    }
                } else {
                    let error_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                    warn!("LetterExpress API error ({}): {}", status, error_text);
                    Ok(false)
                }
            }
            Err(e) => {
                error!("Failed to connect to LetterExpress: {}", e);
                Ok(false)  // Connection failed
            }
        }
    }
}