//! gRPC client for LinkedIn Dossier Service
//! 
//! This crate provides strongly-typed Rust bindings for the dossier service gRPC API.

// Include the generated code directly
pub mod dossier {
    pub mod v1 {
        include!("generated/dossier.v1.rs");
    }
}

// Re-export all types for convenience
pub use dossier::v1::*;
pub use dossier::v1::dossier_service_client::DossierServiceClient;