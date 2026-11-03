//! The BSC LIVE tab's projection (`bsc.live.state`, plan 28.10.): the
//! native owner's LIVE gate as it holds now with every precondition and its
//! reason, the grant and its scope, the daily and campaign attempts, the
//! grant's spend, the scope's entities, the runtime's LIVE path and the
//! LIVE ledger's positions, transactions and net result, and the LIVE
//! campaign's verdict and attempts ([`super::campaign`]). Every value is the
//! owner's, the runtime's, the ledger's or the evaluator's; nothing is
//! inferred here and nothing here can authorize anything.

use super::projection::safe_text;
use super::wire::bsc::*;
use darkbot_bsc_execution::live_ledger::{Ledger, LotDecision, fold};
use darkbot_bsc_execution::live_qualification;
use darkbot_bsc_execution::runtime::RuntimeConfig;
use darkbot_bsc_execution::sources::{
    self, ComponentId, Entity, Generation, LiveContext, RUNTIME_MODE, SourceId, SwitchId,
};
use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;
use darkbot_bsc_record::{self as record, hex};
use std::path::Path;

fn hex0x(bytes: &[u8]) -> String {
    format!("0x{}", hex(bytes))
}

/// R10 review L4: the runtime status is older than the owner's freshness
/// rule (or the clock is unreadable): its probe values are not shown.
pub const RUNTIME_STATUS_STALE: &str = "RUNTIME_STATUS_STALE";

pub use darkbot_bsc_execution::sources::RUNTIME_CONFIG_PATH;

/// The snapshot's runtime configuration by the runtime's own rule (`RuntimeConfig::parse`), or
/// the warning why there is none: unreadable, or one the runtime would refuse.
pub fn runtime_config(root: &Path) -> Result<RuntimeConfig, &'static str> {
    let text = std::fs::read_to_string(root.join(RUNTIME_CONFIG_PATH))
        .map_err(|_| "RUNTIME_CONFIG_UNREADABLE")?;
    RuntimeConfig::parse(&text).map_err(|_| "RUNTIME_CONFIG_INVALID")
}

/// The ingress sources a grant can name (Audit E 1.6): those the configuration gives an adapter
/// endpoint, in source order. A source without one never sights a launch, so no qualification
/// can ever name it.
pub fn grantable_sources(config: &RuntimeConfig) -> Vec<String> {
    SourceId::ALL
        .iter()
        .filter(|s| config.endpoints.iter().any(|(e, _)| e == *s))
        .map(|s| s.name().into())
        .collect()
}

/// One entity of the scope: as requested, and as it holds now under the
/// gate.
fn scope_row(g: &Generation, e: Entity, ctx: &LiveContext<'_>) -> ControlSwitch {
    let now = g.effective_live_now(e, ctx);
    ControlSwitch {
        id: e.name().into(),
        requested: g.requested_of(e).name(),
        effective: now.name(),
        block_reason: now.reason().unwrap_or_default().into(),
        generation: g.generation,
        ..Default::default()
    }
}

/// R10 (Audit D): the runtime's LIVE path alone ([`path`]) at `now_ms`, for
/// the Wallets tab's LIVE buyer probe.
pub fn path_only(rundir: &Path, now_ms: Option<u64>) -> BscLiveState {
    let mut out = BscLiveState::default();
    path(rundir, &mut out, now_ms);
    out
}

