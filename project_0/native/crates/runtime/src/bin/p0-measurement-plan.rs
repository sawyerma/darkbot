//! Offline plan compiler. The launcher forwards one duration, no RPC/keypair access.
//!
//! `p0-measurement-plan --out PLAN.p0j`: the plan is kept as a typed single-value journal (the
//! runtime's input, JFI-04) and stdout carries only its duration in seconds (text). There is no
//! JSON output (JFI-01, P0-PROD-CTRL-02).
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = match args.as_slice() {
        [flag, path] if flag == "--out" => std::path::PathBuf::from(path),
        _ => {
            eprintln!("usage: p0-measurement-plan --out PLAN.p0j");
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
        Ok(plan) => {
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
        Err(e) => {
            eprintln!("MeasurementPlan: {e}");
            std::process::exit(2);
        }
    }
}
