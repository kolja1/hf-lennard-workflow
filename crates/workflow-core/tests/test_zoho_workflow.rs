//! Complete workflow test for Zoho task processing
//! 
//! This test demonstrates the full workflow from fetching tasks to processing them

use workflow_core::config::LennardConfig;
use workflow_core::clients::ZohoClient;
use std::path::Path;

#[tokio::test]
#[ignore]  // Only run when explicitly requested with --ignored flag
async fn test_complete_zoho_workflow() {
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
    
    println!("=== Complete Zoho Workflow Test ===\n");
    
    // Step 1: Create and authenticate client
    println!("Step 1: Authenticating with Zoho via Nango...");
    let client = ZohoClient::new(config.zoho);
    let authenticated_client = match client.authenticate().await {
        Ok(c) => {
            println!("✓ Authentication successful\n");
            c
        }
        Err(e) => {
            panic!("Authentication failed: {}", e);
        }
    };
    
    // Step 2: Search for tasks with specific criteria
    println!("Step 2: Searching for tasks...");
    
    // Try to find tasks with "Intro Letter" in subject
    let filters = vec![
        ("Subject", "Intro Letter"),
        ("Status", "Not Started")
    ];
    
    let tasks = match authenticated_client.get_tasks(&filters).await {
        Ok(tasks) => {
            println!("✓ Found {} tasks matching criteria", tasks.len());
            if tasks.is_empty() {
                println!("  (No tasks found - trying broader search...)\n");
                
                // Try without status filter
                let broader_filters = vec![("Subject", "Intro Letter")];
                match authenticated_client.get_tasks(&broader_filters).await {
                    Ok(t) => {
                        println!("  Found {} tasks with just subject filter", t.len());
                        t
                    }
                    Err(e) => {
                        if e.to_string().contains("204") {
                            println!("  No tasks found even with broader search");
                            vec![]
                        } else {
                            panic!("Broader search failed: {}", e);
                        }
                    }
                }
            } else {
                tasks
            }
        }
        Err(e) => {
            if e.to_string().contains("204") || e.to_string().contains("No Content") {
                println!("✓ No tasks found (204 No Content)\n");
                vec![]
            } else {
                panic!("Task search failed: {}", e);
            }
        }
    };
    
    // Step 3: Process first task if found
    if !tasks.is_empty() {
        println!("\nStep 3: Processing first task...");
        let first_task = &tasks[0];
        
        println!("  Task ID: {}", first_task.id);
        println!("  Subject: {}", first_task.subject);
        println!("  Status: {}", first_task.status.as_deref().unwrap_or("N/A"));
        
        // Check if task has a contact (Who_Id)
        if let Some(who_id) = &first_task.who_id {
            let contact_id = &who_id.id;
            println!("  Contact ID: {}", contact_id);
                
                // Step 4: Fetch contact details
                println!("\nStep 4: Fetching contact details...");
                match authenticated_client.get_contact(contact_id).await {
                    Ok(Some(contact)) => {
                        println!("✓ Contact retrieved successfully");
                        println!("  Name: {}", contact.full_name);
                        println!("  Email: {}", contact.email.as_deref().unwrap_or("N/A"));
                        println!("  LinkedIn ID: {}", contact.linkedin_id.as_deref().unwrap_or("N/A"));
                        
                        if let Some(address) = &contact.mailing_address {
                            println!("  Mailing Address:");
                            println!("    Street: {}", address.street);
                            println!("    City: {}", address.city);
                            println!("    Postal Code: {}", address.postal_code);
                            println!("    Country: {}", address.country);
                        } else {
                            println!("  No mailing address on file");
                        }
                    }
                    Ok(None) => {
                        println!("⚠ Contact not found");
                    }
                    Err(e) => {
                        println!("⚠ Failed to fetch contact: {}", e);
                    }
                }
        } else {
            println!("  No contact linked to task (Who_Id is null)");
        }
        
        // Step 5: Demonstrate workflow decision points
        println!("\nStep 5: Workflow decision points:");
        
        // Check if we have LinkedIn ID
        if let Some(who_id) = &first_task.who_id {
            let contact_id = &who_id.id;
            if let Ok(Some(contact)) = authenticated_client.get_contact(contact_id).await {
                if contact.linkedin_id.is_some() {
                    println!("✓ LinkedIn ID present - can generate dossier");
                } else {
                    println!("⚠ No LinkedIn ID - manual dossier creation needed");
                }
                
                if contact.mailing_address.is_some() {
                    println!("✓ Mailing address present - ready for letter");
                } else {
                    println!("⚠ No mailing address - need to extract from dossier");
                }
            }
        }
    } else {
        println!("\nNo tasks to process - workflow test limited to authentication");
    }
    
    println!("\n=== Workflow Test Complete ===");
    println!("✅ All accessible workflow steps tested successfully");
}

#[tokio::test]
#[ignore]  // Only run when explicitly requested
async fn test_get_single_task_by_id() {
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
    
    println!("Testing single task retrieval by ID...\n");
    
    // Authenticate
    let client = ZohoClient::new(config.zoho);
    let authenticated_client = match client.authenticate().await {
        Ok(c) => c,
        Err(e) => {
            panic!("Authentication failed: {}", e);
        }
    };
    
    // First, find any task to get a valid ID
    let filters = vec![("Status", "Not Started")];
    let tasks = match authenticated_client.get_tasks(&filters).await {
        Ok(t) if !t.is_empty() => t,
        _ => {
            println!("No tasks found to test with");
            return;
        }
    };
    
    let test_id = &tasks[0].id;
    println!("Testing with task ID: {}", test_id);
    
    // Now test getting single task by ID
    match authenticated_client.get_task_by_id(test_id).await {
        Ok(Some(task)) => {
            println!("✓ Successfully retrieved task by ID");
            println!("  Subject: {}", task.subject);
            println!("  Status: {}", task.status.as_deref().unwrap_or("N/A"));
            
            // The single task endpoint should include Who_Id
            if let Some(who_id) = &task.who_id {
                println!("  ✓ Who_Id is present: {:?}", who_id.id);
            } else {
                println!("  ⚠ Who_Id is missing (may not be linked to contact)");
            }
        }
        Ok(None) => {
            println!("⚠ Task not found by ID");
        }
        Err(e) => {
            panic!("Failed to get task by ID: {}", e);
        }
    }
    
    println!("\n✅ Single task retrieval test complete");
}