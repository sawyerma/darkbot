//! The BSC runtime (C0/C1): the read-only observation process and the
//! native owner of the source modes.
//!
//! ```text
//! bsc_runtime run --sources <dir> --rundir <dir> --config <ingress.public.env>
//! bsc_runtime source init --state <dir>
//! bsc_runtime source show --state <dir>
//! bsc_runtime source request <SOURCE_ID> <OFF|SHADOW|LIVE> --expect-generation <n> --request-id <id> --state <dir>
//! ```
//!
//! `request` applies one user request with its generation as the CAS and
//! prints its receipt as one line; the same request again prints the same
//! receipt. A requested LIVE is recorded and stays BLOCKED (no economic
//! authorization exists). Exit 0 when applied, 2 when refused or stale.

use darkbot_bsc_execution::runtime::{RuntimeConfig, run as run_runtime};
use darkbot_bsc_execution::sources::{Mode, SourceId, Store};
use std::process::ExitCode;

/// Generations kept besides the current one.
const KEEP_GENERATIONS: u64 = 64;

fn flag<'a>(args: &'a [String], name: &str) -> Result<&'a str, String> {
    let at = args
        .iter()
        .position(|a| a == name)
        .ok_or_else(|| format!("{name} is required"))?;
    args.get(at + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("{name} needs a value"))
}

fn run(args: &[String]) -> Result<bool, String> {
    match args {
        [cmd, rest @ ..] if cmd == "run" => {
            let config_path = flag(rest, "--config")?;
            let text = std::fs::read_to_string(config_path).map_err(|e| format!("{config_path}: {e}"))?;
            let config = RuntimeConfig::parse(&text)?;
            run_runtime(
                &config,
                std::path::Path::new(flag(rest, "--sources")?),
                std::path::Path::new(flag(rest, "--rundir")?),
            )?;
            Ok(true)
        }
        [area, cmd, rest @ ..] if area == "source" && cmd == "init" => {
            let store = Store::open(flag(rest, "--state")?, KEEP_GENERATIONS)?;
            let g = store.init(darkbot_bsc_record::observed_now()?)?;
            println!("generation={}", g.generation);
            Ok(true)
        }
        [area, cmd, rest @ ..] if area == "source" && cmd == "show" => {
            let store = Store::open(flag(rest, "--state")?, KEEP_GENERATIONS)?;
            let g = store.current()?;
            println!("generation={}", g.generation);
            for s in g.sources {
                println!(
                    "{} requested={} effective={}{}",
                    s.source.name(),
                    s.requested.name(),
                    s.effective.name(),
                    s.effective
                        .reason()
                        .map(|r| format!(" block_reason={r}"))
                        .unwrap_or_default()
                );
            }
            Ok(true)
        }
        [area, cmd, source, mode, rest @ ..] if area == "source" && cmd == "request" => {
            let source = SourceId::parse(source).ok_or("unknown source ID")?;
            let mode = Mode::parse(mode).ok_or("the mode is OFF, SHADOW or LIVE")?;
            let expected = flag(rest, "--expect-generation")?
                .parse::<u64>()
                .map_err(|_| "--expect-generation is an unsigned integer")?;
            let store = Store::open(flag(rest, "--state")?, KEEP_GENERATIONS)?;
            let now = darkbot_bsc_record::observed_now()?;
            let receipt = store.request(flag(rest, "--request-id")?, source, mode, expected, now)?;
            println!("{}", receipt.line());
            Ok(receipt.applied())
        }
        _ => Err("usage: bsc_runtime run --sources DIR --rundir DIR --config FILE | source init|show --state DIR | source request SOURCE_ID MODE --expect-generation N --request-id ID --state DIR".into()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(2),
        Err(e) => {
            eprintln!("STOP: {e}");
            ExitCode::from(2)
        }
    }
}
