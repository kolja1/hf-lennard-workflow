//! gRPC service implementation wrapping the existing workflow orchestrator

use std::sync::Arc;
use tokio::sync::RwLock;
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
use std::collections::HashMap;

/// Wrapper struct for gRPC services
#[derive(Clone)]
pub struct GrpcServiceWrapper {
    orchestrator: Arc<WorkflowOrchestrator<WorkflowProcessor>>,
    // Store workflow states in memory (in production, use a database)
    workflow_states: Arc<RwLock<HashMap<String, ProtoWorkflowState>>>,
    approval_states: Arc<RwLock<HashMap<String, ProtoApprovalState>>>,
}

impl GrpcServiceWrapper {
    pub fn new(orchestrator: Arc<WorkflowOrchestrator<WorkflowProcessor>>) -> Self {
        Self {
            orchestrator,
            workflow_states: Arc::new(RwLock::new(HashMap::new())),
            approval_states: Arc::new(RwLock::new(HashMap::new())),
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
        
        // Convert proto to core type
        let core_trigger = proto_to_core_trigger(proto_trigger);
        
        // Process workflow using existing orchestrator
        let result = self.orchestrator
            .process_workflow(core_trigger)
            .await
            .map_err(|e| Status::internal(format!("Workflow processing failed: {}", e)))?;
        
        // Create workflow state from result
        let workflow_state = ProtoWorkflowState {
            workflow_id: trigger_id.clone(),
            status: if result.processed {
                workflow_grpc::workflow_state::Status::Completed as i32
            } else {
                workflow_grpc::workflow_state::Status::Failed as i32
            },
            started_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
            updated_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
            task_results: vec![], // TODO: Populate from actual results
            error_message: result.result.clone().filter(|_r| !result.processed),
            total_tasks: result.max_tasks,
            completed_tasks: if result.processed { result.max_tasks } else { 0 },
        };
        
        // Store state
        self.workflow_states.write().await.insert(trigger_id, workflow_state.clone());
        
        Ok(Response::new(workflow_state))
    }
    
    async fn get_workflow_state(
        &self,
        request: Request<GetWorkflowStateRequest>,
    ) -> Result<Response<ProtoWorkflowState>, Status> {
        let workflow_id = request.into_inner().workflow_id;
        
        let states = self.workflow_states.read().await;
        states.get(&workflow_id)
            .cloned()
            .ok_or_else(|| Status::not_found(format!("Workflow {} not found", workflow_id)))
            .map(Response::new)
    }
    
    async fn list_workflows(
        &self,
        _request: Request<ListWorkflowsRequest>,
    ) -> Result<Response<ListWorkflowsResponse>, Status> {
        let states = self.workflow_states.read().await;
        let workflows: Vec<ProtoWorkflowState> = states.values().cloned().collect();
        
        Ok(Response::new(ListWorkflowsResponse {
            workflows,
            pagination: None,
        }))
    }
    
    type StreamWorkflowUpdatesStream = Pin<Box<dyn Stream<Item = Result<WorkflowUpdate, Status>> + Send>>;
    
    async fn stream_workflow_updates(
        &self,
        request: Request<StreamWorkflowRequest>,
    ) -> Result<Response<Self::StreamWorkflowUpdatesStream>, Status> {
        let workflow_id = request.into_inner().workflow_id;
        
        // Create a channel for streaming updates
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        
        // Spawn task to send updates (simplified - in production, monitor actual workflow)
        let states = self.workflow_states.clone();
        tokio::spawn(async move {
            // Send initial state
            if let Some(state) = states.read().await.get(&workflow_id) {
                let update = WorkflowUpdate {
                    workflow_id: workflow_id.clone(),
                    status: state.status,
                    latest_task: None,
                    timestamp: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
                    message: "Current state".to_string(),
                };
                let _ = tx.send(Ok(update)).await;
            }
        });
        
        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream)))
    }
    
    async fn cancel_workflow(
        &self,
        request: Request<CancelWorkflowRequest>,
    ) -> Result<Response<ProtoWorkflowState>, Status> {
        let req = request.into_inner();
        let workflow_id = req.workflow_id;
        
        // Update state to cancelled
        let mut states = self.workflow_states.write().await;
        if let Some(state) = states.get_mut(&workflow_id) {
            state.status = workflow_grpc::workflow_state::Status::Failed as i32;
            state.error_message = Some(format!("Cancelled: {}", req.reason));
            state.updated_at = Some(prost_types::Timestamp::from(std::time::SystemTime::now()));
            Ok(Response::new(state.clone()))
        } else {
            Err(Status::not_found(format!("Workflow {} not found", workflow_id)))
        }
    }
    
    async fn get_workflow_metrics(
        &self,
        _request: Request<GetMetricsRequest>,
    ) -> Result<Response<WorkflowMetrics>, Status> {
        let states = self.workflow_states.read().await;
        
        let total = states.len() as u32;
        let completed = states.values()
            .filter(|s| s.status == workflow_grpc::workflow_state::Status::Completed as i32)
            .count() as u32;
        let failed = states.values()
            .filter(|s| s.status == workflow_grpc::workflow_state::Status::Failed as i32)
            .count() as u32;
        let active = total - completed - failed;
        
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
        
        // Create or update approval state
        let mut states = self.approval_states.write().await;
        let state = states.entry(approval_id.clone()).or_insert_with(|| {
            ProtoApprovalState {
                approval_id: approval_id.clone(),
                status: workflow_grpc::approval_state::Status::Pending as i32,
                iterations: vec![],
                final_pdf: None,
                final_pdf_filename: None,
            }
        });
        
        // Update based on decision
        state.status = match approval.decision() {
            workflow_grpc::approval_response::Decision::Approved => 
                workflow_grpc::approval_state::Status::Approved as i32,
            workflow_grpc::approval_response::Decision::Rejected => 
                workflow_grpc::approval_state::Status::Rejected as i32,
            workflow_grpc::approval_response::Decision::NeedsRevision => 
                workflow_grpc::approval_state::Status::InRevision as i32,
            _ => workflow_grpc::approval_state::Status::Pending as i32,
        };
        
        Ok(Response::new(state.clone()))
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
        
        let states = self.approval_states.read().await;
        states.get(&approval_id)
            .cloned()
            .ok_or_else(|| Status::not_found(format!("Approval {} not found", approval_id)))
            .map(Response::new)
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
    addr: std::net::SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let service_wrapper = GrpcServiceWrapper::new(orchestrator);
    
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