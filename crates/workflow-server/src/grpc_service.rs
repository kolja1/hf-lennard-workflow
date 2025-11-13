//! gRPC service implementation wrapping the existing workflow orchestrator

use std::sync::Arc;
use tonic::{Request, Response, Status};
use workflow_grpc::{
    WorkflowService, WorkflowServiceServer,
    ApprovalService, ApprovalServiceServer,
    Health, HealthServer,
    WorkflowTrigger as ProtoWorkflowTrigger,
    WorkflowState as ProtoWorkflowState,
    ApprovalResponse as ProtoApprovalResponse,
    ApprovalState as ProtoApprovalState,
    GetWorkflowStateRequest, ListWorkflowsRequest, ListWorkflowsResponse,
    StreamWorkflowRequest, WorkflowUpdate, CancelWorkflowRequest,
    GetMetricsRequest, WorkflowMetrics,
    GetPendingApprovalsRequest, GetPendingApprovalsResponse,
    GetApprovalStateRequest, StreamApprovalRequest, ApprovalUpdate,
    DownloadPdfRequest, PdfDocument, RegeneratePdfRequest,
    HealthCheckRequest, HealthCheckResponse,
};
use workflow_core::{
    workflow::{WorkflowOrchestrator, approval_types},
    services::WorkflowProcessor,
};
use futures::Stream;
use tokio_stream::wrappers::ReceiverStream;
use std::pin::Pin;

/// Wrapper struct for gRPC services
#[derive(Clone)]
pub struct GrpcServiceWrapper {
    orchestrator: Arc<WorkflowOrchestrator<WorkflowProcessor>>,
    // Approval states are managed by ApprovalQueue (disk-based storage)
    approval_queue: Arc<workflow_core::workflow::ApprovalQueue>,
}

impl GrpcServiceWrapper {
    pub fn new(
        orchestrator: Arc<WorkflowOrchestrator<WorkflowProcessor>>,
        approval_queue: Arc<workflow_core::workflow::ApprovalQueue>,
    ) -> Self {
        Self {
            orchestrator,
            approval_queue,
        }
    }
}

// Convert between proto and core types
fn proto_to_core_trigger(proto: ProtoWorkflowTrigger) -> approval_types::WorkflowTrigger {
    approval_types::WorkflowTrigger {
        trigger_id: proto.trigger_id,
        requested_by: approval_types::UserId::new(proto.requested_by),
        requested_at: chrono::Utc::now(), // Convert from proto timestamp
        max_tasks: proto.max_tasks,
        dry_run: proto.dry_run,
        processed: false,
        processed_at: None,
        result: None,
    }
}

#[tonic::async_trait]
impl WorkflowService for GrpcServiceWrapper {
    async fn trigger_workflow(
        &self,
        request: Request<ProtoWorkflowTrigger>,
    ) -> Result<Response<ProtoWorkflowState>, Status> {
        let proto_trigger = request.into_inner();
        let trigger_id = proto_trigger.trigger_id.clone();
        
        log::info!("Processing workflow trigger {} for up to {} tasks", 
                   trigger_id, proto_trigger.max_tasks);
        
        // Convert proto to core type
        let core_trigger = proto_to_core_trigger(proto_trigger);
        
        // Process tasks directly - no state storage needed
        let result = self.orchestrator
            .process_workflow(core_trigger)
            .await
            .map_err(|e| Status::internal(format!("Task processing failed: {}", e)))?;
        
        // Return a simple ephemeral response - not stored anywhere
        let response = ProtoWorkflowState {
            workflow_id: trigger_id,
            status: if result.processed {
                workflow_grpc::workflow_state::Status::Completed as i32
            } else {
                workflow_grpc::workflow_state::Status::Failed as i32
            },
            started_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
            updated_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
            task_results: vec![], // Individual approvals track their own state
            error_message: result.result.clone().filter(|_r| !result.processed),
            total_tasks: result.max_tasks,
            completed_tasks: if result.processed { result.max_tasks } else { 0 },
        };
        
        log::info!("Trigger {} processed: {} tasks", 
                   response.workflow_id, response.completed_tasks);
        
        // No longer storing state - workflows are ephemeral
        // Each approval tracks its own lifecycle independently
        
        Ok(Response::new(response))
    }
    
    async fn get_workflow_state(
        &self,
        request: Request<GetWorkflowStateRequest>,
    ) -> Result<Response<ProtoWorkflowState>, Status> {
        let workflow_id = request.into_inner().workflow_id;
        
        // Workflows are ephemeral - they only exist during processing
        // Each approval tracks its own state independently
        log::info!("get_workflow_state called for {} - workflows are ephemeral", workflow_id);
        
        Err(Status::not_found(
            "Workflows are ephemeral. Check individual approval states instead."
        ))
    }
    
