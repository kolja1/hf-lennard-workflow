//! Letter generation service gRPC client

// Include the generated proto code
pub mod letter_generation {
    tonic::include_proto!("letter_generation");
}

// Re-export commonly used types
pub use letter_generation::{
    letter_generation_service_client::LetterGenerationServiceClient,
    GenerateLetterRequest,
    GenerateLetterResponse,
    GenerateLetterWithApprovalRequest,
    RecipientInfo,
    DossierContent,
    LetterContent,
    ApprovalData,
    LetterHistoryEntry,
    GenerationMetadata,
    ConversationTurn,
    HealthRequest,
    HealthResponse,
};