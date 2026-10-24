//! Offline plan compiler. The launcher forwards one duration, no RPC/keypair access.
//!
//! `p0-measurement-plan --out PLAN.p0j`: the plan is kept as a typed single-value journal (the
//! runtime's input, JFI-04) and stdout carries only its duration in seconds (text). There is no
//! JSON output (JFI-01, P0-PROD-CTRL-02).
//!
//! `p0-measurement-plan --historical-plan`: the isolated offline path of the pinned historical
//! readers (JFI-03) for a run without its own plan record: the plan the Rust authority derives
//! from `P0_MEASUREMENT_DURATION`, printed as `key=value` text; nothing is written and it is
//! never a runtime input.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = match args.as_slice() {
        [flag, path] if flag == "--out" => Some(std::path::PathBuf::from(path)),
        [flag] if flag == "--historical-plan" => None,
        _ => {
            eprintln!("usage: p0-measurement-plan --out PLAN.p0j | --historical-plan");
            std::process::exit(2);
        }
    };
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
    let result = darkbot_runner::analytics::measurement::MeasurementPlan::from_lookup(&|k| {
        std::env::var(k).ok()
    });
    match result {
        Ok(plan) => match out {
            Some(out) => {
                if let Err(e) = darkbot_runner::analytics::journal::write_typed_value(
                    &out,
                    darkbot_runner::analytics::journal::source::MEASUREMENT,
                    &plan,
                ) {
                    eprintln!("MeasurementPlan: {}: {e}", out.display());
                    std::process::exit(2);
                }
                println!("{}", plan.duration_seconds);
            }
            None => print!("{}", plan.historical_text()),
        },
        Err(e) => {
            eprintln!("MeasurementPlan: {e}");
            std::process::exit(2);
        }
    }
}
