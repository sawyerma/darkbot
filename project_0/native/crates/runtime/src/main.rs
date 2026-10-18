//! Project_0 executable: configuration and lifecycle are owned by orchestration.
fn main() {
    if let Err(e) = darkbot_runner::orchestration::run() {
        eprintln!(
            "p0-longtail: {}",
            darkbot_runner::orchestration::failure_report(e.as_ref())
        );
        std::process::exit(2);
    }
}
