//! Directory watcher for processing approved workflows
//! 
//! This module watches the approved directory and processes files as they appear,
//! treating the directory like a message queue.

use crate::error::{LennardError, Result};
use crate::workflow::approval_types::ApprovalData;
use crate::workflow::orchestrator::WorkflowOrchestrator;
use crate::workflow::traits::WorkflowSteps;
use crate::workflow::ApprovalQueue;
use crate::paths;
use std::sync::Arc;
use std::path::{Path, PathBuf};
use tokio::time::{sleep, Duration};
use log::{info, error, warn, debug};

/// Watches the approved directory and processes approved workflows
pub struct ApprovalWatcher<T: WorkflowSteps> {
    orchestrator: Arc<WorkflowOrchestrator<T>>,
    approved_dir: PathBuf,
    processing_interval: Duration,
}

impl<T: WorkflowSteps + Send + Sync + 'static> ApprovalWatcher<T> {
    pub fn new(
        _approval_queue: Arc<ApprovalQueue>,  // Keep for API compatibility but unused
        orchestrator: Arc<WorkflowOrchestrator<T>>,
    ) -> Self {
        let approved_dir = paths::approved_dir();
        
        Self {
            orchestrator,
            approved_dir,
            processing_interval: Duration::from_secs(5), // Check every 5 seconds
        }
    }
    
    /// Start watching the approved directory
    pub async fn start(self: Arc<Self>) {
        info!("Starting approval watcher for directory: {:?}", self.approved_dir);
        
        // Process any existing approved files on startup
        self.process_existing_approvals().await;
        
        // Then continuously watch for new files
        loop {
            self.check_and_process_approvals().await;
            sleep(self.processing_interval).await;
        }
    }
    
    /// Process any existing approved files on startup
    async fn process_existing_approvals(&self) {
        info!("Checking for existing approved workflows to process...");
        
        match std::fs::read_dir(&self.approved_dir) {
            Ok(entries) => {
                let mut count = 0;
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                        if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                            if file_name.starts_with("approval_") {
                                count += 1;
                                info!("Found existing approved workflow: {}", file_name);
                                self.process_approval_file(&path).await;
                            }
                        }
                    }
                }
                
                if count == 0 {
                    info!("No existing approved workflows found");
                } else {
                    info!("Processed {} existing approved workflows", count);
                }
            }
            Err(e) => {
                error!("Failed to read approved directory: {}", e);
            }
        }
    }
    
    /// Check for and process new approved files
    async fn check_and_process_approvals(&self) {
        match std::fs::read_dir(&self.approved_dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                        if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                            if file_name.starts_with("approval_") && !file_name.contains(".processing") {
                                debug!("Found approved workflow to process: {}", file_name);
                                self.process_approval_file(&path).await;
                            }
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to read approved directory: {}", e);
            }
        }
    }
    
    /// Process a single approval file
    async fn process_approval_file(&self, path: &Path) {
        let file_name = path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
            
        info!("Processing approved workflow: {}", file_name);
        
        // Mark file as being processed by renaming it
        let processing_path = path.with_extension("json.processing");
        if let Err(e) = std::fs::rename(path, &processing_path) {
            error!("Failed to mark file as processing: {}", e);
            return;
        }
        
        // Read and parse the approval data
        match self.read_approval_data(&processing_path) {
            Ok(approval_data) => {
                info!(
                    "Processing approval {} for task {} (recipient: {})",
                    approval_data.approval_id,
                    approval_data.task_id,
                    approval_data.recipient_name
                );
                
                // Continue the workflow
                match self.orchestrator.continue_after_approval(&approval_data).await {
                    Ok(tracking_id) => {
                        info!(
                            "Successfully sent letter for approval {} - tracking ID: {}",
                            approval_data.approval_id,
                            tracking_id
                        );
                        
                        // Move to processed directory
                        let processed_dir = paths::processed_dir();
                            
                        // Ensure processed directory exists
                        if let Err(e) = std::fs::create_dir_all(&processed_dir) {
                            error!("Failed to create processed directory: {}", e);
                        }
                        
                        let processed_path = processed_dir.join(format!(
                            "approval_{}_processed_{}.json",
                            approval_data.approval_id,
                            chrono::Utc::now().format("%Y%m%d_%H%M%S")
                        ));
                        
                        if let Err(e) = std::fs::rename(&processing_path, processed_path) {
                            error!("Failed to move file to processed: {}", e);
                        }
                    }
                    Err(e) => {
                        error!(
                            "Failed to continue workflow for approval {}: {}",
                            approval_data.approval_id,
                            e
                        );
                        
                        // Move to failed directory
                        let failed_dir = paths::failed_dir();
                            
                        // Ensure failed directory exists
                        if let Err(e) = std::fs::create_dir_all(&failed_dir) {
                            error!("Failed to create failed directory: {}", e);
                        }
                        
                        let failed_path = failed_dir.join(format!(
                            "approval_{}_failed_{}.json",
                            approval_data.approval_id,
                            chrono::Utc::now().format("%Y%m%d_%H%M%S")
                        ));
                        
                        if let Err(e) = std::fs::rename(&processing_path, failed_path) {
                            error!("Failed to move file to failed: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to read approval data from {}: {}", file_name, e);
                
                // Move to error directory (use failed dir for errors too)
                let error_dir = paths::failed_dir();
                    
                if let Err(e) = std::fs::create_dir_all(&error_dir) {
                    error!("Failed to create error directory: {}", e);
                }
                
                let error_path = error_dir.join(format!(
                    "{}_error_{}.json",
                    file_name,
                    chrono::Utc::now().format("%Y%m%d_%H%M%S")
                ));
                
                if let Err(e) = std::fs::rename(&processing_path, error_path) {
                    error!("Failed to move file to error: {}", e);
                }
            }
        }
    }
    
    /// Read and parse approval data from file
    fn read_approval_data(&self, path: &Path) -> Result<ApprovalData> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| LennardError::IoError(format!("Failed to read approval file: {}", e)))?;

        let mut approval_data: ApprovalData = serde_json::from_str(&content)
            .map_err(|e| LennardError::Serialization(format!("Failed to parse approval data: {}", e)))?;

        // Unescape literal \n characters that may have been stored in JSON
        approval_data.current_letter.unescape_newlines();

        Ok(approval_data)
    }
}