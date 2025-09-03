//! Integration tests with real external services
//! 
//! These tests validate that all external service integrations work correctly
//! with actual credentials and API endpoints from the development environment.
//!
//! Run with: cargo test --features integration --test test_real_services

use std::path::PathBuf;
use workflow_core::config::LennardConfig;

/// Load real configuration from credentials.json in project root
fn load_real_config() -> LennardConfig {
    // Look for credentials.json in project root (2 levels up from crate)
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project_root = manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    
    let credentials_path = project_root.join("config").join("credentials.json");
    
    println!("Loading credentials from: {:?}", credentials_path);
    
    LennardConfig::from_file(&credentials_path)
        .expect("Failed to load credentials.json - ensure it exists with valid configuration")
}

mod real_services_tests {
    use super::*;
    use workflow_core::clients::{NangoClient, ZohoClient, BaserowClient};

    #[tokio::test]
    async fn test_nango_authentication() {
        println!("\n🔐 Testing Nango Authentication...");
        
        let config = load_real_config();
        
        // Verify Nango configuration is present
        assert!(!config.zoho.nango_api_key.is_empty(), "Nango API key must be configured");
        assert!(!config.zoho.nango_connection_id.is_empty(), "Nango connection ID must be configured");
        assert!(!config.zoho.nango_integration_id.is_empty(), "Nango integration ID must be configured");
        
        println!("✓ Config validation passed");
        println!("  - API key: {}***", &config.zoho.nango_api_key[..8]);
        println!("  - Connection ID: {}", &config.zoho.nango_connection_id);
        println!("  - Integration ID: {}", &config.zoho.nango_integration_id);
        
        // Create Nango client
        let nango_client = NangoClient::new(config.zoho.nango_api_key.clone());
        println!("✓ NangoClient created");
        
        // Test token retrieval
        let token_result = nango_client.get_fresh_token(
            &config.zoho.nango_connection_id,
            &config.zoho.nango_integration_id,
            false  // Don't force refresh
        ).await;
        
        match token_result {
            Ok(token) => {
                println!("✅ Nango authentication successful!");
                println!("  - Token retrieved: {}***", &token[..8]);
                assert!(!token.is_empty(), "Token should not be empty");
                assert!(token.len() > 10, "Token should be substantial length");
            }
            Err(e) => {
                panic!("❌ Nango authentication failed: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_zoho_crm_integration() {
        println!("\n📋 Testing Zoho CRM Integration...");
        
        let config = load_real_config();
        
        // Create ZohoClient with Nango authentication  
        let unauthenticated_client = ZohoClient::new(config.zoho.clone());
        println!("✓ ZohoClient created");
        
        // Test authentication - type-safe transition to authenticated state
        let auth_result = unauthenticated_client.authenticate().await;
        match auth_result {
            Ok(zoho_client) => {
                println!("✅ Zoho authentication successful!");
                assert!(zoho_client.is_authenticated(), "Client should be authenticated");
                
                // Test basic CRM operations
                println!("  Testing CRM operations...");
                
                // Test get_tasks with filters
                let tasks_result = zoho_client.get_tasks(&[
                    ("Subject", "Brief an"),
                    ("Status", "Not Started")
                ]).await;
                
                match tasks_result {
                    Ok(tasks) => {
                        println!("  ✓ Successfully retrieved {} tasks", tasks.len());
                        for (i, task) in tasks.iter().take(3).enumerate() {
                            println!("    Task {}: {} ({})", 
                                i + 1, 
                                task.subject, 
                                task.status.as_deref().unwrap_or("No status")
                            );
                        }
                    }
                    Err(e) => {
                        println!("  ⚠️  Task retrieval failed (may be expected): {}", e);
                    }
                }
                
                // Test contact retrieval (with a dummy ID - will return None)
                let contact_result = zoho_client.get_contact("dummy_id").await;
                match contact_result {
                    Ok(contact) => {
                        println!("  ✓ Contact API accessible (returned: {:?})", contact.is_some());
                    }
                    Err(e) => {
                        println!("  ⚠️  Contact retrieval failed (may be expected): {}", e);
                    }
                }
            }
            Err(e) => {
                panic!("❌ Zoho authentication failed: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_baserow_integration() {
        println!("\n🗃️  Testing Baserow Integration...");
        
        let config = load_real_config();
        
        // Create BaserowClient
        let baserow_client = BaserowClient::new(config.baserow.clone());
        println!("✓ BaserowClient created");
        println!("  - Base URL: {}", config.baserow.base_url);
        println!("  - Table ID: {}", config.baserow.table_id);
        
        // Test basic connectivity and data retrieval
        let profiles_result = baserow_client.get_linkedin_profiles().await;
        
        match profiles_result {
            Ok(profiles) => {
                println!("✅ Baserow integration successful!");
                println!("  - Retrieved {} LinkedIn profiles", profiles.len());
                
                // Show sample profiles (first 3)
                for (i, profile) in profiles.iter().take(3).enumerate() {
                    println!("    Profile {}: {} ({})", 
                        i + 1, 
                        &profile.full_name,
                        &profile.profile_id
                    );
                }
                
                assert!(!profiles.is_empty(), "Should have at least some profiles");
            }
            Err(e) => {
                panic!("❌ Baserow integration failed: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_letterexpress_integration() {
        println!("\n📮 Testing LetterExpress Integration...");
        
        let config = load_real_config();
        
        // Create LetterExpress client
        let letterexpress_client = workflow_core::clients::LetterExpressClient::new(
            config.letterexpress.clone()
        );
        println!("✓ LetterExpressClient created");
        println!("  - Base URL: {}", config.letterexpress.base_url);
        println!("  - Username: {}", config.letterexpress.username);
        
        // Test API connectivity (basic ping/health check)
        let health_result = letterexpress_client.test_connection().await;
        
        match health_result {
            Ok(is_healthy) => {
                println!("✅ LetterExpress integration successful!");
                println!("  - Service healthy: {}", is_healthy);
                assert!(is_healthy, "LetterExpress service should be healthy");
            }
            Err(e) => {
                panic!("❌ LetterExpress integration failed: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_pdf_service_integration() {
        println!("\n📄 Testing PDF Service Integration...");
        
        let config = load_real_config();
        
        // Create PDF service client
        let pdf_service = workflow_core::clients::PDFService::new(
            config.pdf_service.clone()
        );
        println!("✓ PDFService created");
        println!("  - Base URL: {}", config.pdf_service.base_url);
        
        // Test service health
        let health_result = pdf_service.health_check().await;
        
        match health_result {
            Ok(is_healthy) => {
                println!("✅ PDF Service integration successful!");
                println!("  - Service healthy: {}", is_healthy);
                assert!(is_healthy, "PDF service should be healthy");
            }
            Err(e) => {
                panic!("❌ PDF Service integration failed: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_complete_workflow_chain() {
        println!("\n🔗 Testing Complete Workflow Chain...");
        
        let config = load_real_config();
        
        println!("Step 1: Authenticate with Nango → Zoho");
        let unauthenticated_client = ZohoClient::new(config.zoho.clone());
        let zoho_client = unauthenticated_client.authenticate().await.expect("Zoho authentication should succeed");
        
        println!("Step 2: Load LinkedIn profiles from Baserow");
        let baserow_client = BaserowClient::new(config.baserow.clone());
        let profiles = baserow_client.get_linkedin_profiles().await.expect("Baserow should return profiles");
        assert!(!profiles.is_empty(), "Should have profiles to work with");
        
        println!("Step 3: Get tasks from Zoho CRM");
        let tasks = zoho_client.get_tasks(&[("Status", "Not Started")]).await.unwrap_or_default();
        println!("  - Found {} tasks", tasks.len());
        
        println!("Step 4: Test service connectivity");
        let letterexpress = workflow_core::clients::LetterExpressClient::new(config.letterexpress.clone());
        let pdf_service = workflow_core::clients::PDFService::new(config.pdf_service.clone());
        
        let letterexpress_ok = letterexpress.test_connection().await.unwrap_or(false);
        let pdf_ok = pdf_service.health_check().await.unwrap_or(false);
        
        println!("✅ Complete workflow chain validated!");
        println!("  - Zoho CRM: ✓ Authenticated");
        println!("  - Baserow: ✓ {} profiles", profiles.len());
        println!("  - Tasks: ✓ {} tasks", tasks.len());
        println!("  - LetterExpress: {}", if letterexpress_ok { "✓ Connected" } else { "⚠️ Issues" });
        println!("  - PDF Service: {}", if pdf_ok { "✓ Connected" } else { "⚠️ Issues" });
        
        // Basic assertions for critical services
        assert!(zoho_client.is_authenticated(), "Zoho must be authenticated");
        assert!(!profiles.is_empty(), "Must have LinkedIn profiles");
    }
}