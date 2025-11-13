# TODO: Dossier-Speicherung in Zoho CRM

## Problem
Die generierten Dossiers (Person + Company Research) werden aktuell NICHT in Zoho CRM gespeichert.

**Status Quo:**
- ✅ Nur Postadresse wird in Zoho gespeichert
- ❌ Person Dossier Content wird nicht gespeichert
- ❌ Company Dossier Content wird nicht gespeichert
- Dossiers existieren nur temporär in Approval-JSON-Dateien

## Implementierung

### 1. traits.rs - Neue Methode hinzufügen
```rust
async fn update_contact_dossiers(&self, contact_id: &str, person_dossier: &str, company_dossier: &str) -> Result<()>;
```

### 2. clients/zoho.rs - Zoho-Client-Methode
PUT Request an `/crm/v2/Contacts/{id}` mit Description-Feld:
```rust
pub async fn update_contact_dossiers(&self, contact_id: &str, person_dossier: &str, company_dossier: &str) -> Result<()> {
    let update_data = json!({
        "data": [{
            "id": contact_id,
            "Description": format!("Person Dossier:\n{}\n\nCompany Dossier:\n{}", person_dossier, company_dossier)
        }]
    });
    // PUT request implementieren
}
```

### 3. services/workflow_processor.rs - Implementation
```rust
async fn update_contact_dossiers(&self, contact_id: &str, person_dossier: &str, company_dossier: &str) -> Result<()> {
    self.zoho_client.update_contact_dossiers(contact_id, person_dossier, company_dossier).await
}
```

### 4. workflow/orchestrator.rs - Workflow-Step
Nach Zeile 183 (nach Address-Update):
```rust
// Step 3.6: Store dossier data in Zoho contact
self.steps.update_contact_dossiers(
    &contact.id,
    &dossier_result.person_dossier_content,
    &dossier_result.company_dossier_content
).await?;
```

## Vorteile
- Dossier-Daten bleiben dauerhaft in Zoho erhalten
- Keine redundante Recherche bei späteren Workflows
- Zentrale Datenhaltung im CRM
- Möglichkeit zur manuellen Review/Bearbeitung in Zoho

## Code-Locations
- `crates/workflow-core/src/workflow/traits.rs`
- `crates/workflow-core/src/clients/zoho.rs`
- `crates/workflow-core/src/services/workflow_processor.rs`
- `crates/workflow-core/src/workflow/orchestrator.rs`
