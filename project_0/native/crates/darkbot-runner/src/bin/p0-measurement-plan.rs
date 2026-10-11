//! Offline plan compiler. The launcher forwards one duration, no RPC/keypair access.
fn main() {
    let result =
        darkbot_runner::measurement::MeasurementPlan::from_lookup(&|k| std::env::var(k).ok());
    match result {
        Ok(plan) => println!("{}", serde_json::to_string_pretty(&plan).unwrap()),
        Err(e) => {
            eprintln!("MeasurementPlan: {e}");
            std::process::exit(2);
        }
    }
}
