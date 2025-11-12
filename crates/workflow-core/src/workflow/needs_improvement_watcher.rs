//! Directory watcher for processing workflows that need improvement
//! 
//! This module watches the needs_improvement directory and processes files as they appear,
//! generating improved letters based on user feedback.

use crate::error::{LennardError, Result};
use crate::workflow::approval_types::ApprovalData;
use crate::workflow::orchestrator::WorkflowOrchestrator;
use crate::workflow::traits::WorkflowSteps;
use crate::paths;
use std::sync::Arc;
use std::path::{Path, PathBuf};
use tokio::time::{sleep, Duration};
use log::{info, error, warn, debug};

/// Watches the needs_improvement directory and processes workflows needing revision
pub struct NeedsImprovementWatcher<T: WorkflowSteps> {
    orchestrator: Arc<WorkflowOrchestrator<T>>,
    needs_improvement_dir: PathBuf,
    processing_interval: Duration,
}

impl<T: WorkflowSteps + Send + Sync + 'static> NeedsImprovementWatcher<T> {
    pub fn new(orchestrator: Arc<WorkflowOrchestrator<T>>) -> Self {
        let needs_improvement_dir = paths::needs_improvement_dir();
        
        Self {
            orchestrator,
            needs_improvement_dir,
            processing_interval: Duration::from_secs(5), // Check every 5 seconds
        }
    }
    
    /// Start watching the needs_improvement directory
    pub async fn start(self: Arc<Self>) {
        info!("Starting needs improvement watcher for directory: {:?}", self.needs_improvement_dir);
        
        // Process any existing files on startup
        self.process_existing_improvements().await;
        
        // Then continuously watch for new files
        loop {
            self.check_and_process_improvements().await;
            sleep(self.processing_interval).await;
        }
    }
    
    /// Process any existing files needing improvement on startup
    async fn process_existing_improvements(&self) {
        info!("Checking for existing workflows needing improvement...");
        
        match std::fs::read_dir(&self.needs_improvement_dir) {
            Ok(entries) => {
                let mut count = 0;
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                        if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                            if file_name.starts_with("approval_") {
                                count += 1;
                                info!("Found existing workflow needing improvement: {}", file_name);
                                self.process_improvement_file(&path).await;
                            }
                        }
                    }
                }
                
                if count == 0 {
                    info!("No existing workflows needing improvement found");
                } else {
                    info!("Processing {} existing workflows needing improvement", count);
                }
            }
            Err(e) => {
                error!("Failed to read needs_improvement directory: {}", e);
            }
        }
    }
    
    /// Check for and process new files needing improvement
    async fn check_and_process_improvements(&self) {
        match std::fs::read_dir(&self.needs_improvement_dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                        if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                            if file_name.starts_with("approval_") && !file_name.contains(".processing") {
                                debug!("Found workflow needing improvement: {}", file_name);
                                self.process_improvement_file(&path).await;
                            }
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to read needs_improvement directory: {}", e);
            }
        }
    }
    
    /// Process a single file needing improvement
    async fn process_improvement_file(&self, path: &Path) {
        let file_name = path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
            
        info!("Processing workflow needing improvement: {}", file_name);
        
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
                    "Processing improvement for approval {} for task {} (recipient: {})",
                    approval_data.approval_id,
                    approval_data.task_id,
                    approval_data.recipient_name
                );
                
                // Get the latest feedback from letter history
                let feedback = approval_data.letter_history.last()
                    .and_then(|entry| entry.feedback.as_ref())
                    .map(|f| f.text.clone());
                    
                if let Some(feedback_text) = feedback {
                    info!("Using feedback for improvement: {}", feedback_text);
                    
                    // Process the improvement request
                    match self.orchestrator.process_improvement_request(&approval_data, &feedback_text).await {
                        Ok(improved_approval) => {
                            info!(
                                "Successfully generated improved letter for approval {}",
                                approval_data.approval_id
                            );
                            
                            // Move the improved approval to awaiting_response since it was sent to Telegram
                            let awaiting_path = paths::awaiting_response_dir().join(format!(
                                "approval_{}.json",
                                improved_approval.approval_id
                            ));
                            
                            // Write the improved approval data
                            match serde_json::to_string_pretty(&improved_approval) {
                                Ok(json) => {
                                    if let Err(e) = std::fs::write(&awaiting_path, json) {
                                        error!("Failed to write improved approval to pending: {}", e);
                                        self.move_to_failed(&processing_path, file_name);
                                        return;
                                    }
                                    
                                    // Delete the processing file
                                    if let Err(e) = std::fs::remove_file(&processing_path) {
                                        error!("Failed to remove processing file: {}", e);
                                    }
                                    
                                    info!("Moved improved approval {} to awaiting_response after sending to Telegram", 
                                          approval_data.approval_id);
                                }
                                Err(e) => {
                                    error!("Failed to serialize improved approval: {}", e);
                                    self.move_to_failed(&processing_path, file_name);
                                }
                            }
                        }
                        Err(e) => {
                            error!(
                                "Failed to process improvement for approval {}: {}",
                                approval_data.approval_id,
                                e
                            );
                            self.move_to_failed(&processing_path, file_name);
                        }
                    }
                } else {
                    error!("No feedback found in approval data for improvement");
                    self.move_to_failed(&processing_path, file_name);
                }
            }
            Err(e) => {
                error!("Failed to read approval data from {}: {}", file_name, e);
                self.move_to_failed(&processing_path, file_name);
            }
        }
    }
    
    /// Move file to failed directory
    fn move_to_failed(&self, processing_path: &Path, file_name: &str) {
        let failed_dir = paths::failed_dir();
        
        // Ensure failed directory exists
        if let Err(e) = std::fs::create_dir_all(&failed_dir) {
            error!("Failed to create failed directory: {}", e);
        }
        
        let failed_path = failed_dir.join(format!(
            "{}_improvement_failed_{}.json",
            file_name,
            chrono::Utc::now().format("%Y%m%d_%H%M%S")
        ));
        
        if let Err(e) = std::fs::rename(processing_path, failed_path) {
            error!("Failed to move file to failed: {}", e);
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