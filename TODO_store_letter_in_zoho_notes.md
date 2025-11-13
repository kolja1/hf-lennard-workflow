# TODO: Store Letter Content in Zoho CRM Notes

## Overview
Store letter content, send date, and LetterExpress tracking ID in Zoho CRM using the Notes API after successful letter delivery.

## Changes Required

### 1. Add Zoho Notes API method
**File:** `crates/workflow-core/src/clients/zoho.rs`

Add `create_contact_note()` method to `ZohoClient<Authenticated>` impl block:

```rust
pub async fn create_contact_note(
    &self,
    contact_id: &str,
    note_title: &str,
    note_content: &str
) -> Result<()> {
    let url = format!("{}/crm/v8/Contacts/{}/Notes", self.base_url, contact_id);

    let access_token = self.get_fresh_token().await?;

    let note_data = json!({
        "data": [{
            "Note_Title": note_title,
            "Note_Content": note_content,
            "Parent_Id": {
                "id": contact_id
            }
        }]
    });

    let response = self.http_client
        .post(&url)
        .bearer_auth(&access_token)
        .json(&note_data)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(LennardError::ServiceUnavailable(
            format!("Failed to create Zoho note: {}", error_text)
        ));
    }

    log::info!("Created note in Zoho contact {}: {}", contact_id, note_title);
    Ok(())
}
```

### 2. Add trait method
**File:** `crates/workflow-core/src/workflow/traits.rs`

Add to `WorkflowSteps` trait:

```rust
/// Store letter content in Zoho CRM after successful send
async fn store_letter_content(
    &self,
    contact_id: &str,
    company_name: &str,
    letter: &LetterContent,
    tracking_id: &str
) -> Result<()>;
```

### 3. Implement trait method
**File:** `crates/workflow-core/src/services/workflow_processor.rs`

```rust
async fn store_letter_content(
    &self,
    contact_id: &str,
    company_name: &str,
    letter: &LetterContent,
    tracking_id: &str
) -> Result<()> {
    let now = chrono::Utc::now();
    let date_str = now.format("%Y-%m-%d").to_string();

    let note_title = format!("Brief versendet - {} - {}", date_str, company_name);

    let note_content = format!(
        "LetterExpress Tracking-ID: {}\nVersanddatum: {}\n\nBetreff: {}\n\n{}\n\n{}",
        tracking_id,
        date_str,
        letter.subject,
        letter.greeting,
        letter.body
    );

    self.zoho_client.create_contact_note(contact_id, &note_title, &note_content).await
}
```

### 4. Call after letter send
**File:** `crates/workflow-core/src/workflow/orchestrator.rs`

In `continue_after_approval()` after line 341 (after successful `send_pdf_binary`):

```rust
// Store letter content in Zoho CRM Notes
if let Err(e) = self.steps.store_letter_content(
    approval_data.contact_id.as_str(),
    &approval_data.company_name,
    &approval_data.current_letter,
    &tracking_id
).await {
    log::error!("Failed to store letter content in Zoho notes: {}", e);
    // Don't fail the whole workflow if note creation fails
} else {
    log::info!("Stored letter content in Zoho notes for contact {}", approval_data.contact_id);
}
```

## Note Format
```
Title: Brief versendet - 2025-11-12 - Company GmbH

Content:
LetterExpress Tracking-ID: ABC123XYZ
Versanddatum: 2025-11-12

Betreff: {subject}

{greeting}

{body}
```

## API Documentation
- Zoho CRM v8 Notes API: https://www.zoho.com/crm/developer/docs/api/v8/insert-notes.html
- Endpoint: `POST /crm/v8/Contacts/{contact_id}/Notes`
- Authentication: Bearer token via Nango

## Status
- Branch created: `feature/store-letter-content-in-zoho-notes`
- Implementation: Pending (deferred to investigate feedback processing issue)
