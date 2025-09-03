//! Type-safe Zoho CRM client with compile-time authentication enforcement
//!
//! This design prevents API calls without authentication by using Rust's type system.
//! The compiler will refuse to compile code that tries to call API methods on an
//! unauthenticated client.

use crate::config::{ZohoConfig, LennardConfig};
use crate::error::{LennardError, Result};
use crate::types::{ZohoContact, MailingAddress};
use crate::clients::NangoClient;
use reqwest::Client as HttpClient;
use serde_json::{json, Value};
use std::marker::PhantomData;
use zoho_generated_types::{TasksResponse, TasksApiResponse};

// Type-safe authentication states
pub struct Unauthenticated;
pub struct Authenticated;

// Strong-typed configuration contexts
pub enum ConfigContext {
    /// Running from src/rust/module/ during build time
    BuildTime,
    /// Running from project root in production/runtime
    Runtime,
}

/// ZohoClient with compile-time authentication state enforcement
/// Simplified to only use Nango authentication
pub struct ZohoClient<State = Unauthenticated> {
    // Nango authentication fields
    nango_client: NangoClient,
    connection_id: String,
    integration_id: String,
    user_id: String,
    
    // HTTP and API configuration
    http_client: HttpClient,
    base_url: String,
    
    // Compile-time state tracking
    _state: PhantomData<State>,
}

// Implementation for unauthenticated client (only creation and auth methods)
impl ZohoClient<Unauthenticated> {
    /// Create a new unauthenticated ZohoClient for build-time usage
    /// Loads config from ../../../credentials.json relative to src/rust/module/
    pub fn new_for_build() -> Result<Self> {
        Self::new_with_context(ConfigContext::BuildTime)
    }
    
    /// Create a new unauthenticated ZohoClient for runtime usage
    /// Loads config from ./credentials.json relative to current directory
    pub fn new_for_runtime() -> Result<Self> {
        Self::new_with_context(ConfigContext::Runtime)
    }
    
    /// Create a new unauthenticated ZohoClient with explicit config context
    fn new_with_context(context: ConfigContext) -> Result<Self> {
        let config_path = match context {
            ConfigContext::BuildTime => "../../../credentials.json",
            ConfigContext::Runtime => "./credentials.json",
        };
        
        let lennard_config = LennardConfig::from_file(config_path)?;
        Self::new_from_zoho_config(lennard_config.zoho)
    }
    
    /// Create a new unauthenticated ZohoClient from ZohoConfig (legacy method)
    pub fn new(config: ZohoConfig) -> Self {
        Self::new_from_zoho_config(config).expect("Failed to create ZohoClient")
    }
    
    /// Internal method to create ZohoClient from ZohoConfig
    fn new_from_zoho_config(config: ZohoConfig) -> Result<Self> {
        // Validate Nango config is present
        if config.nango_api_key.is_empty() {
            return Err(LennardError::Config(
                "Nango API key is required".to_string()
            ));
        }
        
        if config.nango_connection_id.is_empty() || config.nango_integration_id.is_empty() {
            return Err(LennardError::Config(
                "Nango connection_id and integration_id are required".to_string()
            ));
        }
        
        let http_client = HttpClient::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| LennardError::Config(format!("Failed to create HTTP client: {}", e)))?;
            
        Ok(Self {
            nango_client: NangoClient::new(config.nango_api_key),
            connection_id: config.nango_connection_id,
            integration_id: config.nango_integration_id,
            user_id: config.nango_user_id,
            http_client,
            base_url: config.base_url,
            _state: PhantomData,
        })
    }
    
    
    /// Authenticate and transition to authenticated state
    /// This is the ONLY way to get an authenticated client
    pub async fn authenticate(self) -> Result<ZohoClient<Authenticated>> {
        // Verify we can get a token from Nango
        let _token = self.nango_client.get_fresh_token(
            &self.connection_id,
            &self.integration_id,
            false  // Don't force refresh on initial auth
        ).await?;
        
        // Transition to authenticated state (no token storage needed)
        Ok(ZohoClient {
            nango_client: self.nango_client,
            connection_id: self.connection_id,
            integration_id: self.integration_id,
            user_id: self.user_id,
            http_client: self.http_client,
            base_url: self.base_url,
            _state: PhantomData::<Authenticated>,
        })
    }
    
}

