#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_parse_task_with_nested_who_id() {
        // Test data that mimics actual Zoho CRM API v2 response
        let task_json = json!({
            "id": "task-123",
            "Subject": "Test Task",
            "Description": "Test description",
            "Status": "In Progress",
            "Who_Id": {
                "id": "contact-456",
                "name": "John Doe"
            },
            "Created_Time": "2025-08-14T10:00:00Z"
        });
        
        let client = ZohoClient::unauthenticated();
        let task = client.parse_task(&task_json).unwrap();
        
        assert_eq!(task.id, "task-123");
        assert_eq!(task.subject, "Test Task");
        assert_eq!(task.contact_id, Some("contact-456".to_string()));
    }
    
    #[test]
    fn test_parse_task_with_null_who_id() {
        // Test when Who_Id is null
        let task_json = json!({
            "id": "task-789",
            "Subject": "Another Task",
            "Status": "Not Started",
            "Who_Id": null,
            "Created_Time": "2025-08-14T11:00:00Z"
        });
        
        let client = ZohoClient::unauthenticated();
        let task = client.parse_task(&task_json).unwrap();
        
        assert_eq!(task.id, "task-789");
        assert_eq!(task.contact_id, None);
    }
    
    #[test]
    fn test_parse_task_with_legacy_string_who_id() {
        // Test backward compatibility with direct string ID (shouldn't happen but good to handle)
        let task_json = json!({
            "id": "task-old",
            "Subject": "Legacy Task",
            "Status": "Completed",
            "Who_Id": "contact-legacy",
            "Created_Time": "2025-08-14T12:00:00Z"
        });
        
        let client = ZohoClient::unauthenticated();
        let task = client.parse_task(&task_json).unwrap();
        
        assert_eq!(task.contact_id, Some("contact-legacy".to_string()));
    }
}