    async fn list_workflows(
        &self,
        _request: Request<ListWorkflowsRequest>,
    ) -> Result<Response<ListWorkflowsResponse>, Status> {
        // Workflows are ephemeral - return empty list
        log::info!("list_workflows called - returning empty (workflows are ephemeral)");
        
        Ok(Response::new(ListWorkflowsResponse {
            workflows: vec![],
            pagination: None,
        }))
    }
    
    type StreamWorkflowUpdatesStream = Pin<Box<dyn Stream<Item = Result<WorkflowUpdate, Status>> + Send>>;
    
    async fn stream_workflow_updates(
        &self,
        request: Request<StreamWorkflowRequest>,
    ) -> Result<Response<Self::StreamWorkflowUpdatesStream>, Status> {
        let workflow_id = request.into_inner().workflow_id;
        
        log::info!("stream_workflow_updates called for {} - not supported", workflow_id);
        
        // Workflows are ephemeral - no updates to stream
        // Return an empty stream that immediately closes
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream)))
    }
    
    async fn cancel_workflow(
        &self,
        request: Request<CancelWorkflowRequest>,
    ) -> Result<Response<ProtoWorkflowState>, Status> {
        let req = request.into_inner();
        let workflow_id = req.workflow_id;
        
        log::info!("cancel_workflow called for {} - not supported", workflow_id);
        
        // Workflows are ephemeral - nothing to cancel
        Err(Status::not_found(
            "Cannot cancel workflow. Workflows are ephemeral and run to completion."
        ))
    }
    
    async fn get_workflow_metrics(
        &self,
        _request: Request<GetMetricsRequest>,
    ) -> Result<Response<WorkflowMetrics>, Status> {
        // Calculate metrics from approval files instead
        // This could scan the approval directories to get real metrics
        log::info!("get_workflow_metrics called - returning zeros (workflows are ephemeral)");
        
        let total = 0;
        let completed = 0;
        let failed = 0;
        let active = 0;
        
        Ok(Response::new(WorkflowMetrics {
            total_workflows: total,
            completed_workflows: completed,
            failed_workflows: failed,
            active_workflows: active,
            average_processing_time_seconds: 30.0, // Placeholder
            total_tasks_processed: completed * 5, // Estimate
        }))
    }
}

