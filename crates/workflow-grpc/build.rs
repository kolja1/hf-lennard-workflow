use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Only rebuild if protos change
    println!("cargo:rerun-if-changed=../../proto/");
    
    // Use relative path from CARGO_MANIFEST_DIR
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let proto_root = manifest_dir.join("../../proto");
    
    // List all proto files using relative paths
    let proto_files = ["common.proto",
        "workflow_types.proto",
        "approval_types.proto",
        "workflow_service.proto"];
    
    // Build full paths and verify they exist
    let proto_paths: Vec<PathBuf> = proto_files
        .iter()
        .map(|f| proto_root.join(f))
        .collect();
    
    // Verify all proto files exist
    for proto_file in &proto_paths {
        if !proto_file.exists() {
            panic!("Proto file not found: {:?}", proto_file);
        }
    }
    
    println!("Generating gRPC code from proto files...");
    
    // Generate the gRPC code
    tonic_build::configure()
        .build_server(true)    // Generate server code
        .build_client(true)    // Generate client code
        .compile(
            &proto_paths.iter().map(|p| p.to_str().unwrap()).collect::<Vec<_>>(),
            &[proto_root.to_str().unwrap()],
        )?;
    
    println!("Successfully generated gRPC code");
    Ok(())
}