//! Service modules for business logic

pub mod address_extractor;
pub mod letter_generator;
pub mod workflow_processor;

// Re-export service types
pub use address_extractor::AddressExtractor;
pub use letter_generator::LetterGenerator;
pub use workflow_processor::WorkflowProcessor;