#[tonic::async_trait]
impl ApprovalService for GrpcServiceWrapper {
    async fn submit_approval(
        &self,
        request: Request<ProtoApprovalResponse>,
    ) -> Result<Response<ProtoApprovalState>, Status> {
        let approval = request.into_inner();
        let approval_id = approval.approval_id.clone();
        
        log::info!("=== SUBMIT APPROVAL CALLED ===");
        log::info!("Received approval response for ID: {} with decision: {:?}", 
            approval_id, approval.decision());
        if let Some(ref feedback) = approval.feedback {
            log::info!("Feedback provided: {}", feedback);
        }
        
        // Process the approval through the approval queue
        let result = match approval.decision() {
            workflow_grpc::approval_response::Decision::Approved => {
                // Convert string ID to ApprovalId type
                let approval_id_typed = workflow_core::workflow::approval_types::ApprovalId::from(approval_id.clone());
                
                // Process approval through the queue - this will move the file to approved directory
                // The ApprovalWatcher will then pick it up and process it
                match self.approval_queue.handle_user_approval(&approval_id_typed) {
                    Ok(Some(approval_data)) => {
                        log::info!("Successfully moved approval {} to approved directory for task: {:?}", 
                            approval_id, approval_data.task_id);
                        log::info!("ApprovalWatcher will process this approval shortly");
                        
                        // Note: We do NOT trigger continue_after_approval here!
                        // The ApprovalWatcher will handle it when it detects the file in the approved directory
                        
                        ProtoApprovalState {
                            approval_id: approval_id.clone(),
                            status: workflow_grpc::approval_state::Status::Approved as i32,
                            iterations: vec![],
                            final_pdf: None,
                            final_pdf_filename: None,
                        }
                    }
                    Ok(None) => {
                        log::warn!("Approval {} not found or not in awaiting state", approval_id);
                        return Err(Status::not_found(format!("Approval {} not found or not awaiting response", approval_id)));
                    }
                    Err(e) => {
                        log::error!("Failed to process approval {}: {}", approval_id, e);
                        return Err(Status::internal(format!("Failed to process approval: {}", e)));
                    }
                }
            }
            workflow_grpc::approval_response::Decision::Rejected => {
                // Handle rejection - move to failed directory and update Zoho
                let approval_id_typed = workflow_core::workflow::approval_types::ApprovalId::from(approval_id.clone());
                let feedback = approval.feedback.unwrap_or_else(|| "No feedback provided".to_string());
                let user_id = workflow_core::workflow::approval_types::UserId::new(approval.decided_by);

                log::info!("Processing rejection for approval ID: {} with reason: {}", approval_id, feedback);

                // Mark as rejected (moves to failed directory)
                match self.approval_queue.mark_as_rejected(&approval_id_typed, feedback.clone(), user_id) {
                    Ok(Some(approval_data)) => {
                        log::info!("Marked approval {} as rejected, moved to failed directory", approval_id);

                        // Update Zoho task and send Telegram notification
                        let task_id = approval_data.task_id.as_str();
                        let recipient_name = approval_data.recipient_name.as_str();

                        if let Err(e) = self.orchestrator.handle_rejection(task_id, recipient_name, &feedback).await {
                            log::error!("Failed to handle rejection for task {}: {}", task_id, e);
                            // Continue anyway - file is already moved to failed
                        }

                        ProtoApprovalState {
                            approval_id: approval_id.clone(),
                            status: workflow_grpc::approval_state::Status::Rejected as i32,
                            iterations: vec![],
                            final_pdf: None,
                            final_pdf_filename: None,
                        }
                    }
                    Ok(None) => {
                        log::warn!("Approval {} not found or not in awaiting state", approval_id);
                        return Err(Status::not_found(format!("Approval {} not found or not awaiting response", approval_id)));
                    }
                    Err(e) => {
                        log::error!("Failed to mark approval {} as rejected: {}", approval_id, e);
                        return Err(Status::internal(format!("Failed to process rejection: {}", e)));
                    }
                }
            }
            workflow_grpc::approval_response::Decision::NeedsRevision => {
                // Handle revision request - move to needs_improvement for automatic retry
                let approval_id_typed = workflow_core::workflow::approval_types::ApprovalId::from(approval_id.clone());
                let feedback = approval.feedback.unwrap_or_else(|| "No feedback provided".to_string());
                let user_id = workflow_core::workflow::approval_types::UserId::new(approval.decided_by);

                log::info!("Processing revision request for approval ID: {} with feedback: {}", approval_id, feedback);

                // Process revision through the queue (will be picked up by NeedsImprovementWatcher)
                match self.approval_queue.handle_user_feedback(&approval_id_typed, feedback, user_id) {
                    Ok(Some(_approval_data)) => {
                        log::info!("Successfully processed revision request for approval: {}", approval_id);

                        ProtoApprovalState {
                            approval_id: approval_id.clone(),
                            status: workflow_grpc::approval_state::Status::InRevision as i32,
                            iterations: vec![],
                            final_pdf: None,
                            final_pdf_filename: None,
                        }
                    }
                    Ok(None) => {
                        log::warn!("Approval {} not found or not in awaiting state", approval_id);
                        return Err(Status::not_found(format!("Approval {} not found or not awaiting response", approval_id)));
                    }
                    Err(e) => {
                        log::error!("Failed to process revision request for approval {}: {}", approval_id, e);
                        return Err(Status::internal(format!("Failed to process revision: {}", e)));
                    }
                }
            }
            _ => {
                return Err(Status::invalid_argument("Unknown approval decision"));
            }
        };
        
        Ok(Response::new(result))
    }
    
    async fn get_pending_approvals(
        &self,
        _request: Request<GetPendingApprovalsRequest>,
    ) -> Result<Response<GetPendingApprovalsResponse>, Status> {
        // Return empty list for now - in production, query actual pending approvals
        Ok(Response::new(GetPendingApprovalsResponse {
            approvals: vec![],
            total_count: 0,
        }))
    }
    
