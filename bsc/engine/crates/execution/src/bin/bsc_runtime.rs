//! The BSC runtime: the observation process, its PAPER and (under an
//! operator's verified grant) LIVE paths, and the native owner of the
//! source modes, switches and the LIVE grant.
//!
//! ```text
//! bsc_runtime run --sources <dir> --rundir <dir> --config <ingress.public.env>
//! bsc_runtime source init|reset|show --state <dir>
//! bsc_runtime source request <SOURCE_COMPONENT_OR_VENUE_ID> <OFF|SHADOW|LIVE> --expect-generation <n> --request-id <id> --state <dir>
//! bsc_runtime source request <WALLET_OR_ECONOMIC_SWITCH_ID> <DISABLED|ENABLED> --expect-generation <n> --request-id <id> --state <dir>
//! bsc_runtime control require [--state <dir>] <SWITCH_ID>...
//! bsc_runtime live scope|grant|revoke|qualify|status|positions|idle|bound-inputs|loopback-identity ...
//! bsc_runtime live attempts --rundir <dir> --state <dir> --config <ingress.public.env> [--manifest <live_campaign.env>] [--witness-dir <dir>]
//! bsc_runtime live campaign-precommit --rundir <dir> --manifest <live_campaign.env> --decided-by <who>
//! bsc_runtime live lot resume|write-off|exit-now --lot <0x hash> --state <dir> --config <ingress.public.env> --request-id <id> --decided-by <who>
//! bsc_runtime live nonce-burn --tx <0x hash> --state <dir> --config <ingress.public.env> --request-id <id> --decided-by <who>
//! bsc_runtime live adopt-g07 --g08 <result.dbrec> --capture <chain journal> --state <dir> --config <ingress.public.env> --request-id <id> --decided-by <who>
//! bsc_runtime live ledger check --state <dir>
//! bsc_runtime live ledger quarantine --seq <n> --state <dir> --request-id <id> --decided-by <who>
//! bsc_runtime census --rundir <dir> --config <ingress.public.env>
//! bsc_runtime g04-precommit --rundir <dir> --criteria <g04_criteria.env> --decided-by <who>
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
//! `g04-precommit` commits the confirmed criteria file's hash for the
//! campaign in the run directory (`bsc-runtime/g04-precommit.dbrec`, written
//! once, before the campaign's first journal starts; BSC3-033).
//!
//! `g04-verdict` replays the census and rates it against the G0.4 criteria
//! file (order V2 §8) into `bsc-runtime/g04-verdict.dbrec` and prints it;
//! unconfirmed criteria, or criteria without a pre-commitment of their hash
//! older than the campaign, rate nothing. Exit 0 only for a verdict
//! (CONTINUE, ADAPT or STOP).
//!
//! `request` applies one user request with its generation as the CAS and
//! prints its receipt as one line; the same request again prints the same
//! receipt. A requested LIVE is effective only while every condition of
//! the LIVE gate holds under the operator's grant (`live status` names
//! each). Exit 0 when applied, 2 when refused or stale.
//!
//! `source init` puts the owner state on file (generation 0 once) and
//! leaves a running owner's switches and grant as they are (01.11., R1:
//! `darkbotctl up` and the app run it). `source reset` is the owner's
//! reset (a stop, plan 27.10.): every ENABLED wallet and economic switch
//! DISABLED and the LIVE grant ended; `run` resets at its start too.
//! `live idle` exits 0 only while no grant, held position or unresolved
//! transaction exists (what `darkbotctl deploy` checks before it restarts
//! a running runtime).
//!
//! `live attempts` (REV.1.0 R2, master plan §11/§13 J) evaluates every
//! written LIVE BUY of the ledger under `--state` against the journals of
//! `--rundir`: its outcome, placement, costs, local timing, the market
//! window from its launch's block (master plan §11: 15 s) and
//! each component's verdict, with the counters up to it, into
//! `bsc-runtime/live-attempts/<tx>.dbrec`; the campaign and release verdict
//! against the manifest into `bsc-runtime/live-campaign-verdict.dbrec`
//! (`CAMPAIGN_NOT_READY` without a reviewed, complete manifest of this
//! build and configuration committed before the campaign's attempts). A
//! producer timing verdict `<attempt id>.dbrec` in `--witness-dir` (from
//! `g08_producer_timing`) gives an attempt's timing; without one it is
//! UNKNOWN. Exit 0 unless the campaign is not ready. `live
//! campaign-precommit` commits a reviewed manifest's hash once, before the
//! campaign's first attempt.
//!
//! `live lot` (REV.1.0 R3) writes the operator's decision of a lot (its
//! BUY's hash) as a ledger fact, once per request ID: `resume` lets its
//! stopped exits go on and its curve be read again, `write-off` ends it as
//! lost (its input and gas; no exit, no BUY held back by it; nothing of
//! its exit out), `exit-now` wants its exit now (`BSC_EXEC_LIVE_OPERATOR_EXIT`
//! ON). `live nonce-burn` (R3b) writes the operator's wish to burn the
//! nonce of an unresolved transaction whose signed bytes may still land
//! (its block left the chain, or its window is unknown), once per request
//! ID (`BSC_EXEC_LIVE_NONCE_BURN` ON): the armed runtime sends the buyer's
//! plain transfer of nothing to itself at that nonce, as an exit step
//! goes; one that never landed is not sent again at the same gas price
//! (NONCE_BURN_SPENT; another goes once `BSC_EXEC_GAS_PRICE_WEI` and
//! `BSC_STRATEGY_C_GAS_PRICE_WEI` change together). `live adopt-g07` takes a G0.7 BUY's acquisition from its G0.8
//! result and chain capture as an included lot of the LIVE buyer (its
//! launch's block final). The running runtime acts on both at its next
//! reconcile. `live ledger check` decodes every LIVE ledger entry on its
//! own (exit 0 only when each does); `live ledger quarantine` moves one that
//! does not decode out, once per request (its record first): the ledger
//! reads again, but what the entry said is the operator's reconstruction
//! (a launch's C3 BUY has no admission of its own); the BUYs of every grant
//! written before stay refused and no BUY attempt's slot goes back.
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
        exit_only: rest.iter().any(|a| a == "--exit-only"),
        // BSC3-069: the execution risk the operator grants, every bound.
        risk: Some(darkbot_bsc_execution::sources::GrantRisk {
            max_buy_slippage_bps: u16::try_from(number("--max-buy-slippage-bps")?)
                .map_err(|_| "--max-buy-slippage-bps out of range")?,
            max_exit_slippage_bps: u16::try_from(number("--max-exit-slippage-bps")?)
                .map_err(|_| "--max-exit-slippage-bps out of range")?,
            max_gas_price_wei: u64::try_from(number("--max-gas-price-wei")?)
                .map_err(|_| "--max-gas-price-wei out of range")?,
            max_buy_gas_limit: u64::try_from(number("--max-buy-gas-limit")?)
                .map_err(|_| "--max-buy-gas-limit out of range")?,
            max_approve_gas_limit: u64::try_from(number("--max-approve-gas-limit")?)
                .map_err(|_| "--max-approve-gas-limit out of range")?,
            max_sell_gas_limit: u64::try_from(number("--max-sell-gas-limit")?)
                .map_err(|_| "--max-sell-gas-limit out of range")?,
        }),
        // 31.10. order: the positions and losses the operator grants.
        limits: Some(darkbot_bsc_execution::sources::GrantLimits {
            max_open_positions: u32::try_from(number("--max-open-positions")?)
                .map_err(|_| "--max-open-positions out of range")?,
            max_realized_loss_wei: number("--max-realized-loss-wei")?,
            max_daily_realized_loss_wei: number("--max-daily-realized-loss-wei")?,
            min_buy_interval_ms: u64::try_from(number("--min-buy-interval-ms")?)
                .map_err(|_| "--min-buy-interval-ms out of range")?,
        }),
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

