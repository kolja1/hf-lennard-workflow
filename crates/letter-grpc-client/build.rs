fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile the letter service proto
    tonic_build::configure()
        .build_server(false)
        .compile(
            &["../../proto/letter_service.proto"],
            &["../../proto"],
        )?;
    Ok(())
}