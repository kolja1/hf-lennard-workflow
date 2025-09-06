//! Dossier client for generating company/person dossiers

use crate::config::DossierConfig;
use crate::error::{LennardError, Result};
use crate::types::MailingAddress;
use crate::paths;
use dossier_grpc_client::{
    DossierServiceClient,
    BothDossiersRequest,
};
use tonic::transport::Channel;
use std::fs;
use std::path::PathBuf;
use chrono::Utc;

/// Result from dossier generation containing structured data
#[derive(Debug, Clone)]
pub struct DossierResult {
    pub company_dossier_content: String,
    pub company_name: String,
    pub mailing_address: Option<MailingAddress>,
}

pub struct DossierClient {
    grpc_url: String,  // gRPC endpoint URL
}

impl DossierClient {
    pub fn new(config: DossierConfig) -> Self {
        let grpc_url = format!("http://{}:{}", config.grpc_host, config.grpc_port);
        log::info!("DossierClient configured for gRPC endpoint: {}", grpc_url);
        
        Self {
            grpc_url,
        }
    }
    
    /// Generate and upload dossiers for a LinkedIn profile (legacy method for compatibility)
    pub async fn generate_and_upload_dossiers(&self, linkedin_data: &serde_json::Value, contact_id: &str) -> Result<()> {
        // Use the new method and ignore the structured result for backward compatibility
        let _ = self.generate_and_get_dossiers(linkedin_data, contact_id).await?;
        Ok(())
    }
    
