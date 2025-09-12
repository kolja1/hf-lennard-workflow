//! Letter generation service client using gRPC

use crate::config::LetterServiceConfig;
use crate::error::{LennardError, Result};
use crate::types::{LetterContent, LinkedInProfile, ZohoContact};
use crate::clients::dossier::DossierResult;
use letter_grpc_client::{
    LetterGenerationServiceClient,
    GenerateLetterRequest,
    GenerateLetterWithApprovalRequest,
    RecipientInfo,
    DossierContent,
    ApprovalData,
    LetterHistoryEntry,
};
use tonic::transport::Channel;

pub struct LetterServiceClient {
    grpc_url: String,
}

impl LetterServiceClient {
    pub fn new(config: LetterServiceConfig) -> Self {
        let grpc_url = format!("http://{}:{}", config.grpc_host, config.grpc_port);
        log::info!("LetterServiceClient configured for gRPC endpoint: {}", grpc_url);
        
        Self {
            grpc_url,
        }
    }
    
    /// Generate personalized letter content using gRPC service
    pub async fn generate_letter(
        &self,
        contact: &ZohoContact,
        profile: &LinkedInProfile,
        dossier_result: &DossierResult,
    ) -> Result<LetterContent> {
        // Create gRPC connection on-demand
        let channel = Channel::from_shared(self.grpc_url.clone())
            .map_err(|e| LennardError::Config(format!("Invalid gRPC URL: {}", e)))?
            .connect()
            .await
            .map_err(|e| LennardError::ServiceUnavailable(
                format!("Failed to connect to letter service at {}: {}", self.grpc_url, e)
            ))?;
            
        log::info!("Connected to letter generation service at {}", self.grpc_url);
        let mut grpc_client = LetterGenerationServiceClient::new(channel);
        
        // Build recipient info from contact
        let recipient_info = RecipientInfo {
            first_name: contact.full_name.split_whitespace().next().unwrap_or("").to_string(),
            last_name: contact.full_name.split_whitespace().skip(1).collect::<Vec<_>>().join(" "),
            full_name: contact.full_name.clone(),
            email: contact.email.clone().unwrap_or_default(),
            title: profile.headline.clone().unwrap_or_default(),
            account_name: contact.company.clone().unwrap_or_default(),
            mailing_street: contact.mailing_address.as_ref()
                .map(|a| a.street.clone())
                .unwrap_or_default(),
            mailing_city: contact.mailing_address.as_ref()
                .map(|a| a.city.clone())
                .unwrap_or_default(),
            mailing_zip: contact.mailing_address.as_ref()
                .map(|a| a.postal_code.clone())
                .unwrap_or_default(),
            mailing_country: contact.mailing_address.as_ref()
                .map(|a| a.country.clone())
                .unwrap_or_default(),
        };
        
        // Build dossier content with BOTH personal and company dossiers
        let dossier_content = DossierContent {
            person_dossier: dossier_result.person_dossier_content.clone(),
            company_dossier: dossier_result.company_dossier_content.clone(),
        };
        
        // Create the request
        let request = GenerateLetterRequest {
            recipient_info: Some(recipient_info),
            our_company_info: "HEIN+FRICKE GmbH & Co.KG - Führender IT-Dienstleister".to_string(),
            feedback_history: vec![],
            letter_type: "sales_introduction".to_string(),
            dossier_content: Some(dossier_content),
        };
        
        // Send the gRPC request
        log::info!("Sending letter generation request for {}", contact.full_name);
        log::info!("Letter generation details - Name: {}, Company: {}", 
                  contact.full_name, 
                  dossier_result.company_name);
        log::info!("Dossiers included - Person: {} chars, Company: {} chars",
                  dossier_result.person_dossier_content.len(),
                  dossier_result.company_dossier_content.len());
        let response = grpc_client
            .generate_letter(request)
            .await
            .map_err(|e| LennardError::ServiceUnavailable(
                format!("Letter generation gRPC call failed: {}", e)
            ))?;
            
        let response_inner = response.into_inner();
        
        if !response_inner.success {
            return Err(LennardError::Processing(
                format!("Letter generation failed: {}", response_inner.error_message)
            ));
        }
        
        let letter_grpc = response_inner.letter
            .ok_or_else(|| LennardError::Processing("No letter content in response".to_string()))?;
            
        // Convert gRPC letter content to our internal type
        Ok(LetterContent {
            subject: letter_grpc.betreff,
            greeting: letter_grpc.anrede,
            body: letter_grpc.brieftext,
            sender_name: letter_grpc.sender_name,
            recipient_name: contact.full_name.clone(),
            company_name: dossier_result.company_name.clone(),
        })
    }
    
