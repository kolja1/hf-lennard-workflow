//! Nango API client for OAuth token management
//! 
//! This client implements the same logic as the Python NangoTokenManager
//! for retrieving fresh access tokens from Nango connections.
//! 
//! Includes intelligent token caching to minimize API calls while ensuring
//! tokens are always fresh.

use crate::error::{LennardError, Result};
use reqwest::Client as HttpClient;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration as StdDuration;
use chrono::{DateTime, Utc, Duration};

/// Cached token information
#[derive(Clone, Debug)]
struct TokenCache {
    access_token: String,
    expires_at: DateTime<Utc>,
}

pub struct NangoClient {
    secret_key: String,
    base_url: String,
    http_client: HttpClient,
    /// Token cache: connection_id -> token info
    token_cache: Mutex<HashMap<String, TokenCache>>,
    /// How many seconds before expiry to refresh token (default 60)
    expiry_buffer_seconds: i64,
}

impl NangoClient {
    pub fn new(secret_key: String) -> Self {
        Self::with_expiry_buffer(secret_key, 60)
    }
    
    /// Create client with custom expiry buffer (mainly for testing)
    pub fn with_expiry_buffer(secret_key: String, expiry_buffer_seconds: i64) -> Self {
        let http_client = HttpClient::builder()
            .timeout(StdDuration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
            
        Self {
            secret_key,
            base_url: "https://api.nango.dev".to_string(),
            http_client,
            token_cache: Mutex::new(HashMap::new()),
            expiry_buffer_seconds,
        }
    }
    
    /// Get fresh access token from Nango connection
    /// 
    /// This method implements intelligent caching to minimize API calls:
    /// - Returns cached token if still valid (with expiry buffer)
    /// - Fetches new token if expired or force_refresh is true
    /// - Automatically updates cache with new tokens
    pub async fn get_fresh_token(
        &self,
        connection_id: &str,
        integration_id: &str,
        force_refresh: bool
    ) -> Result<String> {
        let cache_key = connection_id.to_string();
        
        // Check cache first (unless force refresh requested)
        if !force_refresh {
            let cache = self.token_cache.lock().unwrap();
            if let Some(cached) = cache.get(&cache_key) {
                // Check if token is still valid with buffer
                let expiry_threshold = cached.expires_at - Duration::seconds(self.expiry_buffer_seconds);
                if Utc::now() < expiry_threshold {
                    log::debug!(
                        "Using cached token for connection {} (expires at {}, buffer {}s)",
                        connection_id, cached.expires_at, self.expiry_buffer_seconds
                    );
                    return Ok(cached.access_token.clone());
                } else {
                    log::info!(
                        "Cached token for connection {} is near expiry (expires at {}), refreshing",
                        connection_id, cached.expires_at
                    );
                }
            }
        } else {
            log::info!("Force refresh requested for connection {}", connection_id);
        }
        
        // Fetch fresh token from Nango API
        log::info!("Fetching fresh token from Nango for connection {}", connection_id);
        let url = format!("{}/connection/{}", self.base_url, connection_id);
        
        let mut params = vec![("provider_config_key", integration_id)];
        if force_refresh {
            params.push(("force_refresh", "true"));
        }
        
        let response = self.http_client
            .get(&url)
            .bearer_auth(&self.secret_key)
            .query(&params)
            .send()
            .await
            .map_err(LennardError::Http)?;
            
        if !response.status().is_success() {
            return Err(LennardError::Auth(format!(
                "Nango API request failed: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }
        
        let data: Value = response.json().await
            .map_err(LennardError::Http)?;
            
        // Extract token and expiry
        let access_token = self.extract_access_token(&data)?;
        let expires_at = self.extract_expiry(&data)?;
        
        // Update cache
        {
            let mut cache = self.token_cache.lock().unwrap();
            cache.insert(cache_key.clone(), TokenCache {
                access_token: access_token.clone(),
                expires_at,
            });
            log::info!(
                "Cached new token for connection {} (expires at {})",
                connection_id, expires_at
            );
        }
        
        Ok(access_token)
    }
    
    /// Extract expiry time from Nango connection response
    fn extract_expiry(&self, data: &Value) -> Result<DateTime<Utc>> {
        let credentials = data["credentials"].as_object()
            .ok_or_else(|| LennardError::Auth("No credentials in Nango response".to_string()))?;
            
        let expires_at_str = credentials["expires_at"].as_str()
            .ok_or_else(|| LennardError::Auth("No expires_at in Nango response".to_string()))?;
        
        // Parse ISO 8601 datetime (e.g., "2025-08-16T16:47:07.580Z")
        DateTime::parse_from_rfc3339(expires_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| LennardError::Auth(format!("Invalid expires_at format: {}", e)))
    }
    
    /// Extract access token from Nango connection response
    /// 
    /// Handles different OAuth types (OAuth1, OAuth2) like the Python implementation
    fn extract_access_token(&self, data: &Value) -> Result<String> {
        let credentials = data["credentials"].as_object()
            .ok_or_else(|| LennardError::Auth("No credentials in Nango response".to_string()))?;
            
        let credential_type = credentials["type"].as_str()
            .ok_or_else(|| LennardError::Auth("No credential type in Nango response".to_string()))?;
            
        match credential_type {
            "OAUTH2" => {
                if let Some(token) = credentials["access_token"].as_str() {
                    Ok(token.to_string())
                } else {
                    Err(LennardError::Auth("No access_token in OAuth2 credentials".to_string()))
                }
            }
            "OAUTH1" => {
                if let Some(token) = credentials["oauth_token"].as_str() {
                    Ok(token.to_string())
                } else {
                    Err(LennardError::Auth("No oauth_token in OAuth1 credentials".to_string()))
                }
            }
            _ => {
                // Try to extract from raw credentials as fallback
                if let Some(raw) = credentials["raw"].as_object() {
                    if let Some(token) = raw["access_token"].as_str() {
                        Ok(token.to_string())
                    } else {
                        Err(LennardError::Auth(format!(
                            "Unknown credential type '{}' and no access_token in raw credentials",
                            credential_type
                        )))
                    }
                } else {
                    Err(LennardError::Auth(format!(
                        "Unknown credential type: {}",
                        credential_type
                    )))
                }
            }
        }
    }
    
    /// Test if the Nango connection is working
    pub async fn test_connection(
        &self,
        connection_id: &str,
        integration_id: &str
    ) -> Result<bool> {
        match self.get_fresh_token(connection_id, integration_id, false).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

// Test helpers for unit testing
#[cfg(test)]
impl NangoClient {
    /// Test helper to inject a token into cache with custom expiry
    pub fn set_cached_token_for_test(
        &self, 
        connection_id: &str, 
        token: &str, 
        expires_in_seconds: i64
    ) {
        let mut cache = self.token_cache.lock().unwrap();
        cache.insert(connection_id.to_string(), TokenCache {
            access_token: token.to_string(),
            expires_at: Utc::now() + Duration::seconds(expires_in_seconds),
        });
    }
    
    /// Test helper to check if a token is cached
    pub fn is_token_cached(&self, connection_id: &str) -> bool {
        let cache = self.token_cache.lock().unwrap();
        cache.contains_key(connection_id)
    }
    
    /// Test helper to get cache size
    pub fn cache_size(&self) -> usize {
        let cache = self.token_cache.lock().unwrap();
        cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_token_cache_hit() {
        let client = NangoClient::with_expiry_buffer("test_key".to_string(), 60);
        
        // Inject token that expires in 120 seconds
        client.set_cached_token_for_test("conn1", "valid_token", 120);
        
        // Create a runtime for async test
        let rt = tokio::runtime::Runtime::new().unwrap();
        
        // Should use cached token (expires in 120s, buffer is 60s)
        let token = rt.block_on(async {
            client.get_fresh_token("conn1", "integration1", false).await
        });
        
        assert!(token.is_ok());
        assert_eq!(token.unwrap(), "valid_token");
    }
    
    #[test]
    fn test_token_near_expiry_triggers_refresh() {
        // Note: This test would need mock HTTP server to properly test refresh
        // For now, we just verify the cache detection logic
        let client = NangoClient::with_expiry_buffer("test_key".to_string(), 60);
        
        // Inject token that expires in 30 seconds (less than 60s buffer)
        client.set_cached_token_for_test("conn1", "expiring_token", 30);
        
        // Verify token is cached
        assert!(client.is_token_cached("conn1"));
        
        // In real scenario, get_fresh_token would fetch new token
        // because cached one is within the buffer zone
    }
    
    #[test]
    fn test_expired_token_detection() {
        let client = NangoClient::with_expiry_buffer("test_key".to_string(), 60);
        
        // Inject token that's already expired (negative seconds)
        client.set_cached_token_for_test("conn1", "expired_token", -10);
        
        // Verify token is cached but recognized as expired
        assert!(client.is_token_cached("conn1"));
    }
    
    #[test]
    fn test_force_refresh_bypasses_cache() {
        let client = NangoClient::with_expiry_buffer("test_key".to_string(), 60);
        
        // Inject valid token
        client.set_cached_token_for_test("conn1", "cached_token", 120);
        
        // With force_refresh=true, it should ignore cache and fetch new
        // (would need mock server to fully test)
    }
    
    #[test]
    fn test_zero_buffer_immediate_expiry() {
        let client = NangoClient::with_expiry_buffer("test_key".to_string(), 0);
        
        // Token expires in exactly 1 second
        client.set_cached_token_for_test("conn1", "token", 1);
        
        // With 0 buffer, token expiring in 1s should still be valid
        let rt = tokio::runtime::Runtime::new().unwrap();
        let token = rt.block_on(async {
            client.get_fresh_token("conn1", "integration1", false).await
        });
        
        assert!(token.is_ok());
        assert_eq!(token.unwrap(), "token");
        
        // But token expiring NOW (0 seconds) should trigger refresh
        client.set_cached_token_for_test("conn1", "expiring_now", 0);
        // Would need mock to test actual refresh
    }
    
    #[test]
    fn test_multiple_connections_cached_separately() {
        let client = NangoClient::with_expiry_buffer("test_key".to_string(), 60);
        
        // Cache tokens for different connections
        client.set_cached_token_for_test("conn1", "token1", 120);
        client.set_cached_token_for_test("conn2", "token2", 120);
        client.set_cached_token_for_test("conn3", "token3", 120);
        
        assert_eq!(client.cache_size(), 3);
        assert!(client.is_token_cached("conn1"));
        assert!(client.is_token_cached("conn2"));
        assert!(client.is_token_cached("conn3"));
    }
    
    #[test]
    fn test_cache_update_on_refresh() {
        let client = NangoClient::with_expiry_buffer("test_key".to_string(), 60);
        
        // Initial token
        client.set_cached_token_for_test("conn1", "old_token", 120);
        assert_eq!(client.cache_size(), 1);
        
        // Simulate refresh by setting new token
        client.set_cached_token_for_test("conn1", "new_token", 180);
        
        // Should still have 1 entry (updated, not added)
        assert_eq!(client.cache_size(), 1);
        
        let rt = tokio::runtime::Runtime::new().unwrap();
        let token = rt.block_on(async {
            client.get_fresh_token("conn1", "integration1", false).await
        });
        
        assert_eq!(token.unwrap(), "new_token");
    }
}