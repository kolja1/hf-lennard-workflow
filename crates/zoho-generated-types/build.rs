use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../contracts/zoho-tasks-dynamic.json");
    
    let spec_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()  // crates
        .parent().unwrap()  // rust-workflow
        .join("contracts")
        .join("zoho-tasks-dynamic.json");
    
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_file = PathBuf::from(&out_dir).join("zoho_tasks_client.rs");
    
    // Check if the spec file exists
    if !spec_path.exists() {
        panic!("OpenAPI spec not found at {:?}. Please ensure contracts/zoho-tasks-dynamic.json exists", spec_path);
    }
    
    eprintln!("[INFO] Generating client from {:?}", spec_path);
    
    // Generate the client using Progenitor
    let spec_str = std::fs::read_to_string(&spec_path).expect("Failed to read spec file");
    let spec: openapiv3::OpenAPI = serde_json::from_str(&spec_str)
        .expect("Failed to parse OpenAPI spec");
    
    match progenitor::Generator::default()
        .generate_tokens(&spec) 
    {
        Ok(tokens) => {
            // Format the generated code
            let ast = syn::parse2(tokens).expect("Failed to parse generated tokens");
            let formatted = prettyplease::unparse(&ast);
            std::fs::write(&out_file, formatted).expect("Failed to write generated client");
            eprintln!("[INFO] Successfully generated client to {:?}", out_file);
        }
        Err(e) => {
            panic!("Failed to generate client from OpenAPI spec: {}", e);
        }
    }
}