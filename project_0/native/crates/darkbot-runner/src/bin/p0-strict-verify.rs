use darkbot_runner::market_observer::verification::{run, Job};
fn main() {
    let result = (|| {
        let path = std::env::args().nth(1).ok_or("JOB_REQUIRED".to_string())?;
        let job: Job = serde_json::from_reader(std::fs::File::open(path).map_err(|_| "JOB_IO")?)
            .map_err(|_| "JOB_INVALID")?;
        darkbot_solana::provider::enable_measurement_budget();
        run(job)
    })();
    match result {
        Ok(value) => println!("{}", value),
        Err(error) => {
            eprintln!("HISTORICAL_INCOMPLETE: {error}");
            std::process::exit(2);
        }
    }
}
