//! The BSC runtime (C0/C1): the read-only observation process and the
//! native owner of the source modes.
//!
//! ```text
//! bsc_runtime run --sources <dir> --rundir <dir> --config <ingress.public.env>
//! bsc_runtime source init --state <dir>
//! bsc_runtime source show --state <dir>
//! bsc_runtime source request <SOURCE_OR_COMPONENT_ID> <OFF|SHADOW|LIVE> --expect-generation <n> --request-id <id> --state <dir>
//! bsc_runtime census --rundir <dir> --config <ingress.public.env>
//! bsc_runtime replay-proof --rundir <dir> --config <ingress.public.env> [--run <started_unix_ms>]
//! ```
//!
//! `replay-proof` proves the capture→replay identity of one finished run
//! (the newest without `--run`; Phase F contract §9) into
//! `bsc-runtime/replay-proof-<run>.dbrec` and prints it; exit 0 only on
//! PASS.
//!
//! `census` replays every runtime journal of the run directory into the
//! G0.4 census record (`bsc-runtime/g04-census.dbrec`) and prints its text
//! view; the running runtime refreshes the same record itself.
//!
//! `request` applies one user request with its generation as the CAS and
//! prints its receipt as one line; the same request again prints the same
//! receipt. A requested LIVE is recorded and stays BLOCKED (no economic
//! authorization exists). Exit 0 when applied, 2 when refused or stale.

use darkbot_bsc_execution::replay_proof;
use darkbot_bsc_execution::runtime::{
    RuntimeConfig, journal_max_payload, run as run_runtime, stop_file, write_census,
};
use darkbot_bsc_execution::sources::{Entity, Mode, Store};
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
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
            let rundir = std::path::Path::new(flag(rest, "--rundir")?);
            stop_on_signals(stop_file(rundir))?;
            // The repository holds the configuration at bsc/config/; the
            // PAPER identity must live outside it.
            let repo_root = std::path::Path::new(config_path)
                .canonicalize()
                .map_err(|e| format!("{config_path}: {e}"))?
                .ancestors()
                .nth(3)
                .ok_or("--config is not under <repo>/bsc/config/")?
                .to_path_buf();
            run_runtime(
                &config,
                std::path::Path::new(flag(rest, "--sources")?),
                rundir,
                &repo_root,
            )?;
            Ok(true)
        }
        [cmd, rest @ ..] if cmd == "census" => {
            let config_path = flag(rest, "--config")?;
            let text = std::fs::read_to_string(config_path).map_err(|e| format!("{config_path}: {e}"))?;
            let config = RuntimeConfig::parse(&text)?;
            // Offline replay: a journal a runtime still writes reads UNTERMINATED here.
            let census = write_census(&config, std::path::Path::new(flag(rest, "--rundir")?), None)?;
            print!("{}", census.summary());
            Ok(true)
        }
        [cmd, rest @ ..] if cmd == "replay-proof" => {
            let config_path = flag(rest, "--config")?;
            let text = std::fs::read_to_string(config_path).map_err(|e| format!("{config_path}: {e}"))?;
            let config = RuntimeConfig::parse(&text)?;
            let dir = std::path::Path::new(flag(rest, "--rundir")?).join("bsc-runtime");
            let journal = dir.join("journal");
            let run = match rest.iter().any(|a| a == "--run") {
                true => flag(rest, "--run")?.to_string(),
                false => replay_proof::latest_run(&journal)?,
            };
            let status = std::fs::read(dir.join("status.dbrec")).ok();
            let proof = replay_proof::prove(
                &journal,
                &run,
                status.as_deref(),
                journal_max_payload(&config),
                config.max_launch_calldata,
            )?;
            let observed = darkbot_bsc_record::observed_now()?;
            replay_proof::write(
                &dir.join(format!("replay-proof-{run}.dbrec")),
                &proof.encode(observed)?,
            )?;
            println!("{}", proof.summary());
            Ok(proof.verdict == "PASS")
        }
        [area, cmd, rest @ ..] if area == "paper-identity" && (cmd == "init" || cmd == "show") => {
            // The C3 PAPER identity: a local key outside the repository,
            // never funded. Prints its address only.
            let config_path = flag(rest, "--config")?;
            let text = std::fs::read_to_string(config_path).map_err(|e| format!("{config_path}: {e}"))?;
            let config = RuntimeConfig::parse(&text)?;
            let setup = config.exec.ok_or("BSC_EXEC_PAPER_IDENTITY is not configured")?;
            let repo_root = std::path::Path::new(config_path)
                .canonicalize()
                .map_err(|e| format!("{config_path}: {e}"))?
                .ancestors()
                .nth(3)
                .ok_or("--config is not under <repo>/bsc/config/")?
                .to_path_buf();
            let address = if cmd == "init" {
                darkbot_bsc_execution::paper::PaperIdentity::create(&setup.identity_path, &repo_root)
                    .map_err(|e| e.to_string())?
            } else {
                darkbot_bsc_execution::paper::PaperIdentity::load(
                    &setup.identity_path,
                    &repo_root,
                    &setup.refused_accounts,
                )
                .map_err(|e| e.to_string())?
                .address()
            };
            println!("paper_sender=0x{}", alloy_primitives::hex::encode(address));
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
            let rows = g
                .sources
                .iter()
                .map(|s| (s.source.name(), s.requested, s.effective))
                .chain(
                    g.components
                        .iter()
                        .map(|c| (c.component.name(), c.requested, c.effective)),
                );
            for (name, requested, effective) in rows {
                println!(
                    "{} requested={} effective={}{}",
                    name,
                    requested.name(),
                    effective.name(),
                    effective
                        .reason()
                        .map(|r| format!(" block_reason={r}"))
                        .unwrap_or_default()
                );
            }
            Ok(true)
        }
        [area, cmd, source, mode, rest @ ..] if area == "source" && cmd == "request" => {
            let source = Entity::parse(source).ok_or("unknown source or component ID")?;
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
        _ => Err("usage: bsc_runtime run --sources DIR --rundir DIR --config FILE | source init|show --state DIR | source request SOURCE_ID MODE --expect-generation N --request-id ID --state DIR | census --rundir DIR --config FILE | paper-identity init|show --config FILE".into()),
    }
}

/// SIGTERM (launchd, logout, shutdown) and SIGINT become the runtime's own
/// stop request: the journal is committed, the window closed and the final
/// status and census written within the configured stop bound.
fn stop_on_signals(stop: std::path::PathBuf) -> Result<(), String> {
    let mut signals =
        Signals::new([SIGTERM, SIGINT]).map_err(|e| format!("signal handler: {e}"))?;
    std::thread::Builder::new()
        .name("bsc-runtime-signals".into())
        .spawn(move || {
            if signals.forever().next().is_some()
                && let Err(e) = std::fs::write(&stop, b"signal\n")
            {
                eprintln!("STOP request on signal failed: {}: {e}", stop.display());
            }
        })
        .map_err(|e| format!("signal thread: {e}"))?;
    Ok(())
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
