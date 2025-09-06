//! Common types used throughout the Lennard system

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// LinkedIn profile data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedInProfile {
    pub profile_id: String,
    pub profile_url: String,
    pub full_name: String,
    pub headline: Option<String>,
    pub location: Option<String>,
    pub company: Option<String>,
    pub raw_data: HashMap<String, serde_json::Value>,
}

/// Contact information from Zoho CRM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZohoContact {
    pub id: String,
    pub full_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub company: Option<String>,
    pub linkedin_id: Option<String>,
    pub mailing_address: Option<MailingAddress>,
}

/// Mailing address structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailingAddress {
    pub street: String,
    pub city: String,
    pub state: Option<String>,
    pub postal_code: String,
    pub country: String,
}

// ZohoTask removed - using generated TasksResponse from zoho-generated-types instead

/// Generated letter content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LetterContent {
    pub subject: String,
    pub greeting: String,
    pub body: String,
    pub sender_name: String,
    pub recipient_name: String,
    pub company_name: String,
}

/// PDF generation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PDFRequest {
    pub template_path: String,
    pub data: HashMap<String, serde_json::Value>,
}

/// PDF bookmark names as constants
pub struct PDFBookmarks;

impl PDFBookmarks {
    // German bookmark names from the ODT template
    pub const BETREFF: &'static str = "Betreff";
    pub const ANREDE: &'static str = "Anrede";
    pub const BRIEFTEXT: &'static str = "Brieftext";
    pub const SENDER_NAME: &'static str = "Sender-Name";
    pub const RECIPIENT: &'static str = "Reciepient";
    pub const STREET_1: &'static str = "Street-1";
    pub const STREET_2: &'static str = "Street-2";
    pub const CITY: &'static str = "City";
    pub const PLZ: &'static str = "PLZ";
    pub const COUNTRY: &'static str = "Country";
}

/// Strongly typed PDF template data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PDFTemplateData {
    #[serde(rename = "Betreff")]
    pub betreff: String,
    
    #[serde(rename = "Anrede")]
    pub anrede: String,
    
    #[serde(rename = "Brieftext")]
    pub brieftext: String,
    
    #[serde(rename = "Sender-Name")]
    pub sender_name: String,
    
    #[serde(rename = "Company")]  // NEW field for company name
    pub company: String,
    
    #[serde(rename = "Recipient")]  // Fixed spelling to match bookmark
    pub recipient: String,
    
    #[serde(rename = "Street 1")]  // Changed to match bookmark with space
    pub street_1: String,
    
    #[serde(rename = "Street-2")]
    pub street_2: Option<String>,
    
    #[serde(rename = "City")]
    pub city: String,
    
    #[serde(rename = "ZipCode")]  // Changed from PLZ to match bookmark
    pub plz: String,
    
    #[serde(rename = "Country")]
    pub country: String,
}

impl PDFTemplateData {
    /// Create from letter content and mailing address
    pub fn from_letter_and_address(letter: &LetterContent, address: &MailingAddress) -> Self {
        Self {
            betreff: letter.subject.clone(),
            anrede: letter.greeting.clone(),
            brieftext: letter.body.clone(),
            sender_name: letter.sender_name.clone(),
            company: letter.company_name.clone(),  // Add company name from letter
            recipient: letter.recipient_name.clone(),
            street_1: address.street.clone(),
            street_2: address.state.clone(),
            city: address.city.clone(),
            plz: address.postal_code.clone(),
            country: address.country.clone(),
        }
    }
}

/// Letter sending request for LetterExpress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LetterExpressRequest {
    pub pdf_data: Vec<u8>,
    pub recipient_address: MailingAddress,
    pub sender_address: MailingAddress,
    pub color: PrintColor,
    pub mode: PrintMode,
    pub shipping: ShippingType,
}

/// Print color options
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PrintColor {
    BlackWhite,
    Color,
}

/// Print mode options  
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PrintMode {
    Simplex,
    Duplex,
}

/// Shipping type options
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ShippingType {
    Standard,
    Express,
    Registered,
}


mod tests {

    #[test]
    fn test_pdf_bookmarks_constants() {
        use super::PDFBookmarks;
        
        // Verify all bookmark constants have the correct German names
        assert_eq!(PDFBookmarks::BETREFF, "Betreff");
        assert_eq!(PDFBookmarks::ANREDE, "Anrede");
        assert_eq!(PDFBookmarks::BRIEFTEXT, "Brieftext");
        assert_eq!(PDFBookmarks::SENDER_NAME, "Sender-Name");
        assert_eq!(PDFBookmarks::RECIPIENT, "Reciepient"); // Note: typo is in template
        assert_eq!(PDFBookmarks::STREET_1, "Street-1");
        assert_eq!(PDFBookmarks::STREET_2, "Street-2");
        assert_eq!(PDFBookmarks::CITY, "City");
        assert_eq!(PDFBookmarks::PLZ, "PLZ");
        assert_eq!(PDFBookmarks::COUNTRY, "Country");
    }

