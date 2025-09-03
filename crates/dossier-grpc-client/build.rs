use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Only rebuild if proto changes
    println!("cargo:rerun-if-changed=dossier_service.proto");
    
    // Find the proto file - it's now in the same directory as this crate
    let proto_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("dossier_service.proto");
    
    // Output to src/generated (not OUT_DIR)
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("generated");
    
    // Create output directory
    std::fs::create_dir_all(&out_dir)?;
    
    // Check if proto exists
    if !proto_path.exists() {
        panic!("Proto file not found at {:?}. Please ensure dossier_service.proto exists", proto_path);
    }
    
    eprintln!("[INFO] Generating gRPC client from {:?}", proto_path);
    
    // Generate the gRPC client code
    tonic_build::configure()
        .build_server(false)           // Don't generate server code
        .build_client(true)            // Generate client code
        .out_dir(&out_dir)             // Output to src/generated
        .compile(
            &[proto_path.to_str().unwrap()],
            &[proto_path.parent().unwrap().to_str().unwrap()],
        )?;
    
    eprintln!("[INFO] Successfully generated gRPC client in {:?}", out_dir);
    Ok(())
}