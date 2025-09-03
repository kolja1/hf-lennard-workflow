//! Baserow client for LinkedIn profile lookup

use crate::config::BaserowConfig;
use crate::error::{LennardError, Result};
use crate::types::LinkedInProfile;
use reqwest::Client as HttpClient;
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct BaserowClient {
    config: BaserowConfig,
    http_client: HttpClient,
}

impl BaserowClient {
    pub fn new(config: BaserowConfig) -> Self {
        let http_client = HttpClient::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
            
        Self {
            config,
            http_client,
        }
    }
    
    /// Get LinkedIn profile by profile ID
    pub async fn get_linkedin_profile(&self, profile_id: &str) -> Result<Option<LinkedInProfile>> {
        let url = format!("{}/api/database/rows/table/{}/", 
                         self.config.base_url, self.config.table_id);
        
        // Use correct Baserow filter format with filter_type and field as integer
        // field_4866518 contains the LinkedIn profile ID as string
        // field_4866519 contains the full JSON export
        let filter = json!({
            "filter_type": "AND",
            "filters": [
                {
                    "type": "equal",
                    "field": 4866518,  // LinkedIn profile ID field (not the JSON field)
                    "value": profile_id
                }
            ]
        });
        
        let response = self.http_client
            .get(&url)
            .header("Authorization", format!("Token {}", self.config.api_key))
            .query(&[("filters", filter.to_string())])
            .send()
            .await?;
            
        if !response.status().is_success() {
            return Err(LennardError::Http(reqwest::Error::from(response.error_for_status().unwrap_err())));
        }
        
        let data: Value = response.json().await?;
        
        if let Some(results) = data["results"].as_array() {
            if let Some(row) = results.first() {
                return Ok(Some(self.parse_profile(row)?));
            }
        }
        
        Ok(None)
    }
    
    /// Get all LinkedIn profiles from Baserow table
    pub async fn get_linkedin_profiles(&self) -> Result<Vec<LinkedInProfile>> {
        let url = format!("{}/api/database/rows/table/{}/", 
                         self.config.base_url, self.config.table_id);
        
        let response = self.http_client
            .get(&url)
            .header("Authorization", format!("Token {}", self.config.api_key))
            .query(&[("size", "100")])  // Limit to 100 for testing
            .send()
            .await?;
            
        if !response.status().is_success() {
            return Err(LennardError::Http(reqwest::Error::from(response.error_for_status().unwrap_err())));
        }
        
        let data: Value = response.json().await?;
        let mut profiles = Vec::new();
        
        if let Some(results) = data["results"].as_array() {
            for row in results {
                match self.parse_profile(row) {
                    Ok(profile) => profiles.push(profile),
                    Err(_) => continue,  // Skip invalid profiles
                }
            }
        }
        
        Ok(profiles)
    }
    
    fn parse_profile(&self, row: &Value) -> Result<LinkedInProfile> {
        // The actual LinkedIn data is stored as a JSON string in field_4866519
        let json_str = row["field_4866519"].as_str()
            .ok_or_else(|| LennardError::Workflow("Missing LinkedIn JSON data in field_4866519".to_string()))?;
        
        // Parse the JSON string to get the actual profile data
        let profile_data: Value = serde_json::from_str(json_str)
            .map_err(|e| LennardError::Workflow(format!("Failed to parse LinkedIn JSON: {}", e)))?;
        
        // Create raw_data from the parsed JSON
        let mut raw_data = HashMap::new();
        if let Some(obj) = profile_data.as_object() {
            for (k, v) in obj {
                raw_data.insert(k.clone(), v.clone());
            }
        }
        
        // Extract profile fields from the parsed JSON
        Ok(LinkedInProfile {
            profile_id: profile_data["id"].as_str().unwrap_or("").to_string(),
            profile_url: profile_data["profile_url"].as_str().unwrap_or("").to_string(),
            full_name: profile_data["full_name"].as_str().unwrap_or("").to_string(),
            headline: profile_data["headline"].as_str().map(|s| s.to_string()),
            location: profile_data["location_name"].as_str().map(|s| s.to_string()),
            company: profile_data["current_company"].as_str().map(|s| s.to_string()),
            raw_data,
        })
    }
}