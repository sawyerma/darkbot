//! The BSC LIVE tab's projection (`bsc.live.state`, plan 28.10.): the
//! native owner's LIVE gate as it holds now with every precondition and its
//! reason, the grant and its scope, the daily and campaign attempts, the
//! grant's spend, the scope's entities, the runtime's LIVE path and the
//! LIVE ledger's positions, transactions and net result. Every value is the
//! owner's, the runtime's or the ledger's; nothing is inferred here and
//! nothing here can authorize anything.

use super::wire::bsc::*;
use darkbot_bsc_execution::live_ledger::{Ledger, fold};
use darkbot_bsc_execution::live_qualification;
use darkbot_bsc_execution::sources::{
    self, ComponentId, Entity, Generation, LiveContext, RUNTIME_MODE, SwitchId,
};
use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;
use darkbot_bsc_record::{self as record, hex};
use std::path::Path;

fn hex0x(bytes: &[u8]) -> String {
    format!("0x{}", hex(bytes))
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

/// The runtime's LIVE path from its status record (absent: not running).
fn path(rundir: &Path, out: &mut BscLiveState) {
    use rs::live as l;
    out.path_state = "NOT_RUNNING".into();
    let Ok(bytes) = std::fs::read(rundir.join("bsc-runtime/status.dbrec")) else {
        return;
    };
    let Ok(r) = record::decode(&rs::SCHEMA, &bytes) else {
        out.warnings.push("RUNTIME_STATUS_INVALID".into());
        return;
    };
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
    out.buyer_balance_wei = p
        .opt_u256(l::BALANCE_WEI)
        .ok()
        .flatten()
        .map(|w| record::u256_to_decimal(&w));
    out.released = p.u64(l::RELEASED).unwrap_or_default();
    out.unknown = p.u64(l::UNKNOWN).unwrap_or_default();
    out.last_refusal = p
        .opt_text(l::LAST_REFUSAL)
        .ok()
        .flatten()
        .unwrap_or_default()
        .into();
}

/// The LIVE tab at `now_ms` (none: the clock is unreadable, the gate is
/// UNKNOWN).
pub fn live_state(root: &Path, rundir: &Path, now_ms: Option<u64>) -> BscLiveState {
    let mut out = BscLiveState {
        gate: "UNKNOWN".into(),
        ..Default::default()
    };
    path(rundir, &mut out);
    let (verified, invalid) = live_qualification::view(root);
    (out.qualification, out.qualification_detail) = match invalid {
        Some(why) => ("INVALID".into(), why),
        None if verified.sha256 == [0; 32] => ("NONE".into(), String::new()),
        None => ("VERIFIED".into(), hex(&verified.sha256)),
    };
    // The ledger is the owner state's, whatever the gate.
    let dir = rundir.join("bsc-sources");
    match Ledger::open(&dir).facts() {
        Ok(facts) => {
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
                })
                .collect();
            out.realized_pnl_wei = book.realized_pnl_wei.to_string();
            out.gas_wei = book.gas_wei.to_string();
        }
        Err(_) => out.warnings.push("LIVE_LEDGER_INVALID".into()),
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
    let reads = sources::GateReads::read(&dir, &g, now);
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
            request_id: by.request_id.clone(),
            set_generation: by.generation,
            exit_only: grant.exit_only,
        });
        let mut scope = vec![Entity::Runtime, ComponentId::StateC2.into()];
        if grant.strategies.c3() {
            scope.push(ComponentId::ExecC3.into());
        }
        if grant.strategies.strategy_c() {
            scope.push(ComponentId::StrategyC.into());
        }
        scope.push(grant.venue.into());
        scope.extend(grant.sources.iter().map(|s| Entity::from(*s)));
        out.scope = scope.into_iter().map(|e| scope_row(&g, e, &ctx)).collect();
    }
    out
}

/// Whether the LIVE path trades now: the gate clear and the runtime armed.
pub fn trading(live: &BscLiveState) -> bool {
    live.gate == "CLEAR" && live.path_state == "ARMED"
}
