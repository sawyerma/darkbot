use std::path::PathBuf;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../protocol/schemas");
    let files = [
        "darkbot/control/v1/topics.proto",
        "darkbot/control/v1/stream.proto",
        "darkbot/control/v1/engine_feed.proto",
        "darkbot/project0/v1/status.proto",
        "darkbot/project0/v1/capture.proto",
        "darkbot/project0/v1/system.proto",
    ];
    for file in files {
        println!("cargo:rerun-if-changed={}", root.join(file).display());
    }
    let descriptors = protox::compile(files.iter().map(|p| root.join(p)), [root.as_path()])?;
    let mut build = prost_build::Config::new();
    build.bytes(["."]);
    build.compile_fds(descriptors)?;
    Ok(())
}