    #[test]
    fn test_pdf_template_data_from_letter_and_address() {
        use super::{PDFTemplateData, LetterContent, MailingAddress};
        
        let letter = LetterContent {
            subject: "Test Subject".to_string(),
            greeting: "Dear Test".to_string(),
            body: "Test body content".to_string(),
            sender_name: "Sender Name".to_string(),
            recipient_name: "Recipient Name".to_string(),
            company_name: "Test Company".to_string(),
        };

        let address = MailingAddress {
            street: "Test Street 123".to_string(),
            city: "Test City".to_string(),
            state: Some("Test State".to_string()),
            postal_code: "12345".to_string(),
            country: "Test Country".to_string(),
        };

        let pdf_data = PDFTemplateData::from_letter_and_address(&letter, &address);

        assert_eq!(pdf_data.betreff, "Test Subject");
        assert_eq!(pdf_data.anrede, "Dear Test");
        assert_eq!(pdf_data.brieftext, "Test body content");
        assert_eq!(pdf_data.sender_name, "Sender Name");
        assert_eq!(pdf_data.recipient, "Recipient Name");
        assert_eq!(pdf_data.street_1, "Test Street 123");
        assert_eq!(pdf_data.street_2, Some("Test State".to_string()));
        assert_eq!(pdf_data.city, "Test City");
        assert_eq!(pdf_data.plz, "12345");
        assert_eq!(pdf_data.country, "Test Country");
    }

    #[test]
    fn test_pdf_template_data_serialization() {
        use super::PDFTemplateData;
        
        let pdf_data = PDFTemplateData {
            betreff: "Subject".to_string(),
            anrede: "Greeting".to_string(),
            brieftext: "Body".to_string(),
            sender_name: "Sender".to_string(),
            company: "Company".to_string(),
            recipient: "Recipient".to_string(),
            street_1: "Street".to_string(),
            street_2: None,
            city: "City".to_string(),
            plz: "PLZ".to_string(),
            country: "Country".to_string(),
        };

        // Serialize to JSON
        let json = serde_json::to_string(&pdf_data).unwrap();
        
        // Verify German bookmark names are used in JSON
        assert!(json.contains("\"Betreff\":\"Subject\""));
        assert!(json.contains("\"Anrede\":\"Greeting\""));
        assert!(json.contains("\"Brieftext\":\"Body\""));
        assert!(json.contains("\"Sender-Name\":\"Sender\""));
        assert!(json.contains("\"Company\":\"Company\""));
        assert!(json.contains("\"Recipient\":\"Recipient\"")); // Fixed spelling
        assert!(json.contains("\"Street 1\":\"Street\""));     // With space
        assert!(json.contains("\"City\":\"City\""));
        assert!(json.contains("\"ZipCode\":\"PLZ\""));         // Changed to ZipCode
        assert!(json.contains("\"Country\":\"Country\""));
        
        // Street-2 should be null when None
        assert!(json.contains("\"Street-2\":null"));
    }

    #[test]
    fn test_pdf_template_data_deserialization() {
        use super::PDFTemplateData;
        
        let json = r#"{
            "Betreff": "Test Subject",
            "Anrede": "Test Greeting",
            "Brieftext": "Test Body",
            "Sender-Name": "Test Sender",
            "Reciepient": "Test Recipient",
            "Street-1": "Test Street",
            "Street-2": "Test State",
            "City": "Test City",
            "PLZ": "12345",
            "Country": "Germany"
        }"#;

        let pdf_data: PDFTemplateData = serde_json::from_str(json).unwrap();
        
        assert_eq!(pdf_data.betreff, "Test Subject");
        assert_eq!(pdf_data.anrede, "Test Greeting");
        assert_eq!(pdf_data.brieftext, "Test Body");
        assert_eq!(pdf_data.sender_name, "Test Sender");
        assert_eq!(pdf_data.recipient, "Test Recipient");
        assert_eq!(pdf_data.street_1, "Test Street");
        assert_eq!(pdf_data.street_2, Some("Test State".to_string()));
        assert_eq!(pdf_data.city, "Test City");
        assert_eq!(pdf_data.plz, "12345");
        assert_eq!(pdf_data.country, "Germany");
    }

    #[test]
    fn test_pdf_template_data_with_empty_state() {
        use super::{PDFTemplateData, LetterContent, MailingAddress};
        
        let letter = LetterContent {
            subject: "Subject".to_string(),
            greeting: "Greeting".to_string(),
            body: "Body".to_string(),
            sender_name: "Sender".to_string(),
            recipient_name: "Recipient".to_string(),
            company_name: "Company".to_string(),
        };

        let address = MailingAddress {
            street: "Street".to_string(),
            city: "City".to_string(),
            state: None, // No state
            postal_code: "12345".to_string(),
            country: "Country".to_string(),
        };

        let pdf_data = PDFTemplateData::from_letter_and_address(&letter, &address);
        
        // Street-2 should be None when state is None
        assert_eq!(pdf_data.street_2, None);
    }
}