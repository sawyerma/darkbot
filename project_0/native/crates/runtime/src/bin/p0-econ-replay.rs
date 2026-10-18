use darkbot_runner::{econ::replay_canonical, measurement::MeasurementPlan};
use std::path::Path;
fn main() {
    let result = (|| -> Result<(), String> {
        let args: Vec<_> = std::env::args().collect();
        if !matches!(args.len(), 7 | 8) {
            return Err(
                "usage: p0-econ-replay MARKET SHADOW OUT PLAN START_UNIX_NS MAX_SECONDS [CAPTURE_START_NS]".into(),
            );
        }
        let plan: MeasurementPlan =
            serde_json::from_reader(std::fs::File::open(&args[4]).map_err(|_| "PLAN_IO")?)
                .map_err(|_| "PLAN_DECODE")?;
        replay_canonical(
            Path::new(&args[1]),
            Path::new(&args[2]),
            Path::new(&args[3]),
            &plan,
            args[5].parse().map_err(|_| "START_INVALID")?,
            args[6].parse().map_err(|_| "BUDGET_INVALID")?,
            // A3: the monotone capture start applies the live evaluation window.
            args.get(7)
                .map(|v| v.parse().map_err(|_| "CAPTURE_START_INVALID"))
                .transpose()?,
        )
    })();
    if let Err(error) = result {
        eprintln!("REPLAY_INCOMPLETE: {error}");
        std::process::exit(2);
    }
}