    async fn get_approval_state(
        &self,
        request: Request<GetApprovalStateRequest>,
    ) -> Result<Response<ProtoApprovalState>, Status> {
        let approval_id = request.into_inner().approval_id;
        
        log::info!("=== GET APPROVAL STATE CALLED for ID: {} ===", approval_id);
        
        // First, try to get the approval from the ApprovalQueue (disk storage)
        let approval_id_typed = workflow_core::workflow::approval_types::ApprovalId::from(approval_id.clone());
        
        match self.approval_queue.get_approval_request(&approval_id_typed, None) {
            Ok(Some(approval_data)) => {
                log::info!("Found approval {} with state: {:?}", approval_id, approval_data.state);
                
                // Convert ApprovalData to ProtoApprovalState
                let proto_state = ProtoApprovalState {
                    approval_id: approval_id.clone(),
                    status: match approval_data.state {
                        workflow_core::workflow::approval_types::ApprovalState::PendingApproval => 
                            workflow_grpc::approval_state::Status::Pending as i32,
                        workflow_core::workflow::approval_types::ApprovalState::AwaitingUserResponse => 
                            workflow_grpc::approval_state::Status::Pending as i32,  // Map to Pending since there's no AwaitingResponse
                        workflow_core::workflow::approval_types::ApprovalState::Approved => 
                            workflow_grpc::approval_state::Status::Approved as i32,
                        workflow_core::workflow::approval_types::ApprovalState::NeedsImprovement => 
                            workflow_grpc::approval_state::Status::InRevision as i32,
                        workflow_core::workflow::approval_types::ApprovalState::Failed => 
                            workflow_grpc::approval_state::Status::Rejected as i32,  // Map Failed to Rejected
                    },
                    iterations: vec![],  // TODO: Convert letter history
                    final_pdf: None,     // TODO: Add PDF support
                    final_pdf_filename: None,
                };
                
                Ok(Response::new(proto_state))
            }
            Ok(None) => {
                log::warn!("Approval {} not found in ApprovalQueue", approval_id);
                Err(Status::not_found(format!("Approval {} not found", approval_id)))
            }
            Err(e) => {
                log::error!("Error reading approval {}: {}", approval_id, e);
                Err(Status::internal(format!("Failed to read approval: {}", e)))
            }
        }
    }
    
    type StreamApprovalUpdatesStream = Pin<Box<dyn Stream<Item = Result<ApprovalUpdate, Status>> + Send>>;
    
    async fn stream_approval_updates(
        &self,
        _request: Request<StreamApprovalRequest>,
    ) -> Result<Response<Self::StreamApprovalUpdatesStream>, Status> {
        // Create empty stream for now
        let (_, rx) = tokio::sync::mpsc::channel(10);
        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream)))
    }
    
    async fn download_approval_pdf(
        &self,
        _request: Request<DownloadPdfRequest>,
    ) -> Result<Response<PdfDocument>, Status> {
        // Return placeholder PDF for now
        Ok(Response::new(PdfDocument {
            content: vec![],
            filename: "approval.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            size_bytes: 0,
            generated_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
        }))
    }
    
    async fn regenerate_pdf(
        &self,
        _request: Request<RegeneratePdfRequest>,
    ) -> Result<Response<PdfDocument>, Status> {
        // Return placeholder PDF for now
        Ok(Response::new(PdfDocument {
            content: vec![],
            filename: "regenerated.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            size_bytes: 0,
            generated_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
        }))
    }
}

#[tonic::async_trait]
impl Health for GrpcServiceWrapper {
    async fn check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse {
            status: workflow_grpc::health_check_response::ServingStatus::Serving as i32,
        }))
    }
    
    type WatchStream = Pin<Box<dyn Stream<Item = Result<HealthCheckResponse, Status>> + Send>>;
    
    async fn watch(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        // Create a channel for health updates
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        
        // Send periodic health updates
        tokio::spawn(async move {
            loop {
                let _ = tx.send(Ok(HealthCheckResponse {
                    status: workflow_grpc::health_check_response::ServingStatus::Serving as i32,
                })).await;
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        });
        
        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream)))
    }
}

/// Start the gRPC server
pub async fn start_grpc_server(
    orchestrator: Arc<WorkflowOrchestrator<WorkflowProcessor>>,
    approval_queue: Arc<workflow_core::workflow::ApprovalQueue>,
    addr: std::net::SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let service_wrapper = GrpcServiceWrapper::new(orchestrator, approval_queue);
    
    let workflow_service = WorkflowServiceServer::new(service_wrapper.clone());
    let approval_service = ApprovalServiceServer::new(service_wrapper.clone());
    let health_service = HealthServer::new(service_wrapper);
    
    log::info!("Starting gRPC server on {}", addr);
    
    // Attempt to bind and serve - this will fail immediately if port is unavailable
    match tonic::transport::Server::builder()
        .add_service(workflow_service)
        .add_service(approval_service)
        .add_service(health_service)
        .serve(addr)
        .await
    {
        Ok(_) => {
            log::info!("gRPC server stopped normally");
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to start gRPC server on {}: {}", addr, e);
            Err(Box::new(e))
        }
    }
}