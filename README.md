# Lennard Workflow Engine

A high-performance workflow processing engine built in Rust with gRPC API for managing the complete letter generation and approval workflow.

## Overview

This standalone service processes Zoho CRM tasks through a 7-step workflow:
1. Load tasks from Zoho CRM
2. Fetch contact information
3. Load LinkedIn profiles and generate dossiers
4. Extract and update mailing addresses
5. Generate personalized letters with AI
6. Request approval via Telegram
7. Send physical mail via LetterExpress

## Detailed Workflow Steps

### Step 1: Load Available Tasks from Zoho CRM
- **Purpose**: Fetch pending tasks that need to be processed
- **Source**: Zoho CRM Tasks module
- **Filters Applied**:
  - Subject contains: "Connect on LinkedIn"
  - Status: "Nicht gestartet" (Not started)
  - Owner: Specific configured user (ID: 1294764000001730350)
  - Sorting: Oldest tasks first (by creation time)
- **Output**: List of `TasksResponse` objects with contact and company references (`who_id`, `what_id`)
- **Error Handling**: Retries with exponential backoff, graceful degradation if CRM is unavailable

### Step 2: Load Contact Information
- **Purpose**: Retrieve detailed contact information for the task's associated contact
- **Process**:
  1. Extract contact ID from task's `who_id` field (contains `{id, name}`)
  2. Fetch full contact record via Zoho API: `GET /crm/v2/Contacts/{contact_id}`
  3. Parse and validate contact data
- **Data Retrieved from Zoho**:
  - `id`: Contact's Zoho ID
  - `Full_Name`: Complete name as stored in Zoho
  - `Email`: Primary email (optional)
  - `Phone`: Primary phone number (optional)
  - `Account_Name`: Associated company name (optional)
  - `LinkedIn_ID`: Custom field with LinkedIn profile identifier (required)
  - Mailing address (optional at this stage):
    - `Mailing_Street`: Street address
    - `Mailing_City`: City
    - `Mailing_State`: State/Province (optional)
    - `Mailing_Code`: Postal/ZIP code
    - `Mailing_Country`: Country
- **Authentication**: Uses Nango-managed OAuth token with automatic refresh
- **Validation**: 
  - Task must have `who_id` field populated
  - Contact must exist in Zoho CRM
  - Contact must have `LinkedIn_ID` custom field populated
- **Output**: `ZohoContact` struct with all available information
- **Error Cases**: 
  - "Task has no associated contact" - if `who_id` is missing
  - "Contact {id} not found" - if contact doesn't exist in Zoho
  - "Contact has no LinkedIn ID" - if LinkedIn_ID field is empty

### Step 3: Load LinkedIn Profile and Generate Dossiers
- **Purpose**: Create comprehensive dossiers for intelligent letter generation
- **Process**:
  1. Fetch LinkedIn JSON data from Baserow database (table ID: 600870)
     - Uses `linkedin_id` from contact as lookup key
     - Contains full LinkedIn profile export
  2. Generate both person and company dossiers via gRPC service (localhost:50051):
     - **Person Dossier**: Professional background, skills, experience, education
     - **Company Dossier**: Extracted from company website URL found in LinkedIn data
  3. Extract structured data from dossiers:
     - Company name (from dossier content, NOT from task.what_id)
     - Mailing address with validation
- **Output**: `DossierResult` containing:
  - `company_dossier_content`: Markdown-formatted company analysis
  - `company_name`: Extracted company name
  - `mailing_address`: Optional extracted address

### Step 4: Extract and Update Mailing Address
- **Purpose**: Ensure accurate mailing address for physical letter delivery
- **Process**:
  1. Check if Zoho contact already has a mailing address
  2. If missing, use address extracted from company dossier (Step 3)
  3. Update Zoho contact record with new address (currently TODO in implementation)
- **Validation**: Address completeness check (street, city, postal code, country required)

### Step 5: Generate Personalized Letter
- **Purpose**: Create highly personalized introduction letters using AI
- **Service**: `LetterGenerator` using OpenAI API
- **Input Context**:
  - Full contact information (`ZohoContact`)
  - LinkedIn profile data (`LinkedInProfile`)
  - Person and company dossiers
