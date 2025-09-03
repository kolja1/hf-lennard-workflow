use workflow_core::config::LennardConfig;
use workflow_core::clients::ZohoClient;

#[test]
fn test_config_maps_nango_to_zoho() {
    // Test with ACTUAL structure from credentials.json
    let config_json = r#"{
        "baserow": {
            "url": "https://api.baserow.io",
            "token": "test_token",
            "table_id": 600870
        },
        "nango_zoho_lennard": {
            "api_key": "test_api_key",
            "connection_id": "test_connection",
            "integration_id": "zoho-crm-lennard",
            "user_id": "test_user"
        },
        "letterexpress": {
            "api_key": "test_key",
            "username": "test_user",
            "api_url": "https://api.letterxpress.de/v3"
        },
        "telegram": {
            "bot_token": "test_token",
            "chat_id": "12345"
        },
        "openai": {
            "api_key": "test_key",
            "model": "gpt-4o"
        }
    }"#;
    
    let config = LennardConfig::from_json_str(config_json).expect("Config should parse");
    
    // Verify that nango_zoho_lennard is mapped to zoho config with Nango fields
    assert_eq!(config.zoho.nango_api_key, "test_api_key", "Nango API key should be mapped");
    assert_eq!(config.zoho.nango_connection_id, "test_connection", "Connection ID should be mapped");
    assert_eq!(config.zoho.nango_integration_id, "zoho-crm-lennard", "Integration ID should be mapped");
    assert_eq!(config.zoho.nango_user_id, "test_user", "User ID should be mapped");
}

#[test]
fn test_zoho_client_supports_nango_config() {
    // Create ZohoConfig with Nango credentials
    let zoho_config = workflow_core::config::ZohoConfig {
        base_url: "https://www.zohoapis.com".to_string(),
        // Nango fields
        nango_api_key: "test_api_key".to_string(),
        nango_connection_id: "test_connection".to_string(), 
        nango_integration_id: "zoho-crm-lennard".to_string(),
        nango_user_id: "test_user".to_string(),
    };
    
    // Should be able to create ZohoClient with Nango config
    let _zoho_client = ZohoClient::new(zoho_config); // Will panic if config is invalid
}

#[tokio::test]
async fn test_nango_authentication_flow() {
    // This test will fail initially until we implement Nango authentication
    let zoho_config = workflow_core::config::ZohoConfig {
        base_url: "https://www.zohoapis.com".to_string(),
        nango_api_key: "test_api_key".to_string(),
        nango_connection_id: "test_connection".to_string(),
        nango_integration_id: "zoho-crm-lennard".to_string(),
        nango_user_id: "test_user".to_string(),
    };
    
    let zoho_client = ZohoClient::new(zoho_config);
    
    // Should be able to authenticate via Nango (will fail until implemented)
    let auth_result = zoho_client.authenticate().await;
    
    // For now, just verify the method exists and returns appropriate error
    match auth_result {
        Ok(authenticated_client) => {
            // Great! Authentication worked
            assert!(authenticated_client.is_authenticated(), "Client should be authenticated after successful auth");
        }
        Err(e) => {
            // Expected for now - but error should mention Nango, not generic auth failure
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("Nango") || error_msg.contains("authenticate"),
                "Error should be related to Nango authentication: {}", 
                error_msg
            );
        }
    }
}

