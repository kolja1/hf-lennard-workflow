//! Generated gRPC code for workflow services

// Re-export all generated code
pub mod lennard {
    pub mod workflow {
        pub mod v1 {
            // Include the generated proto code
            tonic::include_proto!("lennard.workflow.v1");
        }
    }
}

// Convenience re-exports
pub use lennard::workflow::v1::*;

// Re-export service traits
pub use lennard::workflow::v1::workflow_service_server::{WorkflowService, WorkflowServiceServer};
pub use lennard::workflow::v1::approval_service_server::{ApprovalService, ApprovalServiceServer};
pub use lennard::workflow::v1::health_server::{Health, HealthServer};

// Re-export client types
pub use lennard::workflow::v1::workflow_service_client::WorkflowServiceClient;
pub use lennard::workflow::v1::approval_service_client::ApprovalServiceClient;
pub use lennard::workflow::v1::health_client::HealthClient;