//! Error types for the Lennard system

use thiserror::Error;

/// Main error type for all Lennard operations
#[derive(Error, Debug)]
pub enum LennardError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    
    #[error("JSON parsing failed: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Authentication failed: {0}")]
    Auth(String),
    
    #[error("Validation failed: {0}")]
    Validation(String),
    
    #[error("File system error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("IO error: {0}")]
    IoError(String),
    
    #[error("Workflow error: {0}")]
    Workflow(String),
    
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Processing error: {0}")]
    Processing(String),
    
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("Deserialization error: {0}")]
    Deserialization(String),
}

/// Result type for Lennard operations
pub type Result<T> = std::result::Result<T, LennardError>;