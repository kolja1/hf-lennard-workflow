//! Configuration management for the Lennard system

use serde::{Deserialize, Serialize};
use crate::error::{LennardError, Result};
use std::path::Path;

/// Raw configuration structure matching credentials.json exactly
#[derive(Debug, Deserialize)]
struct RawConfig {
    pub baserow: BaserowConfig,
    
    // Nango Zoho configuration (required)
    #[serde(rename = "nango_zoho_lennard")]
    pub nango_zoho_lennard: NangoZohoConfig,
    
    pub letterexpress: LetterExpressConfig,
    pub openai: OpenAIConfig,
    pub telegram: TelegramConfig,
    
    #[serde(default = "default_pdf_service")]
    pub pdf_service: PDFServiceConfig,
    
    #[serde(default = "default_dossier")]
    pub dossier: DossierConfig,
}

#[derive(Debug, Deserialize)]
struct NangoZohoConfig {
    pub api_key: String,  // Use api_key field from credentials.json
    pub connection_id: String,
    pub integration_id: String,
    #[serde(default)]
    pub user_id: String,
}


/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LennardConfig {
    pub baserow: BaserowConfig,
    pub zoho: ZohoConfig,
    pub letterexpress: LetterExpressConfig,
    pub openai: OpenAIConfig,
    pub telegram: TelegramConfig,
    pub pdf_service: PDFServiceConfig,
    pub dossier: DossierConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaserowConfig {
    #[serde(alias = "token")]  // Accept both 'api_key' and 'token'
    pub api_key: String,
    
    #[serde(alias = "url")]     // Accept both 'base_url' and 'url'
    pub base_url: String,
    
    pub table_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ZohoConfig {
    // Nango authentication fields (required)
    pub nango_api_key: String,
    pub nango_connection_id: String,
    pub nango_integration_id: String,
    #[serde(default)]
    pub nango_user_id: String,
    
    #[serde(alias = "api_domain", default = "default_zoho_base_url")]
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LetterExpressConfig {
    pub api_key: String,
    pub username: String,
    
    #[serde(alias = "api_url")]  // Accept both 'base_url' and 'api_url'
    pub base_url: String,
    
    #[serde(default = "default_letterexpress_mode")]
    pub mode: String,  // "test" or "live"
}

fn default_letterexpress_mode() -> String {
    "test".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    pub api_key: String,
    pub model: String,
    
    #[serde(default)]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PDFServiceConfig {
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DossierConfig {
    #[serde(default = "default_dossier_grpc_host")]
    pub grpc_host: String,
    
    #[serde(default = "default_dossier_grpc_port")]
    pub grpc_port: u16,
}

// Default functions
fn default_pdf_service() -> PDFServiceConfig {
    PDFServiceConfig {
        base_url: "http://localhost:8000".to_string()
    }
}

fn default_dossier() -> DossierConfig {
    DossierConfig {
        grpc_host: default_dossier_grpc_host(),
        grpc_port: default_dossier_grpc_port(),
    }
}

fn default_dossier_grpc_host() -> String {
    "localhost".to_string()
}

fn default_dossier_grpc_port() -> u16 {
    50052
}

fn default_zoho_base_url() -> String {
    "https://www.zohoapis.com".to_string()
}

impl LennardConfig {
    /// Load configuration from a JSON file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| LennardError::Config(format!("Failed to read config file: {}", e)))?;
        
        Self::from_json_str(&content)
    }
    
    /// Load configuration from a JSON string
    pub fn from_json_str(json: &str) -> Result<Self> {
        let raw_config: RawConfig = serde_json::from_str(json)
            .map_err(|e| LennardError::Config(format!("Failed to parse config: {}", e)))?;
        
        let config = Self::from_raw_config(raw_config);
        config.validate()?;
        Ok(config)
    }
    
    /// Convert raw config to structured config with proper field mapping
    fn from_raw_config(raw: RawConfig) -> Self {
        // Map Nango config to Zoho config
        let nango = raw.nango_zoho_lennard;
        let zoho_config = ZohoConfig {
            nango_api_key: nango.api_key,
            nango_connection_id: nango.connection_id,
            nango_integration_id: nango.integration_id,
            nango_user_id: nango.user_id,
            base_url: default_zoho_base_url(),
        };
        
        Self {
            baserow: raw.baserow,
            zoho: zoho_config,
            letterexpress: raw.letterexpress,
            openai: raw.openai,
            telegram: raw.telegram,
            pdf_service: raw.pdf_service,
            dossier: raw.dossier,
        }
    }
    
    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.baserow.api_key.is_empty() {
            return Err(LennardError::Config("Baserow API key is required".to_string()));
        }
        
        // Zoho validation: Nango credentials are required
        if self.zoho.nango_api_key.is_empty() || self.zoho.nango_connection_id.is_empty() {
            return Err(LennardError::Config(
                "Zoho Nango authentication is required (api_key + connection_id)".to_string()
            ));
        }
        
        if self.openai.api_key.is_empty() {
            return Err(LennardError::Config("OpenAI API key is required".to_string()));
        }
        
        if self.telegram.bot_token.is_empty() {
            return Err(LennardError::Config("Telegram bot token is required".to_string()));
        }
        
        Ok(())
    }
}