- **Output**: `LetterContent` object with markdown-formatted letter text

### Step 6: Request Approval via Telegram
- **Purpose**: Human-in-the-loop quality control
- **Current Implementation**: 
  - Generates PDF from letter content
  - Creates approval request with unique ID
  - Sends to Telegram with recipient info and PDF
  - Note: Actual Telegram interaction handled by Python orchestrator layer
- **Approval States**: Pending, Approved, Rejected, Needs Revision

### Step 7: Send PDF via LetterExpress
- **Purpose**: Physical mail delivery of approved letters
- **Prerequisites**: Letter must be approved in Step 6
- **Process**:
  1. Use existing PDF from approval
  2. Submit to LetterExpress API
  3. Receive tracking information
  4. Update Zoho task status to "Completed"
- **Services**: `LetterExpressClient` for mail submission

## Architecture

The service exposes a gRPC API with strong typing using Protocol Buffers, ensuring type safety across service boundaries.

```
┌─────────────────┐         gRPC          ┌────────────────────┐
│  Python Client  │◄─────────────────────►│ Rust Workflow      │
│  (Telegram Bot) │                       │ Engine (gRPC)      │
└─────────────────┘                       └────────────────────┘
                                                    │
                                ┌───────────────────┼───────────────────┐
                                │                   │                   │
                            ┌───▼───┐          ┌────▼────┐      ┌───────▼───────┐
                            │ Zoho  │          │Baserow  │      │LetterExpress │
                            │ CRM   │          │Database │      │Mail Service   │
                            └───────┘          └─────────┘      └───────────────┘
```

## Features

- **Strong Typing**: Complete type safety with gRPC/Protobuf
- **PDF Support**: Approval requests include generated PDFs
- **Real-time Updates**: Stream workflow progress via gRPC
- **Error Recovery**: Comprehensive error handling and retry logic
- **Monitoring**: Built-in health checks and Prometheus metrics
- **Scalable**: Async/await with Tokio runtime

## Quick Start

### Prerequisites

- Rust 1.75 or higher
- Protocol Buffer compiler (protoc)
- Docker and docker-compose (for containerized deployment)

### Build

```bash
# Build all crates
cargo build --release

# Run tests
cargo test

# Run the server
cargo run --bin workflow-server
```

### Docker

```bash
# Build and run with Docker
docker-compose up --build

# Run in production mode
docker-compose -f docker-compose.prod.yml up -d
```

## API Documentation

The service provides three gRPC services:

### WorkflowService
- `TriggerWorkflow` - Start workflow processing
- `GetWorkflowState` - Check workflow status
- `ListWorkflows` - List active workflows
- `StreamWorkflowUpdates` - Real-time updates

### ApprovalService
- `GetPendingApprovals` - Get approvals awaiting decision
- `SubmitApproval` - Submit approval decision
- `DownloadApprovalPdf` - Download PDF for review

### Health
- Standard gRPC health checking protocol

See `proto/` directory for complete API definitions.

## Configuration

Configuration can be provided via:
1. Configuration file (`config/default.toml`)
2. Environment variables (prefix: `WORKFLOW_`)
3. Command-line arguments

Example configuration:
```toml
[server]
host = "0.0.0.0"
grpc_port = 50051

[workflow]
max_concurrent_tasks = 10
task_timeout_seconds = 300
```

## Development

### Project Structure

```
rust-workflow/
├── proto/              # Protocol buffer definitions
├── crates/
│   ├── workflow-types/ # Shared types
│   ├── workflow-core/  # Business logic
│   ├── workflow-grpc/  # Generated gRPC code
│   └── workflow-server/# gRPC server implementation
├── config/             # Configuration files
├── tests/              # Integration tests
└── examples/           # Client examples
```

### Testing

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_workflow_processing