    /// Generate improved letter based on feedback with full approval context
    pub async fn generate_improved_letter_with_approval(
        &self,
        approval_data: &crate::workflow::approval_types::ApprovalData,
        feedback: &str
    ) -> Result<LetterContent> {
        // Validate that we have required mailing address
        let mailing_address = approval_data.mailing_address.as_ref()
            .ok_or_else(|| LennardError::Workflow(
                format!("Missing mailing address for approval {}. Cannot generate letter without recipient address.", 
                        approval_data.approval_id)
            ))?;
        // Create gRPC connection on-demand
        let channel = Channel::from_shared(self.grpc_url.clone())
            .map_err(|e| LennardError::Config(format!("Invalid gRPC URL: {}", e)))?
            .connect()
            .await
            .map_err(|e| LennardError::ServiceUnavailable(
                format!("Failed to connect to letter service at {}: {}", self.grpc_url, e)
            ))?;
            
        log::info!("Connected to letter generation service for improvement request");
        let mut grpc_client = LetterGenerationServiceClient::new(channel);
        
        // Build letter history from approval data
        let letter_history: Vec<LetterHistoryEntry> = approval_data.letter_history.iter().map(|entry| {
            LetterHistoryEntry {
                iteration: entry.iteration as i32,
                content: format!("{}\n{}\n{}", entry.content.subject, entry.content.greeting, entry.content.body),
                timestamp: entry.created_at.to_rfc3339(),
                generated_by: "AI".to_string(),
                feedback: entry.feedback.as_ref().map(|f| f.text.clone()).unwrap_or_default(),
            }
        }).collect();
        
        // Build approval data for the gRPC request
        let grpc_approval_data = ApprovalData {
            approval_id: approval_data.approval_id.to_string(),
            state: "needs_improvement".to_string(),
            created_at: approval_data.requested_at.to_rfc3339(),
            updated_at: approval_data.updated_at.to_rfc3339(),
            letter_content: format!("{}\n{}\n{}", 
                approval_data.current_letter.subject, 
                approval_data.current_letter.greeting, 
                approval_data.current_letter.body),
            contact_name: approval_data.recipient_name.clone(),
            company_name: approval_data.company_name.clone(),
            letter_history,
            current_iteration: approval_data.letter_history.len() as i32 + 1,
            zoho_task_id: approval_data.task_id.to_string(),
            feedback_text: feedback.to_string(),
            regeneration_requested: true,
        };
        
        // Build request using the approval-aware endpoint with full context
        let request = tonic::Request::new(GenerateLetterWithApprovalRequest {
            approval_data: Some(grpc_approval_data),
            recipient_info: Some(RecipientInfo {
                first_name: "".to_string(), // Not needed when full_name is provided
                last_name: "".to_string(),  // Not needed when full_name is provided
                full_name: approval_data.recipient_name.clone(),
                email: approval_data.recipient_email.clone().unwrap_or_default(),
                title: approval_data.recipient_title.clone().unwrap_or_default(),
                account_name: approval_data.company_name.clone(),
                mailing_street: mailing_address.street.clone(),
                mailing_city: mailing_address.city.clone(),
                mailing_zip: mailing_address.postal_code.clone(),
                mailing_country: mailing_address.country.clone(),
            }),
            our_company_info: "HEIN+FRICKE GmbH & Co.KG - Führender IT-Dienstleister".to_string(),
            letter_type: "improvement".to_string(),
            dossier_content: Some(DossierContent {
                person_dossier: approval_data.person_dossier.clone().unwrap_or_default(),
                company_dossier: approval_data.company_dossier.clone().unwrap_or_default(),
            })
        });
        
        log::info!("Sending improvement request to gRPC service with feedback and full context");
        log::info!("Improvement request details - Name: {}, Email: {:?}, Title: {:?}, Company: {}", 
                  approval_data.recipient_name, 
                  approval_data.recipient_email,
                  approval_data.recipient_title,
                  approval_data.company_name);
        log::info!("Dossiers included - Person: {} chars, Company: {} chars",
                  approval_data.person_dossier.as_ref().map(|d| d.len()).unwrap_or(0),
                  approval_data.company_dossier.as_ref().map(|d| d.len()).unwrap_or(0));
        
        // Call the gRPC service
        let response = grpc_client
            .generate_letter_with_approval(request)
            .await
            .map_err(|e| LennardError::ServiceUnavailable(
                format!("Letter improvement service failed: {}", e)
            ))?;
            
        let response_inner = response.into_inner();
        
        if !response_inner.success {
            return Err(LennardError::Processing(
                format!("Letter generation failed: {}", response_inner.error_message)
            ));
        }
        
        let letter_grpc = response_inner.letter
            .ok_or_else(|| LennardError::Workflow("No improved letter returned from service".to_string()))?;
            
        log::info!("Successfully received improved letter from gRPC service");
        
        Ok(LetterContent {
            subject: letter_grpc.betreff,
            greeting: letter_grpc.anrede,
            body: letter_grpc.brieftext,
            sender_name: letter_grpc.sender_name,
            recipient_name: approval_data.recipient_name.clone(),
            company_name: approval_data.company_name.clone(),
        })
    }
}