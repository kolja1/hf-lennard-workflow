use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Only rebuild if protos change
    println!("cargo:rerun-if-changed=../../proto/");
    
    let proto_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()  // crates
        .parent().unwrap()  // rust-workflow
        .join("proto");
    
    // List all proto files
    let proto_files = vec![
        proto_root.join("common.proto"),
        proto_root.join("workflow_types.proto"),
        proto_root.join("approval_types.proto"),
        proto_root.join("workflow_service.proto"),
    ];
    
    // Verify all proto files exist
    for proto_file in &proto_files {
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
            &proto_files.iter().map(|p| p.to_str().unwrap()).collect::<Vec<_>>(),
            &[proto_root.to_str().unwrap()],
        )?;
    
    println!("Successfully generated gRPC code");
    Ok(())
}