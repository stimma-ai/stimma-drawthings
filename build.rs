fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);
    tonic_prost_build::configure()
        .build_server(true)
        .compile_protos(&["schemas/image_service.proto"], &["schemas"])?;
    println!("cargo:rerun-if-changed=schemas/image_service.proto");
    Ok(())
}
