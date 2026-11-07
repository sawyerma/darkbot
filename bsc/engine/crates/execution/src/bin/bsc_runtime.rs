//! The BSC runtime: the observation process, its PAPER and (under an
//! operator's verified grant) LIVE paths, and the native owner of the
//! source modes, switches and the LIVE grant.
//!
//! ```text
//! bsc_runtime run --sources <dir> --rundir <dir> --config <ingress.public.env>
//! bsc_runtime source init|reset --state <dir>
//! bsc_runtime source show --state <dir> --config <ingress.public.env>
//! bsc_runtime source request <SOURCE_COMPONENT_OR_VENUE_ID> <OFF|SHADOW|LIVE> --expect-generation <n> --request-id <id> --state <dir> --config <ingress.public.env>
//! bsc_runtime source request <WALLET_OR_ECONOMIC_SWITCH_ID> <DISABLED|ENABLED> --expect-generation <n> --request-id <id> --state <dir> --config <ingress.public.env>
//! bsc_runtime strategy show --state <dir> --config <ingress.public.env>
//! bsc_runtime strategy set <BSC_STRATEGY_C_KEY> <VALUE> --expect-generation <n> --request-id <id> --state <dir> --config <ingress.public.env>
//! bsc_runtime strategy set-all <BSC_STRATEGY_C_KEY>=<VALUE>... --expect-generation <n> --request-id <id> --state <dir> --config <ingress.public.env>
//! bsc_runtime control require [--state <dir>] <SWITCH_ID>...
//! bsc_runtime live scope|grant|revoke|qualify|status|positions|idle|bound-inputs|loopback-identity ...
//! bsc_runtime live key init --key <file> --root <repo>
//! bsc_runtime config-check --config <ingress.public.env>
//! bsc_runtime live attempts --rundir <dir> --state <dir> --config <ingress.public.env> [--manifest <live_campaign.env>] [--witness-dir <dir>]
//! bsc_runtime live campaign-precommit --rundir <dir> --manifest <live_campaign.env> --decided-by <who>
//! bsc_runtime live lot resume|write-off|exit-now --lot <0x hash> --expect-version <n> --state <dir> --config <ingress.public.env> --request-id <id> --decided-by <who>
//! bsc_runtime live nonce-burn --tx <0x hash> --expect-version <n> --state <dir> --config <ingress.public.env> --request-id <id> --decided-by <who>
//! bsc_runtime live adopt-g07 --g08 <result.dbrec> --capture <chain journal> --state <dir> --config <ingress.public.env> --request-id <id> --decided-by <who>
//! bsc_runtime live ledger check --state <dir>
//! bsc_runtime live ledger quarantine --seq <n> --expect-generation <n> --state <dir> --request-id <id> --decided-by <who>
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
//! reconcile. `live lot` and `live nonce-burn` expect the ledger's version
//! (`--expect-version`, its next entry's number; the LIVE tab's from R3b-2b-2),
//! `live ledger quarantine` the owner generation (`--expect-generation`):
//! their compare-and-swap, checked under the ledger's operator lock after
//! the request's own replay (LIVE_REQUEST_STALE otherwise, a ledger
//! refusal on stderr as the others). Each answers with one
//! `outcome=APPLIED` line (the gateway's receipt; `replay=true` for the
//! same request again). `live ledger check` decodes every LIVE ledger entry on its
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
use darkbot_bsc_execution::sources::{Entity, Mode, OwnerLimits, Store, SwitchId};
use darkbot_bsc_execution::{capture_preflight, capture_quality, g04_criteria, replay_proof};
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use std::process::ExitCode;

/// Audit E 3.1/3.4: the owner's bounds of the runtime configuration
/// `--config` (the one the runtime runs with).
fn owner_limits(args: &[String]) -> Result<OwnerLimits, String> {
    OwnerLimits::at(std::path::Path::new(flag(args, "--config")?))
}

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
            // R4 (BSC3-069): the operator confirms an outlier risk.
            outlier_ack: rest.iter().any(|a| a == "--risk-outlier-ack"),
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

/// R3b: the unsigned number an operator's ledger request expects
/// (`--expect-version` or `--expect-generation`: its compare-and-swap).
fn expected(rest: &[String], name: &str) -> Result<u64, String> {
    flag(rest, name)?
        .parse::<u64>()
        .map_err(|_| format!("{name} is an unsigned integer"))
}

