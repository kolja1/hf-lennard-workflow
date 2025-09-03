//! Zoho CRM client

use crate::config::ZohoConfig;
use crate::error::{LennardError, Result};
use crate::types::{ZohoContact, ZohoTask, MailingAddress};
use crate::clients::NangoClient;
use reqwest::Client as HttpClient;
use serde_json::Value;

pub struct ZohoClient {
    config: ZohoConfig,
    http_client: HttpClient,
    nango_client: Option<NangoClient>,
    access_token: Option<String>,
}

impl ZohoClient {
    pub fn new(config: ZohoConfig) -> Self {
        let http_client = HttpClient::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
            
        // Initialize Nango client if Nango credentials are available
        let nango_client = if !config.nango_api_key.is_empty() {
            Some(NangoClient::new(config.nango_api_key.clone()))
        } else {
            None
        };
            
        Self {
            config,
            http_client,
            nango_client,
            access_token: None,
        }
    }
    
    /// Check if this client has Nango authentication configured
    pub fn has_nango_auth(&self) -> bool {
        self.nango_client.is_some() && !self.config.nango_connection_id.is_empty()
    }
    
    /// Check if this client has OAuth authentication configured  
    pub fn has_oauth_auth(&self) -> bool {
        !self.config.refresh_token.is_empty() && !self.config.client_id.is_empty()
    }
    
    /// Check if the client is currently authenticated
    pub fn is_authenticated(&self) -> bool {
        self.access_token.is_some()
    }
    
    /// Authenticate using the best available method (Nango or OAuth)
    pub async fn authenticate(&mut self) -> Result<()> {
        // Try Nango authentication first (preferred method)
        if self.has_nango_auth() {
            return self.authenticate_via_nango().await;
        }
        
        // Fallback to direct OAuth if refresh_token available
        if self.has_oauth_auth() {
            return self.authenticate_via_oauth().await;
        }
        
        Err(LennardError::Auth(
            "No authentication method available - need either Nango credentials or OAuth refresh token".to_string()
        ))
    }
    
    /// Authenticate via Nango service
    async fn authenticate_via_nango(&mut self) -> Result<()> {
        if let Some(nango) = &self.nango_client {
            let token = nango.get_fresh_token(
                &self.config.nango_connection_id,
                &self.config.nango_integration_id,
                false  // Don't force refresh on first attempt
            ).await?;
            
            self.access_token = Some(token);
            Ok(())
        } else {
            Err(LennardError::Auth("Nango client not initialized".to_string()))
        }
    }
    
    /// Authenticate via direct OAuth refresh token
    async fn authenticate_via_oauth(&mut self) -> Result<()> {
        let url = "https://accounts.zoho.com/oauth/v2/token";
        
        let response = self.http_client
            .post(url)
            .form(&[
                ("refresh_token", &self.config.refresh_token),
                ("client_id", &self.config.client_id),
                ("client_secret", &self.config.client_secret),
                ("grant_type", &"refresh_token".to_string()),
            ])
            .send()
            .await?;
            
        if !response.status().is_success() {
            return Err(LennardError::Auth("Failed to authenticate with Zoho via OAuth".to_string()));
        }
        
        let data: Value = response.json().await?;
        
        if let Some(token) = data["access_token"].as_str() {
            self.access_token = Some(token.to_string());
            Ok(())
        } else {
            Err(LennardError::Auth("No access token in OAuth response".to_string()))
        }
    }
    
    /// Get contact by ID
    pub async fn get_contact(&self, contact_id: &str) -> Result<Option<ZohoContact>> {
        self.ensure_authenticated().await?;
        
        let url = format!("{}/crm/v2/Contacts/{}", self.config.base_url, contact_id);
        
        let response = self.http_client
            .get(&url)
            .bearer_auth(self.access_token.as_ref().unwrap())
            .send()
            .await?;
            
        if response.status().as_u16() == 404 {
            return Ok(None);
        }
        
        if !response.status().is_success() {
            return Err(LennardError::Http(reqwest::Error::from(response.error_for_status().unwrap_err())));
        }
        
        let data: Value = response.json().await?;
        
        if let Some(contact_data) = data["data"].as_array().and_then(|arr| arr.first()) {
            return Ok(Some(self.parse_contact(contact_data)?));
        }
        
        Ok(None)
    }
    
    /// Get tasks with filters
    pub async fn get_tasks(&self, filters: &[(&str, &str)]) -> Result<Vec<ZohoTask>> {
        self.ensure_authenticated().await?;
        
        let url = format!("{}/crm/v2/Tasks", self.config.base_url);
        
        let response = self.http_client
            .get(&url)
            .bearer_auth(self.access_token.as_ref().unwrap())
            .query(filters)
            .send()
            .await?;
            
        if !response.status().is_success() {
            return Err(LennardError::Http(reqwest::Error::from(response.error_for_status().unwrap_err())));
        }
        
        let data: Value = response.json().await?;
        let mut tasks = Vec::new();
        
        if let Some(task_array) = data["data"].as_array() {
            for task_data in task_array {
                tasks.push(self.parse_task(task_data)?);
            }
        }
        
        Ok(tasks)
    }
    
    async fn ensure_authenticated(&self) -> Result<()> {
        if self.access_token.is_none() {
            return Err(LennardError::Auth("Not authenticated - call authenticate() first".to_string()));
        }
        Ok(())
    }
    
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
    
    fn parse_task(&self, data: &Value) -> Result<ZohoTask> {
        Ok(ZohoTask {
            id: data["id"].as_str().unwrap_or("").to_string(),
            subject: data["Subject"].as_str().unwrap_or("").to_string(),
            description: data["Description"].as_str().map(|s| s.to_string()),
            status: data["Status"].as_str().unwrap_or("").to_string(),
            contact_id: data["Who_Id"].as_str().map(|s| s.to_string()),
            created_time: data["Created_Time"].as_str().unwrap_or("").to_string(),
        })
    }
    
    fn parse_mailing_address(&self, data: &Value) -> Option<MailingAddress> {
        let street = data["Mailing_Street"].as_str()?;
        let city = data["Mailing_City"].as_str()?;
        let postal_code = data["Mailing_Code"].as_str()?;
        let country = data["Mailing_Country"].as_str().unwrap_or("Germany");
        
        Some(MailingAddress {
            street: street.to_string(),
            city: city.to_string(),
            state: data["Mailing_State"].as_str().map(|s| s.to_string()),
            postal_code: postal_code.to_string(),
            country: country.to_string(),
        })
    }
}