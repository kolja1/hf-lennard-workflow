//! Letter generation service client using gRPC

use crate::config::LetterServiceConfig;
use crate::error::{LennardError, Result};
use crate::types::{LetterContent, LinkedInProfile, ZohoContact};
use crate::clients::dossier::DossierResult;
use letter_grpc_client::{
    LetterGenerationServiceClient,
    GenerateLetterRequest,
    RecipientInfo,
    CompanyInfo,
    DossierContent,
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
        
        // Build company info from dossier result
        let company_info = CompanyInfo {
            account_name: dossier_result.company_name.clone(),
            industry: String::new(), // Could be extracted from dossier if available
            website: String::new(),  // Could be extracted from profile data
        };
        
        // Build dossier content
        let dossier_content = DossierContent {
            person_dossier: String::new(), // We could add person dossier if available
            company_dossier: dossier_result.company_dossier_content.clone(),
        };
        
        // Create the request
        let request = GenerateLetterRequest {
            recipient_info: Some(recipient_info),
            company_info: Some(company_info),
            our_company_info: "HEIN+FRICKE GmbH & Co.KG - Führender IT-Dienstleister".to_string(),
            feedback_history: vec![],
            letter_type: "sales_introduction".to_string(),
            dossier_content: Some(dossier_content),
        };
        
        // Send the gRPC request
        log::info!("Sending letter generation request for {}", contact.full_name);
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
}