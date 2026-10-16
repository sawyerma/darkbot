//! Project_0 executable: configuration and lifecycle are owned by orchestration.
fn main() {
    if let Err(e) = darkbot_runner::orchestration::run() {
        eprintln!("p0-longtail: {}", e);
        std::process::exit(2);
    }
}
