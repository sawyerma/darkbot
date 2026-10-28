//! The BSC runtime (C0/C1): the read-only observation process and the
//! native owner of the source modes.
//!
//! ```text
//! bsc_runtime run --sources <dir> --rundir <dir> --config <ingress.public.env>
//! bsc_runtime source init --state <dir>
//! bsc_runtime source show --state <dir>
//! bsc_runtime source request <SOURCE_COMPONENT_OR_VENUE_ID> <OFF|SHADOW|LIVE> --expect-generation <n> --request-id <id> --state <dir>
//! bsc_runtime source request <WALLET_OR_ECONOMIC_SWITCH_ID> <DISABLED|ENABLED> --expect-generation <n> --request-id <id> --state <dir>
//! bsc_runtime control require [--state <dir>] <SWITCH_ID>...
//! bsc_runtime census --rundir <dir> --config <ingress.public.env>
//! bsc_runtime g04-verdict --rundir <dir> --config <ingress.public.env> --criteria <g04_criteria.env>
//! bsc_runtime replay-proof --rundir <dir> --config <ingress.public.env> [--run <started_unix_ms>]
//! bsc_runtime capture-preflight --rundir <dir> --config <ingress.public.env>
//! bsc_runtime capture-quality --rundir <dir> --config <ingress.public.env> [--run <started_unix_ms>]
//! bsc_runtime strategy-replay --rundir <dir> --config <ingress.public.env> [--run <started_unix_ms>]
//! ```
//!
//! `strategy-replay` replays one finished run (the newest without `--run`)
//! through Strategy C with the configured terms and frozen gas evidence:
//! every journaled `Y` output must be reproduced, nothing else (Phase H).
//! Writes `bsc-runtime/strategy-replay-<run>.dbrec`, prints it; exit 0
//! only on PASS.
//!
//! `capture-quality` writes the Phase F quality verdict of one finished run
//! (the newest without `--run`; contract §11/§13) into
//! `bsc-runtime/capture-quality-<run>.dbrec` and prints it; exit 0 only when
//! CLEAN.
//!
//! `capture-preflight` measures this host for a scoped capture of
//! `BSC_RUNTIME_CAPTURE_DURATION_S` (master plan §11) into
//! `bsc-runtime/capture-preflight.dbrec` and prints it; exit 0 only when
//! QUALIFIED.
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
//! `g04-verdict` replays the census and rates it against the G0.4 criteria
//! file (order V2 §8) into `bsc-runtime/g04-verdict.dbrec` and prints it;
//! unconfirmed criteria rate nothing. Exit 0 only for a verdict
//! (CONTINUE, ADAPT or STOP).
//!
//! `request` applies one user request with its generation as the CAS and
//! prints its receipt as one line; the same request again prints the same
//! receipt. A requested LIVE is recorded and stays BLOCKED (no economic
//! authorization exists). Exit 0 when applied, 2 when refused or stale.
//!
//! `source init` is the owner's start (darkbotctl start and restart): it
//! writes generation 0 once and DISABLES every ENABLED wallet and economic
//! switch (plan 27.10.); `run` does the same at its start.
//!
//! `control require` is the check a Mainnet signing or sending path runs
//! before it touches a wallet key: exit 0 only if every named switch is
//! ENABLED in the canonical owner state (`.darkbot/run/bsc-sources` under
//! the account home; a different `HOME` is refused) of this boot session,
//! else it prints the refusal and exits 2. `--state DIR` checks another
//! owner state instead (diagnosis only; no Mainnet path reads it).

use darkbot_bsc_execution::runtime::{
    RuntimeConfig, journal_max_payload, preflight_setup, run as run_runtime, stop_file,
    write_census,
};
use darkbot_bsc_execution::sources::{Entity, KEEP_GENERATIONS, Mode, Store, SwitchId};
use darkbot_bsc_execution::{capture_preflight, capture_quality, g04_criteria, replay_proof};
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use std::process::ExitCode;

fn flag<'a>(args: &'a [String], name: &str) -> Result<&'a str, String> {
    let at = args
        .iter()
        .position(|a| a == name)
        .ok_or_else(|| format!("{name} is required"))?;
    args.get(at + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("{name} needs a value"))
}

