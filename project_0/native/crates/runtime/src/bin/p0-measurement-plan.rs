//! Offline plan compiler. The launcher forwards one duration, no RPC/keypair access.
//!
//! `p0-measurement-plan [--out PLAN.p0j]`: with `--out` the plan is kept as a typed single-value
//! journal (the runtime's input, JFI-04); stdout always carries the JSON render the pinned run
//! store stages as its `measurement_plan` part.
fn main() {
    if std::env::var_os("P0_OPERATIONAL_CONFIG_FILE").is_some()
        || std::env::var_os("P0_PROFILE").is_some()
    {
        let result =
            darkbot_solana::runtime_config::Resolved::parse(&[], &|k| std::env::var(k).ok());
        match result {
            Ok(resolved) => {
                for (k, v) in resolved.config.environment() {
                    std::env::set_var(k, v);
                }
            }
            Err(e) => {
                eprintln!("OperationalConfig: {e}");
                std::process::exit(2);
            }
        }
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = match args.as_slice() {
        [] => None,
        [flag, path] if flag == "--out" => Some(std::path::PathBuf::from(path)),
        _ => {
            eprintln!("usage: p0-measurement-plan [--out PLAN.p0j]");
            std::process::exit(2);
        }
    };
    let result = darkbot_runner::analytics::measurement::MeasurementPlan::from_lookup(&|k| {
        std::env::var(k).ok()
    });
    match result {
        Ok(plan) => {
            if let Some(path) = out {
                if let Err(e) = darkbot_runner::analytics::journal::write_typed_value(
                    &path,
                    darkbot_runner::analytics::journal::source::MEASUREMENT,
                    &plan,
                ) {
                    eprintln!("MeasurementPlan: {}: {e}", path.display());
                    std::process::exit(2);
                }
            }
            println!("{}", serde_json::to_string_pretty(&plan).unwrap())
        }
        Err(e) => {
            eprintln!("MeasurementPlan: {e}");
            std::process::exit(2);
        }
    }
}
