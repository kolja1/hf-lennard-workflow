//! Connection test for Zoho API authentication
//! 
//! This test verifies that authentication works correctly
//! It only performs read operations - no writes

use workflow_core::config::LennardConfig;
use workflow_core::clients::ZohoClient;
use std::path::Path;

#[tokio::test]
#[ignore]  // Only run when explicitly requested with --ignored flag
async fn test_zoho_connection_and_authentication() {
    // Skip if credentials file doesn't exist
    let creds_path = "../../config/credentials.json";
    if !Path::new(creds_path).exists() {
        panic!("ERROR: credentials.json not found at {} - required for tests", creds_path);
    }
    
    // Load real configuration
    let config = match LennardConfig::from_file(creds_path) {
        Ok(c) => c,
        Err(e) => {
            println!("Skipping: Failed to load config: {}", e);
            return;
        }
    };
    
    println!("Creating ZohoClient with Nango authentication...");
    
    // Create unauthenticated client
    let client = ZohoClient::new(config.zoho);
    
    println!("Authenticating via Nango...");
    
    // Authenticate - this will fetch a token from Nango
    let authenticated_client = match client.authenticate().await {
        Ok(c) => {
            println!("✓ Authentication successful!");
            c
        }
        Err(e) => {
            panic!("Authentication failed: {}", e);
        }
    };
    
    // Verify client is authenticated (compile-time guarantee, but let's check)
    assert!(authenticated_client.is_authenticated());
    
    println!("✓ Client is authenticated");
    
    // Try to make a simple read-only API call to verify the token works
    // Search for tasks with a very specific filter that likely returns empty
    println!("Testing API access with a simple search query...");
    
    let filters = vec![
        ("Subject", "TEST_TASK_THAT_DOES_NOT_EXIST_123456789"),
        ("Status", "Not Started")
    ];
    
    match authenticated_client.get_tasks(&filters).await {
        Ok(tasks) => {
            println!("✓ API call successful! Found {} tasks", tasks.len());
            // We expect 0 tasks with this specific subject
            assert_eq!(tasks.len(), 0, "Should find no tasks with test subject");
        }
        Err(e) => {
            // If it's a 204 No Content, that's still a success
            if e.to_string().contains("204") || e.to_string().contains("No Content") {
                println!("✓ API call successful! (204 No Content - no matching tasks)");
            } else {
                panic!("API call failed: {}", e);
            }
        }
    }
    
    println!("\n✅ All connection tests passed!");
    println!("   - Nango authentication works");
    println!("   - Access token is valid");
    println!("   - API calls succeed");
}

#[tokio::test]
#[ignore]  // Only run when explicitly requested
async fn test_zoho_token_refresh() {
    // Skip if credentials file doesn't exist
    let creds_path = "../../config/credentials.json";
    if !Path::new(creds_path).exists() {
        panic!("ERROR: credentials.json not found - required for tests");
    }
    
    let config = match LennardConfig::from_file(creds_path) {
        Ok(c) => c,
        Err(e) => {
            println!("Skipping: Failed to load config: {}", e);
            return;
        }
    };
    
    println!("Testing token refresh mechanism...");
    
    // Create and authenticate client
    let client = ZohoClient::new(config.zoho);
    let authenticated_client = match client.authenticate().await {
        Ok(c) => c,
        Err(e) => {
            panic!("Initial authentication failed: {}", e);
        }
    };
    
    println!("✓ Initial authentication successful");
    
    // Force a token refresh
    println!("Forcing token refresh...");
    match authenticated_client.refresh_token().await {
        Ok(()) => {
            println!("✓ Token refresh successful");
        }
        Err(e) => {
            panic!("Token refresh failed: {}", e);
        }
    }
    
    // Verify the refreshed token still works
    println!("Verifying refreshed token works...");
    let filters = vec![("Subject", "TEST_NONEXISTENT")];
    
    match authenticated_client.get_tasks(&filters).await {
        Ok(_) => {
            println!("✓ API call with refreshed token successful");
        }
        Err(e) => {
            if e.to_string().contains("204") || e.to_string().contains("No Content") {
                println!("✓ API call with refreshed token successful (no results)");
            } else {
                panic!("API call with refreshed token failed: {}", e);
            }
        }
    }
    
    println!("\n✅ Token refresh test passed!");
}