/// 64 lowercase hex digits as 32 bytes.
fn hex32(value: &str) -> Option<[u8; 32]> {
    // The digits checked first: slicing never meets a multi-byte character.
    (value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    .then(|| {
        (0..32)
            .map(|i| u8::from_str_radix(&value[2 * i..2 * i + 2], 16).ok())
            .collect::<Option<Vec<u8>>>()
    })
    .flatten()
    .and_then(|v| <[u8; 32]>::try_from(v).ok())
}

/// The SHA-256 `name` names: 64 lowercase hex digits (from `live scope`).
fn sha256_flag(rest: &[String], name: &str) -> Result<[u8; 32], String> {
    hex32(flag(rest, name)?).ok_or(format!(
        "{name} is 64 lowercase hex digits (from `live scope`)"
    ))
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
    // BSC3-012: the release binary of `bsc_runtime` this qualification
    // qualifies; the LIVE path arms only in it.
    let runtime = flag(rest, "--runtime-binary")?;
    let bytes = std::fs::read(runtime).map_err(|e| format!("--runtime-binary {runtime}: {e}"))?;
    spec.runtime_sha256 = Some(darkbot_bsc_record::sha256(&bytes));
    spec.acquisition = match flags(rest, "--acquisition").first() {
        None => None,
        Some(v) => {
            let (g08, g07) = v
                .split_once('@')
                .ok_or("--acquisition G08_RESULT@G07_ATTEMPT")?;
            Some((g08.to_string(), g07.to_string()))
        }
    };
    if let Some(by) = flags(rest, "--risk-by").first() {
        let evidence = flag(rest, "--risk-evidence")?
            .split(',')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        spec.risk = Some(darkbot_bsc_execution::live_qualification::RiskSpec {
            decided_by: by.to_string(),
            decided_unix_ms: now,
            statement: flag(rest, "--risk-statement")?.into(),
            // BSC3-049: the grant risk scope it is decided for.
            scope_sha256: sha256_flag(rest, "--risk-scope-sha256")?,
            evidence,
        });
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
            "qualification=VERIFIED sha256={} runtime={} venues={} sources={} adapters={} acquisition={} risk_decision={} risk_scope={}",
            hex(&v.sha256),
            v.runtime_sha256.map_or("NONE".to_string(), |b| hex(&b)),
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
            v.view
                .risk_scope_sha256
                .map_or("NONE".to_string(), |b| hex(&b)),
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
            println!("risk_scope_sha256={}", hex(&grant.risk_scope_sha256()));
            Ok(true)
        }
        [area, cmd, rest @ ..] if area == "live" && cmd == "grant" => {
            let grant = grant_args(rest)?;
            let digest = sha256_flag(rest, "--scope-sha256")?;
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
        // 01.11. (R1): the files the LIVE qualification in `--root` binds
        // (the record first): what a runtime snapshot carries, one per line.
        [area, cmd, rest @ ..] if area == "live" && cmd == "bound-inputs" => {
            let root = std::path::Path::new(flag(rest, "--root")?);
            for path in darkbot_bsc_execution::live_qualification::bound_inputs(root)? {
                println!("{path}");
            }
            Ok(true)
        }
        // BSC3-026: the loopback stand-in's own test identity, drawn in a
        // test's owner state (never the account's), outside `--root`.
        // Prints its address only.
        [area, cmd, sub, rest @ ..]
            if area == "live" && cmd == "loopback-identity" && (sub == "init" || sub == "show") =>
        {
            use darkbot_bsc_execution::live_key::{create_loopback_identity, loopback_identity};
            // An existing state only (its real place is what is refused).
            let state = flag(rest, "--state")?;
            let state = std::fs::canonicalize(state).map_err(|e| format!("{state}: {e}"))?;
            let state = state.as_path();
            let root = std::path::Path::new(flag(rest, "--root")?);
            let address = if sub == "init" {
                let refused = darkbot_bsc_execution::runtime::live::loopback_refused(
                    state,
                    darkbot_bsc_execution::sources::canonical_state_dir(),
                );
                create_loopback_identity(state, root, refused)?
            } else {
                darkbot_bsc_execution::paper::PaperIdentity::load(&loopback_identity(state), root, &[])
                    .map_err(|e| e.to_string())?
                    .address()
            };
            println!("loopback_identity=0x{}", hex(&address));
            Ok(true)
        }
        // 01.11. (R1): whether nothing LIVE holds in `--state`: no grant, no
        // position pending, open or exiting, no transaction unresolved (a
        // restart of the runtime then ends nothing); refused with why.
        [area, cmd, rest @ ..] if area == "live" && cmd == "idle" => {
            use darkbot_bsc_execution::live_ledger::{Ledger, PositionState, fold};
            let dir = std::path::Path::new(flag(rest, "--state")?);
            let g = Store::open(dir, KEEP_GENERATIONS)?.current()?;
            let book = fold(&Ledger::open(dir).facts()?);
            let held = book
                .positions
                .iter()
                .filter(|p| {
                    matches!(
                        p.state,
                        PositionState::Pending | PositionState::Open | PositionState::Exiting
                    )
                })
                .count();
            let unresolved = book.unresolved().count();
            let grant = g.live.is_some();
            println!("live_grant={grant} positions_held={held} unresolved={unresolved}");
            if grant || held > 0 || unresolved > 0 {
                return Err(format!(
                    "LIVE_ACTIVE: grant={grant} positions_held={held} unresolved={unresolved}"
                ));
            }
            Ok(true)
        }
        // Plan 28.10.: the LIVE ledger's positions and net result.
        [area, cmd, rest @ ..] if area == "live" && cmd == "attempts" => {
            use darkbot_bsc_execution::live_attempts::{CAMPAIGN_NOT_READY, Manifest, evaluate};
            let config_path = flag(rest, "--config")?;
            let text = std::fs::read_to_string(config_path).map_err(|e| format!("{config_path}: {e}"))?;
            let config = RuntimeConfig::parse(&text)?;
            let manifest = flags(rest, "--manifest").first().map(|m| {
                std::fs::read(m)
                    .map_err(|e| format!("{m}: {e}"))
                    .and_then(|b| Manifest::parse(&b))
            });
            let witness_dir = flags(rest, "--witness-dir").first().copied().map(std::path::Path::new);
            let e = evaluate(
                std::path::Path::new(flag(rest, "--rundir")?),
                std::path::Path::new(flag(rest, "--state")?),
                journal_max_payload(&config),
                manifest.as_ref(),
                witness_dir,
                &config
                    .exec
                    .as_ref()
                    .map(|e| e.refused_accounts.clone())
                    .unwrap_or_default(),
                config
                    .canonical
                    .as_ref()
                    .map_or(0, |c| c.limits.stall_after.as_millis() as u64)
                    .saturating_add(config.stop_timeout_ms),
                darkbot_bsc_execution::runtime_journal::own_binary_sha256(),
                config.source_sha256,
                darkbot_bsc_record::observed_now()?,
            )?;
            for a in &e.attempts {
                println!("{}", a.line());
            }
            print!("{}", e.campaign.summary());
            Ok(e.campaign.verdict != CAMPAIGN_NOT_READY)
        }
        [area, cmd, rest @ ..] if area == "live" && cmd == "campaign-precommit" => {
            use darkbot_bsc_execution::live_attempts::{Manifest, Precommit};
            let path = flag(rest, "--manifest")?;
            let manifest = Manifest::parse(&std::fs::read(path).map_err(|e| format!("{path}: {e}"))?)?;
            let p = Precommit::write(
                std::path::Path::new(flag(rest, "--rundir")?),
                &manifest,
                flag(rest, "--decided-by")?,
                darkbot_bsc_record::observed_now()?,
            )?;
            println!(
                "LIVE campaign manifest sha256 {} committed by {} at {} ms",
                darkbot_bsc_record::hex(&p.manifest_sha256),
                p.decided_by,
                p.committed_unix_ms
            );
            Ok(true)
        }
        // REV.1.0 R3: the operator's decision of a lot (its BUY's hash) as
        // a ledger fact, once per request; the runtime acts on it.
        [area, cmd, what, rest @ ..] if area == "live" && cmd == "lot" => {
            use darkbot_bsc_execution::live_ledger::{Ledger, LotDecision};
            let what = LotDecision::parse(what).ok_or("live lot resume|write-off|exit-now")?;
            let lot = flag(rest, "--lot")?;
            let lot = hex32(lot.strip_prefix("0x").unwrap_or(lot))
                .ok_or("--lot is the lot's BUY hash, 0x and 64 lowercase hex digits")?;
            let config_path = flag(rest, "--config")?;
            let text = std::fs::read_to_string(config_path).map_err(|e| format!("{config_path}: {e}"))?;
            let config = RuntimeConfig::parse(&text)?;
            let operator_exit = config.exec.as_ref().is_some_and(|x| x.exec.live_operator_exit);
            let written = Ledger::open(std::path::Path::new(flag(rest, "--state")?)).decide(
                lot,
                what,
                flag(rest, "--request-id")?,
                flag(rest, "--decided-by")?,
                operator_exit,
                darkbot_bsc_record::observed_now()?,
            )?;
            match written {
                Some(seq) => println!("lot=0x{} decision={what:?} fact={seq}", hex(&lot)),
                None => println!("lot=0x{} decision={what:?} written before", hex(&lot)),
            }
            Ok(true)
        }
        // REV.1.0 R3b: the operator's nonce burn of an unresolved
        // transaction (its hash) as a ledger fact, once per request; the
        // armed runtime burns the nonce.
        [area, cmd, rest @ ..] if area == "live" && cmd == "nonce-burn" => {
            use darkbot_bsc_execution::live_ledger::Ledger;
            let tx = flag(rest, "--tx")?;
            let tx = hex32(tx.strip_prefix("0x").unwrap_or(tx))
                .ok_or("--tx is the transaction's hash, 0x and 64 lowercase hex digits")?;
            let config_path = flag(rest, "--config")?;
            let text = std::fs::read_to_string(config_path).map_err(|e| format!("{config_path}: {e}"))?;
            let config = RuntimeConfig::parse(&text)?;
            let nonce_burn = config.exec.as_ref().is_some_and(|x| x.exec.live_nonce_burn);
            let buyer = config.live.as_ref().map(|l| l.buyer);
            let written = Ledger::open(std::path::Path::new(flag(rest, "--state")?)).request_burn(
                tx,
                buyer,
                flag(rest, "--request-id")?,
                flag(rest, "--decided-by")?,
                nonce_burn,
                darkbot_bsc_record::observed_now()?,
            )?;
            match written {
                Some(seq) => println!("tx=0x{} burn requested fact={seq}", hex(&tx)),
                None => println!("tx=0x{} burn requested before", hex(&tx)),
            }
            Ok(true)
        }
        // REV.1.0 R3: a G0.7 BUY's acquisition (its G0.8 result and that
        // result's chain capture) as an included lot of the LIVE buyer.
        [area, cmd, rest @ ..] if area == "live" && cmd == "adopt-g07" => {
            use darkbot_bsc_execution::live_ledger::{Ledger, g07_buy};
            let config_path = flag(rest, "--config")?;
            let text = std::fs::read_to_string(config_path).map_err(|e| format!("{config_path}: {e}"))?;
            let config = RuntimeConfig::parse(&text)?;
            let buyer = config.live.as_ref().ok_or("BSC_LIVE_BUYER is not configured")?.buyer;
            let finality = config
                .canonical
                .as_ref()
                .ok_or("BSC_CANONICAL_RPC is not configured")?
                .limits
                .reorg_depth;
            let path = flag(rest, "--g08")?;
            let result = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            let capture_path = flag(rest, "--capture")?;
            let capture = std::fs::read(capture_path).map_err(|e| format!("{capture_path}: {e}"))?;
            let buy = g07_buy(&result, &capture, buyer, finality)?;
            let written = Ledger::open(std::path::Path::new(flag(rest, "--state")?)).adopt(
                &buy,
                darkbot_bsc_execution::wss::BSC_MAINNET_CHAIN_ID,
                flag(rest, "--request-id")?,
                flag(rest, "--decided-by")?,
                darkbot_bsc_record::observed_now()?,
            )?;
            println!(
                "lot=0x{} token=0x{} tokens={} input_wei={} gas_wei={} evidence_sha256={} {}",
                hex(&buy.tx_hash),
                hex(&buy.token),
                buy.tokens,
                buy.value_wei,
                buy.gas_wei,
                hex(&buy.evidence),
                written.map_or("written before".into(), |seq| format!("fact={seq}")),
            );
            Ok(true)
        }
        // REV.1.0 R3 (BSC3-045): every entry of the LIVE ledger decoded on
        // its own; exit 0 only when each one does.
        [area, cmd, sub, rest @ ..] if area == "live" && cmd == "ledger" && sub == "check" => {
            use darkbot_bsc_execution::live_ledger::Ledger;
            let ledger = Ledger::open(std::path::Path::new(flag(rest, "--state")?));
            let (entries, bad) = ledger.check()?;
            println!(
                "entries={entries} unreadable={} quarantined_at_generation={}",
                bad.len(),
                ledger.quarantined()?.map_or("-".into(), |g| g.to_string())
            );
            for (seq, why) in &bad {
                println!("unreadable seq={seq} why={why}");
            }
            Ok(bad.is_empty())
        }
        // R3 (BSC3-045): the operator moves an entry that does not decode
        // out of the ledger, once per request; BUYs under the grants
        // written before stay refused.
        [area, cmd, sub, rest @ ..] if area == "live" && cmd == "ledger" && sub == "quarantine" => {
            use darkbot_bsc_execution::live_ledger::Ledger;
            let dir = std::path::Path::new(flag(rest, "--state")?);
            let seq: u64 = flag(rest, "--seq")?
                .parse()
                .map_err(|_| "--seq is the entry's sequence number")?;
            let generation = Store::open(dir, KEEP_GENERATIONS)?.current()?.generation;
            Ledger::open(dir).quarantine(
                seq,
                flag(rest, "--request-id")?,
                flag(rest, "--decided-by")?,
                generation,
                darkbot_bsc_record::observed_now()?,
            )?;
            println!("seq={seq} quarantined at generation {generation}");
            Ok(true)
        }
        [area, cmd, rest @ ..] if area == "live" && cmd == "positions" => {
            use darkbot_bsc_execution::live_ledger::{Ledger, fold};
            let dir = std::path::Path::new(flag(rest, "--state")?);
            let book = fold(&Ledger::open(dir).facts()?);
            for p in &book.positions {
                println!(
                    "lot=0x{} token=0x{} component={} state={} tokens={} input_wei={} proceeds_wei={} gas_wei={} pnl_wei={}",
                    hex(&p.lot),
                    hex(&p.token),
                    p.component.name(),
                    p.state.name(),
                    p.tokens,
                    p.input_wei,
                    p.proceeds_wei,
                    p.gas_wei,
                    p.pnl_wei.map_or("-".into(), |v| v.to_string()),
                );
            }
            for t in book.unresolved() {
                println!(
                    "unresolved tx=0x{} intent={} state={}",
                    hex(&t.released.tx_hash),
                    t.released.intent.name(),
                    t.state.name()
                );
            }
            println!(
                "realized_pnl_wei={} gas_wei={}",
                book.realized_pnl_wei, book.gas_wei
            );
            Ok(true)
        }
        // The LIVE gate as it holds now: every missing condition, then
        // what each LIVE request is now.
        [area, cmd, rest @ ..] if area == "live" && cmd == "status" => {
            use darkbot_bsc_execution::sources::{
                LiveContext, current_generation, reported_held_attempt, spent_by_grant,
                usage_besides,
            };
            let dir = std::path::Path::new(flag(rest, "--state")?);
            let now = darkbot_bsc_record::observed_now()?;
            let g = current_generation(dir)?;
            // BSC3-078: besides the attempt the running LIVE path holds, as
            // its status in --rundir reports it (as the tabs read it).
            let held = flags(rest, "--rundir").first().and_then(|r| {
                reported_held_attempt(
                    &std::path::Path::new(r).join("bsc-runtime/status.dbrec"),
                    &g,
                    now,
                )
            });
            let (daily, total) = usage_besides(dir, now, held.as_deref());
            let daily = daily.ok_or("BUY_ATTEMPT_LIMIT_UNREADABLE")?;
            let total = total.ok_or("BUY_ATTEMPT_LIMIT_UNREADABLE")?;
            let boot = darkbot_bsc_execution::sources::boot_session().ok();
            let qualification = print_qualification(rest);
            // What the grant on file has spent (its admitted intents).
            let spent = match &g.live_set_by {
                Some(by) => spent_by_grant(dir, by.generation).ok(),
                None => Some(0),
            };
            let ctx = LiveContext {
                boot: boot.as_deref(),
                now_unix_ms: now,
                daily: Some(&daily),
                total: Some(&total),
                spent_wei: spent,
                qualification: &qualification,
            };
            println!("generation={}", g.generation);
            match &g.live {
                Some(grant) => {
                    print!("{}", grant.scope_text());
                    println!("scope_sha256={}", hex(&grant.scope_sha256()));
                    println!("risk_scope_sha256={}", hex(&grant.risk_scope_sha256()));
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
            match spent {
                Some(wei) => println!("spent_wei={wei}"),
                None => println!("spent_wei=UNREADABLE"),
            }
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
        [cmd, rest @ ..] if cmd == "g04-precommit" => {
            let criteria_path = flag(rest, "--criteria")?;
            let criteria = g04_criteria::Criteria::parse(
                &std::fs::read(criteria_path).map_err(|e| format!("{criteria_path}: {e}"))?,
            )?;
            let p = g04_criteria::Precommit::write(
                std::path::Path::new(flag(rest, "--rundir")?),
                &criteria,
                flag(rest, "--decided-by")?,
                darkbot_bsc_record::observed_now()?,
            )?;
            println!(
                "G0.4 criteria sha256 {} committed by {} at {} ms",
                darkbot_bsc_record::hex(&p.criteria_sha256),
                p.decided_by,
                p.committed_unix_ms
            );
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
            let verdict_path = rundir.join("bsc-runtime/g04-verdict.dbrec");
            // An unreadable pre-commitment leaves no older verdict standing.
            let precommit = g04_criteria::Precommit::read(rundir).inspect_err(|_| {
                let _ = std::fs::remove_file(&verdict_path);
            })?;
            let census = write_census(&config, rundir, None)?;
            let now = darkbot_bsc_record::observed_now()?;
            let verdict = g04_criteria::evaluate(&criteria, precommit.as_ref(), &census, now);
            replay_proof::write(&verdict_path, &verdict.encode(now)?)?;
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
        // The owner state on file (generation 0 if none): a running owner's
        // switches and grant stay (01.11., R1: `darkbotctl up` and the app
        // ran a reset here and revoked a running LIVE path).
        [area, cmd, rest @ ..] if area == "source" && cmd == "init" => {
            let store = Store::open(flag(rest, "--state")?, KEEP_GENERATIONS)?;
            let g = store.init(darkbot_bsc_record::observed_now()?)?;
            println!("generation={}", g.generation);
            Ok(true)
        }
        // The owner's reset (a stop, plan 27.10.): every switch DISABLED,
        // the grant ended.
        [area, cmd, rest @ ..] if area == "source" && cmd == "reset" => {
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
            let mut receipt = store.request(flag(rest, "--request-id")?, source, mode, expected, now)?;
            // Plan 28.10.: a requested LIVE as the owner's LIVE gate decides it
            // now (the qualification in --root; without it none verifies),
            // the same decision the runtime and every tab read: besides the
            // attempt the running LIVE path holds, as its status in --rundir
            // reports it (BSC3-078).
            if mode == Mode::Live {
                use darkbot_bsc_execution::sources::{GateReads, reported_held_attempt};
                use darkbot_bsc_execution::live_qualification;
                let dir = std::path::Path::new(flag(rest, "--state")?);
                let g = darkbot_bsc_execution::sources::current_generation(dir)?;
                if g.generation == receipt.generation {
                    let verified = flags(rest, "--root")
                        .first()
                        .map(|root| live_qualification::view(std::path::Path::new(root)).0.view)
                        .unwrap_or_default();
                    let held = flags(rest, "--rundir").first().and_then(|r| {
                        reported_held_attempt(
                            &std::path::Path::new(r).join("bsc-runtime/status.dbrec"),
                            &g,
                            now,
                        )
                    });
                    let reads = GateReads::read_holding(dir, &g, now, held.as_deref());
                    receipt.effective = g.effective_live_now(source, &reads.ctx(&verified));
                }
            }
            println!("{}", receipt.line());
            Ok(receipt.applied())
        }
        _ => Err("usage: bsc_runtime run --sources DIR --rundir DIR --config FILE | source init|reset|show --state DIR | source request ID MODE --expect-generation N --request-id ID --state DIR [--root DIR] [--rundir DIR] | live scope|grant --venue V --strategies S --sources A,B --adapters X --max-input-wei N --max-spend-wei N --max-attempts N --expires-unix-ms N --max-buy-slippage-bps N --max-exit-slippage-bps N --max-gas-price-wei N --max-buy-gas-limit N --max-approve-gas-limit N --max-sell-gas-limit N --max-open-positions N --max-realized-loss-wei N --max-daily-realized-loss-wei N --min-buy-interval-ms N [--exit-only] [--scope-sha256 H --expect-generation N --request-id ID --state DIR] | live revoke --expect-generation N --request-id ID --state DIR | live qualify --root DIR --runtime-binary FILE [--venue V@EV,..] [--source S@EV,..] [--adapter A@FINDING,..@EV,..] [--acquisition G08@G07] [--risk-by WHO --risk-statement TEXT --risk-scope-sha256 H --risk-evidence EV,..] [--out FILE] | live bound-inputs --root DIR | live idle --state DIR | live status --state DIR [--root DIR] [--rundir DIR] | live loopback-identity init|show --state DIR --root DIR | live positions --state DIR | live lot resume|write-off|exit-now --lot H --state DIR --config FILE --request-id ID --decided-by WHO | live nonce-burn --tx H --state DIR --config FILE --request-id ID --decided-by WHO | live adopt-g07 --g08 FILE --capture FILE --state DIR --config FILE --request-id ID --decided-by WHO | live ledger check --state DIR | live ledger quarantine --seq N --state DIR --request-id ID --decided-by WHO | control require [--state DIR] SWITCH_ID... | census --rundir DIR --config FILE | g04-precommit --rundir DIR --criteria FILE --decided-by WHO | g04-verdict --rundir DIR --config FILE --criteria FILE | replay-proof|capture-quality|strategy-replay --rundir DIR --config FILE [--run ID] | capture-preflight --rundir DIR --config FILE | paper-identity init|show --config FILE".into()),
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
    // BSC3-012 review: this process's build, read before anything else.
    let _ = darkbot_bsc_execution::live_qualification::running_build();
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