// Implementation for authenticated client (API methods only available here)
impl ZohoClient<Authenticated> {
    /// Check if client is authenticated (always true for Authenticated state)
    pub fn is_authenticated(&self) -> bool {
        true  // Compile-time guarantee
    }
    
    /// Get a fresh access token from Nango (uses smart caching)
    async fn get_fresh_token(&self) -> Result<String> {
        self.nango_client.get_fresh_token(
            &self.connection_id,
            &self.integration_id,
            false  // Let NangoClient decide when to refresh based on expiry
        ).await
    }
    
    /// Force refresh the access token
    pub async fn refresh_token(&self) -> Result<()> {
        // Force a token refresh in Nango's cache
        let _token = self.nango_client.get_fresh_token(
            &self.connection_id,
            &self.integration_id,
            true  // Force refresh
        ).await?;
        
        Ok(())
    }
    
    /// Get contact by ID (only available for authenticated clients)
    pub async fn get_contact(&self, contact_id: &str) -> Result<Option<ZohoContact>> {
        let url = format!("{}/crm/v2/Contacts/{}", self.base_url, contact_id);
        
        // Get fresh token from Nango (automatically refreshes if needed)
        let access_token = self.get_fresh_token().await?;
        
        let response = self.http_client
            .get(&url)
            .bearer_auth(&access_token)
            .send()
            .await?;
            
        if !response.status().is_success() {
            return Err(LennardError::Http(reqwest::Error::from(response.error_for_status().unwrap_err())));
        }
        
        let data: Value = response.json().await?;
        
        if let Some(contact_data) = data["data"].as_array().and_then(|arr| arr.first()) {
            return Ok(Some(self.parse_contact(contact_data)?));
        }
        
        Ok(None)
    }
    
    /// Update contact's mailing address in Zoho CRM
    pub async fn update_contact_address(&self, contact_id: &str, address: &MailingAddress) -> Result<()> {
        let url = format!("{}/crm/v2/Contacts/{}", self.base_url, contact_id);
        
        // Get fresh token from Nango (automatically refreshes if needed)
        let access_token = self.get_fresh_token().await?;
        
        // Build the update payload with address fields
        let update_data = json!({
            "data": [{
                "id": contact_id,
                "Mailing_Street": address.street,
                "Mailing_City": address.city,
                "Mailing_State": address.state.as_deref().unwrap_or(""),
                "Mailing_Code": address.postal_code,
                "Mailing_Country": address.country
            }]
        });
        
        log::info!("Updating contact {} with address: {:?}", contact_id, address);
        
        let response = self.http_client
            .put(&url)
            .bearer_auth(&access_token)
            .json(&update_data)
            .send()
            .await?;
            
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            log::error!("Failed to update contact address: {}", error_text);
            return Err(LennardError::Workflow(format!("Failed to update contact address: {}", error_text)));
        }
        
