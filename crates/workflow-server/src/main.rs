//! Main workflow processor executable
//! 
//! This binary replaces the Python main_workflow.py

mod grpc_service;

use clap::{Arg, Command};
use workflow_core::{
    LennardConfig, 
    workflow::{WorkflowOrchestrator, ApprovalWatcher, approval_types::WorkflowTrigger}, 
    services::WorkflowProcessor,
    clients::{BaserowClient, ZohoClient, DossierClient, LetterExpressClient, LetterServiceClient, PDFService, TelegramClient},
    services::AddressExtractor,
    paths,
};
use std::sync::Arc;
use notify::{RecommendedWatcher, Watcher, RecursiveMode, Event, EventKind};
use std::sync::mpsc::channel;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize logging with INFO as default if RUST_LOG not set
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();
    
    let matches = Command::new("main-workflow")
        .version("1.0.0")
        .about("Lennard workflow processor")
        .arg(
            Arg::new("config")
                .long("config")
                .short('c')
                .value_name("FILE")
                .help("Configuration file path")
                .default_value("/app/config/credentials.json")
        )
        .arg(
            Arg::new("task-id")
                .long("task-id")
                .value_name("ID") 
                .help("Process specific task ID")
        )
        .arg(
            Arg::new("monitor-workflows")
                .long("monitor-workflows")
                .help("Monitor workflow triggers")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("grpc-server")
                .long("grpc-server")
                .help("Start gRPC server")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("grpc-port")
                .long("grpc-port")
                .value_name("PORT")
                .help("gRPC server port")
                .default_value("50051")
        )
        .arg(
            Arg::new("data-dir")
                .long("data-dir")
                .value_name("DIR")
                .help("Data directory for workflow files")
                .default_value("/data/workflows")
        )
        .arg(
            Arg::new("logs-dir")
                .long("logs-dir")
                .value_name("DIR")
                .help("Logs directory for debugging")
                .default_value("/logs")
        )
        .arg(
            Arg::new("templates-dir")
                .long("templates-dir")
                .value_name("DIR")
                .help("Templates directory for ODT templates")
                .default_value("/app/templates")
        )
        .get_matches();
    
    // Initialize data directory
    let data_dir = matches.get_one::<String>("data-dir").unwrap();
    if let Err(e) = paths::init_data_root(data_dir.clone()) {
        // If it fails, it means it was already initialized (shouldn't happen in main)
        log::warn!("Data root initialization warning: {}", e);
    }
    log::info!("Using data directory: {}", data_dir);
    
    // Initialize logs directory
    let logs_dir = matches.get_one::<String>("logs-dir").unwrap();
    if let Err(e) = paths::init_logs_root(logs_dir.clone()) {
        log::warn!("Logs root initialization warning: {}", e);
    }
    log::info!("Using logs directory: {}", logs_dir);
    
    // Initialize templates directory
    let templates_dir = matches.get_one::<String>("templates-dir").unwrap();
    if let Err(e) = paths::init_templates_root(templates_dir.clone()) {
        log::warn!("Templates root initialization warning: {}", e);
    }
    log::info!("Using templates directory: {}", templates_dir);
    
    // Load configuration
    let config_path = matches.get_one::<String>("config").unwrap();
    let config = LennardConfig::from_file(config_path)?;
    
    log::info!("Loaded configuration from {}", config_path);
    
    // Initialize all service clients with type-safe authentication
    let unauthenticated_zoho_client = ZohoClient::new(config.zoho.clone());
    
    // Authenticate ZohoClient - compiler enforces this transition
    let authenticated_zoho_client = match unauthenticated_zoho_client.authenticate().await {
        Ok(client) => {
            log::info!("✅ ZohoClient authenticated successfully");
            client
        }
        Err(e) => {
            log::error!("Failed to authenticate ZohoClient: {}", e);
            return Err(e.into());
        }
    };
    
    let zoho_client = Arc::new(authenticated_zoho_client);
    let baserow_client = Arc::new(BaserowClient::new(config.baserow.clone()));
    let dossier_client = Arc::new(DossierClient::new(config.dossier.clone()));
    let letterexpress_client = Arc::new(LetterExpressClient::new(config.letterexpress.clone()));
    let pdf_service = Arc::new(PDFService::new(config.pdf_service.clone()));
    let address_extractor = Arc::new(AddressExtractor::new(config.openai.clone()));
    let letter_service = Arc::new(LetterServiceClient::new(config.letter_service.clone()));
    let telegram_client: Arc<dyn workflow_core::clients::TelegramClientTrait> = Arc::new(TelegramClient::new(config.telegram.clone()));
    
    // Create ApprovalQueue with the workflows data directory
    let approval_queue = Arc::new(
        workflow_core::workflow::ApprovalQueue::new(paths::workflow_data_root())
            .expect("Failed to initialize ApprovalQueue")
    );
    log::info!("Initialized ApprovalQueue at {}", paths::workflow_data_root().display());
    
    // Create workflow processor with all services
    let workflow_processor = WorkflowProcessor::new(
        zoho_client,
        baserow_client, 
        dossier_client,
        letterexpress_client,
        pdf_service,
        address_extractor,
        letter_service,
        telegram_client,
        approval_queue.clone(),
    );
    
    // Create orchestrator with strongly-typed workflow steps
    let orchestrator = Arc::new(WorkflowOrchestrator::new(workflow_processor));
    
    log::info!("Initialized all services and orchestrator");
    
    if let Some(_task_id) = matches.get_one::<String>("task-id") {
        log::info!("Processing single task mode (will dynamically load first available task)");
        
        // Create a generic WorkflowTrigger for single task processing
        // Note: task loading is now handled dynamically by the orchestrator
        let trigger = WorkflowTrigger {
            trigger_id: uuid::Uuid::new_v4().to_string(),
            requested_by: workflow_core::workflow::approval_types::UserId::new(1), // Default user
            requested_at: chrono::Utc::now(),
            max_tasks: 1, // Process exactly 1 task
            dry_run: false,
            processed: false,
            processed_at: None,
            result: None,
        };
        
        // Process using orchestrator with dynamic task loading
        match orchestrator.process_workflow(trigger).await {
            Ok(result) => {
                log::info!("Workflow processed successfully");
                if let Some(result_msg) = &result.result {
                    log::info!("Result: {}", result_msg);
                }
            },
            Err(e) => log::error!("Failed to process workflow: {}", e),
        }
    } else if matches.get_flag("monitor-workflows") {
        log::info!("Starting workflow monitor mode");
        // Monitor workflow triggers
        monitor_workflows(orchestrator).await?;
    } else if matches.get_flag("grpc-server") {
        let port: u16 = matches.get_one::<String>("grpc-port")
            .unwrap()
            .parse()
            .expect("Invalid port number");
        
        let addr = format!("0.0.0.0:{}", port).parse()?;
        
        log::info!("Starting gRPC server on port {}", port);
        
        // Start gRPC server, workflow monitor, and approval watcher in parallel
        let orchestrator_grpc = orchestrator.clone();
        let orchestrator_monitor = orchestrator.clone();
        let orchestrator_watcher = orchestrator.clone();
        
        let approval_queue_grpc = approval_queue.clone();
        let approval_queue_watcher = approval_queue.clone();
        
        // Create approval watcher
        let approval_watcher = Arc::new(ApprovalWatcher::new(
            approval_queue_watcher,
            orchestrator_watcher,
        ));
        
        let grpc_handle = tokio::spawn(async move {
            grpc_service::start_grpc_server(orchestrator_grpc, approval_queue_grpc, addr).await
        });
        
        let monitor_handle = tokio::spawn(async move {
            monitor_workflows(orchestrator_monitor).await
        });
        
        let watcher_handle = tokio::spawn(async move {
            approval_watcher.start().await;
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        });
        
        // Wait for any to complete (or fail)
        tokio::select! {
            result = grpc_handle => {
                match result {
                    Ok(Ok(_)) => log::info!("gRPC server exited normally"),
                    Ok(Err(e)) => {
                        log::error!("gRPC server failed to start: {}", e);
                        std::process::exit(1);
                    }
                    Err(e) => {
                        log::error!("gRPC server task panicked: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            result = monitor_handle => {
                match result {
                    Ok(Ok(_)) => log::info!("Workflow monitor exited normally"),
                    Ok(Err(e)) => {
                        log::error!("Workflow monitor failed: {}", e);
                        std::process::exit(1);
                    }
                    Err(e) => {
                        log::error!("Workflow monitor task panicked: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            result = watcher_handle => {
                match result {
                    Ok(Ok(_)) => log::info!("Approval watcher exited normally"),
                    Ok(Err(e)) => {
                        log::error!("Approval watcher failed: {}", e);
                        std::process::exit(1);
                    }
                    Err(e) => {
                        log::error!("Approval watcher task panicked: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
    } else {
        log::error!("No action specified. Use --help for options.");
        std::process::exit(1);
    }
    
    Ok(())
}

// Removed - now handled directly by WorkflowProcessor

async fn monitor_workflows(orchestrator: Arc<WorkflowOrchestrator<WorkflowProcessor>>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let triggers_path = paths::triggers_dir();
    let processed_path = paths::triggers_processed_dir();
    
    log::info!("Monitoring workflow triggers in {}/", triggers_path.display());
    
    // Ensure directories exist
    std::fs::create_dir_all(&triggers_path)?;
    std::fs::create_dir_all(&processed_path)?;
    
    // Set up file system watcher
    let (tx, rx) = channel();
    let mut watcher = RecommendedWatcher::new(
        move |result: Result<Event, notify::Error>| {
            if let Ok(event) = result {
                tx.send(event).unwrap();
            }
        },
        notify::Config::default(),
    )?;
    watcher.watch(&triggers_path, RecursiveMode::NonRecursive)?;
    
    log::info!("Started monitoring workflow triggers");
    
    // Process existing files first
    if let Ok(entries) = std::fs::read_dir(&triggers_path) {
        for entry in entries {
            if let Ok(entry) = entry {
                if entry.path().is_file() {
                    process_trigger_file(&orchestrator, &entry.path(), &processed_path).await?;
                }
            }
        }
    }
    
    // Monitor for new files
    loop {
        match rx.recv() {
            Ok(event) => {
                log::debug!("File system event: {:?}", event);
                
                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) => {
                        for path in event.paths {
                            if path.is_file() {
                                if let Err(e) = process_trigger_file(&orchestrator, &path, &processed_path).await {
                                    log::error!("Failed to process trigger file {:?}: {}", path, e);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Err(e) => {
                log::error!("Watcher error: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
    }
}

async fn process_trigger_file(
    orchestrator: &Arc<WorkflowOrchestrator<WorkflowProcessor>>,
    trigger_path: &Path,
    processed_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let file_name = trigger_path.file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid file name")?;
    
    log::info!("Processing trigger file: {}", file_name);
    
    // Read and deserialize WorkflowTrigger
    let content = std::fs::read_to_string(trigger_path)?;
    
    let trigger: WorkflowTrigger = 
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse WorkflowTrigger JSON: {}", e))?;
    
    log::info!("Processing WorkflowTrigger: {} for up to {} tasks", trigger.trigger_id, trigger.max_tasks);
    
    // Process the workflow using orchestrator with strongly-typed steps
    let workflow_result = orchestrator.process_workflow(trigger).await;
    
    // Determine destination based on result
    let destination_dir = match &workflow_result {
        Ok(result) => {
            log::info!("Successfully processed workflow: {}", result.trigger_id);
            if let Some(result_msg) = &result.result {
                log::info!("Result: {}", result_msg);
            }
            processed_path
        }
        Err(e) => {
            log::error!("Failed to process workflow: {}", e);
            // Use the failed directory for error cases
            &paths::triggers_failed_dir()
        }
    };
    
    // Move trigger file to appropriate directory
    let destination_file = destination_dir.join(file_name);
    std::fs::rename(trigger_path, destination_file)?;
    
    log::debug!("Moved trigger file to {}: {}", 
        if workflow_result.is_ok() { "processed" } else { "failed" }, 
        file_name);
    
    // Always return Ok since we handled the file move regardless of workflow result
    Ok(())
}