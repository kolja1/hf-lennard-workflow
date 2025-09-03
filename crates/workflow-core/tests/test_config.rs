use workflow_core::config::LennardConfig;

#[test]
fn test_parse_actual_credentials_json() {
    // Test with EXACT structure from real credentials.json
    let json = r#"{
        "baserow": {
            "url": "https://api.baserow.io",
            "token": "test_token",
            "database_id": 253159,
            "table_id": 600870
        },
        "nango_zoho_lennard": {
            "api_key": "test_api_key",
            "connection_id": "test_conn",
            "integration_id": "zoho-crm"
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
    
    // This should parse successfully
    let config = LennardConfig::from_json_str(json).expect("Failed to parse config");
    
    // Verify values are correctly mapped
    assert_eq!(config.baserow.api_key, "test_token", "Baserow token should map to api_key");
    assert_eq!(config.baserow.base_url, "https://api.baserow.io", "Baserow url should map to base_url");
    assert_eq!(config.baserow.table_id, 600870);
    
    assert_eq!(config.zoho.nango_api_key, "test_api_key");
    assert_eq!(config.zoho.nango_connection_id, "test_conn");
    assert_eq!(config.zoho.nango_integration_id, "zoho-crm");
    assert_eq!(config.zoho.base_url, "https://www.zohoapis.com", "Zoho should have default base_url");
    
    assert_eq!(config.letterexpress.api_key, "test_key");
    assert_eq!(config.letterexpress.username, "test_user");
    assert_eq!(config.letterexpress.base_url, "https://api.letterxpress.de/v3", "LetterExpress api_url should map to base_url");
    
    assert_eq!(config.telegram.bot_token, "test_token");
    assert_eq!(config.telegram.chat_id, "12345");
    
    assert_eq!(config.openai.api_key, "test_key");
    assert_eq!(config.openai.model, "gpt-4o");
    
    // PDF service should have default value
    assert_eq!(config.pdf_service.base_url, "http://localhost:8000", "PDF service should have default value");
}

#[test]
fn test_parse_minimal_config() {
    // Test with minimal required fields
    let json = r#"{
        "baserow": {
            "url": "https://api.baserow.io",
            "token": "token",
            "table_id": 123
        },
        "nango_zoho_lennard": {
            "api_key": "key",
            "connection_id": "conn",
            "integration_id": "zoho-crm"
        },
        "letterexpress": {
            "api_key": "key",
            "username": "user",
            "api_url": "https://api.letterxpress.de"
        },
        "telegram": {
            "bot_token": "token",
            "chat_id": "123"
        },
        "openai": {
            "api_key": "key",
            "model": "gpt-4"
        }
    }"#;
    
    let config = LennardConfig::from_json_str(json).expect("Failed to parse minimal config");
    
    // Check defaults are applied
    assert_eq!(config.zoho.nango_api_key, "key");
    assert_eq!(config.zoho.nango_connection_id, "conn");
    assert_eq!(config.zoho.base_url, "https://www.zohoapis.com", "Default Zoho base URL");
    assert_eq!(config.pdf_service.base_url, "http://localhost:8000", "Default PDF service URL");
}

#[test]
fn test_validate_config() {
    let json = r#"{
        "baserow": {
            "url": "https://api.baserow.io",
            "token": "",
            "table_id": 123
        },
        "nango_zoho_lennard": {
            "api_key": "",
            "connection_id": "",
            "integration_id": ""
        },
        "letterexpress": {
            "api_key": "key",
            "username": "user",
            "api_url": "https://api.letterxpress.de"
        },
        "telegram": {
            "bot_token": "",
            "chat_id": "123"
        },
        "openai": {
            "api_key": "",
            "model": "gpt-4"
        }
    }"#;
    
    // Parsing should fail due to validation with empty required fields
    let result = LennardConfig::from_json_str(json);
    assert!(result.is_err(), "Parsing should fail with empty required fields");
    assert!(result.unwrap_err().to_string().contains("required"), "Error should mention required fields");
}