        log::info!("Successfully updated contact {} address in Zoho CRM", contact_id);
        Ok(())
    }
    
    /// Get a single task by ID (only available for authenticated clients)
    /// Uses the single record endpoint which returns complete data including Who_Id
    pub async fn get_task_by_id(&self, task_id: &str) -> Result<Option<TasksResponse>> {
        let url = format!("{}/crm/v2/Tasks/{}", self.base_url, task_id);
        
        // Get fresh token from Nango (automatically refreshes if needed)
        let access_token = self.get_fresh_token().await?;
        
        let response = self.http_client
            .get(&url)
            .bearer_auth(&access_token)
            .send()
            .await?;
            
        if response.status() == 404 {
            return Ok(None);
        }
            
        if !response.status().is_success() {
            return Err(LennardError::Http(reqwest::Error::from(response.error_for_status().unwrap_err())));
        }
        
        // Single record response has the data wrapped differently
        let json: serde_json::Value = response.json().await?;
        if let Some(data_array) = json["data"].as_array() {
            if let Some(first) = data_array.first() {
                let task: TasksResponse = serde_json::from_value(first.clone())?;
                return Ok(Some(task));
            }
        }
        
        Ok(None)
    }
    
    /// Update task status and description
    pub async fn update_task_status(&self, task_id: &str, status: &str, description: &str) -> Result<()> {
        let url = format!("{}/crm/v2/Tasks/{}", self.base_url, task_id);
        
        // Get fresh token from Nango
        let access_token = self.get_fresh_token().await?;
        
        // Build update payload
        let update_data = serde_json::json!({
            "data": [{
                "Status": status,
                "Description": description,
            }]
        });
        
        let response = self.http_client
            .put(&url)
            .bearer_auth(&access_token)
            .json(&update_data)
            .send()
            .await?;
            
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(LennardError::ServiceUnavailable(
                format!("Failed to update Zoho task: {}", error_text)
            ));
        }
        
        log::info!("Updated Zoho task {} status to '{}' with description", task_id, status);
        Ok(())
    }
    
    /// Get tasks with filters using Search API (only available for authenticated clients)
    pub async fn get_tasks(&self, filters: &[(&str, &str)]) -> Result<Vec<TasksResponse>> {
        // Use Search API which properly respects Owner filter
        let url = format!("{}/crm/v2/Tasks/search", self.base_url);
        
        // Get fresh token from Nango (automatically refreshes if needed)
        let access_token = self.get_fresh_token().await?;
        
        // Build criteria string from filters
        // Search API requires different syntax for Owner and different operators
        let criteria_parts: Vec<String> = filters.iter()
            .map(|(key, value)| {
                // URL-encode the value by replacing spaces
                let encoded_value = value.replace(" ", "%20");
                match *key {
                    // Search API doesn't support 'contains' for Subject, use 'equals'
                    "Subject" => format!("(Subject:equals:{})", encoded_value),
                    "Status" => format!("(Status:equals:{})", encoded_value),
                    // For Owner, use Owner.id:equals syntax for Search API
                    "Owner" => format!("(Owner.id:equals:{})", encoded_value),
                    _ => format!("({}:equals:{})", key, encoded_value),
                }
            })
            .collect();
        
        // For multiple criteria, wrap the entire expression in parentheses
        let criteria = if criteria_parts.len() > 1 {
            format!("({})", criteria_parts.join("and"))
        } else {
            criteria_parts.join("")
        };
        
        log::debug!("Zoho Tasks API URL: {}", url);
        log::debug!("Criteria: {}", criteria);
        
        // Build full URL manually to avoid double encoding
        let full_url = format!("{}?criteria={}", url, criteria);
        log::debug!("Full URL: {}", full_url);
        
        let response = self.http_client
            .get(&full_url)
            .bearer_auth(&access_token)
            .send()
            .await?;
            
        let status = response.status();
        log::debug!("Zoho API response status: {}", status);
        
        // Handle 204 No Content - return empty array
        if status == reqwest::StatusCode::NO_CONTENT {
            log::info!("Zoho Search API returned no results (204 No Content)");
            return Ok(Vec::new());
        }
        
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            log::error!("Zoho API error (status {}): {}", status, error_text);
            return Err(LennardError::ServiceUnavailable(format!("Zoho API error (status {}): {}", status, error_text)));
        }
        
        // Get response text for debugging
        let response_text = response.text().await?;
        if response_text.is_empty() {
            log::error!("Zoho API returned empty response");
            return Err(LennardError::ServiceUnavailable("Zoho API returned empty response".to_string()));
        }
        log::debug!("Zoho API response length: {} bytes", response_text.len());
        log::debug!("Zoho API response (first 500 chars): {}", &response_text[..response_text.len().min(500)]);
        
        // Deserialize directly into the generated types
        let api_response: TasksApiResponse = serde_json::from_str(&response_text)
            .map_err(|e| {
                log::error!("Failed to deserialize Zoho response: {}", e);
                log::error!("Response was: {}", &response_text[..response_text.len().min(1000)]);
                LennardError::ServiceUnavailable(format!("Failed to parse Zoho response: {}", e))
            })?;
        
        Ok(api_response.data)
    }
    
    /// Update contact (only available for authenticated clients)
    pub async fn update_contact(&self, contact_id: &str, updates: &Value) -> Result<()> {
        let url = format!("{}/crm/v2/Contacts/{}", self.base_url, contact_id);
        
        // Get fresh token from Nango (automatically refreshes if needed)
        let access_token = self.get_fresh_token().await?;
        
        let response = self.http_client
            .put(&url)
            .bearer_auth(&access_token)
            .json(updates)
            .send()
            .await?;
            
        if !response.status().is_success() {
            return Err(LennardError::Http(reqwest::Error::from(response.error_for_status().unwrap_err())));
        }
        
        Ok(())
    }
    
    /// Generic API call method for custom endpoints (only available for authenticated clients)
    pub async fn call_zoho_api(&self, method: &str, endpoint: &str, body: Option<&Value>) -> Result<Value> {
        let url = format!("{}{}", self.base_url, endpoint);
        
        // Get fresh token from Nango (automatically refreshes if needed)
        let access_token = self.get_fresh_token().await?;
        
        let mut request = match method.to_uppercase().as_str() {
            "GET" => self.http_client.get(&url),
            "POST" => self.http_client.post(&url),
            "PUT" => self.http_client.put(&url),
            "DELETE" => self.http_client.delete(&url),
            _ => return Err(LennardError::Config(
                format!("Unsupported HTTP method: {}", method)
            )),
        };
        
        request = request.bearer_auth(&access_token);
        
        if let Some(body) = body {
            request = request.json(body);
        }
        
        let response = request.send().await?;
            
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(LennardError::Config(
                format!("Zoho API call failed with status {}: {}", status, error_text)
            ));
        }
        
        let data: Value = response.json().await?;
        Ok(data)
    }
}

