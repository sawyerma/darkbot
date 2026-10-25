use std::path::PathBuf;

// Einzige Rust-Codegen-Quelle: protocol/schemas. Reines Rust (protox), kein protoc.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../protocol/schemas");
    let files = [
        "darkbot/control/v1/numeric.proto",
        "darkbot/control/v1/topics.proto",
        "darkbot/control/v1/stream.proto",
        "darkbot/control/v1/commands.proto",
        "darkbot/control/v1/overview.proto",
        "darkbot/control/v1/engine_feed.proto",
        "darkbot/bsc/v1/status.proto",
        "darkbot/project0/v1/status.proto",
        "darkbot/project0/v1/capture.proto",
        "darkbot/project0/v1/system.proto",
        "darkbot/project0/v1/analytics.proto",
    ];
    for f in files {
        println!("cargo:rerun-if-changed={}", root.join(f).display());
    }
    let fds = protox::compile(files.iter().map(|f| root.join(f)), [root.as_path()])?;
    let mut cfg = prost_build::Config::new();
    cfg.bytes(["."]);
    cfg.compile_fds(fds)?;
    Ok(())
}