/// The runtime's LIVE path from its status record (absent: not running) at
/// `now_ms` (none: the clock is unreadable).
fn path(rundir: &Path, out: &mut BscLiveState, now_ms: Option<u64>) {
    use rs::live as l;
    out.path_state = "NOT_RUNNING".into();
    let Ok(bytes) = std::fs::read(rundir.join("bsc-runtime/status.dbrec")) else {
        return;
    };
    let Ok(r) = record::decode(&rs::SCHEMA, &bytes) else {
        out.warnings.push("RUNTIME_STATUS_INVALID".into());
        return;
    };
    // R10 (R7 display): the second source's verdict, LIVE path or not.
    out.confirm_state = r
        .opt_nested(rs::CANONICAL)
        .ok()
        .flatten()
        .and_then(|c| {
            c.opt_text(rs::canonical::CONFIRM_STATE)
                .ok()
                .flatten()
                .map(safe_text)
        })
        .unwrap_or_default();
    let Ok(Some(p)) = r.opt_nested(rs::LIVE) else {
        out.path_state = "NOT_CONFIGURED".into();
        return;
    };
    out.path_state = p.text(l::STATE).unwrap_or("UNKNOWN").into();
    out.path_reason = p
        .opt_text(l::REASON)
        .ok()
        .flatten()
        .unwrap_or_default()
        .into();
    out.route = p.text(l::ROUTE).unwrap_or_default().into();
    out.buyer = p
        .opt_text(l::BUYER)
        .ok()
        .flatten()
        .unwrap_or_default()
        .into();
    // R10 review L4: the probe's values (balance, nonce, the arm and the
    // next C3 attempt) only from a status the owner's own rule still takes
    // as current (`sources::status_is_current`); the arm and the attempt
    // only while ARMED.
    let fresh = now_ms.is_some_and(|now| sources::status_is_current(&r, now));
    if !fresh {
        out.warnings.push(RUNTIME_STATUS_STALE.into());
    }
    let armed = fresh && out.path_state == "ARMED";
    out.buyer_balance_wei = p
        .opt_u256(l::BALANCE_WEI)
        .ok()
        .flatten()
        .filter(|_| fresh)
        .map(|w| record::u256_to_decimal(&w));
    out.released = p.u64(l::RELEASED).unwrap_or_default();
    out.unknown = p.u64(l::UNKNOWN).unwrap_or_default();
    out.last_refusal = p
        .opt_text(l::LAST_REFUSAL)
        .ok()
        .flatten()
        .unwrap_or_default()
        .into();
    // R1/R3b: the exit deciders and the receipt recovery.
    let texts = |tag| -> Vec<String> {
        p.texts(tag)
            .map(|t| t.iter().map(|s| safe_text(s)).collect())
            .unwrap_or_default()
    };
    let text = |tag| {
        p.opt_text(tag)
            .ok()
            .flatten()
            .map(safe_text)
            .unwrap_or_default()
    };
    out.exit_undecided = texts(l::EXIT_UNDECIDED);
    out.stranded = texts(l::STRANDED);
    out.lot_read_faults = p.u64(l::LOT_READ_FAULTS).unwrap_or_default();
    out.lot_read_fault = text(l::LOT_READ_FAULT);
    out.recovery_pending = texts(l::RECOVERY_PENDING);
    out.recovered = p.u64(l::RECOVERED).unwrap_or_default();
    out.recovery_faults = p.u64(l::RECOVERY_FAULTS).unwrap_or_default();
    out.recovery_fault = text(l::RECOVERY_FAULT);
    // R10 (Audit D): the path's nonce, arm, refusals, answers, C2 evidence,
    // probes, ledger read, latency and next C3 attempt, as the status names them.
    let count = |tag| p.u64(tag).unwrap_or_default();
    let opt = |tag| p.opt_u64(tag).ok().flatten();
    out.next_nonce = opt(l::NEXT_NONCE).filter(|_| fresh);
    out.armed_generation = opt(l::GRANT_GENERATION).filter(|_| armed);
    out.refusals = texts(l::REFUSALS);
    out.responses = texts(l::RESPONSES);
    out.included = count(l::INCLUDED);
    out.reverted = count(l::REVERTED);
    out.replaced = count(l::REPLACED);
    out.finalized = count(l::FINAL);
    out.orphaned = count(l::ORPHANED);
    out.probes = count(l::PROBES);
    out.ledger_error = text(l::LEDGER_ERROR);
    out.latency_samples = count(l::LATENCY_SAMPLES);
    out.latency_p50_ns = opt(l::LATENCY_P50_NS);
    out.latency_p99_ns = opt(l::LATENCY_P99_NS);
    out.latency_max_ns = opt(l::LATENCY_MAX_NS);
    out.lease_attempt = p
        .opt_b256(l::LEASE_ATTEMPT)
        .ok()
        .flatten()
        .filter(|_| armed)
        .map(|a| hex0x(&a))
        .unwrap_or_default();
}

/// The LIVE tab at `now_ms` (none: the clock is unreadable, the gate is
/// UNKNOWN) under the snapshot's runtime configuration ([`runtime_config`]).
/// The campaign section comes last: it takes only what the rest of the tab
/// leaves of the state bound (review L-1).
pub fn live_state(
    root: &Path,
    rundir: &Path,
    config: Result<&RuntimeConfig, &str>,
    now_ms: Option<u64>,
) -> BscLiveState {
    let mut out = owner_tab(root, rundir, config, now_ms);
    super::campaign::project(rundir, &mut out);
    out
}