/// The LIVE grant a `live scope|grant` command names (plan 28.10.).
fn grant_args(rest: &[String]) -> Result<darkbot_bsc_execution::sources::LiveGrant, String> {
    use darkbot_bsc_execution::sources::{
        LiveAdapter, LiveGrant, LiveStrategies, SourceId, VenueId,
    };
    let list = |name: &str| -> Result<Vec<&str>, String> {
        Ok(flag(rest, name)?
            .split(',')
            .filter(|s| !s.is_empty())
            .collect())
    };
    let number = |name: &str| -> Result<u128, String> {
        let text = flag(rest, name)?;
        if text.is_empty()
            || !text.bytes().all(|b| b.is_ascii_digit())
            || (text.len() > 1 && text.starts_with('0'))
        {
            return Err(format!("{name} is a plain decimal"));
        }
        text.parse::<u128>()
            .map_err(|_| format!("{name} out of range"))
    };
    Ok(LiveGrant {
        venue: VenueId::parse(flag(rest, "--venue")?).ok_or("--venue: unknown venue")?,
        strategies: LiveStrategies::parse(flag(rest, "--strategies")?)
            .ok_or("--strategies: C3, STRATEGY_C or C3+STRATEGY_C")?,
        sources: list("--sources")?
            .into_iter()
            .map(|s| SourceId::parse(s).ok_or(format!("--sources: unknown source {s}")))
            .collect::<Result<_, _>>()?,
        adapters: list("--adapters")?
            .into_iter()
            .map(|a| LiveAdapter::parse(a).ok_or(format!("--adapters: unknown adapter {a}")))
            .collect::<Result<_, _>>()?,
        max_input_wei: number("--max-input-wei")?,
        max_spend_wei: number("--max-spend-wei")?,
        max_attempts: u32::try_from(number("--max-attempts")?)
            .map_err(|_| "--max-attempts out of range")?,
        expires_unix_ms: u64::try_from(number("--expires-unix-ms")?)
            .map_err(|_| "--expires-unix-ms out of range")?,
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Every value of a repeated flag.
fn flags<'a>(args: &'a [String], name: &str) -> Vec<&'a str> {
    args.windows(2)
        .filter(|w| w[0] == name)
        .map(|w| w[1].as_str())
        .collect()
}

/// `NAME@A,B[@C,D]`: a name and its comma lists.
fn at_lists(value: &str) -> (&str, Vec<Vec<String>>) {
    let mut parts = value.split('@');
    let name = parts.next().unwrap_or_default();
    let lists = parts
        .map(|p| {
            p.split(',')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .collect();
    (name, lists)
}

/// The LIVE qualification a `live qualify` command names (plan 28.10.).
fn qualify_args(
    rest: &[String],
    now: u64,
) -> Result<darkbot_bsc_execution::live_qualification::Spec, String> {
    use darkbot_bsc_execution::sources::{LiveAdapter, SourceId, VenueId};
    use darkbot_bsc_trading::submission_policy::qualification::Finding;
    let mut spec = darkbot_bsc_execution::live_qualification::Spec::default();
    for value in flags(rest, "--venue") {
        let (name, lists) = at_lists(value);
        let venue = VenueId::parse(name).ok_or(format!("--venue: unknown venue {name}"))?;
        let [evidence] = <[Vec<String>; 1]>::try_from(lists).map_err(|_| "--venue V@EV,EV")?;
        spec.venues.push((venue, evidence));
    }
    for value in flags(rest, "--source") {
        let (name, lists) = at_lists(value);
        let source = SourceId::parse(name).ok_or(format!("--source: unknown source {name}"))?;
        let [evidence] = <[Vec<String>; 1]>::try_from(lists).map_err(|_| "--source S@EV,EV")?;
        spec.sources.push((source, evidence));
    }
    for value in flags(rest, "--adapter") {
        let (name, lists) = at_lists(value);
        let adapter =
            LiveAdapter::parse(name).ok_or(format!("--adapter: unknown adapter {name}"))?;
        let [findings, evidence] =
            <[Vec<String>; 2]>::try_from(lists).map_err(|_| "--adapter A@FINDING,FINDING@EV,EV")?;
        let findings = findings
            .iter()
            .map(|f| Finding::parse(f).ok_or(format!("--adapter: unknown finding {f}")))
            .collect::<Result<_, _>>()?;
        spec.adapters.push((adapter, findings, evidence));
    }
    spec.acquisition = flags(rest, "--acquisition").first().map(|p| p.to_string());
    if let Some(by) = flags(rest, "--risk-by").first() {
        let evidence = flag(rest, "--risk-evidence")?
            .split(',')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        spec.risk = Some((
            by.to_string(),
            now,
            flag(rest, "--risk-statement")?.into(),
            evidence,
        ));
    }
    Ok(spec)
}

/// The verified LIVE qualification in `--root` (none: nothing qualified),
/// printed as its scope.
fn print_qualification(rest: &[String]) -> darkbot_bsc_execution::sources::LiveQualification {
    use darkbot_bsc_execution::live_qualification::{LIVE_QUALIFICATION_PATH, view};
    let Some(root) = flags(rest, "--root")
        .first()
        .copied()
        .map(std::path::Path::new)
    else {
        println!("qualification=NONE root=UNSET");
        return Default::default();
    };
    let (v, why) = view(root);
    match why {
        Some(why) => println!("qualification=INVALID path={LIVE_QUALIFICATION_PATH} reason={why}"),
        None if v.sha256 == [0; 32] => {
            println!("qualification=NONE path={LIVE_QUALIFICATION_PATH}")
        }
        None => println!(
            "qualification=VERIFIED sha256={} venues={} sources={} adapters={} acquisition={} risk_decision={}",
            hex(&v.sha256),
            v.view
                .venues
                .iter()
                .map(|x| x.name())
                .collect::<Vec<_>>()
                .join(","),
            v.view
                .sources
                .iter()
                .map(|x| x.name())
                .collect::<Vec<_>>()
                .join(","),
            v.view
                .adapters
                .iter()
                .map(|x| x.name())
                .collect::<Vec<_>>()
                .join(","),
            v.view.acquisition,
            v.view.risk_decision,
        ),
    }
    v.view
}

fn run(args: &[String]) -> Result<bool, String> {
    match args {
        // Plan 28.10.: the canonical scope text of a LIVE grant and its
        // SHA-256, the value an operator confirms with `live grant`.
        [area, cmd, rest @ ..] if area == "live" && cmd == "scope" => {
            let grant = grant_args(rest)?;
            grant.validate()?;
            print!("{}", grant.scope_text());
            println!("scope_sha256={}", hex(&grant.scope_sha256()));
            Ok(true)
        }
        [area, cmd, rest @ ..] if area == "live" && cmd == "grant" => {
            let grant = grant_args(rest)?;
            let confirmed = flag(rest, "--scope-sha256")?;
            let digest = (confirmed.len() == 64)
                .then(|| {
                    (0..32)
                        .map(|i| u8::from_str_radix(&confirmed[2 * i..2 * i + 2], 16).ok())
                        .collect::<Option<Vec<u8>>>()
                })
                .flatten()
                .and_then(|v| <[u8; 32]>::try_from(v).ok())
                .filter(|_| confirmed.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
                .ok_or("--scope-sha256 is 64 lowercase hex digits (from `live scope`)")?;
            let expected = flag(rest, "--expect-generation")?
                .parse::<u64>()
                .map_err(|_| "--expect-generation is an unsigned integer")?;
            let store = Store::open(flag(rest, "--state")?, KEEP_GENERATIONS)?;
            let now = darkbot_bsc_record::observed_now()?;
            let receipt = store.grant_live(flag(rest, "--request-id")?, grant, digest, expected, now)?;
            println!("{}", receipt.line());
            Ok(receipt.applied())
        }
        [area, cmd, rest @ ..] if area == "live" && cmd == "revoke" => {
            let expected = flag(rest, "--expect-generation")?
                .parse::<u64>()
                .map_err(|_| "--expect-generation is an unsigned integer")?;
            let store = Store::open(flag(rest, "--state")?, KEEP_GENERATIONS)?;
            let now = darkbot_bsc_record::observed_now()?;
            let receipt = store.request(
                flag(rest, "--request-id")?,
                Entity::LiveGrant,
                Mode::Disabled,
                expected,
                now,
            )?;
            println!("{}", receipt.line());
            Ok(receipt.applied())
        }
        // Plan 28.10.: the LIVE qualification record of a scope, each
        // evidence file bound by path and SHA-256; written create-new to
        // `--root`/bsc/config/live_qualification.dbrec (or `--out`).
        [area, cmd, rest @ ..] if area == "live" && cmd == "qualify" => {
            use darkbot_bsc_execution::live_qualification::{LIVE_QUALIFICATION_PATH, build};
            use std::io::Write;
            let root = std::path::Path::new(flag(rest, "--root")?);
            let now = darkbot_bsc_record::observed_now()?;
            let bytes = build(root, &qualify_args(rest, now)?, now)?;
            let out = match flags(rest, "--out").first() {
                Some(out) => std::path::PathBuf::from(out),
                None => root.join(LIVE_QUALIFICATION_PATH),
            };
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&out)
                .and_then(|mut f| f.write_all(&bytes).and_then(|()| f.sync_all()))
                .map_err(|e| format!("{}: {e}", out.display()))?;
            println!("written={} sha256={}", out.display(), hex(&darkbot_bsc_record::sha256(&bytes)));
            Ok(true)
        }
        // The LIVE gate as it holds now: every missing condition, then
        // what each LIVE request is now.
        [area, cmd, rest @ ..] if area == "live" && cmd == "status" => {
            use darkbot_bsc_execution::sources::{LiveContext, live_inputs};
            let dir = std::path::Path::new(flag(rest, "--state")?);
            let now = darkbot_bsc_record::observed_now()?;
            let (g, daily, total) = live_inputs(dir, now)?;
            let boot = darkbot_bsc_execution::sources::boot_session().ok();
            let qualification = print_qualification(rest);
            let ctx = LiveContext {
                boot: boot.as_deref(),
                now_unix_ms: now,
                daily: Some(&daily),
                total: Some(&total),
                spent_wei: Some(0),
                qualification: &qualification,
            };
            println!("generation={}", g.generation);
            match &g.live {
                Some(grant) => {
                    print!("{}", grant.scope_text());
                    println!("scope_sha256={}", hex(&grant.scope_sha256()));
                }
                None => println!("grant=NONE"),
            }
            let blockers = g.live_blockers(&ctx);
            println!(
                "live={} blockers={}",
                if blockers.is_empty() { "CLEAR" } else { "BLOCKED" },
                blockers.join(",")
            );
            println!(
                "buy_attempts day_utc={} used={} limit={} total_used={} total_limit={}",
                daily.day.utc, daily.used, daily.limit, total.used, total.limit
            );
            Ok(blockers.is_empty())
        }
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
        [cmd, rest @ ..] if cmd == "g04-verdict" => {
            let config_path = flag(rest, "--config")?;
            let text = std::fs::read_to_string(config_path).map_err(|e| format!("{config_path}: {e}"))?;
            let config = RuntimeConfig::parse(&text)?;
            let criteria_path = flag(rest, "--criteria")?;
            let criteria = g04_criteria::Criteria::parse(
                &std::fs::read(criteria_path).map_err(|e| format!("{criteria_path}: {e}"))?,
            )?;
            let rundir = std::path::Path::new(flag(rest, "--rundir")?);
            let census = write_census(&config, rundir, None)?;
            let now = darkbot_bsc_record::observed_now()?;
            let verdict = g04_criteria::evaluate(&criteria, &census, now);
            replay_proof::write(
                &rundir.join("bsc-runtime/g04-verdict.dbrec"),
                &verdict.encode(now)?,
            )?;
            print!("{}", verdict.summary());
            Ok(verdict.verdict != g04_criteria::NOT_EVALUABLE
                && verdict.verdict != darkbot_bsc_execution::g04_census::CRITERIA_UNCONFIRMED)
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
                Some(config.source_sha256),
                darkbot_bsc_execution::runtime_journal::own_binary_sha256(),
            )?;
            let observed = darkbot_bsc_record::observed_now()?;
            replay_proof::write(
                &dir.join(format!("replay-proof-{run}.dbrec")),
                &proof.encode(observed)?,
            )?;
            println!("{}", proof.summary());
            Ok(proof.verdict == "PASS")
        }
        [cmd, rest @ ..] if cmd == "capture-quality" => {
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
            let q = capture_quality::assess(
                &journal,
                &run,
                status.as_deref(),
                journal_max_payload(&config),
                config.max_launch_calldata,
                Some(config.source_sha256),
                darkbot_bsc_execution::runtime_journal::own_binary_sha256(),
                config.capture_duration_s,
            )?;
            replay_proof::write(
                &dir.join(format!("capture-quality-{run}.dbrec")),
                &q.encode(darkbot_bsc_record::observed_now()?)?,
            )?;
            println!("{}", q.summary());
            Ok(q.clean)
        }
        [cmd, rest @ ..] if cmd == "strategy-replay" => {
            let config_path = flag(rest, "--config")?;
            let text = std::fs::read_to_string(config_path).map_err(|e| format!("{config_path}: {e}"))?;
            let config = RuntimeConfig::parse(&text)?;
            let repo_root = std::path::Path::new(config_path)
                .canonicalize()
                .map_err(|e| format!("{config_path}: {e}"))?
                .ancestors()
                .nth(3)
                .ok_or("--config is not under <repo>/bsc/config/")?
                .to_path_buf();
            let gas = config.strategy.gas(&repo_root)?;
            let dir = std::path::Path::new(flag(rest, "--rundir")?).join("bsc-runtime");
            let journal = dir.join("journal");
            let run = match rest.iter().any(|a| a == "--run") {
                true => flag(rest, "--run")?.to_string(),
                false => replay_proof::latest_run(&journal)?,
            };
            let replay = darkbot_bsc_execution::runtime_strategy::replay(
                &journal,
                &run,
                journal_max_payload(&config),
                &config.strategy.engine,
                &gas,
                Some(config.source_sha256),
            )?;
            replay_proof::write(
                &dir.join(format!("strategy-replay-{run}.dbrec")),
                &replay.encode(darkbot_bsc_record::observed_now()?)?,
            )?;
            println!("{}", replay.summary());
            Ok(replay.verdict() == "PASS")
        }
        [cmd, rest @ ..] if cmd == "capture-preflight" => {
            let config_path = flag(rest, "--config")?;
            let text = std::fs::read_to_string(config_path).map_err(|e| format!("{config_path}: {e}"))?;
            let config = RuntimeConfig::parse(&text)?;
            let dir = std::path::Path::new(flag(rest, "--rundir")?).join("bsc-runtime");
            let report = capture_preflight::check(&dir.join("journal"), preflight_setup(&config))?;
            replay_proof::write(
                &dir.join("capture-preflight.dbrec"),
                &report.encode(darkbot_bsc_record::observed_now()?)?,
            )?;
            println!("{}", report.summary());
            Ok(report.qualified())
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
            let g = store.restart(darkbot_bsc_record::observed_now()?)?;
            println!("generation={}", g.generation);
            Ok(true)
        }
        [area, cmd, rest @ ..] if area == "control" && cmd == "require" && !rest.is_empty() => {
            // `--state DIR` checks that owner state (a diagnosis, e.g. of an
            // isolated test run); without it the canonical one every Mainnet
            // path reads.
            let (state, ids) = match rest {
                [flag, dir, ids @ ..] if flag == "--state" => (Some(dir), ids),
                ids => (None, ids),
            };
            if ids.is_empty() {
                return Err("control require [--state DIR] SWITCH_ID...".into());
            }
            let needed = ids
                .iter()
                .map(|id| SwitchId::parse(id).ok_or(format!("{id} is not a switch")))
                .collect::<Result<Vec<_>, _>>()?;
            let checked = match state {
                Some(dir) => darkbot_bsc_execution::sources::boot_session().and_then(|boot| {
                    darkbot_bsc_execution::sources::require_enabled(
                        std::path::Path::new(dir),
                        &boot,
                        &needed,
                    )
                }),
                None => darkbot_bsc_execution::sources::require_enabled_here(&needed),
            };
            match checked {
                Ok(generation) => {
                    println!("controls=ENABLED generation={generation}");
                    Ok(true)
                }
                Err(refusal) => {
                    println!("controls=REFUSED {refusal}");
                    Ok(false)
                }
            }
        }
        [area, cmd, rest @ ..] if area == "source" && cmd == "show" => {
            let store = Store::open(flag(rest, "--state")?, KEEP_GENERATIONS)?;
            let g = store.current()?;
            println!("generation={}", g.generation);
            let boot = darkbot_bsc_execution::sources::boot_session().ok();
            // As each holds in this boot session (switches, LIVE reasons).
            let rows = g
                .sources
                .iter()
                .map(|s| (s.source.name(), s.requested, Entity::Source(s.source)))
                .chain(g.components.iter().map(|c| {
                    (
                        c.component.name(),
                        c.requested,
                        Entity::Component(c.component),
                    )
                }))
                .chain(
                    g.venues
                        .iter()
                        .map(|v| (v.venue.name(), v.requested, Entity::Venue(v.venue))),
                )
                .chain(
                    g.switches
                        .iter()
                        .map(|s| (s.switch.name(), s.requested, Entity::Switch(s.switch))),
                )
                .chain([
                    (
                        darkbot_bsc_execution::sources::RUNTIME_MODE,
                        g.runtime.requested,
                        Entity::Runtime,
                    ),
                    (
                        darkbot_bsc_execution::sources::BUY_ATTEMPTS_PER_DAY,
                        g.buy_limit.requested,
                        Entity::BuyLimit,
                    ),
                    (
                        darkbot_bsc_execution::sources::BUY_ATTEMPTS_TOTAL,
                        g.buy_total.requested,
                        Entity::BuyTotal,
                    ),
                    (
                        darkbot_bsc_execution::sources::LIVE_GRANT,
                        g.requested_of(Entity::LiveGrant),
                        Entity::LiveGrant,
                    ),
                ])
                .collect::<Vec<_>>();
            for (name, requested, entity) in rows {
                let effective = g.effective_now(entity, boot.as_deref());
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
            let usage = darkbot_bsc_execution::sources::buy_usage(
                std::path::Path::new(flag(rest, "--state")?),
                darkbot_bsc_record::observed_now()?,
            )?;
            println!(
                "buy_attempts day_utc={} used={} limit={} remaining={}",
                usage.day.utc,
                usage.used,
                usage.limit,
                usage.remaining()
            );
            let total = darkbot_bsc_execution::sources::total_usage(std::path::Path::new(flag(rest, "--state")?))?;
            println!(
                "buy_attempts_total used={} limit={} remaining={}",
                total.used,
                total.limit,
                total.remaining()
            );
            Ok(true)
        }
        [area, cmd, source, mode, rest @ ..] if area == "source" && cmd == "request" => {
            let source =
                Entity::parse(source).ok_or("unknown source, component, venue or switch ID")?;
            let mode = Mode::parse(mode)
                .filter(|m| source.accepts(*m))
                .ok_or(format!("{} takes {}", source.name(), source.modes()))?;
            let expected = flag(rest, "--expect-generation")?
                .parse::<u64>()
                .map_err(|_| "--expect-generation is an unsigned integer")?;
            let store = Store::open(flag(rest, "--state")?, KEEP_GENERATIONS)?;
            let now = darkbot_bsc_record::observed_now()?;
            let receipt = store.request(flag(rest, "--request-id")?, source, mode, expected, now)?;
            println!("{}", receipt.line());
            Ok(receipt.applied())
        }
        _ => Err("usage: bsc_runtime run --sources DIR --rundir DIR --config FILE | source init|show --state DIR | source request ID MODE --expect-generation N --request-id ID --state DIR | live scope|grant --venue V --strategies S --sources A,B --adapters X --max-input-wei N --max-spend-wei N --max-attempts N --expires-unix-ms N [--scope-sha256 H --expect-generation N --request-id ID --state DIR] | live revoke --expect-generation N --request-id ID --state DIR | live qualify --root DIR [--venue V@EV,..] [--source S@EV,..] [--adapter A@FINDING,..@EV,..] [--acquisition EV] [--risk-by WHO --risk-statement TEXT --risk-evidence EV,..] [--out FILE] | live status --state DIR [--root DIR] | control require [--state DIR] SWITCH_ID... | census --rundir DIR --config FILE | g04-verdict --rundir DIR --config FILE --criteria FILE | replay-proof|capture-quality|strategy-replay --rundir DIR --config FILE [--run ID] | capture-preflight --rundir DIR --config FILE | paper-identity init|show --config FILE".into()),
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