    /// Generate dossiers and return structured data with company name and address
    pub async fn generate_and_get_dossiers(
        &self,
        linkedin_data: &serde_json::Value,
        contact_id: &str
    ) -> Result<DossierResult> {
        let linkedin_data_str = linkedin_data.to_string();
        let linkedin_id = linkedin_data.get("linkedin_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        
        // Create gRPC connection on-demand
        match Channel::from_shared(self.grpc_url.clone()) {
                Ok(endpoint) => {
                    match endpoint.connect().await {
                        Ok(channel) => {
                            log::info!("Connected to gRPC service at {}", self.grpc_url);
                            let mut grpc_client = DossierServiceClient::new(channel);
                            
                            // Prepare request
                            let request_msg = BothDossiersRequest {
                                zoho_contact_id: contact_id.to_string(),
                                linkedin_id: linkedin_id.clone(),
                                linkedin_profile_json: linkedin_data_str.clone(),
                                extract_address: true,
                                extract_company_name: true,
                                preferred_location: String::new(),  // Empty string for optional field
                            };
                            
                            // Log request to file
                            let timestamp = Utc::now().format("%Y%m%d_%H%M%S_%f").to_string();
                            let request_log_path = self.log_grpc_request(&request_msg, contact_id, &timestamp)?;
                            log::info!("Logged gRPC request to: {}", request_log_path.display());
                            
                            // Use GenerateBothDossiers - the service handles URL extraction internally
                            let request = tonic::Request::new(request_msg);
                            
                            match grpc_client.generate_both_dossiers(request).await {
                                Ok(response) => {
                                    let bundle = response.into_inner();
                                    
                                    // Log response to file
                                    let response_log_path = self.log_grpc_response(&bundle, contact_id, &timestamp)?;
                                    log::info!("Logged gRPC response to: {}", response_log_path.display());
                                    
                                    log::info!("Successfully generated dossiers via gRPC");
                                    
                                    // Extract data from company dossier
                                    let company_dossier = bundle.company_dossier
                                        .ok_or_else(|| LennardError::Processing("No company dossier in response".to_string()))?;
                                    
                                    log::info!("Extracted company name: {}", company_dossier.company_name);
                                    
                                    // Convert gRPC address to our domain type
                                    let mailing_address = company_dossier.mailing_address.map(|addr| MailingAddress {
                                        street: addr.street,
                                        city: addr.city,
                                        state: Some(addr.state),
                                        postal_code: addr.postal_code,
                                        country: addr.country,
                                    });
                                    
                                    if mailing_address.is_some() {
                                        log::info!("Address extraction successful");
                                    }
                                    
                                    Ok(DossierResult {
                                        company_dossier_content: company_dossier.content,
                                        company_name: company_dossier.company_name,
                                        mailing_address,
                                    })
                                }
                                Err(e) => {
                                    // Log error to file
                                    let error_msg = format!("gRPC call failed: {:?}", e);
                                    let error_log_path = self.log_grpc_error(&error_msg, contact_id, &timestamp)?;
                                    log::error!("Logged gRPC error to: {}", error_log_path.display());
                                    
                                    Err(LennardError::ServiceUnavailable(
                                        format!("gRPC call to dossier service failed: {}", e)
                                    ))
                                }
                            }
                        }
                        Err(e) => {
                            Err(LennardError::ServiceUnavailable(
                                format!("Could not connect to gRPC dossier service at {}: {}", self.grpc_url, e)
                            ))
                        }
                    }
                }
                Err(e) => {
                    Err(LennardError::Config(
                        format!("Invalid gRPC URL {}: {}", self.grpc_url, e)
                    ))
                }
            }
    }
    
    // Helper methods for logging gRPC messages to files
    fn log_grpc_request(&self, request: &BothDossiersRequest, contact_id: &str, timestamp: &str) -> Result<PathBuf> {
        let log_dir = paths::dossier_logs_dir();
        fs::create_dir_all(&log_dir).map_err(|e| 
            LennardError::IoError(format!("Failed to create logs directory: {}", e))
        )?;
        
        let filename = format!("{}_{}_request.json", timestamp, contact_id);
        let log_path = log_dir.join(filename);
        
        // Create a JSON object with both debug format and structured data
        let log_content = serde_json::json!({
            "timestamp": timestamp,
            "contact_id": contact_id,
            "type": "request",
            "debug_format": format!("{:?}", request),
            "request": {
                "zoho_contact_id": &request.zoho_contact_id,
                "linkedin_id": &request.linkedin_id,
                "extract_address": request.extract_address,
                "extract_company_name": request.extract_company_name,
                "preferred_location": &request.preferred_location,
                "linkedin_profile_json_length": request.linkedin_profile_json.len()
            }
        });
        
        let content = serde_json::to_string_pretty(&log_content)
            .map_err(|e| LennardError::Serialization(format!("Failed to serialize request log: {}", e)))?;
        
        fs::write(&log_path, content)
            .map_err(|e| LennardError::IoError(format!("Failed to write request log: {}", e)))?;
        
        Ok(log_path)
    }
    
    fn log_grpc_response<T: std::fmt::Debug>(&self, response: &T, contact_id: &str, timestamp: &str) -> Result<PathBuf> {
        let log_dir = paths::dossier_logs_dir();
        fs::create_dir_all(&log_dir).map_err(|e| 
            LennardError::IoError(format!("Failed to create logs directory: {}", e))
        )?;
        
        let filename = format!("{}_{}_response.json", timestamp, contact_id);
        let log_path = log_dir.join(filename);
        
        // Create a JSON object with debug format
        let log_content = serde_json::json!({
            "timestamp": timestamp,
            "contact_id": contact_id,
            "type": "response",
            "debug_format": format!("{:?}", response)
        });
        
        let content = serde_json::to_string_pretty(&log_content)
            .map_err(|e| LennardError::Serialization(format!("Failed to serialize response log: {}", e)))?;
        
        fs::write(&log_path, content)
            .map_err(|e| LennardError::IoError(format!("Failed to write response log: {}", e)))?;
        
        Ok(log_path)
    }
    
    fn log_grpc_error(&self, error: &str, contact_id: &str, timestamp: &str) -> Result<PathBuf> {
        let log_dir = paths::dossier_logs_dir();
        fs::create_dir_all(&log_dir).map_err(|e| 
            LennardError::IoError(format!("Failed to create logs directory: {}", e))
        )?;
        
        let filename = format!("{}_{}_error.json", timestamp, contact_id);
        let log_path = log_dir.join(filename);
        
        let log_content = serde_json::json!({
            "timestamp": timestamp,
            "contact_id": contact_id,
            "type": "error",
            "error": error
        });
        
        let content = serde_json::to_string_pretty(&log_content)
            .map_err(|e| LennardError::Serialization(format!("Failed to serialize error log: {}", e)))?;
        
        fs::write(&log_path, content)
            .map_err(|e| LennardError::IoError(format!("Failed to write error log: {}", e)))?;
        
        Ok(log_path)
    }
}