/// The LIVE tab without its campaign section.
fn owner_tab(
    root: &Path,
    rundir: &Path,
    config: Result<&RuntimeConfig, &str>,
    now_ms: Option<u64>,
) -> BscLiveState {
    let mut out = BscLiveState {
        gate: "UNKNOWN".into(),
        // Audit E 4.9: the owner's own vocabulary, with or without its state.
        grant_venues: darkbot_bsc_execution::sources::LIVE_VENUES
            .iter()
            .map(|v| v.name().into())
            .collect(),
        grant_strategies: darkbot_bsc_execution::sources::LiveStrategies::ALL
            .iter()
            .map(|s| s.name().into())
            .collect(),
        grant_adapters: darkbot_bsc_execution::sources::LiveAdapter::ALL
            .iter()
            .map(|a| a.name().into())
            .collect(),
        ..Default::default()
    };
    // R3b: the buyer whose nonces may be burnt (none: no LIVE path configured).
    let buyer = config.ok().and_then(|c| c.live.as_ref()).map(|l| l.buyer);
    match config {
        Ok(c) => {
            out.grantable_sources = grantable_sources(c);
            // R3b: the switches that lock an operator's exit-now and nonce burn.
            out.operator_exit = c.exec.as_ref().map(|x| x.exec.live_operator_exit);
            out.nonce_burn = c.exec.as_ref().map(|x| x.exec.live_nonce_burn);
            // R4 (BSC3-069): the soft bounds of a grant's risk, the outlier switch.
            if let Some(x) = c.exec.as_ref().map(|x| &x.exec) {
                out.risk_soft_buy_slippage_bps = Some(u32::from(x.live_risk_soft_buy_slippage_bps));
                out.risk_soft_exit_slippage_bps =
                    Some(u32::from(x.live_risk_soft_exit_slippage_bps));
                out.risk_soft_gas_price_wei = Some(x.live_risk_soft_gas_price_wei);
                out.risk_outlier = Some(x.live_risk_outlier);
                // R4: the C3 re-arm switch; the bound builder policy.
                out.c3_rearm = Some(x.live_c3_rearm);
                // R5: the exit re-price and the public exit route.
                out.exit_reprice_bps = Some(u32::from(x.live_exit_reprice_bps));
                out.public_exit = Some(x.live_public_exit);
                out.dex_exit = Some(x.live_dex_exit);
                // R10: the divergence quarantine (R7), the presence anchor
                // (R9, empty: OFF), the qualification's age bound (R8).
                out.divergence_quarantine = Some(x.live_divergence_quarantine);
                out.presence_anchor = Some(
                    x.live_presence_anchor
                        .map(|a| hex0x(&a))
                        .unwrap_or_default(),
                );
                out.qualification_max_age_ms = Some(x.live_qualification_max_age_ms);
                use darkbot_bsc_execution::builder_policy as bp;
                out.builder_policy_fresh_until_unix_ms = bp::read(root, &x.live_builder_policy)
                    .ok()
                    .and_then(|p| bp::oldest_fetch_s(&p))
                    .and_then(|t| t.checked_add(bp::MAX_AGE_S)?.checked_mul(1000));
                out.builder_policy_refusal = now_ms.and_then(|now| {
                    bp::live_bound(root, &x.live_builder_policy, x.gas_price_wei, now).err()
                });
            }
            // Audit E 3.1: the attempts' maximum.
            out.buy_attempts_total_max = Some(c.owner.buy_attempts_total_max);
            // R4: the campaign's cumulative budget.
            out.campaign_spend_max_wei = c
                .exec
                .as_ref()
                .map(|x| x.exec.live_spend_total_wei.to_string());
        }
        Err(why) => out.warnings.push(why.into()),
    }
    path(rundir, &mut out, now_ms);
    let (verified, invalid) = live_qualification::view(root);
    (out.qualification, out.qualification_detail) = match invalid {
        Some(why) => ("INVALID".into(), why),
        None if verified.sha256 == [0; 32] => ("NONE".into(), String::new()),
        None => ("VERIFIED".into(), hex(&verified.sha256)),
    };
    // R10 (R8 display): the risk scope its decision names and its age.
    if out.qualification == "VERIFIED" {
        out.qualification_risk_scope_sha256 = verified
            .view
            .risk_scope_sha256
            .map(|h| hex(&h))
            .unwrap_or_default();
        out.qualification_observed_unix_ms = Some(verified.observed_unix_ms);
    }
    // The ledger is the owner state's, whatever the gate.
    let dir = rundir.join("bsc-sources");
    match sources::spent_total(&dir) {
        Ok(w) => out.campaign_spent_wei = Some(w.to_string()),
        Err(_) => out.warnings.push(sources::LIVE_SPEND_UNREADABLE.into()),
    }
    let ledger = Ledger::open(&dir);
    // R3b: the version before the facts shown, the CAS of an operator's
    // decision: once a fact is newer than they, a request at it is refused
    // (never one decided on less than the ledger holds).
    let version = ledger.version();
    match ledger.facts() {
        Ok(facts) => {
            out.ledger_version = version.ok();
            let book = fold(&facts);
            out.positions = book
                .positions
                .iter()
                .map(|p| LivePosition {
                    lot: hex0x(&p.lot),
                    token: hex0x(&p.token),
                    component: p.component.name().into(),
                    state: p.state.name().into(),
                    tokens: p.tokens.to_string(),
                    input_wei: p.input_wei.to_string(),
                    proceeds_wei: p.proceeds_wei.to_string(),
                    gas_wei: p.gas_wei.to_string(),
                    pnl_wei: p.pnl_wei.map(|v| v.to_string()),
                    proceeds_unknown: p.proceeds_unknown,
                    decisions: LotDecision::ALL
                        .into_iter()
                        .filter(|d| book.decidable(&p.lot, *d).is_ok())
                        .map(|d| d.name().into())
                        .collect(),
                })
                .collect();
            out.transactions = book
                .txs
                .iter()
                .map(|t| LiveTransaction {
                    tx_hash: hex0x(&t.released.tx_hash),
                    intent: t.released.intent.name().into(),
                    token: hex0x(&t.released.token),
                    nonce: t.released.nonce,
                    state: t.state.name().into(),
                    block_number: t.inclusion.as_ref().and_then(|f| f.block).map(|b| b.0),
                    source: t
                        .inclusion
                        .as_ref()
                        .and_then(|f| f.source)
                        .map(|s| s.name().into())
                        .unwrap_or_default(),
                    burn_requested: t.decided.burn_requested,
                    burnable: buyer.is_some_and(|b| book.burnable(&t.released.tx_hash, b).is_ok()),
                })
                .collect();
            out.realized_pnl_wei = book.realized_pnl_wei.to_string();
            out.gas_wei = book.gas_wei.to_string();
        }
        Err(_) => {
            // R3b: the entries that do not decode, counted, the lowest the
            // one the operator may quarantine next (bounded: one number).
            let bad = ledger.check().map(|(_, bad)| bad).unwrap_or_default();
            out.first_unreadable = bad.iter().map(|(seq, _)| *seq).min();
            out.warnings.push(match out.first_unreadable {
                Some(first) => format!(
                    "LIVE_LEDGER_INVALID {} unreadable, first seq {first}",
                    bad.len()
                ),
                None => "LIVE_LEDGER_INVALID".into(),
            });
        }
    }
    // R3b (BSC3-045): what the operator quarantined of the ledger may be
    // missing from it.
    match Ledger::open(&dir).quarantined_seqs() {
        Ok(seqs) => {
            if let Some(first) = seqs.first() {
                out.warnings.push(format!(
                    "{} {}, first seq {first}",
                    sources::LIVE_LEDGER_QUARANTINED,
                    seqs.len()
                ));
            }
        }
        Err(_) => out
            .warnings
            .push("LIVE_LEDGER_QUARANTINE_UNREADABLE".into()),
    }
    if !dir.join("generations").is_dir() {
        out.warnings.push("OWNER_STATE_ABSENT".into());
        return out;
    }
    let g = match sources::current_generation(&dir) {
        Ok(g) => g,
        Err(_) => {
            out.warnings.push("OWNER_SETTINGS_INVALID".into());
            return out;
        }
    };
    out.generation = g.generation;
    let Some(now) = now_ms else {
        out.warnings.push("CLOCK_UNREADABLE".into());
        return out;
    };
    let reads = super::projection::gate_reads(rundir, &dir, &g, now, out.buy_attempts_total_max);
    let ctx = reads.ctx(&verified.view);
    let (boot, daily, total, spent) = (&reads.boot, &reads.daily, &reads.total, reads.spent_wei);
    let blockers = g.live_blockers(&ctx);
    out.gate = if blockers.is_empty() {
        "CLEAR"
    } else {
        "BLOCKED"
    }
    .into();
    out.blockers = blockers.iter().map(|b| b.to_string()).collect();
    out.preconditions = g
        .live_checklist(&ctx)
        .into_iter()
        .map(|(id, missed)| LivePrecondition {
            id: id.into(),
            met: missed.is_none(),
            reason: missed.unwrap_or_default().into(),
        })
        .collect();
    out.spent_wei = spent.map_or_else(|| "UNREADABLE".into(), |w| w.to_string());
    let runtime = g.effective_live_now(Entity::Runtime, &ctx);
    let set = |s: &sources::Setting| {
        (
            s.set_by
                .as_ref()
                .map(|b| b.request_id.clone())
                .unwrap_or_default(),
            s.set_by.as_ref().map(|b| b.generation),
        )
    };
    let (request_id, set_generation) = set(&g.runtime);
    out.runtime_mode = Some(ControlSwitch {
        id: RUNTIME_MODE.into(),
        requested: g.runtime.requested.name(),
        effective: runtime.name(),
        block_reason: runtime.reason().unwrap_or_default().into(),
        generation: g.generation,
        request_id,
        set_generation,
    });
    let live = g.switch(SwitchId::Live);
    let live_now = g.switch_now(SwitchId::Live, boot.as_deref());
    let request_id = live
        .set_by
        .as_ref()
        .map(|b| b.request_id.clone())
        .unwrap_or_default();
    let set_generation = live.set_by.as_ref().map(|b| b.generation);
    out.economic_live = Some(ControlSwitch {
        id: SwitchId::Live.name().into(),
        requested: live.requested.name(),
        effective: live_now.name(),
        block_reason: live_now.reason().unwrap_or_default().into(),
        generation: g.generation,
        request_id,
        set_generation,
    });
    if let Some(d) = &daily {
        let (request_id, set_generation) = set(&g.buy_limit);
        out.daily = Some(BuyAttemptLimit {
            per_day: u64::from(d.limit),
            day_utc: d.day.utc.clone(),
            used: d.used,
            remaining: d.remaining(),
            generation: g.generation,
            request_id,
            set_generation,
        });
    } else {
        out.warnings.push("BUY_LIMIT_UNREADABLE".into());
    }
    match &total {
        Some(t) => {
            out.total_limit = u64::from(t.limit);
            out.total_used = t.used;
        }
        None => out.warnings.push("ATTEMPT_TOTAL_UNREADABLE".into()),
    }
    if let (Some(grant), Some(by)) = (&g.live, &g.live_set_by) {
        out.grant_attempts_used = total.as_ref().map_or(0, |t| t.used_by_grant(by.generation));
        out.grant = Some(LiveGrant {
            venue: grant.venue.name().into(),
            strategies: grant.strategies.name().into(),
            sources: grant.sources.iter().map(|s| s.name().into()).collect(),
            adapters: grant.adapters.iter().map(|a| a.name().into()).collect(),
            max_input_wei: grant.max_input_wei.to_string(),
            max_spend_wei: grant.max_spend_wei.to_string(),
            max_attempts: grant.max_attempts,
            expires_unix_ms: grant.expires_unix_ms,
            scope_sha256: hex(&grant.scope_sha256()),
            scope_text: grant.scope_text(),
            risk_scope_sha256: hex(&grant.risk_scope_sha256()),
            request_id: by.request_id.clone(),
            set_generation: by.generation,
            exit_only: grant.exit_only,
            risk: grant.risk.map(|r| LiveGrantRisk {
                max_buy_slippage_bps: u32::from(r.max_buy_slippage_bps),
                max_exit_slippage_bps: u32::from(r.max_exit_slippage_bps),
                max_gas_price_wei: r.max_gas_price_wei,
                max_buy_gas_limit: r.max_buy_gas_limit,
                max_approve_gas_limit: r.max_approve_gas_limit,
                max_sell_gas_limit: r.max_sell_gas_limit,
                outlier_ack: r.outlier_ack,
            }),
            limits: grant.limits.map(|l| LiveGrantLimits {
                max_open_positions: l.max_open_positions,
                max_realized_loss_wei: l.max_realized_loss_wei.to_string(),
                max_daily_realized_loss_wei: l.max_daily_realized_loss_wei.to_string(),
                min_buy_interval_ms: l.min_buy_interval_ms,
            }),
        });
        // R10 (R9 display): who signed the grant's presence for its generation.
        out.presence_signer = g
            .live_presence
            .as_ref()
            .and_then(|sig| sources::presence_signer(grant, by.generation, sig))
            .map(|a| hex0x(&a))
            .unwrap_or_default();
        let mut scope = vec![Entity::Runtime, ComponentId::StateC2.into()];
        // 01.11. (R1): Strategy C with every grant (it decides the exits).
        scope.extend(grant.strategies.components().into_iter().map(Entity::from));
        scope.push(grant.venue.into());
        scope.extend(grant.sources.iter().map(|s| Entity::from(*s)));
        out.scope = scope.into_iter().map(|e| scope_row(&g, e, &ctx)).collect();
    }
    out
}