// Common methods available in both states
impl<State> ZohoClient<State> {
    
    // Parse methods available in both states (for testing and internal use)
    
    fn parse_contact(&self, data: &Value) -> Result<ZohoContact> {
        Ok(ZohoContact {
            id: data["id"].as_str().unwrap_or("").to_string(),
            full_name: data["Full_Name"].as_str().unwrap_or("").to_string(),
            email: data["Email"].as_str().map(|s| s.to_string()),
            phone: data["Phone"].as_str().map(|s| s.to_string()),
            company: data["Account_Name"].as_str().map(|s| s.to_string()),
            linkedin_id: data["LinkedIn_ID"].as_str().map(|s| s.to_string()),
            mailing_address: self.parse_mailing_address(data),
        })
    }
    
    fn parse_mailing_address(&self, data: &Value) -> Option<MailingAddress> {
        // Check if required address fields are present
        let street = data["Mailing_Street"].as_str()?;
        let city = data["Mailing_City"].as_str()?;  
        let postal_code = data["Mailing_Code"].as_str()?;
        let country = data["Mailing_Country"].as_str()?;
        
        Some(MailingAddress {
            street: street.to_string(),
            city: city.to_string(),
            state: data["Mailing_State"].as_str().map(|s| s.to_string()),
            postal_code: postal_code.to_string(),
            country: country.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    
    
    
    #[test]
    fn test_compile_time_safety() {
        // This should compile - creating unauthenticated client with valid config
        let config = ZohoConfig {
            nango_api_key: "test_key".to_string(),
            nango_connection_id: "test_conn".to_string(),
            nango_integration_id: "test_integration".to_string(),
            nango_user_id: "test_user".to_string(),
            base_url: "https://www.zohoapis.com".to_string(),
        };
        let _unauthenticated_client = ZohoClient::new(config);
        
        // This should NOT compile if uncommented:
        // _unauthenticated_client.get_tasks(&[]).await;  // Compile error!
        // _unauthenticated_client.get_contact("123").await;  // Compile error!
        
        // After authentication, API methods become available:
        // let authenticated_client = _unauthenticated_client.authenticate().await?;
        // let tasks = authenticated_client.get_tasks(&[]).await?;  // This compiles!
    }
}