# Run with logging
RUST_LOG=debug cargo test
```

## Deployment

See [deployment documentation](docs/deployment.md) for production deployment instructions.

## System Interaction Flow

The following text diagram shows how different services interact during the workflow:

```
Python Client → Workflow Engine: TriggerWorkflow(max_tasks=5)
│
├─ STEP 1: Load Available Tasks
│  └─ Workflow Engine → Zoho CRM: GET /crm/v2/Tasks
│     └─ Filters: Subject="Connect on LinkedIn", Status="Nicht gestartet", Owner=1294764000001730350
│     └─ Zoho CRM → Workflow Engine: TasksResponse[] (filtered & sorted by creation time)
│
└─ FOR EACH TASK:
   │
   ├─ STEP 2: Load Contact Information
   │  └─ Workflow Engine → Zoho CRM: GET /crm/v2/Contacts/{contact_id}
   │     └─ Extract contact_id from task.who_id.id
   │     └─ Zoho CRM → Workflow Engine: ZohoContact with LinkedIn_ID field
   │
   ├─ STEP 3: Load LinkedIn Profile & Generate Dossiers
   │  ├─ Workflow Engine → Baserow DB: GET /api/database/rows/table/600870
   │  │  └─ Query by linkedin_id from contact
   │  │  └─ Baserow → Workflow Engine: LinkedInProfile JSON data
   │  │
   │  └─ Workflow Engine → Dossier Service: GenerateBothDossiers(linkedin_json)
   │     └─ Dossier Service → Workflow Engine: 
   │        - Person dossier (markdown)
   │        - Company dossier (markdown)
   │        - Extracted company_name
   │        - Extracted mailing_address (optional)
   │
   ├─ STEP 4: Extract & Update Mailing Address
   │  └─ IF contact has no mailing address AND dossier has address:
   │     └─ Workflow Engine → Zoho CRM: PATCH /crm/v2/Contacts/{id}
   │        └─ Update mailing fields with extracted address
   │        └─ Zoho CRM → Workflow Engine: Updated contact confirmation
   │
   ├─ STEP 5: Generate Personalized Letter
   │  └─ Workflow Engine → OpenAI (via LetterGenerator): 
   │     └─ Input: contact data + LinkedIn profile + dossiers
   │     └─ OpenAI → Workflow Engine: LetterContent (markdown format)
   │
   ├─ STEP 6: Request Approval via Telegram
   │  ├─ Workflow Engine → PDF Service: Generate PDF
   │  │  └─ Input: letter content + mailing address
   │  │  └─ PDF Service → Workflow Engine: PDF bytes
   │  │
   │  └─ Workflow Engine → Telegram Bot: Send approval request
   │     └─ Message includes: recipient info + PDF attachment
   │     └─ Telegram → Workflow Engine: Approval ID
   │     └─ [WAIT FOR USER DECISION]
   │     └─ Telegram → Workflow Engine: Approval/Rejection decision
   │
   └─ STEP 7: Send PDF via LetterExpress (if approved)
      ├─ Workflow Engine → LetterExpress API: Submit PDF
      │  └─ Include: PDF data + recipient address + sender address
      │  └─ LetterExpress → Workflow Engine: Tracking information
      │
      └─ Workflow Engine → Zoho CRM: Update task status
         └─ Set status to "Completed" with tracking info
         └─ OR set to "Warten auf Andere" if rejected

Workflow Engine → Python Client: WorkflowState (final result)
```

### Service Interaction Summary

**Zoho CRM** (Steps 1, 2, 4, 7):
- Called 4 times per workflow task
- Provides: Tasks, Contacts, Address updates, Status updates
- Authentication: OAuth via Nango token management

**Baserow Database** (Step 3):
- Called once per task
- Provides: LinkedIn profile JSON data
- Table ID: 600870 (linkedin-helper-profile-export)

**Dossier Service** (Step 3):
- Called once per task
- Provides: Person/Company dossiers, extracted company name and address
- Protocol: gRPC on localhost:50051

**OpenAI/LetterGenerator** (Step 5):
- Called once per task
- Provides: Personalized letter content
- Uses context from all previous steps

**PDF Service** (Step 6):
- Called once per task
- Provides: PDF generation from templates
- Input: Letter content + address data

**Telegram Bot** (Step 6):
- Called once per task
- Provides: Human approval interface
- Async: Waits for user decision

**LetterExpress** (Step 7):
- Called only for approved letters
- Provides: Physical mail delivery service
- Returns: Tracking information