/// An operator's fact as its receipt names it: its sequence number at the
/// ledger's version it expected, or the same request again (written
/// before; review R2: then no version of this call).
fn written_as(seq: Option<u64>, version: u64) -> String {
    seq.map_or("replay=true".into(), |seq| {
        format!("fact={seq} version={version}")
    })
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

/// The repository whose bsc/config/ holds the configuration at
/// `config_path`: its keys (the PAPER identity, the LIVE key) live outside
/// it.
fn repo_root_of(config_path: &str) -> Result<std::path::PathBuf, String> {
    Ok(std::path::Path::new(config_path)
        .canonicalize()
        .map_err(|e| format!("{config_path}: {e}"))?
        .ancestors()
        .nth(3)
        .ok_or("--config is not under <repo>/bsc/config/")?
        .to_path_buf())
}

fn run(args: &[String]) -> Result<bool, String> {
    match args {
        // Plan 28.10.: the canonical scope text of a LIVE grant and its
        // SHA-256, the value an operator confirms with `live grant`.
        [area, cmd, rest @ ..] if area == "live" && cmd == "scope" => {
            let grant = grant_args(rest)?;
            grant.validate_within(owner_limits(rest)?.buy_attempts_total_max)?;
            print!("{}", grant.scope_text());
            println!("scope_sha256={}", hex(&grant.scope_sha256()));
            println!("risk_scope_sha256={}", hex(&grant.risk_scope_sha256()));
            // R9 (BSC3-001): what a presence anchor's key signs for the
            // generation the grant will be written as.
            if rest.iter().any(|a| a == "--expect-generation") {
                let expected = flag(rest, "--expect-generation")?
                    .parse::<u64>()
                    .map_err(|_| "--expect-generation is an unsigned integer")?;
                println!(
                    "presence_digest=0x{}",
                    hex(&darkbot_bsc_execution::sources::presence_digest(
                        &grant,
                        expected + 1
                    ))
                );
            }
            Ok(true)
        }
        [area, cmd, rest @ ..] if area == "live" && cmd == "grant" => {
            let grant = grant_args(rest)?;
            let digest = sha256_flag(rest, "--scope-sha256")?;
            let expected = flag(rest, "--expect-generation")?
                .parse::<u64>()
                .map_err(|_| "--expect-generation is an unsigned integer")?;
            // R9 (BSC3-001): the operator's presence signature, optional.
            let presence = match rest.iter().position(|a| a == "--presence-signature") {
                None => None,
                Some(_) => Some(
                    flag(rest, "--presence-signature")?
                        .strip_prefix("0x")
                        .filter(|h| h.len() == 130)
                        .and_then(|h| alloy_primitives::hex::decode(h).ok())
                        .and_then(|b| <[u8; 65]>::try_from(b).ok())
                        .ok_or("--presence-signature is 0x and 130 hex digits")?,
                ),
            };
            let store = Store::open(flag(rest, "--state")?, owner_limits(rest)?)?;
            let now = darkbot_bsc_record::observed_now()?;
            let signer = presence.and_then(|signature| {
                darkbot_bsc_execution::sources::presence_signer(&grant, expected + 1, &signature)
            });
            let receipt = store.grant_live_present(
                flag(rest, "--request-id")?,
                grant,
                digest,
                presence,
                expected,
                now,
            )?;
            if let Some(signer) = signer {
                println!("presence_signer=0x{}", hex(&signer));
            }
            println!("{}", receipt.line());
            Ok(receipt.applied())
        }
        [area, cmd, rest @ ..] if area == "live" && cmd == "revoke" => {
            let expected = flag(rest, "--expect-generation")?
                .parse::<u64>()
                .map_err(|_| "--expect-generation is an unsigned integer")?;
            // Review E3 M1: a revocation never depends on the configuration.
            let store = Store::open_reducing(flag(rest, "--state")?)?;
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
        // `--root`/bsc/config/live_qualification.dbrec (or `--out`). R10:
        // `--replace` replaces an existing one atomically (a requalification
        // per build) and names the record it replaced.
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
            if rest.iter().any(|a| a == "--replace") {
                let before = match std::fs::read(&out) {
                    Ok(old) => hex(&darkbot_bsc_record::sha256(&old)),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => "NONE".into(),
                    Err(e) => return Err(format!("{}: {e}", out.display())),
                };
                darkbot_bsc_execution::runtime::replace(&out, &bytes)?;
                println!("replaced_sha256={before}");
            } else {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&out)
                    .and_then(|mut f| f.write_all(&bytes).and_then(|()| f.sync_all()))
                    .map_err(|e| format!("{}: {e}", out.display()))?;
            }
            println!("written={} sha256={}", out.display(), hex(&darkbot_bsc_record::sha256(&bytes)));
            Ok(true)
        }
        // REV.1.0 R10 (provisioning): the LIVE buyer's key, drawn from the
        // operating system's randomness into a new private file outside
        // `--root` (never over an existing one). Prints its address only:
        // BSC_LIVE_KEY is the path, BSC_LIVE_BUYER (and one of
        // BSC_STATE_WATCHED) the address. The wallet is funded by the
        // operator; nothing here signs or sends.
        [area, cmd, sub, rest @ ..] if area == "live" && cmd == "key" && sub == "init" => {
            let address = darkbot_bsc_execution::paper::create_private_key(
                std::path::Path::new(flag(rest, "--key")?),
                std::path::Path::new(flag(rest, "--root")?),
            )
            .map_err(|e| darkbot_bsc_execution::live_key::refused_as("LIVE key", e))?;
            println!("live_buyer=0x{}", hex(&address));
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
            let g = darkbot_bsc_execution::sources::current_generation(dir)?;
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
            let version = expected(rest, "--expect-version")?;
            let request = flag(rest, "--request-id")?;
            let written = Ledger::open(std::path::Path::new(flag(rest, "--state")?)).decide(
                lot,
                what,
                request,
                flag(rest, "--decided-by")?,
                operator_exit,
                version,
                darkbot_bsc_record::observed_now()?,
            )?;
            println!(
                "outcome=APPLIED lot=0x{} decision={} {} request_id={request}",
                hex(&lot),
                what.kind().name(),
                written_as(written, version)
            );
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
            let version = expected(rest, "--expect-version")?;
            let request = flag(rest, "--request-id")?;
            let written = Ledger::open(std::path::Path::new(flag(rest, "--state")?)).request_burn(
                tx,
                buyer,
                request,
                flag(rest, "--decided-by")?,
                nonce_burn,
                version,
                darkbot_bsc_record::observed_now()?,
            )?;
            println!(
                "outcome=APPLIED tx=0x{} decision={} {} request_id={request}",
                hex(&tx),
                darkbot_bsc_execution::live_ledger::FactKind::BurnRequested.name(),
                written_as(written, version)
            );
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
            let request = flag(rest, "--request-id")?;
            let generation = Ledger::open(dir).quarantine(
                seq,
                request,
                flag(rest, "--decided-by")?,
                expected(rest, "--expect-generation")?,
                || Ok(darkbot_bsc_execution::sources::current_generation(dir)?.generation),
                darkbot_bsc_record::observed_now()?,
            )?;
            println!(
                "outcome=APPLIED seq={seq} decision=QUARANTINED generation={generation} request_id={request}"
            );
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
            let max = owner_limits(rest)?.buy_attempts_total_max;
            let (daily, total) = usage_besides(dir, now, held.as_deref(), max);
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
            let repo_root = repo_root_of(config_path)?;
            run_runtime(
                &config,
                std::path::Path::new(flag(rest, "--sources")?),
                rundir,
                &repo_root,
            )?;
            Ok(true)
        }
        // REV.1.0 R10: the configuration as `run` takes it, and the LIVE
        // path's files as its arm takes them, by place, owner, mode, links
        // and length (neither the key nor the authorization is read).
        // darkbotctl runs it before it starts or restarts the runtime: a
        // refused start would be restarted by launchd again and again.
        [cmd, rest @ ..] if cmd == "config-check" => {
            let config_path = flag(rest, "--config")?;
            let text = std::fs::read_to_string(config_path).map_err(|e| format!("{config_path}: {e}"))?;
            let config = RuntimeConfig::parse(&text)?;
            let repo_root = repo_root_of(config_path)?;
            let sha256 = hex(&darkbot_bsc_record::sha256(text.as_bytes()));
            match &config.live {
                None => println!("config=OK sha256={sha256} live=OFF"),
                Some(live) => {
                    use darkbot_bsc_execution::runtime::live::LiveRouteConfig;
                    darkbot_bsc_execution::live_key::check_files(live, &repo_root)?;
                    let route = match &live.route {
                        LiveRouteConfig::Builders { blockrazor_token: None } => "BUILDERS".into(),
                        LiveRouteConfig::Builders { blockrazor_token: Some(_) } => {
                            "BUILDERS blockrazor_token=OK".into()
                        }
                        LiveRouteConfig::Loopback { port } => format!("LOOPBACK:{port}"),
                    };
                    println!(
                        "config=OK sha256={sha256} live={route} buyer=0x{} key_file=OK",
                        hex(&live.buyer)
                    );
                }
            }
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
                Some(&text),
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
            let repo_root = repo_root_of(config_path)?;
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
            let store = Store::open_reducing(flag(rest, "--state")?)?;
            let g = store.init(darkbot_bsc_record::observed_now()?)?;
            println!("generation={}", g.generation);
            Ok(true)
        }
        // The owner's reset (a stop, plan 27.10.): every switch DISABLED,
        // the grant ended.
        [area, cmd, rest @ ..] if area == "source" && cmd == "reset" => {
            let store = Store::open_reducing(flag(rest, "--state")?)?;
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
            let limits = owner_limits(rest)?;
            let store = Store::open(flag(rest, "--state")?, limits)?;
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
                limits.buy_attempts_total_max,
            )?;
            println!(
                "buy_attempts day_utc={} used={} limit={} remaining={}",
                usage.day.utc,
                usage.used,
                usage.limit,
                usage.remaining()
            );
            let total = darkbot_bsc_execution::sources::total_usage(
                std::path::Path::new(flag(rest, "--state")?),
                limits.buy_attempts_total_max,
            )?;
            println!(
                "buy_attempts_total used={} limit={} remaining={}",
                total.used,
                total.limit,
                total.remaining()
            );
            Ok(true)
        }
        // Frontend package (06.11.): the operator's Strategy C settings,
        // effective at the strategy's next start.
        [area, cmd, rest @ ..] if area == "strategy" && cmd == "show" => {
            use darkbot_bsc_execution::strategy_settings as settings;
            let config_path = flag(rest, "--config")?;
            let text = std::fs::read_to_string(config_path).map_err(|e| format!("{config_path}: {e}"))?;
            let s = settings::current(std::path::Path::new(flag(rest, "--state")?))?;
            println!("generation={}", s.generation);
            for (k, v) in settings::effective_values(&text, &s) {
                let from = if s.value(k).is_some() { "set" } else { "file" };
                println!("{k}={v} from={from}");
            }
            Ok(true)
        }
        [area, cmd, id, value, rest @ ..] if area == "strategy" && cmd == "set" => {
            use darkbot_bsc_execution::strategy_settings as settings;
            let expected = flag(rest, "--expect-generation")?
                .parse::<u64>()
                .map_err(|_| "--expect-generation is an unsigned integer")?;
            let config_path = flag(rest, "--config")?;
            let text = std::fs::read_to_string(config_path).map_err(|e| format!("{config_path}: {e}"))?;
            let owner = settings::Owner::open(std::path::Path::new(flag(rest, "--state")?))?;
            let receipt = owner.set(
                flag(rest, "--request-id")?,
                id,
                value,
                expected,
                &text,
                darkbot_bsc_record::observed_now()?,
            )?;
            println!("{}", receipt.line());
            Ok(receipt.applied())
        }
        [area, cmd, rest @ ..] if area == "strategy" && cmd == "set-all" => {
            // BSC-V8-04: every given key in one generation with one receipt.
            use darkbot_bsc_execution::strategy_settings as settings;
            let given: Vec<&String> = rest.iter().take_while(|a| !a.starts_with("--")).collect();
            // Review: after the pairs only the four flags, each with its value.
            let flags = &rest[given.len()..];
            if flags.len() % 2 != 0
                || flags.chunks(2).any(|f| {
                    !["--expect-generation", "--request-id", "--state", "--config"].contains(&f[0].as_str())
                })
            {
                return Err("strategy set-all: KEY=VALUE... then only --expect-generation, --request-id, --state, --config".into());
            }
            let pairs = given
                .iter()
                .map(|a| a.split_once('=').ok_or(format!("{a}: KEY=VALUE expected")))
                .collect::<Result<Vec<_>, _>>()?;
            let expected = flag(rest, "--expect-generation")?
                .parse::<u64>()
                .map_err(|_| "--expect-generation is an unsigned integer")?;
            let config_path = flag(rest, "--config")?;
            let text = std::fs::read_to_string(config_path).map_err(|e| format!("{config_path}: {e}"))?;
            let owner = settings::Owner::open(std::path::Path::new(flag(rest, "--state")?))?;
            let receipt = owner.set_all(
                flag(rest, "--request-id")?,
                &pairs,
                expected,
                &text,
                darkbot_bsc_record::observed_now()?,
            )?;
            println!("{}", receipt.line());
            Ok(receipt.applied())
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
            let limits = owner_limits(rest)?;
            let store = Store::open(flag(rest, "--state")?, limits)?;
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
                    let reads = GateReads::read_holding(
                        dir,
                        &g,
                        now,
                        held.as_deref(),
                        limits.buy_attempts_total_max,
                    );
                    receipt.effective = g.effective_live_now(source, &reads.ctx(&verified));
                }
            }
            println!("{}", receipt.line());
            Ok(receipt.applied())
        }
        _ => Err("usage: bsc_runtime run --sources DIR --rundir DIR --config FILE | source init|reset --state DIR | source show --state DIR --config FILE | source request ID MODE --expect-generation N --request-id ID --state DIR --config FILE [--root DIR] [--rundir DIR] | strategy show --state DIR --config FILE | strategy set BSC_STRATEGY_C_KEY VALUE --expect-generation N --request-id ID --state DIR --config FILE | strategy set-all BSC_STRATEGY_C_KEY=VALUE... --expect-generation N --request-id ID --state DIR --config FILE | live scope|grant --config FILE --venue V --strategies S --sources A,B --adapters X --max-input-wei N --max-spend-wei N --max-attempts N --expires-unix-ms N --max-buy-slippage-bps N --max-exit-slippage-bps N --max-gas-price-wei N --max-buy-gas-limit N --max-approve-gas-limit N --max-sell-gas-limit N --max-open-positions N --max-realized-loss-wei N --max-daily-realized-loss-wei N --min-buy-interval-ms N [--exit-only] [--risk-outlier-ack] [--scope-sha256 H --expect-generation N --request-id ID --state DIR [--presence-signature S]] | live revoke --expect-generation N --request-id ID --state DIR | live qualify --root DIR --runtime-binary FILE [--venue V@EV,..] [--source S@EV,..] [--adapter A@FINDING,..@EV,..] [--acquisition G08@G07] [--risk-by WHO --risk-statement TEXT --risk-scope-sha256 H --risk-evidence EV,..] [--out FILE] [--replace] | live key init --key FILE --root DIR | config-check --config FILE | live bound-inputs --root DIR | live idle --state DIR | live status --state DIR --config FILE [--root DIR] [--rundir DIR] | live loopback-identity init|show --state DIR --root DIR | live positions --state DIR | live lot resume|write-off|exit-now --lot H --expect-version N --state DIR --config FILE --request-id ID --decided-by WHO | live nonce-burn --tx H --expect-version N --state DIR --config FILE --request-id ID --decided-by WHO | live adopt-g07 --g08 FILE --capture FILE --state DIR --config FILE --request-id ID --decided-by WHO | live ledger check --state DIR | live ledger quarantine --seq N --expect-generation N --state DIR --request-id ID --decided-by WHO | control require [--state DIR] SWITCH_ID... | census --rundir DIR --config FILE | g04-precommit --rundir DIR --criteria FILE --decided-by WHO | g04-verdict --rundir DIR --config FILE --criteria FILE | replay-proof|capture-quality|strategy-replay --rundir DIR --config FILE [--run ID] | capture-preflight --rundir DIR --config FILE | paper-identity init|show --config FILE".into()),
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
