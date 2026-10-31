//! FE-04 durable single-wallet hold at the existing submission boundary.
//! No signer, provider, state/price engine or automatic LIVE activation.
pub mod admission;
pub mod approval_file;
pub mod archive;
pub mod exit;
#[cfg(feature = "crate_health")]
pub mod exit_admission;
pub mod non_landing;
pub mod receipts;
pub mod settlement;
pub mod transport;

use crate::analytics::journal;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    path::Path,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Approval {
    pub schema: u32,
    pub authorized: bool,
    pub approval_id: String,
    pub code_sha: String,
    pub binary_sha256: String,
    pub wallet: String,
    pub program: String,
    pub program_data_hash: String,
    pub funding_class: String,
    pub allowed_targets: BTreeSet<String>,
    pub expires_unix_s: u64,
    pub max_state_age_ms: u64,
    pub max_capital_usd_micro: u64,
    pub max_total_sol_debit_lamports: u64,
    pub max_tip_lamports: u64,
    pub max_rent_lamports: u64,
    pub max_loss_usd_micro: u64,
    pub min_net_usd_micro: u64,
    pub max_slippage_bps: u64,
}

/// Supplied by the existing production pipeline after its full current-state
/// checks. This is not evidence derived from a profile or historical replay.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    pub code_sha: String,
    pub binary_sha256: String,
    pub wallet: String,
    pub program: String,
    pub program_data_hash: String,
    pub funding_class: String,
    pub target: String,
    pub signatures: Vec<String>,
    pub signed_wire_sha256: String,
    pub state_generation: u64,
    pub state_slot: u64,
    pub valid_until_slot: u64,
    pub evaluated_unix_ms: u64,
    pub capital_usd_micro: u64,
    pub transaction_fee_lamports: u64,
    pub tip_lamports: u64,
    pub rent_lamports: u64,
    pub max_loss_usd_micro: u64,
    pub conservative_net_usd_micro: i128,
    pub slippage_bps: u64,
    /// Exact base units. SOL uses key "SOL"; tokens use mint public keys.
    pub holds: BTreeMap<String, u64>,
    /// Includes credited exit/collateral mints even when no inventory is reserved.
    pub expected_wallet_assets: BTreeSet<String>,
    /// C12: the `lastValidBlockHeight` of the signed blockhash (block-height domain, not a
    /// slot): a non-landing proof needs a finalized block height beyond it. `None` for an intent
    /// of schema 2 (before C12): its non-landing is never provable, it stays UNKNOWN until
    /// observed — never a zero.
    #[serde(default)]
    pub last_valid_block_height: Option<u64>,
}

/// Schema-2 layout of `Intent` (FE-04 frames before C12): no `last_valid_block_height`.
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct IntentV2 {
    code_sha: String,
    binary_sha256: String,
    wallet: String,
    program: String,
    program_data_hash: String,
    funding_class: String,
    target: String,
    signatures: Vec<String>,
    signed_wire_sha256: String,
    state_generation: u64,
    state_slot: u64,
    valid_until_slot: u64,
    evaluated_unix_ms: u64,
    capital_usd_micro: u64,
    transaction_fee_lamports: u64,
    tip_lamports: u64,
    rent_lamports: u64,
    max_loss_usd_micro: u64,
    conservative_net_usd_micro: i128,
    slippage_bps: u64,
    holds: BTreeMap<String, u64>,
    expected_wallet_assets: BTreeSet<String>,
}
impl From<IntentV2> for Intent {
    fn from(v: IntentV2) -> Self {
        Self {
            code_sha: v.code_sha,
            binary_sha256: v.binary_sha256,
            wallet: v.wallet,
            program: v.program,
            program_data_hash: v.program_data_hash,
            funding_class: v.funding_class,
            target: v.target,
            signatures: v.signatures,
            signed_wire_sha256: v.signed_wire_sha256,
            state_generation: v.state_generation,
            state_slot: v.state_slot,
            valid_until_slot: v.valid_until_slot,
            evaluated_unix_ms: v.evaluated_unix_ms,
            capital_usd_micro: v.capital_usd_micro,
            transaction_fee_lamports: v.transaction_fee_lamports,
            tip_lamports: v.tip_lamports,
            rent_lamports: v.rent_lamports,
            max_loss_usd_micro: v.max_loss_usd_micro,
            conservative_net_usd_micro: v.conservative_net_usd_micro,
            slippage_bps: v.slippage_bps,
            holds: v.holds,
            expected_wallet_assets: v.expected_wallet_assets,
            last_valid_block_height: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Current {
    pub now_unix_ms: u64,
    pub slot: u64,
    pub state_generation: u64,
    pub program_data_hash: String,
    pub balances: BTreeMap<String, u64>,
    /// P8-RES-01: what every other active attempt holds (`WalletReservations::held_by_others`);
    /// a hold is checked against the balance minus these, never twice the same units.
    pub reserved_elsewhere: BTreeMap<String, u64>,
    /// P8-INV-01: the never-spent reserve per asset (`inventory::reserves`, the LIVE gate floor).
    pub policy_reserve: BTreeMap<String, u64>,
    /// Each existing gate must independently be known and passed. In particular
    /// a single-TX simulation cannot satisfy the bundle_simulated gate.
    pub health_funding_route_fee_exit_known: bool,
    pub canonical_fresh_no_gap_or_fork: bool,
    pub exact_signed_payload_simulated: bool,
    pub bundle_simulated: bool,
}

/// B3: the safety state a durable FE-04 journal implies after a restart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafetyReplay {
    pub held_capital_usd_micro: Option<u64>,
    /// C3: the booked settlement of a settled attempt.
    pub settled: Option<darkbot_project0::safety::SettledAttempt>,
    /// FE-04 exits: every settled exit, each in its own booking day.
    pub exits: Vec<darkbot_project0::safety::SettledAttempt>,
}

/// B3: what the safety layer replays of one attempt (active or archived).
fn safety_replay_of(s: &Saved) -> SafetyReplay {
    SafetyReplay {
        held_capital_usd_micro: (!s.phase.finalized()).then_some(s.intent.capital_usd_micro),
        settled: match &s.phase {
            Phase::Settled(done) => Some(darkbot_project0::safety::SettledAttempt {
                unix_s: done
                    .booked_unix_s
                    .unwrap_or(s.intent.evaluated_unix_ms / 1_000),
                realized_net_usd_micro: done.realized_net_usd_micro,
                landed_success: done.landed_success,
                open_inventory_usd_micro: done.unrealized_inventory_usd_micro,
            }),
            _ => None,
        },
        exits: exit::exit_bookings(s),
    }
}

/// A durable FE-04 hold that is not finalized (P8-RES-02).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveHold {
    pub attempt: String,
    pub target: String,
    pub unknown_outcome: bool,
    pub holds: BTreeMap<String, u64>,
    /// MT5b: the intent's first bound signature and its `last_valid_block_height` — the
    /// SUBMITTED(signature, last_valid_block_height) of the target's ownership (P6-MT.1).
    pub signature: Option<String>,
    pub last_valid_block_height: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Phase {
    Reserved,
    HandoffUnknown,
    HaltFinalized,
    /// C3: the finalized attempt's realized outcome, booked once (appended variant: earlier
    /// journal frames keep their encoding).
    Settled(settlement::Settlement),
}
impl Phase {
    /// Finalized (with or without a booked settlement): no hold remains.
    pub fn finalized(&self) -> bool {
        matches!(self, Phase::HaltFinalized | Phase::Settled(_))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Saved {
    approval: Approval,
    intent: Intent,
    phase: Phase,
    finalized: Option<Vec<Receipt>>,
    #[serde(default)]
    observations: BTreeMap<String, receipts::FinalizedObservation>,
    /// FE-04 exits of the settled attempt's open inventory (schema 5), in order.
    #[serde(default)]
    exits: Vec<exit::ExitAttempt>,
    /// DEC-03 (schema 6): the class of a released non-landing (B+C or C alone); `None` while
    /// not released that way, and for frames before N-2.
    #[serde(default)]
    non_landing: Option<non_landing::NonLandingRelease>,
    /// DEC-04 (schema 7): the hold mode, A's inventory hold and B the attempt was admitted with;
    /// written by the reservation only (`None`: OFF, or a frame before DEC-04).
    #[serde(default)]
    hold: Option<crate::execution::hold_b::HoldEvidence>,
    /// P8-CTRL.5 (schema 8): the FE-06 grant the attempt is booked against, written by the
    /// reservation before any dispatch (`None`: no grant, or a frame before P8-CTRL).
    #[serde(default)]
    fe06: Option<Fe06Attempt>,
}

/// P8-CTRL.5: an attempt's booking against its FE-06 grant — the grant's digest, its campaign,
/// the UTC day and the owner epoch of the reservation. Every attempt booked so, UNKNOWN or sent,
/// consumes the grant's, the campaign's and the day's quota (`archive::records`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fe06Attempt {
    pub grant: String,
    pub campaign: String,
    /// `unix_ms / 86_400_000` of the reservation.
    pub utc_day: u64,
    pub owner_epoch: Option<u64>,
}

/// P8-CTRL.5: one of the three quotas a new attempt is counted against — its UTC day, its
/// grant's campaign and the grant itself. The one count of the admission's limits, the owner's
/// status and `p0ctl live attempts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fe06Quota<'a> {
    UtcDay(u64),
    Campaign(&'a str),
    Grant(&'a str),
}
impl Fe06Quota<'_> {
    pub fn name(&self) -> &'static str {
        match self {
            Self::UtcDay(_) => "UTC_DAY",
            Self::Campaign(_) => "CAMPAIGN",
            Self::Grant(_) => "GRANT",
        }
    }
    /// The booked attempts this quota counts (UNKNOWN or sent alike).
    pub fn used<'b>(&self, bookings: impl IntoIterator<Item = &'b Fe06Attempt>) -> u64 {
        bookings
            .into_iter()
            .filter(|b| match self {
                Self::UtcDay(d) => b.utc_day == *d,
                Self::Campaign(c) => b.campaign == *c,
                Self::Grant(g) => b.grant == *g,
            })
            .count() as u64
    }
}

/// P8-CTRL.5/.8 (`p0ctl live attempts`): a wallet's attempts as they are booked against FE-06
/// grants — each booking, the quotas of the UTC day `today`, of every campaign and every grant,
/// and with `scope` that scope's three quotas against its limits (the count the admission refuses
/// a new attempt on: `Fe06Quota`).
pub fn fe06_attempts_report(
    records: &[crate::execution::accounting::AttemptRecord],
    wallet: &str,
    today: u64,
    scope: Option<(&str, &crate::orchestration::fe06::Scope)>,
) -> Vec<String> {
    let booked: Vec<&Fe06Attempt> = records.iter().filter_map(|r| r.fe06.as_ref()).collect();
    let key = |q: &Fe06Quota| match q {
        Fe06Quota::UtcDay(d) => format!("day {d} (today)"),
        Fe06Quota::Campaign(c) => (*c).to_string(),
        Fe06Quota::Grant(g) => (*g).to_string(),
    };
    let mut out = vec![format!(
        "FE-06 attempts of wallet {wallet}: {} booked, {} without a grant (none, or before P8-CTRL)",
        booked.len(),
        records.len() - booked.len()
    )];
    for r in records {
        if let Some(b) = &r.fe06 {
            out.push(format!(
                "  approval={} state={}{} grant={} campaign={} utc_day={} owner_epoch={}",
                r.approval_id,
                r.state.name(),
                if r.archived { " archived" } else { "" },
                b.grant,
                b.campaign,
                b.utc_day,
                b.owner_epoch
                    .map_or_else(|| "unknown".to_string(), |e| e.to_string())
            ));
        }
    }
    let campaigns: std::collections::BTreeSet<&str> =
        booked.iter().map(|b| b.campaign.as_str()).collect();
    let grants: std::collections::BTreeSet<&str> =
        booked.iter().map(|b| b.grant.as_str()).collect();
    let mut quotas = vec![Fe06Quota::UtcDay(today)];
    quotas.extend(campaigns.into_iter().map(Fe06Quota::Campaign));
    quotas.extend(grants.into_iter().map(Fe06Quota::Grant));
    for q in &quotas {
        out.push(format!(
            "  {} {}: {} used",
            q.name(),
            key(q),
            q.used(booked.iter().copied())
        ));
    }
    if let Some((digest, s)) = scope {
        out.push(format!("scope {digest} ({}):", s.mode.code()));
        for (q, limit) in [
            (Fe06Quota::UtcDay(today), s.max_attempts_per_utc_day),
            (Fe06Quota::Campaign(&s.campaign_id), s.max_attempts_campaign),
            (Fe06Quota::Grant(digest), s.max_attempts_grant),
        ] {
            let used = q.used(booked.iter().copied());
            out.push(format!(
                "  {} {}: {used} of {limit}{}",
                q.name(),
                key(&q),
                if used >= limit {
                    " · EXHAUSTED (no new attempt)"
                } else {
                    ""
                }
            ));
        }
        let exits = Fe06Quota::Grant(digest).used(
            records
                .iter()
                .flat_map(|r| r.exits.iter())
                .filter_map(|e| e.fe06.as_ref()),
        );
        out.push(format!(
            "  EXITS GRANT {digest}: {exits} of {}{}",
            s.max_exits_grant,
            if exits >= s.max_exits_grant {
                " · EXHAUSTED (no new exit)"
            } else {
                ""
            }
        ));
    }
    out
}

/// Schema-8 layout of `Saved` (the attempt's grant booking, before the exits' own).
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct SavedV8 {
    approval: Approval,
    intent: Intent,
    phase: Phase,
    finalized: Option<Vec<Receipt>>,
    observations: BTreeMap<String, receipts::FinalizedObservation>,
    exits: Vec<exit::ExitAttemptV8>,
    non_landing: Option<non_landing::NonLandingRelease>,
    hold: Option<crate::execution::hold_b::HoldEvidence>,
    fe06: Option<Fe06Attempt>,
}
impl From<SavedV8> for Saved {
    fn from(v: SavedV8) -> Self {
        Self {
            approval: v.approval,
            intent: v.intent,
            phase: v.phase,
            finalized: v.finalized,
            observations: v.observations,
            exits: v.exits.into_iter().map(Into::into).collect(),
            non_landing: v.non_landing,
            hold: v.hold,
            fe06: v.fe06,
        }
    }
}

/// Schema-7 layout of `Saved` (the DEC-04 holds, before the P8-CTRL.5 grant booking).
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct SavedV7 {
    approval: Approval,
    intent: Intent,
    phase: Phase,
    finalized: Option<Vec<Receipt>>,
    observations: BTreeMap<String, receipts::FinalizedObservation>,
    exits: Vec<exit::ExitAttemptV8>,
    non_landing: Option<non_landing::NonLandingRelease>,
    hold: Option<crate::execution::hold_b::HoldEvidence>,
}
impl From<SavedV7> for Saved {
    fn from(v: SavedV7) -> Self {
        Self {
            approval: v.approval,
            intent: v.intent,
            phase: v.phase,
            finalized: v.finalized,
            observations: v.observations,
            exits: v.exits.into_iter().map(Into::into).collect(),
            non_landing: v.non_landing,
            hold: v.hold,
            fe06: None,
        }
    }
}
/// Schema ≤ 3 layout of `Phase` (its settlement before the realized/unrealized split).
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
enum PhaseV3 {
    Reserved,
    HandoffUnknown,
    HaltFinalized,
    Settled(settlement::SettlementV3),
}
impl TryFrom<PhaseV3> for Phase {
    type Error = String;
    fn try_from(v: PhaseV3) -> Result<Self, String> {
        Ok(match v {
            PhaseV3::Reserved => Phase::Reserved,
            PhaseV3::HandoffUnknown => Phase::HandoffUnknown,
            PhaseV3::HaltFinalized => Phase::HaltFinalized,
            PhaseV3::Settled(s) => Phase::Settled(s.try_into()?),
        })
    }
}
/// Schema-2 layout of `Saved` (its intent without `last_valid_block_height`).
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct SavedV2 {
    approval: Approval,
    intent: IntentV2,
    phase: PhaseV3,
    finalized: Option<Vec<Receipt>>,
    observations: BTreeMap<String, receipts::FinalizedObservation>,
}
impl TryFrom<SavedV2> for Saved {
    type Error = String;
    fn try_from(v: SavedV2) -> Result<Self, String> {
        Ok(Self {
            approval: v.approval,
            intent: v.intent.into(),
            phase: v.phase.try_into()?,
            finalized: v.finalized,
            observations: v.observations,
            exits: Vec::new(),
            non_landing: None,
            hold: None,
            fe06: None,
        })
    }
}
/// Schema-3 layout of `Saved` (C12 intent, settlement before the split).
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct SavedV3 {
    approval: Approval,
    intent: Intent,
    phase: PhaseV3,
    finalized: Option<Vec<Receipt>>,
    observations: BTreeMap<String, receipts::FinalizedObservation>,
}
impl TryFrom<SavedV3> for Saved {
    type Error = String;
    fn try_from(v: SavedV3) -> Result<Self, String> {
        Ok(Self {
            approval: v.approval,
            intent: v.intent,
            phase: v.phase.try_into()?,
            finalized: v.finalized,
            observations: v.observations,
            exits: Vec::new(),
            non_landing: None,
            hold: None,
            fe06: None,
        })
    }
}
/// Schema-4 layout of `Saved` (before exits).
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct SavedV4 {
    approval: Approval,
    intent: Intent,
    phase: Phase,
    finalized: Option<Vec<Receipt>>,
    observations: BTreeMap<String, receipts::FinalizedObservation>,
}
impl From<SavedV4> for Saved {
    fn from(v: SavedV4) -> Self {
        Self {
            approval: v.approval,
            intent: v.intent,
            phase: v.phase,
            finalized: v.finalized,
            observations: v.observations,
            exits: Vec::new(),
            non_landing: None,
            hold: None,
            fe06: None,
        }
    }
}
/// Schema-6 layout of `Saved` (the DEC-03 release class, before the DEC-04 holds).
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct SavedV6 {
    approval: Approval,
    intent: Intent,
    phase: Phase,
    finalized: Option<Vec<Receipt>>,
    observations: BTreeMap<String, receipts::FinalizedObservation>,
    exits: Vec<exit::ExitAttemptV8>,
    non_landing: Option<non_landing::NonLandingRelease>,
}
impl From<SavedV6> for Saved {
    fn from(v: SavedV6) -> Self {
        Self {
            approval: v.approval,
            intent: v.intent,
            phase: v.phase,
            finalized: v.finalized,
            observations: v.observations,
            exits: v.exits.into_iter().map(Into::into).collect(),
            non_landing: v.non_landing,
            hold: None,
            fe06: None,
        }
    }
}
/// Schema-5 layout of `Saved` (exits, before the DEC-03 release class).
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct SavedV5 {
    approval: Approval,
    intent: Intent,
    phase: Phase,
    finalized: Option<Vec<Receipt>>,
    observations: BTreeMap<String, receipts::FinalizedObservation>,
    exits: Vec<exit::ExitAttemptV5>,
}
impl From<SavedV5> for Saved {
    fn from(v: SavedV5) -> Self {
        Self {
            approval: v.approval,
            intent: v.intent,
            phase: v.phase,
            finalized: v.finalized,
            observations: v.observations,
            exits: v.exits.into_iter().map(Into::into).collect(),
            non_landing: None,
            hold: None,
            fe06: None,
        }
    }
}
/// An earlier layout that no longer converts (an overflowing settlement) is damaged data.
fn earlier<T>(v: Result<T, String>) -> std::io::Result<T> {
    v.map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Receipt {
    pub signature: String,
    pub finalized: bool,
    pub success: bool,
    pub fee_lamports: Option<u64>,
    /// Parser must include every affected wallet SOL/SPL balance in base units.
    pub wallet_deltas: Option<BTreeMap<String, i128>>,
    pub exit_complete: bool,
}

/// Fields drop in declaration order (L1): the journal writer is closed before the wallet lock
/// is released, so a next owner never opens the journal while this one still holds its file.
pub struct Journal {
    wallet: String,
    /// FE-04 archive: the journal directory (its `attempts/` holds the finished attempts).
    directory: std::path::PathBuf,
    saved: Option<Saved>,
    writer: journal::Writer,
    /// Last field: released after the journal file is closed.
    _lock: WalletLock,
}
/// Exclusive wallet flock, released on every drop path including failed opens.
struct WalletLock(File);
impl Drop for WalletLock {
    /// The flock belongs to the open file description, which a concurrently spawned child
    /// process shares until exec. Unlock explicitly so closing frees the wallet immediately.
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
/// Durable transitions of the single-flight automaton (PERSIST-03). Every frame of the
/// wallet journal carries the transition and the complete state version it produced.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Transition {
    Reserve,
    Handoff,
    ReopenUnknown,
    Observe,
    Finalize,
    /// C3 (appended): a proven non-landing finalizes without receipts; the settlement booking.
    NotLanded,
    Settle,
    /// FE-04 exit (appended): the exit automaton of the settled attempt's open inventory.
    ExitReserve,
    ExitHandoff,
    ExitReopenUnknown,
    ExitObserve,
    ExitFinalize,
    ExitNotLanded,
    ExitSettle,
}
impl Transition {
    fn is_exit(self) -> bool {
        matches!(
            self,
            Transition::ExitReserve
                | Transition::ExitHandoff
                | Transition::ExitReopenUnknown
                | Transition::ExitObserve
                | Transition::ExitFinalize
                | Transition::ExitNotLanded
                | Transition::ExitSettle
        )
    }
}
/// Historical schema-1 frame (JSON value tree) of a transition: written in tests only, to
/// replay journals from before F2.
#[cfg(test)]
#[derive(Serialize)]
struct VersionRef<'a> {
    transition: Transition,
    state: &'a Saved,
}
/// F2: one durable transition of the wallet automaton as a typed binary frame
/// (`FE04_TRANSITION`, schema 7 since the DEC-04 holds, 6 since the DEC-03 release class, 5 since
/// exits, 4 since the realized/unrealized settlement, 3 since C12). Schema-6 … -2 and schema-1
/// JSON frames of older journals stay readable.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Version {
    transition: Transition,
    state: Saved,
}
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct VersionV2 {
    transition: Transition,
    state: SavedV2,
}
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct VersionV3 {
    transition: Transition,
    state: SavedV3,
}
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct VersionV4 {
    transition: Transition,
    state: SavedV4,
}
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct VersionV5 {
    transition: Transition,
    state: SavedV5,
}
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct VersionV6 {
    transition: Transition,
    state: SavedV6,
}
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct VersionV7 {
    transition: Transition,
    state: SavedV7,
}
#[derive(Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct VersionV8 {
    transition: Transition,
    state: SavedV8,
}
impl crate::analytics::payload::Payload for Version {
    const KIND: u16 = journal::kind::FE04_TRANSITION;
    /// P8-CTRL.5: 8 — the attempt's FE-06 grant booking; 9 — each exit's own.
    const SCHEMA: u16 = 9;
    fn decode_earlier(schema: u16, bytes: &[u8]) -> Option<std::io::Result<Self>> {
        use crate::analytics::payload::{decode_wire, TYPED};
        match schema {
            TYPED => Some(decode_wire::<VersionV2>(bytes).and_then(|v| {
                Ok(Self {
                    transition: v.transition,
                    state: earlier(v.state.try_into())?,
                })
            })),
            3 => Some(decode_wire::<VersionV3>(bytes).and_then(|v| {
                Ok(Self {
                    transition: v.transition,
                    state: earlier(v.state.try_into())?,
                })
            })),
            4 => Some(decode_wire::<VersionV4>(bytes).map(|v| Self {
                transition: v.transition,
                state: v.state.into(),
            })),
            5 => Some(decode_wire::<VersionV5>(bytes).map(|v| Self {
                transition: v.transition,
                state: v.state.into(),
            })),
            6 => Some(decode_wire::<VersionV6>(bytes).map(|v| Self {
                transition: v.transition,
                state: v.state.into(),
            })),
            7 => Some(decode_wire::<VersionV7>(bytes).map(|v| Self {
                transition: v.transition,
                state: v.state.into(),
            })),
            8 => Some(decode_wire::<VersionV8>(bytes).map(|v| Self {
                transition: v.transition,
                state: v.state.into(),
            })),
            _ => None,
        }
    }
}
/// F2: the import of a pre-binary JSON wallet journal as a typed frame (`FE04_LEGACY_IMPORT`,
/// schema 2); the JSON file itself stays unchanged and is only read.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyImport {
    legacy_sha256: String,
    state: Saved,
}
#[derive(Deserialize)]
struct LegacyImportV2 {
    legacy_sha256: String,
    state: SavedV2,
}
#[derive(Deserialize)]
struct LegacyImportV3 {
    legacy_sha256: String,
    state: SavedV3,
}
#[derive(Deserialize)]
struct LegacyImportV4 {
    legacy_sha256: String,
    state: SavedV4,
}
#[derive(Deserialize)]
struct LegacyImportV5 {
    legacy_sha256: String,
    state: SavedV5,
}
#[derive(Deserialize)]
struct LegacyImportV6 {
    legacy_sha256: String,
    state: SavedV6,
}
#[derive(Deserialize)]
struct LegacyImportV7 {
    legacy_sha256: String,
    state: SavedV7,
}
#[derive(Deserialize)]
struct LegacyImportV8 {
    legacy_sha256: String,
    state: SavedV8,
}
impl crate::analytics::payload::Payload for LegacyImport {
    const KIND: u16 = journal::kind::FE04_LEGACY_IMPORT;
    const SCHEMA: u16 = 9;
    fn decode_earlier(schema: u16, bytes: &[u8]) -> Option<std::io::Result<Self>> {
        use crate::analytics::payload::{decode_wire, TYPED};
        match schema {
            TYPED => Some(decode_wire::<LegacyImportV2>(bytes).and_then(|v| {
                Ok(Self {
                    legacy_sha256: v.legacy_sha256,
                    state: earlier(v.state.try_into())?,
                })
            })),
            3 => Some(decode_wire::<LegacyImportV3>(bytes).and_then(|v| {
                Ok(Self {
                    legacy_sha256: v.legacy_sha256,
                    state: earlier(v.state.try_into())?,
                })
            })),
            4 => Some(decode_wire::<LegacyImportV4>(bytes).map(|v| Self {
                legacy_sha256: v.legacy_sha256,
                state: v.state.into(),
            })),
            5 => Some(decode_wire::<LegacyImportV5>(bytes).map(|v| Self {
                legacy_sha256: v.legacy_sha256,
                state: v.state.into(),
            })),
            6 => Some(decode_wire::<LegacyImportV6>(bytes).map(|v| Self {
                legacy_sha256: v.legacy_sha256,
                state: v.state.into(),
            })),
            7 => Some(decode_wire::<LegacyImportV7>(bytes).map(|v| Self {
                legacy_sha256: v.legacy_sha256,
                state: v.state.into(),
            })),
            8 => Some(decode_wire::<LegacyImportV8>(bytes).map(|v| Self {
                legacy_sha256: v.legacy_sha256,
                state: v.state.into(),
            })),
            _ => None,
        }
    }
}
/// A wallet journal holds a handful of state versions; anything larger is not ours.
const MAX_JOURNAL_BYTES: u64 = 16 * 1024 * 1024;
/// A23-07: versions one exit may still write after its reservation (handoff, reopen, observe
/// per signature and change, finalize or non-landing, settle), with margin for observations.
const EXIT_LIFECYCLE_VERSIONS: u64 = 16;
#[cfg(feature = "historical")]
const LEGACY_MAX_BYTES: u64 = 256 * 1024;
fn err(s: &str) -> String {
    s.into()
}
/// M6: `<code>:<class>` — the bounded, secret-free cause of a journal I/O or decode failure: the
/// typed journal code of damaged data (`journal::Code`: FRAME_CRC, MANIFEST_MISMATCH, …), else
/// the OS error class. Never a path, payload or provider text; every such failure still halts
/// and keeps the holds.
pub(crate) fn cause(code: &str, e: &std::io::Error) -> String {
    use std::io::ErrorKind as K;
    let class = match (journal::code_of(e), e.kind()) {
        (Some(journal_code), _) => journal_code,
        (None, K::InvalidData) => "INVALID_DATA",
        (None, K::UnexpectedEof) => "TRUNCATED",
        (None, K::PermissionDenied | K::ReadOnlyFilesystem) => "PERMISSION",
        (None, K::NotFound) => "NOT_FOUND",
        (None, K::StorageFull | K::QuotaExceeded) => "NO_SPACE",
        _ if e.raw_os_error() == Some(libc::ENOSPC) => "NO_SPACE",
        _ => "IO_OTHER",
    };
    format!("{code}:{class}")
}
#[cfg(feature = "historical")]
/// M6: the class of a JSON decode/encode failure (no serde text, which may quote values).
fn json_cause(code: &str, e: &serde_json::Error) -> String {
    use serde_json::error::Category as C;
    let class = match e.classify() {
        C::Io => "IO",
        C::Syntax => "JSON_SYNTAX",
        C::Data => "JSON_DATA",
        C::Eof => "JSON_EOF",
    };
    format!("{code}:{class}")
}
#[cfg(feature = "historical")]
fn digest(v: &[u8]) -> String {
    solana_sdk::hash::hash(v).to_string()
}
fn hex(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn key(s: &str) -> bool {
    s.parse::<solana_sdk::pubkey::Pubkey>().is_ok()
}
/// The wallet lock (flock) of a journal directory; only one holder (runtime, CLI) at a time.
fn lock_wallet(directory: &Path, wallet: &str) -> Result<WalletLock, String> {
    if !key(wallet) {
        return Err(err("WALLET_INVALID"));
    }
    fs::create_dir_all(directory).map_err(|e| cause("JOURNAL_IO", &e))?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join(format!("{wallet}.lock")))
        .map_err(|e| cause("JOURNAL_LOCK_IO", &e))?;
    lock.try_lock().map_err(|_| err("WALLET_ALREADY_LOCKED"))?;
    Ok(WalletLock(lock))
}
/// The migrated pre-binary journal: kept unchanged as evidence, bound by its bytes' hash.
pub fn migrated_legacy(directory: &Path, wallet: &str) -> std::path::PathBuf {
    directory.join(format!("{wallet}.legacy.json"))
}
/// SHA-256 (hex) of a file's bytes (the legacy binding; the file is never parsed here).
fn file_hex(path: &Path) -> Result<String, String> {
    journal::file_sha256(path)
        .map(|h| journal::hex(&h))
        .map_err(|e| cause("JOURNAL_READ", &e))
}
/// The size limit, then the replay of an active wallet journal.
fn replay_checked(path: &Path, wallet: &str) -> Result<(Option<Saved>, Option<String>), String> {
    if fs::metadata(path)
        .map_err(|e| cause("JOURNAL_READ", &e))?
        .len()
        > MAX_JOURNAL_BYTES
    {
        return Err(err("JOURNAL_SIZE_LIMIT"));
    }
    replay(path, wallet)
}
#[cfg(feature = "historical")]
/// Pre-binary wallet journal (`<wallet>.json`, digest-wrapped). Read only by the explicit
/// migration (`Journal::migrate_legacy`), never rewritten.
fn read_legacy(path: &Path, wallet: &str) -> Result<(String, Saved), String> {
    if fs::metadata(path)
        .map_err(|e| cause("JOURNAL_READ", &e))?
        .len()
        > LEGACY_MAX_BYTES
    {
        return Err(err("JOURNAL_SIZE_LIMIT"));
    }
    let bytes = fs::read(path).map_err(|e| cause("JOURNAL_READ", &e))?;
    let wrapper: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| json_cause("JOURNAL_CORRUPT", &e))?;
    let data =
        serde_json::to_vec(&wrapper["record"]).map_err(|e| json_cause("JOURNAL_CORRUPT", &e))?;
    if wrapper["digest"] != digest(&data) {
        return Err(err("JOURNAL_CORRUPT:DIGEST"));
    }
    let s: Saved = serde_json::from_value(wrapper["record"].clone())
        .map_err(|e| json_cause("JOURNAL_CORRUPT", &e))?;
    if s.approval.wallet != wallet || s.intent.wallet != wallet {
        return Err(err("JOURNAL_WALLET_MISMATCH"));
    }
    Ok((journal::hex(&solana_sdk::hash::hash(&bytes).to_bytes()), s))
}
/// Only the transitions the automaton itself performs are accepted on replay.
fn legal(previous: Option<&Saved>, transition: Transition, next: &Saved) -> bool {
    if transition.is_exit() {
        return previous.is_some_and(|p| exit::legal(p, transition, next));
    }
    // The attempt's own transitions happen before any exit.
    if !next.exits.is_empty() || previous.is_some_and(|p| !p.exits.is_empty()) {
        return false;
    }
    let same = |p: &Saved| p.approval == next.approval && p.intent == next.intent;
    // DEC-04: the holds are written by the reservation and never change afterwards.
    if previous.is_some_and(|p| p.hold != next.hold) {
        return false;
    }
    // DEC-03: only a non-landing writes a release class, and only a proving one.
    let released = match (previous, transition) {
        (None, _) => next.non_landing.is_none(),
        (Some(p), Transition::NotLanded) => {
            p.non_landing.is_none()
                && next
                    .non_landing
                    .as_ref()
                    .is_none_or(|r| r.class.proves_non_landing())
        }
        (Some(p), _) => next.non_landing == p.non_landing,
    };
    if !released {
        return false;
    }
    match (previous, transition) {
        (None, Transition::Reserve) => {
            next.phase == Phase::Reserved
                && next.finalized.is_none()
                && next.observations.is_empty()
        }
        (Some(p), Transition::Handoff | Transition::ReopenUnknown) => {
            same(p)
                && p.phase == Phase::Reserved
                && next.phase == Phase::HandoffUnknown
                && next.finalized.is_none()
                && next.observations == p.observations
        }
        (Some(p), Transition::Observe) => {
            same(p)
                && p.phase == Phase::HandoffUnknown
                && next.phase == Phase::HandoffUnknown
                && next.finalized.is_none()
        }
        (Some(p), Transition::Finalize) => {
            same(p)
                && p.phase == Phase::HandoffUnknown
                && next.phase == Phase::HaltFinalized
                && next.finalized.is_some()
                && next.observations == p.observations
        }
        (Some(p), Transition::NotLanded) => {
            same(p)
                && p.phase == Phase::HandoffUnknown
                && next.phase == Phase::HaltFinalized
                && next.finalized.as_ref().is_some_and(Vec::is_empty)
                && p.observations.is_empty()
                && next.observations.is_empty()
        }
        (Some(p), Transition::Settle) => {
            same(p)
                && p.phase == Phase::HaltFinalized
                && matches!(next.phase, Phase::Settled(_))
                && next.finalized == p.finalized
                && next.observations == p.observations
        }
        _ => false,
    }
}
/// F2: a typed frame (schema 2) is decoded natively; a schema-1 frame is a journal written
/// before F2 and is read through the historical value reader only (never written again).
fn decode_frame<P: crate::analytics::payload::Payload>(
    frame: &journal::Frame,
) -> Result<P, String> {
    if frame.schema == 1 {
        return historical_frame(frame);
    }
    frame.typed().map_err(|e| cause("JOURNAL_CORRUPT", &e))
}
/// A schema-1 FE-04 frame through the historical value reader (DEC-09: `historical` build).
#[cfg(feature = "historical")]
fn historical_frame<P: crate::analytics::payload::Payload>(
    frame: &journal::Frame,
) -> Result<P, String> {
    let value = frame
        .legacy_value()
        .map_err(|e| cause("JOURNAL_CORRUPT", &e))?;
    serde_json::from_value(value).map_err(|e| json_cause("JOURNAL_CORRUPT", &e))
}
/// The standard build reads no schema-1 FE-04 frame: such a wallet journal is refused before
/// any admission (fail-closed), never skipped; the `historical` build reads it.
#[cfg(not(feature = "historical"))]
fn historical_frame<P: crate::analytics::payload::Payload>(
    _: &journal::Frame,
) -> Result<P, String> {
    Err(err("JOURNAL_HISTORICAL_SCHEMA_REQUIRES_HISTORICAL_BUILD"))
}
/// Replays the wallet journal. Any damaged, truncated, foreign or illegal frame is a halt.
fn replay(path: &Path, wallet: &str) -> Result<(Option<Saved>, Option<String>), String> {
    let mut reader = journal::Reader::open(path).map_err(|e| cause("JOURNAL_CORRUPT", &e))?;
    let (mut saved, mut imported) = (None::<Saved>, None);
    let mut typed_seen = false;
    while let Some(frame) = reader
        .next_frame()
        .map_err(|e| cause("JOURNAL_CORRUPT", &e))?
    {
        // F2: historical schema-1 FE-04 frames exist only as the prefix written before F2; once
        // a typed frame was written, a schema-1 version is foreign.
        let in_order = |typed_seen: bool| {
            if frame.schema == 1 && typed_seen {
                Err(err("JOURNAL_CORRUPT:SCHEMA_ORDER"))
            } else {
                Ok(())
            }
        };
        // FE-04 archive: an archived journal is sealed; its manifest closes it.
        if frame.kind == journal::MANIFEST {
            continue;
        }
        let next = match frame.kind {
            journal::kind::FE04_LEGACY_IMPORT if frame.journal_seq == 0 => {
                in_order(typed_seen)?;
                let v: LegacyImport = decode_frame(&frame)?;
                imported = Some(v.legacy_sha256);
                v.state
            }
            journal::kind::FE04_TRANSITION => {
                in_order(typed_seen)?;
                let v: Version = decode_frame(&frame)?;
                if !legal(saved.as_ref(), v.transition, &v.state) {
                    return Err(err("JOURNAL_TRANSITION_INVALID"));
                }
                v.state
            }
            journal::kind::FE04_LEGACY_IMPORT => {
                return Err(err("JOURNAL_CORRUPT:IMPORT_POSITION"))
            }
            _ => return Err(err("JOURNAL_CORRUPT:KIND")),
        };
        if next.approval.wallet != wallet || next.intent.wallet != wallet {
            return Err(err("JOURNAL_WALLET_MISMATCH"));
        }
        typed_seen |= frame.schema != 1;
        saved = Some(next);
    }
    Ok((saved, imported))
}

/// Test seam: an attempt of `wallet` in `dir`, reserved and handed off (UNKNOWN) with one bound
/// signature through the real journal, which is closed again; its intent.
#[cfg(all(test, feature = "crate_health"))]
pub(crate) fn test_unknown_attempt(dir: &Path, wallet: &str) -> Intent {
    test_unknown_attempt_held(dir, wallet, None)
}

/// `test_unknown_attempt` reserved with the holds `hold` (DEC-04).
#[cfg(all(test, feature = "crate_health"))]
pub(crate) fn test_unknown_attempt_held(
    dir: &Path,
    wallet: &str,
    hold: Option<crate::execution::hold_b::HoldEvidence>,
) -> Intent {
    let (_, mut a, mut i, c) = tests::setup();
    a.wallet = wallet.into();
    i.wallet = wallet.into();
    let mut j = Journal::open(dir, wallet).unwrap();
    j.reserve_held(a, i.clone(), &c, hold, None).unwrap();
    j.handoff(&i.signed_wire_sha256, &c, || Ok(())).unwrap();
    i
}

impl Journal {
    /// The current attempt's intent (tests: what the admission bound).
    #[cfg(test)]
    pub(crate) fn intent(&self) -> Option<&Intent> {
        self.saved.as_ref().map(|s| &s.intent)
    }

    pub fn open(directory: &Path, wallet: &str) -> Result<Self, String> {
        let lock = lock_wallet(directory, wallet)?;
        let legacy = directory.join(format!("{wallet}.json"));
        // An interrupted pre-binary write blocks further actions; never silently truncate it.
        if legacy.with_extension("pending").exists() {
            return Err(err("JOURNAL_INTERRUPTED_WRITE_HALT"));
        }
        // F8, P0-PROD-CTRL-02: the productive open never parses a pre-binary JSON journal; it is
        // imported only by the explicit migration (`p0ctl fe04 migrate`, `migrate_legacy`).
        if legacy.exists() {
            return Err(err("JOURNAL_LEGACY_REQUIRES_MIGRATION"));
        }
        let migrated = migrated_legacy(directory, wallet);
        let path = directory.join(format!("{wallet}.p0j"));
        let (saved, writer) = if path.exists() {
            let (saved, imported) = replay_checked(&path, wallet)?;
            // The import binds the migrated JSON bytes (checked by hash, never parsed again); a
            // missing or edited file, or one without an import, is a halt.
            match imported {
                Some(sha) if migrated.exists() && file_hex(&migrated)? == sha => {}
                None if !migrated.exists() => {}
                _ => return Err(err("JOURNAL_LEGACY_MISMATCH")),
            }
            let writer =
                journal::Writer::resume(&path).map_err(|e| cause("JOURNAL_CORRUPT", &e))?;
            (saved, writer)
        } else {
            if migrated.exists() {
                return Err(err("JOURNAL_LEGACY_MISMATCH"));
            }
            (
                None,
                journal::Writer::create(&path).map_err(|e| cause("JOURNAL_IO", &e))?,
            )
        };
        let mut this = Self {
            wallet: wallet.into(),
            directory: directory.to_path_buf(),
            saved,
            writer,
            _lock: lock,
        };
        if let Some(mut prior) = this.saved.clone() {
            if prior.phase == Phase::Reserved {
                prior.phase = Phase::HandoffUnknown;
                this.persist(prior, Transition::ReopenUnknown)?;
            }
        }
        this.reopen_exit()?;
        Ok(this)
    }
    #[cfg(feature = "historical")]
    /// F8 explicit migration (`p0ctl fe04 migrate`) of a pre-binary `<wallet>.json` journal,
    /// offline and under the wallet lock: the JSON is decoded once into the typed import frame
    /// (fsynced), then kept unchanged as hash-bound evidence `<wallet>.legacy.json`. A crash after
    /// the durable import only leaves the rename, which a second migration completes (no second
    /// import). Returns the bound SHA-256 of the JSON bytes.
    pub fn migrate_legacy(directory: &Path, wallet: &str) -> Result<String, String> {
        let lock = lock_wallet(directory, wallet)?;
        let legacy = directory.join(format!("{wallet}.json"));
        if legacy.with_extension("pending").exists() {
            return Err(err("JOURNAL_INTERRUPTED_WRITE_HALT"));
        }
        if !legacy.exists() {
            return Err(err("JOURNAL_LEGACY_ABSENT"));
        }
        let migrated = migrated_legacy(directory, wallet);
        if migrated.exists() {
            return Err(err("JOURNAL_LEGACY_MISMATCH"));
        }
        let (sha, state) = read_legacy(&legacy, wallet)?;
        let path = directory.join(format!("{wallet}.p0j"));
        let (saved, imported, writer) = if path.exists() {
            let (saved, imported) = replay_checked(&path, wallet)?;
            let writer =
                journal::Writer::resume(&path).map_err(|e| cause("JOURNAL_CORRUPT", &e))?;
            (saved, imported, writer)
        } else {
            (
                None,
                None,
                journal::Writer::create(&path).map_err(|e| cause("JOURNAL_IO", &e))?,
            )
        };
        let mut this = Self {
            wallet: wallet.into(),
            directory: directory.to_path_buf(),
            saved,
            writer,
            _lock: lock,
        };
        match imported {
            Some(done) if done == sha => {}
            None if this.saved.is_none() => {
                this.append(&LegacyImport {
                    legacy_sha256: sha.clone(),
                    state: state.clone(),
                })?;
                this.saved = Some(state);
            }
            _ => return Err(err("JOURNAL_LEGACY_MISMATCH")),
        }
        fs::rename(&legacy, &migrated).map_err(|e| cause("JOURNAL_LEGACY_RENAME", &e))?;
        fs::File::open(directory)
            .and_then(|d| d.sync_all())
            .map_err(|e| cause("JOURNAL_LEGACY_SYNC", &e))?;
        Ok(sha)
    }
    /// Appends one typed frame and returns only after fsync confirmed exactly that frame.
    fn append<P: crate::analytics::payload::Payload>(&mut self, value: &P) -> Result<(), String> {
        let provenance = journal::Provenance {
            source: journal::source::FE04,
            source_seq: self.writer.next_seq(),
            ..Default::default()
        };
        let frame = self
            .writer
            .append_typed(provenance, value)
            .map_err(|e| cause("JOURNAL_WRITE", &e))?;
        let durable = self.writer.sync().map_err(|e| cause("JOURNAL_FSYNC", &e))?;
        if durable.journal_seq != frame.journal_seq || durable.hash != frame.hash {
            return Err(err("JOURNAL_DURABILITY_UNCONFIRMED"));
        }
        Ok(())
    }
    /// The in-memory state advances only after the new version is durable. A failed write
    /// poisons the writer, so every later transition (and thus every send) is refused.
    fn persist(&mut self, saved: Saved, transition: Transition) -> Result<(), String> {
        let version = Version {
            transition,
            state: saved,
        };
        self.append(&version)?;
        self.saved = Some(version.state);
        Ok(())
    }
    #[cfg(test)]
    fn reserve(&mut self, approval: Approval, intent: Intent, now: &Current) -> Result<(), String> {
        self.reserve_held(approval, intent, now, None, None)
    }
    /// P8-CTRL.5: the held attempt's FE-06 booking (`None`: none held, or booked under none).
    pub fn attempt_booking(&self) -> Option<&Fe06Attempt> {
        self.saved.as_ref().and_then(|s| s.fe06.as_ref())
    }
    /// P8-CTRL.5: every own attempt of this wallet (archived and active) — what the FE-06 limits
    /// count.
    pub fn attempt_records(
        &self,
    ) -> Result<Vec<crate::execution::accounting::AttemptRecord>, String> {
        archive::records(&self.directory, &self.wallet)
    }
    /// The reservation with the attempt's holds (DEC-04, `None`: OFF).
    fn reserve_held(
        &mut self,
        approval: Approval,
        intent: Intent,
        now: &Current,
        hold: Option<crate::execution::hold_b::HoldEvidence>,
        fe06: Option<Fe06Attempt>,
    ) -> Result<(), String> {
        if self.saved.is_some() {
            return Err(err("ONE_SHOT_ALREADY_CONSUMED_OR_UNKNOWN"));
        }
        if approval.wallet != self.wallet {
            return Err(err("JOURNAL_WALLET_MISMATCH"));
        }
        // FE-04 archive: an approval authorizes one attempt, never a second after archiving.
        archive::single_use(&self.directory, &self.wallet, &approval.approval_id)?;
        validate(&approval, &intent, now)?;
        self.persist(
            Saved {
                approval,
                intent,
                phase: Phase::Reserved,
                finalized: None,
                observations: BTreeMap::new(),
                exits: Vec::new(),
                non_landing: None,
                hold,
                fe06,
            },
            Transition::Reserve,
        )
    }
    /// The callback must be the existing adapter's single submit/submit_bundle
    /// invocation over exactly the bound bytes. No automatic retry is provided.
    fn handoff<T>(
        &mut self,
        wire_sha256: &str,
        now: &Current,
        send: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let mut s = self.saved.clone().ok_or_else(|| err("NOT_RESERVED"))?;
        if s.phase != Phase::Reserved {
            return Err(err("ONE_SHOT_HALTED"));
        }
        if s.intent.signed_wire_sha256 != wire_sha256 {
            return Err(err("SIGNED_PAYLOAD_CHANGED"));
        }
        validate(&s.approval, &s.intent, now)?;
        s.phase = Phase::HandoffUnknown;
        self.persist(s, Transition::Handoff)?; // durable BEFORE any possible acceptance; errors retain holds
        send()
    }
    /// Reopening never authorizes a handoff, even after a pre-send crash.
    pub fn reconcile(&mut self, receipts: Vec<Receipt>) -> Result<(), String> {
        let mut s = self.saved.clone().ok_or_else(|| err("NO_ATTEMPT"))?;
        if s.phase != Phase::HandoffUnknown {
            return Err(err("NOT_UNKNOWN"));
        }
        let signatures: BTreeSet<_> = receipts.iter().map(|r| r.signature.clone()).collect();
        if signatures.len() != receipts.len()
            || signatures != s.intent.signatures.iter().cloned().collect()
            || receipts.iter().any(|r| {
                !r.finalized
                    || r.fee_lamports.is_none()
                    || !r.exit_complete
                    || r.wallet_deltas.as_ref().is_none_or(|d| {
                        !s.intent
                            .expected_wallet_assets
                            .iter()
                            .all(|k| d.contains_key(k))
                    })
            })
        {
            return Err(err("RECONCILIATION_INCOMPLETE_HOLD_RETAINED"));
        }
        s.phase = Phase::HaltFinalized;
        s.finalized = Some(receipts);
        self.persist(s, Transition::Finalize)
    }
    /// P8-RES-02: the attempt this journal still holds — reserved, or handed off with an UNKNOWN
    /// outcome — or else its open exit, to rebuild the global reservation book after a restart;
    /// `None` once finalized.
    pub fn active_hold(&self) -> Option<ActiveHold> {
        let s = self.saved.as_ref()?;
        if s.phase.finalized() {
            return exit::exit_hold(s);
        }
        Some(ActiveHold {
            attempt: s.approval.approval_id.clone(),
            target: s.intent.target.clone(),
            unknown_outcome: s.phase == Phase::HandoffUnknown,
            holds: s.intent.holds.clone(),
            signature: s.intent.signatures.first().cloned(),
            last_valid_block_height: s.intent.last_valid_block_height,
        })
    }
    /// B3: what the safety layer replays after a restart — `None` without any attempt; with one,
    /// the one-shot is consumed, so its HALT is sticky; until it is finalized it still binds its
    /// capital; once settled (C3) its booking — the persisted booking day (a settlement before
    /// schema 4: the evaluation day), realized net, outcome and open inventory — is the same the
    /// running owner booked.
    pub fn safety_replay(&self) -> Option<SafetyReplay> {
        self.saved.as_ref().map(safety_replay_of)
    }
    pub fn holds(&self) -> BTreeMap<String, u64> {
        self.active_hold().map(|h| h.holds).unwrap_or_default()
    }
}

impl Approval {
    /// FE-04: the approval alone, before any intent — schema 1, authorized, its ids, keys,
    /// funding class and bounds well formed, and not expired at `now_unix_s`.
    pub fn check(&self, now_unix_s: u64) -> Result<(), String> {
        if !well_formed(self) {
            return Err(err("INVALID_OR_UNAUTHORIZED_APPROVAL"));
        }
        if now_unix_s >= self.expires_unix_s {
            return Err(err("APPROVAL_EXPIRED"));
        }
        Ok(())
    }
}

fn well_formed(a: &Approval) -> bool {
    !(a.schema != 1
        || !a.authorized
        || a.approval_id.is_empty()
        || !hex(&a.code_sha, 40)
        || !hex(&a.binary_sha256, 64)
        || !key(&a.wallet)
        || !key(&a.program)
        || a.program_data_hash.is_empty()
        || a.allowed_targets.is_empty()
        || a.allowed_targets.iter().any(|k| !key(k))
        || !matches!(
            a.funding_class.as_str(),
            "NO_SWAP_INVENTORY" | "ZERO_PREFUND_SAME_TX" | "JIT_SHORTFALL_EXACT_IN"
        )
        || a.min_net_usd_micro == 0
        || a.max_state_age_ms == 0
        || a.max_slippage_bps > 10_000)
}

fn validate(a: &Approval, i: &Intent, c: &Current) -> Result<(), String> {
    if !well_formed(a) {
        return Err(err("INVALID_OR_UNAUTHORIZED_APPROVAL"));
    }
    if (
        i.code_sha.as_str(),
        i.binary_sha256.as_str(),
        i.wallet.as_str(),
        i.program.as_str(),
        i.program_data_hash.as_str(),
        i.funding_class.as_str(),
    ) != (
        a.code_sha.as_str(),
        a.binary_sha256.as_str(),
        a.wallet.as_str(),
        a.program.as_str(),
        a.program_data_hash.as_str(),
        a.funding_class.as_str(),
    ) || !a.allowed_targets.contains(&i.target)
        || !hex(&i.signed_wire_sha256, 64)
    {
        return Err(err("APPROVAL_BINDING_MISMATCH"));
    }
    let unique: BTreeSet<_> = i.signatures.iter().collect();
    if unique.is_empty()
        || unique.len() > 5
        || unique.len() != i.signatures.len()
        || i.signatures
            .iter()
            .any(|s| s.parse::<solana_sdk::signature::Signature>().is_err())
    {
        return Err(err("SIGNATURE_BINDING_INVALID"));
    }
    if c.now_unix_ms / 1000 >= a.expires_unix_s
        || c.now_unix_ms < i.evaluated_unix_ms
        || c.now_unix_ms - i.evaluated_unix_ms > a.max_state_age_ms
        || c.slot < i.state_slot
        || c.slot > i.valid_until_slot
        || c.state_generation != i.state_generation
        || c.program_data_hash != a.program_data_hash
        || !c.canonical_fresh_no_gap_or_fork
    {
        return Err(err("STALE_FORK_GAP_OR_UPGRADE"));
    }
    if !c.health_funding_route_fee_exit_known
        || !c.exact_signed_payload_simulated
        || (i.signatures.len() > 1 && !c.bundle_simulated)
    {
        return Err(err("CURRENT_EXECUTION_EVIDENCE_UNKNOWN"));
    }
    let debit = i
        .transaction_fee_lamports
        .checked_add(i.tip_lamports)
        .and_then(|n| n.checked_add(i.rent_lamports))
        .ok_or_else(|| err("COST_OVERFLOW"))?;
    if i.capital_usd_micro > a.max_capital_usd_micro
        || debit > a.max_total_sol_debit_lamports
        || i.tip_lamports > a.max_tip_lamports
        || i.rent_lamports > a.max_rent_lamports
        || i.max_loss_usd_micro > a.max_loss_usd_micro
        || i.conservative_net_usd_micro < i128::from(a.min_net_usd_micro)
        || i.slippage_bps > a.max_slippage_bps
    {
        return Err(err("APPROVAL_BUDGET_EXCEEDED"));
    }
    if !i.expected_wallet_assets.contains("SOL")
        || !i.holds.keys().all(|k| i.expected_wallet_assets.contains(k))
        || i.expected_wallet_assets
            .iter()
            .any(|k| k != "SOL" && !key(k))
        || i.holds.get("SOL").copied().unwrap_or(0) < debit
        || i.holds.keys().any(|k| k != "SOL" && !key(k))
        || i.holds.iter().any(|(mint, n)| {
            c.balances.get(mint).is_none_or(|balance| {
                let elsewhere = c.reserved_elsewhere.get(mint).copied().unwrap_or(0);
                let reserve = c.policy_reserve.get(mint).copied().unwrap_or(0);
                balance.saturating_sub(elsewhere.saturating_add(reserve)) < *n
            })
        })
    {
        return Err(err("WALLET_UNDERFUNDED_OR_HOLD_MISSING"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    /// F2: the bincode layout of a wallet-journal transition is pinned. A change to `Version`,
    /// `Saved` or anything inside must bump the FE04 schema with a reader for the old one;
    /// otherwise existing wallet journals would halt on replay.
    #[test]
    fn f2_wallet_transition_layout_is_pinned() {
        let key = |n: u8| solana_sdk::pubkey::Pubkey::new_from_array([n; 32]).to_string();
        let approval = Approval {
            schema: 1,
            authorized: true,
            approval_id: "approval-1".into(),
            code_sha: "a".repeat(40),
            binary_sha256: "b".repeat(64),
            wallet: key(1),
            program: key(2),
            program_data_hash: "pd".into(),
            funding_class: "NO_SWAP_INVENTORY".into(),
            allowed_targets: [key(3)].into_iter().collect(),
            expires_unix_s: 1_800_000_000,
            max_state_age_ms: 2_000,
            max_capital_usd_micro: 10_000_000,
            max_total_sol_debit_lamports: 50_000,
            max_tip_lamports: 1_000,
            max_rent_lamports: 3_000_000,
            max_loss_usd_micro: 1_000_000,
            min_net_usd_micro: 100,
            max_slippage_bps: 50,
        };
        let intent = Intent {
            code_sha: "a".repeat(40),
            binary_sha256: "b".repeat(64),
            wallet: key(1),
            program: key(2),
            program_data_hash: "pd".into(),
            funding_class: "NO_SWAP_INVENTORY".into(),
            target: key(3),
            signatures: vec!["sig-1".into()],
            signed_wire_sha256: "c".repeat(64),
            state_generation: 7,
            state_slot: 300,
            valid_until_slot: 450,
            evaluated_unix_ms: 1_760_000_000_000,
            capital_usd_micro: 5_000_000,
            transaction_fee_lamports: 5_000,
            tip_lamports: 0,
            rent_lamports: 0,
            max_loss_usd_micro: 500_000,
            conservative_net_usd_micro: -12,
            slippage_bps: 30,
            holds: [("SOL".to_string(), 5_000u64)].into_iter().collect(),
            expected_wallet_assets: ["SOL".to_string()].into_iter().collect(),
            last_valid_block_height: Some(500),
        };
        let state = Saved {
            approval,
            intent,
            phase: Phase::HaltFinalized,
            finalized: Some(vec![Receipt {
                signature: "sig-1".into(),
                finalized: true,
                success: true,
                fee_lamports: Some(5_000),
                wallet_deltas: Some([("SOL".to_string(), -5_000i128)].into_iter().collect()),
                exit_complete: true,
            }]),
            observations: [(
                "sig-1".to_string(),
                receipts::FinalizedObservation {
                    signature: "sig-1".into(),
                    slot: 301,
                    success: true,
                    fee_lamports: 5_000,
                    wallet_sol_delta_lamports: -5_000,
                    wallet_token_deltas: None,
                    unknown_reasons: vec![],
                },
            )]
            .into_iter()
            .collect(),
            exits: Vec::new(),
            non_landing: None,
            hold: None,
            fe06: None,
        };
        // Schema 2 (before C12): the mirror reproduces the historical pinned bytes, which still
        // read — with the expiry unknown (`None`), never a zero.
        let i = &state.intent;
        let v2 = VersionV2 {
            transition: Transition::Finalize,
            state: SavedV2 {
                approval: state.approval.clone(),
                intent: IntentV2 {
                    code_sha: i.code_sha.clone(),
                    binary_sha256: i.binary_sha256.clone(),
                    wallet: i.wallet.clone(),
                    program: i.program.clone(),
                    program_data_hash: i.program_data_hash.clone(),
                    funding_class: i.funding_class.clone(),
                    target: i.target.clone(),
                    signatures: i.signatures.clone(),
                    signed_wire_sha256: i.signed_wire_sha256.clone(),
                    state_generation: i.state_generation,
                    state_slot: i.state_slot,
                    valid_until_slot: i.valid_until_slot,
                    evaluated_unix_ms: i.evaluated_unix_ms,
                    capital_usd_micro: i.capital_usd_micro,
                    transaction_fee_lamports: i.transaction_fee_lamports,
                    tip_lamports: i.tip_lamports,
                    rent_lamports: i.rent_lamports,
                    max_loss_usd_micro: i.max_loss_usd_micro,
                    conservative_net_usd_micro: i.conservative_net_usd_micro,
                    slippage_bps: i.slippage_bps,
                    holds: i.holds.clone(),
                    expected_wallet_assets: i.expected_wallet_assets.clone(),
                },
                phase: PhaseV3::HaltFinalized,
                finalized: state.finalized.clone(),
                observations: state.observations.clone(),
            },
        };
        let old = crate::analytics::payload::encode_wire(&v2).unwrap();
        assert_eq!(
            (old.len(), solana_sdk::hash::hash(&old).to_string()),
            (
                1108,
                "HwhygqtWc6VthHdUKxendZqFqfWF593a6mo8EFvxmWtn".to_string()
            ),
            "FE04_TRANSITION schema-2 layout changed"
        );
        let back: Version = crate::analytics::payload::decode(
            journal::kind::FE04_TRANSITION,
            crate::analytics::payload::TYPED,
            &old,
        )
        .unwrap();
        let mut unknown_expiry = state.clone();
        unknown_expiry.intent.last_valid_block_height = None;
        assert_eq!(
            (back.transition, back.state),
            (Transition::Finalize, unknown_expiry)
        );
        // Schema 3 (C12): the intent carries the blockhash expiry; the mirror reproduces the
        // pinned bytes, which read unchanged.
        let v3 = |transition, phase| VersionV3 {
            transition,
            state: SavedV3 {
                approval: state.approval.clone(),
                intent: state.intent.clone(),
                phase,
                finalized: state.finalized.clone(),
                observations: state.observations.clone(),
            },
        };
        let bytes = crate::analytics::payload::encode_wire(&v3(
            Transition::Finalize,
            PhaseV3::HaltFinalized,
        ))
        .unwrap();
        assert_eq!(
            (bytes.len(), solana_sdk::hash::hash(&bytes).to_string()),
            (
                1117,
                "BziVEDHHNU4NgYv4LyN6uU9EvMqKbcDWsR7joesPBsJA".to_string()
            ),
            "FE04_TRANSITION schema-3 layout changed"
        );
        let back: Version =
            crate::analytics::payload::decode(journal::kind::FE04_TRANSITION, 3, &bytes).unwrap();
        assert_eq!(
            (back.transition, back.state),
            (Transition::Finalize, state.clone())
        );
        // Schema 4: an unsettled phase encodes as in schema 3.
        let v4 = |transition, s: &Saved| VersionV4 {
            transition,
            state: SavedV4 {
                approval: s.approval.clone(),
                intent: s.intent.clone(),
                phase: s.phase.clone(),
                finalized: s.finalized.clone(),
                observations: s.observations.clone(),
            },
        };
        assert_eq!(
            crate::analytics::payload::encode_wire(&v4(Transition::Finalize, &state)).unwrap(),
            bytes
        );
        // A schema-3 settlement (one net incl. the token inflow) reads reclassified: realized the
        // SOL part, the inflow open inventory, no booking second; parts not summing to its net
        // are damaged data.
        let legacy = |net| settlement::SettlementV3 {
            landed: true,
            landed_success: true,
            fee_lamports: 5_000,
            wallet_deltas: [("SOL".to_string(), -5_000), ("MINT".to_string(), 1_000)].into(),
            valuation: [("SOL".to_string(), -500), ("MINT".to_string(), 900)].into(),
            realized_net_usd_micro: net,
        };
        let settled = settlement::Settlement {
            landed: true,
            landed_success: true,
            fee_lamports: 5_000,
            wallet_deltas: [("SOL".to_string(), -5_000), ("MINT".to_string(), 1_000)].into(),
            valuation: [("SOL".to_string(), -500), ("MINT".to_string(), 900)].into(),
            realized_net_usd_micro: -500,
            open_inventory: [("MINT".to_string(), 1_000)].into(),
            unrealized_inventory_usd_micro: 900,
            booked_unix_s: None,
        };
        let old = crate::analytics::payload::encode_wire(&v3(
            Transition::Settle,
            PhaseV3::Settled(legacy(400)),
        ))
        .unwrap();
        let back: Version =
            crate::analytics::payload::decode(journal::kind::FE04_TRANSITION, 3, &old).unwrap();
        assert_eq!(back.state.phase, Phase::Settled(settled.clone()));
        let damaged = crate::analytics::payload::encode_wire(&v3(
            Transition::Settle,
            PhaseV3::Settled(legacy(401)),
        ))
        .unwrap();
        assert!(crate::analytics::payload::decode::<Version>(
            journal::kind::FE04_TRANSITION,
            3,
            &damaged
        )
        .is_err());
        // Schema 4 settled layout (split net, open inventory, booking second): the mirror
        // reproduces the pinned bytes, which read without exits.
        let mut booked = state.clone();
        booked.phase = Phase::Settled(settlement::Settlement {
            booked_unix_s: Some(1_760_000_100),
            ..settled
        });
        let bytes =
            crate::analytics::payload::encode_wire(&v4(Transition::Settle, &booked)).unwrap();
        assert_eq!(
            (bytes.len(), solana_sdk::hash::hash(&bytes).to_string()),
            (
                1330,
                "3GKXhSzPgSVs6BGS6wWwUTSuU2vNKiccKiKQsL8vJcwQ".to_string()
            ),
            "FE04_TRANSITION schema-4 layout changed"
        );
        let back: Version =
            crate::analytics::payload::decode(journal::kind::FE04_TRANSITION, 4, &bytes).unwrap();
        assert_eq!(
            (back.transition, back.state),
            (Transition::Settle, booked.clone())
        );
        // Schema 5 (exits, before the DEC-03 release class): the mirror reproduces the pinned
        // bytes with one settled exit of the open position, which read without a class.
        let mut exited = booked;
        exited.exits.push(exit::ExitAttempt {
            approval: exit::ExitApproval {
                schema: 1,
                authorized: true,
                approval_id: "exit-1".into(),
                attempt_approval_id: "approval-1".into(),
                code_sha: "a".repeat(40),
                binary_sha256: "b".repeat(64),
                wallet: key(1),
                mint: key(4),
                route_program: key(5),
                expires_unix_s: 1_760_000_900,
                max_state_age_ms: 2_000,
                min_out_lamports: 9_000_000,
                max_slippage_bps: 50,
                max_fee_lamports: 10_000,
            },
            intent: exit::ExitIntent {
                code_sha: "a".repeat(40),
                binary_sha256: "b".repeat(64),
                wallet: key(1),
                mint: key(4),
                route_program: key(5),
                amount_in: 1_000,
                minimum_out_lamports: 9_500_000,
                slippage_bps: 30,
                transaction_fee_lamports: 5_000,
                signatures: vec!["sig-2".into()],
                signed_wire_sha256: "d".repeat(64),
                state_slot: 900,
                evaluated_unix_ms: 1_760_000_500_000,
                last_valid_block_height: 1_200,
            },
            phase: exit::ExitPhase::Settled(exit::ExitSettlement {
                landed: true,
                landed_success: true,
                fee_lamports: 5_000,
                wallet_deltas: [("SOL".to_string(), 9_995_000), (key(4), -1_000)].into(),
                sold_units: 1_000,
                realized_usd_micro: 999_500,
                released_mark_usd_micro: 900,
                booked_unix_s: 1_760_000_600,
            }),
            observations: BTreeMap::new(),
            finalized: Some(Vec::new()),
            non_landing: None,
            fe06: None,
        });
        let v5 = VersionV5 {
            transition: Transition::ExitSettle,
            state: SavedV5 {
                approval: exited.approval.clone(),
                intent: exited.intent.clone(),
                phase: exited.phase.clone(),
                finalized: exited.finalized.clone(),
                observations: exited.observations.clone(),
                exits: exited
                    .exits
                    .iter()
                    .map(|e| exit::ExitAttemptV5 {
                        approval: e.approval.clone(),
                        intent: e.intent.clone(),
                        phase: e.phase.clone(),
                        observations: e.observations.clone(),
                        finalized: e.finalized.clone(),
                    })
                    .collect(),
            },
        };
        let bytes = crate::analytics::payload::encode_wire(&v5).unwrap();
        let back: Version =
            crate::analytics::payload::decode(journal::kind::FE04_TRANSITION, 5, &bytes).unwrap();
        assert_eq!(
            (back.transition, back.state),
            (Transition::ExitSettle, exited.clone())
        );
        assert_eq!(
            (bytes.len(), solana_sdk::hash::hash(&bytes).to_string()),
            (
                2291,
                "BEzUJj5D5txEFENAAc2cDa4rUn8TzWzdfn9wrcbAihgS".to_string()
            ),
            "FE04_TRANSITION schema-5 layout changed"
        );
        // Schema 6 (DEC-03) is pinned with the exit released as not landed, C alone.
        let mut released = exited;
        let e = released.exits.last_mut().unwrap();
        e.phase = exit::ExitPhase::Finalized;
        e.non_landing = Some(non_landing::NonLandingRelease {
            class: non_landing::Verdict::CAloneQualified,
            usable_operators: ["op-a".to_string()].into(),
            c_blocks: 4,
            c_through_slot: 960,
        });
        // The schema-6 mirror reproduces its pinned bytes, which read without holds.
        let v6 = VersionV6 {
            transition: Transition::ExitNotLanded,
            state: SavedV6 {
                approval: released.approval.clone(),
                intent: released.intent.clone(),
                phase: released.phase.clone(),
                finalized: released.finalized.clone(),
                observations: released.observations.clone(),
                exits: released.exits.iter().cloned().map(Into::into).collect(),
                non_landing: released.non_landing.clone(),
            },
        };
        let bytes = crate::analytics::payload::encode_wire(&v6).unwrap();
        let back: Version =
            crate::analytics::payload::decode(journal::kind::FE04_TRANSITION, 6, &bytes).unwrap();
        assert_eq!(
            (back.transition, back.state),
            (Transition::ExitNotLanded, released.clone())
        );
        assert_eq!(
            (bytes.len(), solana_sdk::hash::hash(&bytes).to_string()),
            (
                2173,
                "AcBexqVXe5JywXEdEuG3dMY31k2bY6Z3qwjVUqS8bYZr".to_string()
            ),
            "FE04_TRANSITION schema-6 layout changed"
        );
        // Schema 7 (DEC-04) is pinned with the attempt's holds: OBSERVE, A's 1.1 USD, an exact B.
        let mut held = released;
        held.hold = Some(crate::execution::hold_b::HoldEvidence {
            mode: crate::execution::hold_b::HoldMode::Observe,
            a_inventory_usd_micro: 1_100,
            b: crate::execution::hold_b::BHold::Exact(crate::execution::hold_b::PlanFlows {
                flows: [(
                    "SOL".to_string(),
                    crate::execution::hold_b::AssetFlow {
                        reserve: 5_000,
                        residual_max: Some(0),
                    },
                )]
                .into(),
                inventory_usd_micro: 100,
                kind: "PLAIN".into(),
            }),
            reserved_inventory_usd_micro: 1_100,
        });
        // The schema-7 mirror reproduces its pinned bytes, which read without a grant booking.
        let v7 = VersionV7 {
            transition: Transition::ExitNotLanded,
            state: SavedV7 {
                approval: held.approval.clone(),
                intent: held.intent.clone(),
                phase: held.phase.clone(),
                finalized: held.finalized.clone(),
                observations: held.observations.clone(),
                exits: held.exits.iter().cloned().map(Into::into).collect(),
                non_landing: held.non_landing.clone(),
                hold: held.hold.clone(),
            },
        };
        let bytes = crate::analytics::payload::encode_wire(&v7).unwrap();
        let back: Version =
            crate::analytics::payload::decode(journal::kind::FE04_TRANSITION, 7, &bytes).unwrap();
        assert_eq!(
            (back.transition, back.state),
            (Transition::ExitNotLanded, held.clone())
        );
        assert_eq!(
            (bytes.len(), solana_sdk::hash::hash(&bytes).to_string()),
            (
                2255,
                "CWE1yXko1Hdpj78WU9rPSahgUq236ba4YqZw47LR4qAz".to_string()
            ),
            "FE04_TRANSITION schema-7 layout changed"
        );
        // Schema 8 (P8-CTRL.5) is pinned with the attempt's FE-06 grant booking.
        let mut booked = held;
        booked.fe06 = Some(Fe06Attempt {
            grant: "a".repeat(64),
            campaign: "first-profit".into(),
            utc_day: 20_390,
            owner_epoch: Some(3),
        });
        let v8 = VersionV8 {
            transition: Transition::ExitNotLanded,
            state: SavedV8 {
                approval: booked.approval.clone(),
                intent: booked.intent.clone(),
                phase: booked.phase.clone(),
                finalized: booked.finalized.clone(),
                observations: booked.observations.clone(),
                exits: booked.exits.iter().cloned().map(Into::into).collect(),
                non_landing: booked.non_landing.clone(),
                hold: booked.hold.clone(),
                fe06: booked.fe06.clone(),
            },
        };
        let bytes = crate::analytics::payload::encode_wire(&v8).unwrap();
        let back: Version =
            crate::analytics::payload::decode(journal::kind::FE04_TRANSITION, 8, &bytes).unwrap();
        assert_eq!(
            (back.transition, back.state),
            (Transition::ExitNotLanded, booked.clone())
        );
        assert_eq!(
            (bytes.len(), solana_sdk::hash::hash(&bytes).to_string()),
            (
                2365,
                "3uDH7ZajpnVMCcQXa22BxtkJEQPk1HadsXXPFDrciaX5".to_string()
            ),
            "FE04_TRANSITION schema-8 layout changed"
        );
        // Schema 9 (P8-CTRL.5) is pinned with the exit's own FE-06 grant booking.
        let mut exit_booked = booked;
        assert!(!exit_booked.exits.is_empty());
        for e in &mut exit_booked.exits {
            e.fe06 = exit_booked.fe06.clone();
        }
        let version = Version {
            transition: Transition::ExitNotLanded,
            state: exit_booked.clone(),
        };
        let bytes = crate::analytics::payload::encode(&version).unwrap();
        let back: Version =
            crate::analytics::payload::decode(journal::kind::FE04_TRANSITION, 9, &bytes).unwrap();
        assert_eq!(
            (back.transition, back.state),
            (Transition::ExitNotLanded, exit_booked)
        );
        assert_eq!(
            (bytes.len(), solana_sdk::hash::hash(&bytes).to_string()),
            (
                2475,
                "J4TEw27jn4U7gQLLpHMgUgm9rrEKq7thpaNjkFxk3ALj".to_string()
            ),
            "FE04_TRANSITION schema-9 layout changed"
        );
    }

    /// P8-CTRL.5/.8 (review M3): `live attempts` counts each booking where it belongs — the
    /// UTC day, its campaign, its grant — names an exhausted quota, and counts an attempt
    /// without a grant apart.
    #[test]
    fn p8ctrl04_the_attempts_report_counts_each_booking_where_it_belongs() {
        use crate::execution::accounting::{AttemptRecord, StepState};
        use crate::orchestration::fe06;
        let record = |id: &str, fe06: Option<Fe06Attempt>| AttemptRecord {
            approval_id: id.into(),
            target: "t".into(),
            funding_class: "ZERO_PREFUND_SAME_TX".into(),
            archived: id != "a4",
            state: StepState::Settled,
            settlement: None,
            exits: vec![],
            open_positions: Default::default(),
            hold: None,
            fe06,
        };
        let booking = |grant: &str, campaign: &str, utc_day| {
            Some(Fe06Attempt {
                grant: grant.into(),
                campaign: campaign.into(),
                utc_day,
                owner_epoch: Some(3),
            })
        };
        let scope = fe06::parse(
            fe06::tests::text(
                &solana_sdk::pubkey::Pubkey::new_unique(),
                &[
                    ("campaign_id", "k1"),
                    ("max_attempts_per_utc_day", "2"),
                    ("max_attempts_campaign", "5"),
                    ("max_attempts_grant", "2"),
                ],
            )
            .as_bytes(),
            &fe06::tests::ceilings(),
        )
        .unwrap();
        let g = "g".repeat(64);
        let mut records = [
            record("a1", booking(&g, "k1", 10)),
            record("a2", booking(&g, "k1", 9)),
            record("a3", booking("h", "k2", 10)),
            record("a4", None),
        ];
        // P8-CTRL.5: an exit of a1 booked against the grant.
        records[0]
            .exits
            .push(crate::execution::accounting::ExitRecord {
                approval_id: "x1".into(),
                mint: "m".into(),
                state: StepState::Unknown,
                settlement: None,
                fe06: booking(&g, "k1", 10),
            });
        let lines = fe06_attempts_report(&records, "W", 10, Some((&g, &scope)));
        assert_eq!(
            lines[0],
            "FE-06 attempts of wallet W: 3 booked, 1 without a grant (none, or before P8-CTRL)"
        );
        let expected: Vec<String> = vec![
            format!("  approval=a1 state=SETTLED archived grant={g} campaign=k1 utc_day=10 owner_epoch=3"),
            "  UTC_DAY day 10 (today): 2 used".into(),
            "  CAMPAIGN k1: 2 used".into(),
            "  CAMPAIGN k2: 1 used".into(),
            format!("  GRANT {g}: 2 used"),
            "  GRANT h: 1 used".into(),
            "  UTC_DAY day 10 (today): 2 of 2 · EXHAUSTED (no new attempt)".into(),
            "  CAMPAIGN k1: 2 of 5".into(),
            format!("  GRANT {g}: 2 of 2 · EXHAUSTED (no new attempt)"),
            format!("  EXITS GRANT {g}: 1 of 1 · EXHAUSTED (no new exit)"),
        ];
        for e in &expected {
            assert!(lines.contains(e), "{e}\n{lines:#?}");
        }
        // The next day's quota is new; the campaign's and the grant's are not.
        let tomorrow = fe06_attempts_report(&records, "W", 11, Some((&g, &scope)));
        assert!(tomorrow.contains(&"  UTC_DAY day 11 (today): 0 of 2".to_string()));
        assert!(tomorrow.contains(&format!("  GRANT {g}: 2 of 2 · EXHAUSTED (no new attempt)")));
    }

    /// P8-CTRL.5: the attempts booked against FE-06 grants in the wallet's journals bound the
    /// next new attempt — per UTC day, per campaign and per grant, each against the scope; a
    /// reserved (or UNKNOWN) attempt counts, one of another day, campaign or grant not.
    #[test]
    fn p8ctrl03_booked_attempts_bound_the_next_one_per_day_campaign_and_grant() {
        use crate::orchestration::fe06;
        let (dir, a, i, c) = setup();
        let booking = |grant: &str, campaign: &str, utc_day| Fe06Attempt {
            grant: grant.into(),
            campaign: campaign.into(),
            utc_day,
            owner_epoch: Some(1),
        };
        let mut j = Journal::open(&dir, &a.wallet).unwrap();
        j.reserve_held(
            a.clone(),
            i.clone(),
            &c,
            None,
            Some(booking("g1", "k1", 10)),
        )
        .unwrap();
        assert_eq!(
            j.attempt_records().unwrap()[0].fe06,
            Some(booking("g1", "k1", 10))
        );
        let scope = |day: &str, campaign: &str, grant: &str| {
            fe06::parse(
                fe06::tests::text(
                    &solana_sdk::pubkey::Pubkey::new_unique(),
                    &[
                        ("max_attempts_per_utc_day", day),
                        ("max_attempts_campaign", campaign),
                        ("max_attempts_grant", grant),
                    ],
                )
                .as_bytes(),
                &fe06::tests::ceilings(),
            )
            .unwrap()
        };
        let check = |b: Fe06Attempt, s: &fe06::Scope| super::admission::within_limits(&j, &b, s);
        assert_eq!(
            check(booking("g1", "k1", 10), &scope("1", "5", "5")),
            Err("FE06_ATTEMPT_LIMIT_UTC_DAY: 1 of 1 attempts used".into())
        );
        assert_eq!(
            check(booking("g1", "k1", 11), &scope("1", "5", "5")),
            Ok(())
        );
        assert_eq!(
            check(booking("g2", "k1", 11), &scope("5", "1", "5")),
            Err("FE06_ATTEMPT_LIMIT_CAMPAIGN: 1 of 1 attempts used".into())
        );
        assert_eq!(
            check(booking("g2", "k2", 11), &scope("5", "1", "5")),
            Ok(())
        );
        assert_eq!(
            check(booking("g1", "k2", 11), &scope("5", "5", "1")),
            Err("FE06_ATTEMPT_LIMIT_GRANT: 1 of 1 attempts used".into())
        );
        assert_eq!(
            check(booking("g2", "k2", 11), &scope("5", "5", "1")),
            Ok(())
        );
        drop(j);
        fs::remove_dir_all(dir).unwrap();
    }

    /// F2: every new wallet-journal frame is a typed binary frame (schema 8, no JSON value
    /// tree); a journal written before F2 (schema-1 frames) still replays and continues with
    /// typed frames; a JSON write of an FE-04 kind is refused by the writer.
    #[test]
    fn f2_wallet_journal_writes_typed_frames_and_reads_historical_json() {
        let frames_of = |file: &Path| {
            let mut r = journal::Reader::open(file).unwrap();
            std::iter::from_fn(|| r.next_frame().unwrap()).collect::<Vec<_>>()
        };
        let (dir, a, i, c) = setup();
        let mut j = Journal::open(&dir, &a.wallet).unwrap();
        j.reserve(a.clone(), i.clone(), &c).unwrap();
        let file = path_of(&j);
        drop(j);
        let frames = frames_of(&file);
        assert_eq!(frames.len(), 1);
        // Schema 9 since the P8-CTRL.5 exit booking; schemas 8 … 2 stay readable.
        assert_eq!(
            (frames[0].kind, frames[0].schema),
            (journal::kind::FE04_TRANSITION, 9)
        );
        assert!(frames[0].legacy_tree().is_err());
        let v: Version = frames[0].typed().unwrap();
        assert_eq!(
            (v.transition, v.state.phase),
            (Transition::Reserve, Phase::Reserved)
        );
        // A pre-F2 journal: the same reservation as a historical schema-1 frame.
        fs::remove_file(&file).unwrap();
        let mut w = journal::Writer::create(&file).unwrap();
        let state = reserved(&a, &i);
        let historical = serde_json::to_value(VersionRef {
            transition: Transition::Reserve,
            state: &state,
        })
        .unwrap();
        assert!(w
            .append(journal::kind::FE04_TRANSITION, 1, Default::default(), b"x")
            .is_err());
        w.test_append_historical(journal::kind::FE04_TRANSITION, &historical)
            .unwrap();
        w.sync().unwrap();
        drop(w);
        // DEC-09: the standard build reads no schema-1 frame; the wallet journal is refused before
        // any admission (fail-closed) and stays unchanged for the historical build.
        #[cfg(not(feature = "historical"))]
        {
            let before = fs::read(&file).unwrap();
            assert_eq!(
                Journal::open(&dir, &a.wallet).err().as_deref(),
                Some("JOURNAL_HISTORICAL_SCHEMA_REQUIRES_HISTORICAL_BUILD")
            );
            assert_eq!(fs::read(&file).unwrap(), before);
        }
        #[cfg(feature = "historical")]
        {
            let reopened = Journal::open(&dir, &a.wallet).unwrap();
            assert_eq!(reopened.holds(), i.holds);
            assert_eq!(
                reopened.saved.as_ref().unwrap().phase,
                Phase::HandoffUnknown
            );
            drop(reopened);
            let frames = frames_of(&file);
            assert_eq!(
                frames.iter().map(|f| f.schema).collect::<Vec<_>>(),
                [1, 9],
                "historical frame kept, the reopen transition is typed"
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }

    /// M6: journal failures carry a bounded, secret-free class — the journal's static code for
    /// damaged data, the OS class for I/O — and never the path; every one stays a halt.
    #[test]
    fn m6_journal_failures_carry_typed_secret_free_causes() {
        let (dir, a, i, c) = setup();
        let mut j = Journal::open(&dir, &a.wallet).unwrap();
        j.reserve(a.clone(), i.clone(), &c).unwrap();
        let file = path_of(&j);
        drop(j);
        let good = fs::read(&file).unwrap();
        let n = good.len();
        let mut payload = good.clone();
        payload[n - 60] ^= 1;
        let mut header = good.clone();
        header[3] ^= 1;
        for (bytes, expected) in [
            (good[..30].to_vec(), "JOURNAL_CORRUPT:HEADER_TRUNCATED"),
            (header, "JOURNAL_CORRUPT:JOURNAL_VERSION_OR_MAGIC"),
        ] {
            fs::write(&file, &bytes).unwrap();
            assert_eq!(
                Journal::open(&dir, &a.wallet).err().as_deref(),
                Some(expected)
            );
        }
        fs::write(&file, &payload).unwrap();
        let damaged = Journal::open(&dir, &a.wallet).err().unwrap();
        assert_eq!(damaged, "JOURNAL_CORRUPT:FRAME_CRC");
        // A message shaped like a journal code is no code: only the typed source counts.
        assert_eq!(
            cause(
                "JOURNAL_CORRUPT",
                &std::io::Error::new(std::io::ErrorKind::InvalidData, "FRAME_CRC")
            ),
            "JOURNAL_CORRUPT:INVALID_DATA"
        );
        for e in [
            Journal::open(&dir, &a.wallet).err().unwrap(),
            cause(
                "JOURNAL_IO",
                &std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            ),
            cause(
                "JOURNAL_FSYNC",
                &std::io::Error::from_raw_os_error(libc::ENOSPC),
            ),
            cause(
                "JOURNAL_READ",
                &std::io::Error::other(dir.display().to_string()),
            ),
        ] {
            assert!(
                !e.contains(&*dir.to_string_lossy()) && !e.contains('/'),
                "{e}"
            );
        }
        assert_eq!(
            cause(
                "JOURNAL_IO",
                &std::io::Error::from(std::io::ErrorKind::PermissionDenied)
            ),
            "JOURNAL_IO:PERMISSION"
        );
        assert_eq!(
            cause(
                "JOURNAL_FSYNC",
                &std::io::Error::from_raw_os_error(libc::ENOSPC)
            ),
            "JOURNAL_FSYNC:NO_SPACE"
        );
        assert_eq!(
            cause(
                "JOURNAL_READ",
                &std::io::Error::other("https://x/?api-key=SECRET")
            ),
            "JOURNAL_READ:IO_OTHER"
        );
        fs::write(&file, &good).unwrap();
        assert_eq!(Journal::open(&dir, &a.wallet).unwrap().holds(), i.holds);
        fs::remove_dir_all(dir).unwrap();
    }
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    pub(super) fn setup() -> (PathBuf, Approval, Intent, Current) {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "p0-shot-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let wallet = solana_sdk::pubkey::Pubkey::new_unique().to_string();
        let program = solana_sdk::pubkey::Pubkey::new_unique().to_string();
        let target = solana_sdk::pubkey::Pubkey::new_unique().to_string();
        let a = Approval {
            schema: 1,
            authorized: true,
            approval_id: "UNIT_TEST_ONLY".into(),
            code_sha: "a".repeat(40),
            binary_sha256: "b".repeat(64),
            wallet: wallet.clone(),
            program: program.clone(),
            program_data_hash: "deployed-pinned".into(),
            funding_class: "ZERO_PREFUND_SAME_TX".into(),
            allowed_targets: [target.clone()].into_iter().collect(),
            expires_unix_s: 100,
            max_state_age_ms: 500,
            max_capital_usd_micro: 100,
            max_total_sol_debit_lamports: 50,
            max_tip_lamports: 10,
            max_rent_lamports: 10,
            max_loss_usd_micro: 50,
            min_net_usd_micro: 1,
            max_slippage_bps: 100,
        };
        let i = Intent {
            code_sha: a.code_sha.clone(),
            binary_sha256: a.binary_sha256.clone(),
            wallet,
            program,
            program_data_hash: a.program_data_hash.clone(),
            funding_class: a.funding_class.clone(),
            target,
            signatures: vec![solana_sdk::signature::Signature::new_unique().to_string()],
            signed_wire_sha256: "c".repeat(64),
            state_generation: 1,
            state_slot: 50,
            valid_until_slot: 55,
            evaluated_unix_ms: 1000,
            capital_usd_micro: 10,
            transaction_fee_lamports: 20,
            tip_lamports: 0,
            rent_lamports: 0,
            max_loss_usd_micro: 20,
            conservative_net_usd_micro: 10,
            slippage_bps: 50,
            holds: [("SOL".into(), 20)].into_iter().collect(),
            expected_wallet_assets: ["SOL".into()].into_iter().collect(),
            last_valid_block_height: Some(60),
        };
        let c = Current {
            now_unix_ms: 1001,
            slot: 51,
            state_generation: 1,
            program_data_hash: a.program_data_hash.clone(),
            balances: [("SOL".into(), 100)].into_iter().collect(),
            reserved_elsewhere: BTreeMap::new(),
            policy_reserve: BTreeMap::new(),
            health_funding_route_fee_exit_known: true,
            canonical_fresh_no_gap_or_fork: true,
            exact_signed_payload_simulated: true,
            bundle_simulated: false,
        };
        (path, a, i, c)
    }
    /// P8-RES-01: a hold is checked against the balance minus what other attempts hold: the
    /// same lamports are never reserved twice.
    #[test]
    fn hold_never_uses_units_another_attempt_holds() {
        let (path, a, i, mut c) = setup();
        let mut book = crate::execution::reservation::WalletReservations::default();
        let other = [("SOL".to_string(), 81)].into_iter().collect();
        book.reserve("other", &other, &c.balances).unwrap();
        c.reserved_elsewhere = book.held_by_others("this");
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        // 100 SOL units, 81 held elsewhere: 19 left for a hold of 20.
        assert_eq!(
            j.reserve(a.clone(), i.clone(), &c).unwrap_err(),
            "WALLET_UNDERFUNDED_OR_HOLD_MISSING"
        );
        book.release("other").unwrap();
        book.reserve(
            "other",
            &[("SOL".to_string(), 80)].into_iter().collect(),
            &c.balances,
        )
        .unwrap();
        c.reserved_elsewhere = book.held_by_others("this");
        // P8-INV-01: the policy reserve is never spent either (100 - 80 - 1 < 20).
        c.policy_reserve = [("SOL".to_string(), 1)].into_iter().collect();
        assert_eq!(
            j.reserve(a.clone(), i.clone(), &c).unwrap_err(),
            "WALLET_UNDERFUNDED_OR_HOLD_MISSING"
        );
        c.policy_reserve.clear();
        assert!(j.reserve(a, i, &c).is_ok());
    }

    /// P8-RES-02: the global reservation book is rebuilt from the durable FE-04 journals after a
    /// crash — a reserved attempt (UNKNOWN after the restart) and a handed-off one with an UNKNOWN
    /// outcome stay held, a finalized one is released — and a second attempt on the same target
    /// is refused.
    #[test]
    fn reservation_book_survives_restart_until_finalized() {
        use crate::execution::reservation::WalletReservations;
        // Reserved, then a crash: held after the restart.
        let (path, a, i, c) = setup();
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        j.reserve(a.clone(), i.clone(), &c).unwrap();
        drop(j);
        let hold = Journal::open(&path, &a.wallet)
            .unwrap()
            .active_hold()
            .unwrap();
        // A crash between reservation and send cannot be told from a send: UNKNOWN after restart.
        assert!(hold.unknown_outcome);
        let mut book = WalletReservations::rebuild([hold.clone()]).unwrap();
        assert_eq!(book.held("SOL"), 20);
        // MT5b: the hold carries the bound signature and the blockhash lifetime.
        assert_eq!(
            (&hold.signature, hold.last_valid_block_height),
            (&i.signatures.first().cloned(), i.last_valid_block_height)
        );
        let second = ActiveHold {
            attempt: "b".into(),
            ..hold.clone()
        };
        assert_eq!(
            book.reserve_target(&second, &c.balances),
            Err("RESERVATION_TARGET_CONTENDED")
        );
        let another = ActiveHold {
            target: "another target".into(),
            ..second
        };
        assert_eq!(book.reserve_target(&another, &c.balances), Ok(()));
        assert_eq!(
            WalletReservations::rebuild([hold.clone(), hold]).unwrap_err(),
            "RESERVATION_EXISTS"
        );
        fs::remove_dir_all(&path).unwrap();
        // Handed off with an UNKNOWN outcome, then a crash: still held.
        let (path, a, i, c) = setup();
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        j.reserve(a.clone(), i.clone(), &c).unwrap();
        assert!(j
            .handoff::<()>(&i.signed_wire_sha256, &c, || Err(
                "timeout after possible acceptance".into()
            ))
            .is_err());
        drop(j);
        let hold = Journal::open(&path, &a.wallet)
            .unwrap()
            .active_hold()
            .unwrap();
        assert!(hold.unknown_outcome);
        assert_eq!(WalletReservations::rebuild([hold]).unwrap().held("SOL"), 20);
        fs::remove_dir_all(&path).unwrap();
        // Finalized: released, nothing to restore.
        let (path, a, i, c) = setup();
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        j.reserve(a.clone(), i.clone(), &c).unwrap();
        j.handoff(&i.signed_wire_sha256, &c, || Ok(())).unwrap();
        j.reconcile(vec![Receipt {
            signature: i.signatures[0].clone(),
            finalized: true,
            success: true,
            fee_lamports: Some(20),
            wallet_deltas: Some([("SOL".into(), -20)].into_iter().collect()),
            exit_complete: true,
        }])
        .unwrap();
        drop(j);
        assert!(Journal::open(&path, &a.wallet)
            .unwrap()
            .active_hold()
            .is_none());
        fs::remove_dir_all(&path).unwrap();
    }

    /// P8-RES-02/FE-04: the owner's book follows the durable journal — no journal is no hold, a
    /// journal locked by a running attempt makes the book unreadable (nothing available), an
    /// unreconciled hold survives the restart, a finalized reconciliation releases it.
    #[test]
    fn owner_book_follows_the_durable_journal() {
        use crate::execution::reservation::WalletReservations;
        let (path, a, i, c) = setup();
        assert_eq!(
            WalletReservations::from_fe04_journal(&path, &a.wallet)
                .unwrap()
                .held("SOL"),
            0
        );
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        j.reserve(a.clone(), i.clone(), &c).unwrap();
        let locked = WalletReservations::refreshed(&path, &a.wallet);
        assert!(locked.is_unreadable());
        assert_eq!(locked.available(&c.balances, "SOL"), Some(0));
        assert!(WalletReservations::from_fe04_journal(&path, &a.wallet).is_err());
        j.handoff(&i.signed_wire_sha256, &c, || Ok(())).unwrap();
        drop(j);
        let book = WalletReservations::refreshed(&path, &a.wallet);
        assert!(!book.is_unreadable());
        assert_eq!(
            (book.held("SOL"), book.available(&c.balances, "SOL")),
            (20, Some(80))
        );
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        j.reconcile(vec![Receipt {
            signature: i.signatures[0].clone(),
            finalized: true,
            success: true,
            fee_lamports: Some(20),
            wallet_deltas: Some([("SOL".into(), -20)].into_iter().collect()),
            exit_complete: true,
        }])
        .unwrap();
        drop(j);
        let released = WalletReservations::refreshed(&path, &a.wallet);
        assert_eq!((released.held("SOL"), released.is_unreadable()), (0, false));
        fs::remove_dir_all(&path).unwrap();
    }

    /// B3: after a restart the safety layer replays the durable attempt — a crashed reserved or
    /// UNKNOWN attempt halts it and keeps its capital and slot; a finalized one halts it without
    /// capital (its PnL is not booked); without a journal nothing changes.
    #[test]
    fn b3_safety_replays_the_durable_attempt_after_restart() {
        use crate::execution::reservation::WalletReservations;
        use darkbot_common::error::HaltReason;
        use darkbot_project0::safety::{Allow, Deny, Limits, Safety};
        let (path, a, i, c) = setup();
        let replay = || {
            WalletReservations::replay_fe04_journal(&path, &a.wallet)
                .unwrap()
                .1
        };
        let restored = |replay: Option<SafetyReplay>| {
            let limits = Limits {
                max_total_capital: u64::MAX,
                max_outstanding_inventory: u64::MAX,
                max_concurrent_positions: 9,
                max_exposure_per_attempt: u64::MAX,
                max_daily_loss: u64::MAX,
                max_failed_attempts_per_day: 9,
                max_tip: u64::MAX,
                max_slippage_bps: u64::MAX,
                max_outstanding_attempts: 9,
            };
            let mut allow = Allow::default();
            allow.execution_classes.insert("longtail");
            let mut s = Safety::new(limits, allow, true);
            if let Some(r) = replay {
                s.restore_attempt(0, r.held_capital_usd_micro, r.settled)
                    .unwrap();
            }
            let admitted = s.preview(0, 1, 0, 0, 0, &[], "longtail");
            (s.halted(), s.capital_in_use(), admitted)
        };
        assert_eq!(replay(), None);
        assert_eq!(restored(None), (None, 0, Ok(())));
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        j.reserve(a.clone(), i.clone(), &c).unwrap();
        drop(j); // crash before the handoff
        let unknown = Deny::Halted(HaltReason::UnknownEconomics);
        let held = Some(SafetyReplay {
            held_capital_usd_micro: Some(i.capital_usd_micro),
            settled: None,
            exits: Vec::new(),
        });
        assert_eq!(replay(), held);
        assert_eq!(
            restored(replay()),
            (
                Some(HaltReason::UnknownEconomics),
                i.capital_usd_micro,
                Err(unknown)
            )
        );
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        j.reconcile(vec![Receipt {
            signature: i.signatures[0].clone(),
            finalized: true,
            success: true,
            fee_lamports: Some(20),
            wallet_deltas: Some([("SOL".into(), -20)].into_iter().collect()),
            exit_complete: true,
        }])
        .unwrap();
        drop(j);
        assert_eq!(
            restored(replay()),
            (Some(HaltReason::UnknownEconomics), 0, Err(unknown))
        );
        // C3: once settled, the replay books the realized net and the outcome into the day the
        // attempt was evaluated, and the consumed one-shot halts as `LimitBreached`.
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        let settled = j
            .settle(
                &|asset, d| (asset == "SOL").then_some(d),
                i.evaluated_unix_ms / 1_000,
            )
            .unwrap();
        drop(j);
        let replayed = replay().unwrap();
        let day = i.evaluated_unix_ms / 1_000;
        assert_eq!(
            replayed.settled,
            Some(darkbot_project0::safety::SettledAttempt {
                unix_s: day,
                realized_net_usd_micro: -20,
                landed_success: true,
                open_inventory_usd_micro: 0,
            })
        );
        assert_eq!(settled.realized_net_usd_micro, -20);
        let book = |now: u64| {
            let mut allow = Allow::default();
            allow.execution_classes.insert("longtail");
            let mut s = Safety::new(
                Limits {
                    max_total_capital: u64::MAX,
                    max_outstanding_inventory: u64::MAX,
                    max_concurrent_positions: 9,
                    max_exposure_per_attempt: u64::MAX,
                    max_daily_loss: u64::MAX,
                    max_failed_attempts_per_day: 9,
                    max_tip: u64::MAX,
                    max_slippage_bps: u64::MAX,
                    max_outstanding_attempts: 9,
                },
                allow,
                true,
            );
            s.restore_attempt(now, replayed.held_capital_usd_micro, replayed.settled)
                .unwrap();
            (s.halted(), s.capital_in_use(), s.day().realized_pnl)
        };
        assert_eq!(book(day), (Some(HaltReason::LimitBreached), 0, -20));
        assert_eq!(
            book(day + 86_400),
            (Some(HaltReason::LimitBreached), 0, 0),
            "another day"
        );
        fs::remove_dir_all(&path).unwrap();
    }

    /// A deadline no test simulation reaches.
    fn test_deadline() -> std::time::Instant {
        std::time::Instant::now() + std::time::Duration::from_secs(60)
    }
    struct RecordingAdapter {
        path: darkbot_solana::submit::SubmissionPath,
        simulation: darkbot_solana::submit::SimResult,
        failure: u8,
        calls: std::cell::Cell<u32>,
        simulated: std::cell::RefCell<Vec<Vec<u8>>>,
    }
    impl darkbot_solana::submit::SubmissionAdapter for RecordingAdapter {
        fn path(&self) -> darkbot_solana::submit::SubmissionPath {
            self.path
        }
        fn supports_bundle(&self) -> bool {
            self.path != darkbot_solana::submit::SubmissionPath::Rpc
        }
        fn fee_for_message(
            &self,
            _: &solana_sdk::transaction::VersionedTransaction,
            _: std::time::Instant,
        ) -> Result<darkbot_solana::submit::MessageFeeQuote, darkbot_solana::submit::FeeQuoteError>
        {
            Err(darkbot_solana::submit::FeeQuoteError::Unavailable(
                "recording adapter: no fee quote",
            ))
        }
        fn simulate(
            &self,
            tx: &solana_sdk::transaction::VersionedTransaction,
            deadline: std::time::Instant,
        ) -> darkbot_solana::submit::SimResult {
            self.simulate_bundle(std::slice::from_ref(tx), deadline)
        }
        fn simulate_bundle(
            &self,
            txs: &[solana_sdk::transaction::VersionedTransaction],
            _: std::time::Instant,
        ) -> darkbot_solana::submit::SimResult {
            *self.simulated.borrow_mut() = txs
                .iter()
                .map(|tx| darkbot_solana::submit::serialize_tx(tx).unwrap())
                .collect();
            self.simulation.clone()
        }
        fn submit(
            &self,
            tx: &solana_sdk::transaction::VersionedTransaction,
            t: u64,
            deadline: std::time::Instant,
        ) -> Result<darkbot_solana::submit::SubmitAck, darkbot_solana::submit::SubmitError>
        {
            self.submit_bundle(std::slice::from_ref(tx), t, deadline)
        }
        fn submit_bundle(
            &self,
            txs: &[solana_sdk::transaction::VersionedTransaction],
            t: u64,
            deadline: std::time::Instant,
        ) -> Result<darkbot_solana::submit::SubmitAck, darkbot_solana::submit::SubmitError>
        {
            self.calls.set(self.calls.get() + 1);
            // DEC-12: like every adapter, nothing is sent after the handoff deadline.
            if std::time::Instant::now() >= deadline {
                return Err(darkbot_solana::submit::SubmitError::Deadline { sent: false });
            }
            let bytes: Vec<_> = txs
                .iter()
                .map(|tx| darkbot_solana::submit::serialize_tx(tx).unwrap())
                .collect();
            assert_eq!(*self.simulated.borrow(), bytes);
            if self.failure == 1 {
                return Err(darkbot_solana::submit::SubmitError::Transport(
                    "possible acceptance".into(),
                ));
            }
            if self.failure == 2 {
                return Err(darkbot_solana::submit::SubmitError::Rejected(
                    "provider error".into(),
                ));
            }
            let mut signatures: Vec<_> = txs.iter().map(|tx| tx.signatures[0]).collect();
            if self.failure == 3 {
                signatures[0] = solana_sdk::signature::Signature::new_unique();
            }
            Ok(darkbot_solana::submit::SubmitAck {
                path: self.path,
                signature: signatures[0],
                signatures,
                submission_id: (self.path == darkbot_solana::submit::SubmissionPath::JitoBundle)
                    .then(|| "mock-bundle-id".into()),
                t_handoff_ns: t,
                serialized_len: bytes.iter().map(Vec::len).sum(),
            })
        }
    }
    fn adapter(path: darkbot_solana::submit::SubmissionPath, failure: u8) -> RecordingAdapter {
        RecordingAdapter {
            path,
            failure,
            calls: Default::default(),
            simulated: Default::default(),
            simulation: darkbot_solana::submit::SimResult::Ok {
                units_consumed: Some(10),
                context_slot: None,
            },
        }
    }
    /// DEC-02: the binding of a test simulation (the generations of `setup`'s `Current`).
    fn test_binding() -> transport::SimulationBinding {
        transport::SimulationBinding {
            state_generation: 1,
            program_data_hash: "pd".into(),
            state_valid: true,
        }
    }
    fn signed(
        payer: &solana_sdk::signature::Keypair,
    ) -> solana_sdk::transaction::VersionedTransaction {
        use solana_sdk::signature::Signer;
        let message = solana_sdk::message::Message::new_with_blockhash(
            &[],
            Some(&payer.pubkey()),
            &solana_sdk::hash::Hash::new_unique(),
        );
        solana_sdk::transaction::VersionedTransaction::try_new(
            solana_sdk::message::VersionedMessage::Legacy(message),
            &[payer],
        )
        .unwrap()
    }
    #[test]
    fn exact_adapter_bytes_ack_timeout_and_rejection_never_rearm() {
        use darkbot_solana::submit::SubmissionPath;
        use solana_sdk::signature::{Keypair, Signer};
        for path_kind in [
            SubmissionPath::Rpc,
            SubmissionPath::JitoSingle,
            SubmissionPath::JitoBundle,
        ] {
            for failure in 0..4 {
                let (dir, mut a, mut i, mut c) = setup();
                let payer = Keypair::new();
                a.wallet = payer.pubkey().to_string();
                i.wallet = a.wallet.clone();
                let txs = if path_kind == SubmissionPath::JitoBundle {
                    vec![signed(&payer), signed(&payer)]
                } else {
                    vec![signed(&payer)]
                };
                let payload =
                    transport::SignedPayload::new(txs, path_kind, payer.pubkey()).unwrap();
                i.signatures = payload.signatures().to_vec();
                i.signed_wire_sha256 = payload.wire_sha256().into();
                c.bundle_simulated = path_kind == SubmissionPath::JitoBundle;
                let adapter = adapter(path_kind, failure);
                let simulated = payload
                    .simulate(&adapter, test_deadline(), test_binding())
                    .unwrap();
                let mut j = Journal::open(&dir, &a.wallet).unwrap();
                j.reserve_payload(a.clone(), i.clone(), &c, &simulated, None, None)
                    .unwrap();
                let result =
                    j.submit_payload(&simulated, &c, 123, test_deadline(), permit(&simulated));
                assert_eq!(result.is_ok(), failure == 0);
                assert_eq!(j.saved.as_ref().unwrap().phase, Phase::HandoffUnknown);
                assert_eq!(j.holds(), i.holds);
                assert!(j
                    .submit_payload(&simulated, &c, 124, test_deadline(), permit(&simulated))
                    .is_err());
                assert_eq!(adapter.calls.get(), 1);
                drop(j);
                let mut reopened = Journal::open(&dir, &a.wallet).unwrap();
                assert!(reopened
                    .submit_payload(&simulated, &c, 125, test_deadline(), permit(&simulated))
                    .is_err());
                assert_eq!(reopened.holds(), i.holds);
                drop(reopened);
                fs::remove_dir_all(dir).unwrap();
            }
        }
    }
    /// P8-CTRL.7: the egress permit of a simulated payload under the offline tests' fully
    /// enabled gates.
    fn permit(
        simulated: &transport::SimulatedPayload<'_>,
    ) -> crate::orchestration::capability_control::EgressPermit {
        use crate::orchestration::capability_control as cap;
        let signer = solana_sdk::pubkey::Pubkey::new_unique();
        let gates = cap::OfflineGates::enabled(signer);
        cap::EgressPermit::issue(
            &gates.gates(),
            &crate::orchestration::fe06::Use::Attempt(
                crate::orchestration::fe06::tests::attempt_use(&signer),
            ),
            simulated.signatures(),
            simulated.path(),
            test_deadline(),
        )
        .unwrap()
    }
    /// DEC-12: a handoff whose deadline passed after the durable reservation stays UNKNOWN with
    /// its holds (the adapter sent nothing, but the fsynced handoff state is never released
    /// here); the one-shot is consumed.
    #[test]
    fn dec12_a_handoff_past_its_deadline_stays_unknown_with_its_holds() {
        use darkbot_solana::submit::SubmissionPath;
        use solana_sdk::signature::{Keypair, Signer};
        let (dir, mut a, mut i, c) = setup();
        let payer = Keypair::new();
        a.wallet = payer.pubkey().to_string();
        i.wallet = a.wallet.clone();
        let payload = transport::SignedPayload::new(
            vec![signed(&payer)],
            SubmissionPath::Rpc,
            payer.pubkey(),
        )
        .unwrap();
        i.signatures = payload.signatures().to_vec();
        i.signed_wire_sha256 = payload.wire_sha256().into();
        let adapter = adapter(SubmissionPath::Rpc, 0);
        let simulated = payload
            .simulate(&adapter, test_deadline(), test_binding())
            .unwrap();
        let mut j = Journal::open(&dir, &a.wallet).unwrap();
        j.reserve_payload(a.clone(), i.clone(), &c, &simulated, None, None)
            .unwrap();
        let past = std::time::Instant::now();
        assert_eq!(
            j.submit_payload(&simulated, &c, 123, past, permit(&simulated))
                .unwrap_err(),
            err("SUBMISSION_RESULT_UNKNOWN:DEADLINE_NOT_SENT")
        );
        assert_eq!(j.saved.as_ref().unwrap().phase, Phase::HandoffUnknown);
        assert_eq!(j.holds(), i.holds);
        assert!(j
            .submit_payload(&simulated, &c, 124, test_deadline(), permit(&simulated))
            .is_err());
        assert_eq!(adapter.calls.get(), 1);
        drop(j);
        fs::remove_dir_all(dir).unwrap();
    }
    /// DEC-02: a simulation stands for a handoff only while every condition holds — the exact
    /// payload (another transaction, or the same one on another path, is another wire digest),
    /// a context slot at or after the state slot (unknown is not after), the binding unchanged
    /// (state generation, ProgramData, fresh state without gap), an age within the approval —
    /// and only a typed `Ok` makes a token, a bundle only from a bundle simulation; the token's
    /// `exact_signed_payload_simulated` is derived from it.
    #[test]
    fn dec02_a_simulation_is_reused_only_while_every_binding_holds() {
        use darkbot_solana::submit::{SimResult, SubmissionPath};
        use solana_sdk::signature::{Keypair, Signer};
        use std::time::Duration;
        let payer = Keypair::new();
        let tx = signed(&payer);
        let payload =
            |txs: Vec<_>, path| transport::SignedPayload::new(txs, path, payer.pubkey()).unwrap();
        let mut sim = adapter(SubmissionPath::Rpc, 0);
        sim.simulation = SimResult::Ok {
            units_consumed: Some(10),
            context_slot: Some(10),
        };
        let token = payload(vec![tx.clone()], SubmissionPath::Rpc)
            .simulate_bound(&sim, test_deadline(), test_binding())
            .unwrap()
            .1
            .unwrap();
        assert!(token.exact_signed_payload_simulated());
        let same = payload(vec![tx.clone()], SubmissionPath::Rpc);
        let minute = Duration::from_secs(60);
        assert_eq!(
            token.reuse_refusal(&same, 10, &test_binding(), minute),
            None
        );
        let other = payload(vec![signed(&payer)], SubmissionPath::Rpc);
        let bundled = payload(vec![tx.clone()], SubmissionPath::JitoBundle);
        for p in [&other, &bundled] {
            assert_eq!(
                token.reuse_refusal(p, 10, &test_binding(), minute),
                Some("SIM_REUSE_WIRE_DIGEST")
            );
        }
        assert_eq!(
            token.reuse_refusal(&same, 11, &test_binding(), minute),
            Some("SIM_REUSE_CONTEXT_SLOT")
        );
        let changed = |edit: fn(&mut transport::SimulationBinding)| {
            let mut b = test_binding();
            edit(&mut b);
            b
        };
        for b in [
            changed(|b| b.state_generation += 1),
            changed(|b| b.program_data_hash = "other".into()),
            changed(|b| b.state_valid = false),
        ] {
            assert_eq!(
                token.reuse_refusal(&same, 10, &b, minute),
                Some("SIM_REUSE_STATE_CHANGED")
            );
        }
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(
            token.reuse_refusal(&same, 10, &test_binding(), Duration::from_millis(1)),
            Some("SIM_REUSE_AGE")
        );
        // An unknown context slot is never at or after the state slot.
        sim.simulation = SimResult::Ok {
            units_consumed: None,
            context_slot: None,
        };
        let unknown = payload(vec![tx.clone()], SubmissionPath::Rpc)
            .simulate_bound(&sim, test_deadline(), test_binding())
            .unwrap()
            .1
            .unwrap();
        assert_eq!(
            unknown.reuse_refusal(&same, 0, &test_binding(), minute),
            Some("SIM_REUSE_CONTEXT_SLOT")
        );
        // No token without a typed Ok; a bundle is simulated only as a bundle.
        for result in [
            SimResult::Failed("X".into()),
            SimResult::Unavailable("u".into()),
            SimResult::Deadline,
            SimResult::BudgetRejected("RPS"),
        ] {
            sim.simulation = result.clone();
            let (back, token) = payload(vec![tx.clone()], SubmissionPath::Rpc)
                .simulate_bound(&sim, test_deadline(), test_binding())
                .unwrap();
            assert_eq!((back, token.is_none()), (result, true));
        }
        assert!(payload(vec![tx], SubmissionPath::JitoBundle)
            .simulate_bound(&sim, test_deadline(), test_binding())
            .is_err());
    }
    /// L1: an owner taking the wallet while the previous one drops sees the complete durable
    /// journal (the writer closes before the lock is released), never a half-closed one.
    #[test]
    fn next_owner_never_opens_a_journal_still_held_by_its_writer() {
        let (dir, a, i, c) = setup();
        for _ in 0..20 {
            let mut j = Journal::open(&dir, &a.wallet).unwrap();
            if j.holds().is_empty() {
                j.reserve(a.clone(), i.clone(), &c).unwrap();
            }
            let wallet = a.wallet.clone();
            let d = dir.clone();
            let contender = std::thread::spawn(move || loop {
                match Journal::open(&d, &wallet) {
                    Ok(next) => return next.holds(),
                    Err(e) if e == "WALLET_ALREADY_LOCKED" => std::thread::yield_now(),
                    Err(e) => panic!("{e}"),
                }
            });
            drop(j);
            assert_eq!(contender.join().unwrap(), i.holds);
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn dropped_journal_releases_wallet_while_lock_descriptor_is_shared() {
        // A child process being spawned concurrently shares this file description until exec;
        // try_clone() shares it the same way. Dropping the journal must still free the wallet.
        let (dir, a, _, _) = setup();
        let j = Journal::open(&dir, &a.wallet).unwrap();
        assert_eq!(
            Journal::open(&dir, &a.wallet).err().as_deref(),
            Some("WALLET_ALREADY_LOCKED")
        );
        let shared = j._lock.0.try_clone().unwrap();
        drop(j);
        let reopened = Journal::open(&dir, &a.wallet).unwrap();
        assert_eq!(
            Journal::open(&dir, &a.wallet).err().as_deref(),
            Some("WALLET_ALREADY_LOCKED")
        );
        drop((reopened, shared));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn failed_open_releases_wallet_while_processes_are_spawned() {
        // A failed open drops its lock without a Journal. Concurrent spawns share that file
        // description until exec; the next open must still report the real halt reason.
        use std::sync::atomic::{AtomicBool, Ordering};
        let (dir, a, _, _) = setup();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{}.pending", a.wallet)), b"").unwrap();
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let spawners: Vec<_> = (0..4)
            .map(|_| {
                let stop = stop.clone();
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let _ = std::process::Command::new("true").output();
                    }
                })
            })
            .collect();
        let wrong = (0..2000)
            .filter(|_| {
                Journal::open(&dir, &a.wallet).err().as_deref()
                    != Some("JOURNAL_INTERRUPTED_WRITE_HALT")
            })
            .count();
        stop.store(true, Ordering::Relaxed);
        for spawner in spawners {
            spawner.join().unwrap();
        }
        assert_eq!(wrong, 0);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn changed_signed_payload_order_path_and_signature_cannot_reach_adapter() {
        use darkbot_solana::submit::SubmissionPath;
        use solana_sdk::signature::{Keypair, Signer};
        let payer = Keypair::new();
        let first = signed(&payer);
        let second = signed(&payer);
        let original = transport::SignedPayload::new(
            vec![first.clone(), second.clone()],
            SubmissionPath::JitoBundle,
            payer.pubkey(),
        )
        .unwrap();
        let reversed = transport::SignedPayload::new(
            vec![second.clone(), first.clone()],
            SubmissionPath::JitoBundle,
            payer.pubkey(),
        )
        .unwrap();
        assert_ne!(original.wire_sha256(), reversed.wire_sha256());
        let rpc =
            transport::SignedPayload::new(vec![first.clone()], SubmissionPath::Rpc, payer.pubkey())
                .unwrap();
        let bundle = transport::SignedPayload::new(
            vec![first.clone()],
            SubmissionPath::JitoBundle,
            payer.pubkey(),
        )
        .unwrap();
        assert_ne!(rpc.wire_sha256(), bundle.wire_sha256());
        assert!(transport::SignedPayload::new(
            vec![first.clone(), first.clone()],
            SubmissionPath::JitoBundle,
            payer.pubkey()
        )
        .is_err());
        assert!(transport::SignedPayload::new(
            vec![first.clone(), second],
            SubmissionPath::Rpc,
            payer.pubkey()
        )
        .is_err());
        assert!(transport::SignedPayload::new(
            vec![first.clone()],
            SubmissionPath::Rpc,
            Keypair::new().pubkey()
        )
        .is_err());
        let mut tampered = first.clone();
        tampered.signatures[0] = solana_sdk::signature::Signature::new_unique();
        assert!(
            transport::SignedPayload::new(vec![tampered], SubmissionPath::Rpc, payer.pubkey())
                .is_err()
        );
        let mut tampered = first;
        tampered
            .message
            .set_recent_blockhash(solana_sdk::hash::Hash::new_unique());
        assert!(
            transport::SignedPayload::new(vec![tampered], SubmissionPath::Rpc, payer.pubkey())
                .is_err()
        );
        let (dir, mut a, mut i, mut c) = setup();
        a.wallet = payer.pubkey().to_string();
        i.wallet = a.wallet.clone();
        c.bundle_simulated = true;
        i.signatures = original.signatures().to_vec();
        i.signed_wire_sha256 = original.wire_sha256().into();
        let adapter = adapter(SubmissionPath::JitoBundle, 0);
        let original = original
            .simulate(&adapter, test_deadline(), test_binding())
            .unwrap();
        let reversed = reversed
            .simulate(&adapter, test_deadline(), test_binding())
            .unwrap();
        let mut j = Journal::open(&dir, &a.wallet).unwrap();
        assert!(j
            .reserve_payload(a.clone(), i.clone(), &c, &reversed, None, None)
            .is_err());
        // DEC-11: a bundle token is reserved only with current bundle evidence.
        let stale = Current {
            bundle_simulated: false,
            ..c.clone()
        };
        let refused = j
            .reserve_payload(a.clone(), i.clone(), &stale, &original, None, None)
            .unwrap_err();
        assert!(
            refused.contains("CURRENT_BUNDLE_EVIDENCE_UNKNOWN"),
            "{refused}"
        );
        j.reserve_payload(a, i, &c, &original, None, None).unwrap();
        assert!(j
            .submit_payload(&reversed, &c, 1, test_deadline(), permit(&reversed))
            .is_err());
        assert_eq!(adapter.calls.get(), 0);
        drop(j);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn failed_unavailable_and_wrong_path_simulation_never_create_send_token() {
        use darkbot_solana::submit::{SimResult, SubmissionPath};
        use solana_sdk::signature::{Keypair, Signer};
        let payer = Keypair::new();
        for simulation in [
            SimResult::Failed("revert".into()),
            SimResult::Unavailable("429".into()),
        ] {
            let mut a = adapter(SubmissionPath::Rpc, 0);
            a.simulation = simulation;
            let p = transport::SignedPayload::new(
                vec![signed(&payer)],
                SubmissionPath::Rpc,
                payer.pubkey(),
            )
            .unwrap();
            assert!(p.simulate(&a, test_deadline(), test_binding()).is_err());
            assert_eq!(a.calls.get(), 0);
        }
        let a = adapter(SubmissionPath::Rpc, 0);
        let p = transport::SignedPayload::new(
            vec![signed(&payer)],
            SubmissionPath::JitoBundle,
            payer.pubkey(),
        )
        .unwrap();
        assert!(p.simulate(&a, test_deadline(), test_binding()).is_err());
        assert!(a.simulated.borrow().is_empty());
    }
    #[cfg(feature = "historical")]
    /// A pre-binary `<wallet>.json` journal exactly as the former JSON writer stored it.
    fn write_legacy(dir: &Path, saved: &Saved, without_observations: bool) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let mut record = serde_json::to_value(saved).unwrap();
        if without_observations {
            record.as_object_mut().unwrap().remove("observations");
        }
        let wrapper = serde_json::json!({"record":record,"digest":digest(&serde_json::to_vec(&record).unwrap())});
        let path = dir.join(format!("{}.json", saved.approval.wallet));
        fs::write(&path, serde_json::to_vec(&wrapper).unwrap()).unwrap();
        path
    }
    /// DEC-04: the reservation writes the holds; no later transition changes them.
    #[test]
    fn dec04_replay_keeps_the_admitted_holds() {
        use crate::execution::hold_b::{BHold, HoldEvidence, HoldMode};
        let (dir, a, i, _) = setup();
        let hold = Some(HoldEvidence {
            mode: HoldMode::Observe,
            a_inventory_usd_micro: 1_100,
            b: BHold::Unqualified("B_SWAP_PROGRAM_UNMODELLED".into()),
            reserved_inventory_usd_micro: 1_100,
        });
        let mut reserve = reserved(&a, &i);
        reserve.hold = hold.clone();
        assert!(legal(None, Transition::Reserve, &reserve));
        let mut unknown = reserve.clone();
        unknown.phase = Phase::HandoffUnknown;
        assert!(legal(Some(&reserve), Transition::Handoff, &unknown));
        let mut changed = unknown.clone();
        changed.hold = None;
        assert!(!legal(Some(&reserve), Transition::Handoff, &changed));
        let mut observed = unknown.clone();
        observed.hold.as_mut().unwrap().mode = HoldMode::Active;
        assert!(!legal(Some(&unknown), Transition::Observe, &observed));
        let _ = fs::remove_dir_all(dir);
    }

    /// DEC-03: replay accepts a release class only from a non-landing and only a proving one
    /// (none: a frame before N-2); every other transition keeps it unchanged.
    #[test]
    fn dec03_replay_accepts_a_release_class_only_from_a_non_landing() {
        use non_landing::{NonLandingRelease, Verdict};
        let (dir, a, i, _) = setup();
        let release = |class| {
            Some(NonLandingRelease {
                class,
                usable_operators: Default::default(),
                c_blocks: 1,
                c_through_slot: i.state_slot,
            })
        };
        let mut reserve = reserved(&a, &i);
        assert!(legal(None, Transition::Reserve, &reserve));
        reserve.non_landing = release(Verdict::CAloneQualified);
        assert!(!legal(None, Transition::Reserve, &reserve));
        let mut unknown = reserved(&a, &i);
        unknown.phase = Phase::HandoffUnknown;
        let mut next = unknown.clone();
        next.phase = Phase::HaltFinalized;
        next.finalized = Some(Vec::new());
        for (class, ok) in [
            (None, true),
            (release(Verdict::BcConfirmed), true),
            (release(Verdict::CAloneQualified), true),
            (release(Verdict::VetoContradiction), false),
            (release(Verdict::VetoLandingConfirmed), false),
            (release(Verdict::UnknownIncomplete), false),
        ] {
            next.non_landing = class;
            assert_eq!(legal(Some(&unknown), Transition::NotLanded, &next), ok);
        }
        let mut observed = unknown.clone();
        observed.non_landing = release(Verdict::CAloneQualified);
        assert!(!legal(Some(&unknown), Transition::Observe, &observed));
        let _ = fs::remove_dir_all(dir);
    }
    fn reserved(a: &Approval, i: &Intent) -> Saved {
        Saved {
            approval: a.clone(),
            intent: i.clone(),
            phase: Phase::Reserved,
            finalized: None,
            observations: BTreeMap::new(),
            exits: Vec::new(),
            non_landing: None,
            hold: None,
            fe06: None,
        }
    }
    pub(super) fn path_of(j: &Journal) -> PathBuf {
        j.writer.path.clone()
    }
    #[cfg(feature = "historical")]
    fn kinds(path: &Path) -> Vec<u16> {
        let mut r = journal::Reader::open(path).unwrap();
        std::iter::from_fn(|| r.next_frame().unwrap())
            .map(|f| f.kind)
            .collect()
    }
    /// F8/DEC-09: in every build a present pre-binary `<wallet>.json` blocks the open (it is
    /// never parsed there) and nothing is written.
    #[test]
    fn a_present_legacy_json_blocks_the_open_in_every_build() {
        let (dir, a, _, _) = setup();
        fs::create_dir_all(&dir).unwrap();
        let legacy = dir.join(format!("{}.json", a.wallet));
        fs::write(&legacy, b"{not json").unwrap();
        assert_eq!(
            Journal::open(&dir, &a.wallet).err().as_deref(),
            Some("JOURNAL_LEGACY_REQUIRES_MIGRATION")
        );
        assert!(!dir.join(format!("{}.p0j", a.wallet)).exists());
        assert_eq!(fs::read(&legacy).unwrap(), b"{not json");
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(feature = "historical")]
    /// F8 / P0-PROD-CTRL-02: the productive open never parses a pre-binary `<wallet>.json`; the
    /// import is the explicit migration (`p0ctl fe04 migrate`), crash-safe and idempotent. The
    /// migrated JSON stays as hash-bound evidence (`<wallet>.legacy.json`), checked by its bytes.
    #[test]
    fn legacy_json_is_migrated_explicitly_never_parsed_on_open() {
        let (dir, a, i, _) = setup();
        let legacy = write_legacy(&dir, &reserved(&a, &i), true);
        let bytes = fs::read(&legacy).unwrap();
        assert_eq!(
            Journal::open(&dir, &a.wallet).err().as_deref(),
            Some("JOURNAL_LEGACY_REQUIRES_MIGRATION")
        );
        let binary = dir.join(format!("{}.p0j", a.wallet));
        assert!(!binary.exists(), "a refused open writes nothing");
        let sha = Journal::migrate_legacy(&dir, &a.wallet).unwrap();
        assert_eq!(
            sha,
            journal::hex(&solana_sdk::hash::hash(&bytes).to_bytes())
        );
        let migrated = dir.join(format!("{}.legacy.json", a.wallet));
        assert!(!legacy.exists());
        assert_eq!(
            fs::read(&migrated).unwrap(),
            bytes,
            "historical evidence, never rewritten"
        );
        let reopened = Journal::open(&dir, &a.wallet).unwrap();
        assert_eq!(reopened.holds(), i.holds);
        assert!(reopened.saved.as_ref().unwrap().observations.is_empty());
        assert_eq!(
            reopened.saved.as_ref().unwrap().phase,
            Phase::HandoffUnknown
        );
        drop(reopened);
        assert_eq!(
            kinds(&binary),
            [
                journal::kind::FE04_LEGACY_IMPORT,
                journal::kind::FE04_TRANSITION
            ]
        );
        assert_eq!(
            Journal::migrate_legacy(&dir, &a.wallet).err().as_deref(),
            Some("JOURNAL_LEGACY_ABSENT")
        );
        // A crash after the durable import but before the rename: open still refuses, a second
        // migration only completes the rename (no second import frame).
        fs::copy(&migrated, &legacy).unwrap();
        fs::remove_file(&migrated).unwrap();
        assert_eq!(
            Journal::open(&dir, &a.wallet).err().as_deref(),
            Some("JOURNAL_LEGACY_REQUIRES_MIGRATION")
        );
        Journal::migrate_legacy(&dir, &a.wallet).unwrap();
        assert_eq!(kinds(&binary).len(), 2);
        assert_eq!(Journal::open(&dir, &a.wallet).unwrap().holds(), i.holds);
        // The binding holds: an edited or missing migrated JSON is a halt.
        let mut edited = bytes.clone();
        edited.push(b' ');
        fs::write(&migrated, edited).unwrap();
        assert_eq!(
            Journal::open(&dir, &a.wallet).err().as_deref(),
            Some("JOURNAL_LEGACY_MISMATCH")
        );
        fs::remove_file(&migrated).unwrap();
        assert_eq!(
            Journal::open(&dir, &a.wallet).err().as_deref(),
            Some("JOURNAL_LEGACY_MISMATCH")
        );
        fs::remove_dir_all(dir).unwrap();
    }
    /// N3: the financial journal never treats a zero-filled tail as a torn capture: HALT, and
    /// the file is neither truncated nor repaired.
    #[test]
    fn wallet_journal_with_zero_tail_is_a_halt() {
        let (dir, a, i, c) = setup();
        let mut j = Journal::open(&dir, &a.wallet).unwrap();
        j.reserve(a.clone(), i.clone(), &c).unwrap();
        let file = path_of(&j);
        drop(j);
        let good = fs::read(&file).unwrap();
        for zeros in [4usize, 64, 70_000] {
            fs::write(&file, [good.clone(), vec![0; zeros]].concat()).unwrap();
            assert_eq!(
                Journal::open(&dir, &a.wallet).err().as_deref(),
                Some("JOURNAL_CORRUPT:TAIL_ZERO_FILLED"),
                "{zeros}"
            );
            assert_eq!(fs::read(&file).unwrap().len(), good.len() + zeros);
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn damaged_truncated_foreign_or_illegal_binary_journal_is_a_halt() {
        let (dir, a, i, c) = setup();
        let mut j = Journal::open(&dir, &a.wallet).unwrap();
        j.reserve(a.clone(), i.clone(), &c).unwrap();
        let file = path_of(&j);
        drop(j);
        let good = fs::read(&file).unwrap();
        let n = good.len();
        let mut flipped = good.clone();
        flipped[n - 60] ^= 1;
        let mut header = good.clone();
        header[3] ^= 1;
        for (bytes, expected) in [
            (good[..n - 1].to_vec(), "JOURNAL_CORRUPT:FRAME_TRUNCATED"),
            (good[..n - 40].to_vec(), "JOURNAL_CORRUPT:FRAME_TRUNCATED"),
            (good[..30].to_vec(), "JOURNAL_CORRUPT:HEADER_TRUNCATED"),
            (
                [good.clone(), vec![0x55]].concat(),
                "JOURNAL_CORRUPT:FRAME_TRUNCATED",
            ),
            (flipped, "JOURNAL_CORRUPT:FRAME_CRC"),
            (header, "JOURNAL_CORRUPT:JOURNAL_VERSION_OR_MAGIC"),
        ] {
            fs::write(&file, &bytes).unwrap();
            assert_eq!(
                Journal::open(&dir, &a.wallet).err().as_deref(),
                Some(expected)
            );
        }
        // Frames the automaton never writes: a second reservation, a foreign kind, a seal.
        let state = reserved(&a, &i);
        let again = serde_json::to_value(VersionRef {
            transition: Transition::Reserve,
            state: &state,
        })
        .unwrap();
        // The same illegal second reservation as a typed frame: the automaton refuses it.
        fs::write(&file, &good).unwrap();
        let mut w = journal::Writer::resume(&file).unwrap();
        w.append_typed(
            Default::default(),
            &Version {
                transition: Transition::Reserve,
                state: state.clone(),
            },
        )
        .unwrap();
        w.sync().unwrap();
        drop(w);
        assert_eq!(
            Journal::open(&dir, &a.wallet).err().as_deref(),
            Some("JOURNAL_TRANSITION_INVALID")
        );
        let illegal: [(u16, serde_json::Value, &str); 3] = [
            (
                // F2: a historical schema-1 version after typed frames is foreign.
                journal::kind::FE04_TRANSITION,
                again,
                "JOURNAL_CORRUPT:SCHEMA_ORDER",
            ),
            (
                journal::kind::OWNER_RECORD,
                serde_json::json!({}),
                "JOURNAL_CORRUPT:KIND",
            ),
            (
                journal::kind::FE04_LEGACY_IMPORT,
                serde_json::json!({}),
                "JOURNAL_CORRUPT:IMPORT_POSITION",
            ),
        ];
        for (kind, value, code_expected) in illegal {
            fs::write(&file, &good).unwrap();
            let mut w = journal::Writer::resume(&file).unwrap();
            // A foreign or illegal frame as found on disk; the writer itself refuses foreign kinds.
            w.test_append_historical(kind, &value).unwrap();
            w.sync().unwrap();
            drop(w);
            assert_eq!(
                Journal::open(&dir, &a.wallet).err().as_deref(),
                Some(code_expected)
            );
        }
        fs::write(&file, &good).unwrap();
        journal::Writer::resume(&file).unwrap().seal(true).unwrap();
        // A sealed journal is an archived attempt, never the active one (e.g. copied back).
        assert_eq!(
            Journal::open(&dir, &a.wallet).err().as_deref(),
            Some("JOURNAL_CORRUPT:JOURNAL_ALREADY_SEALED")
        );
        // Undamaged bytes still reopen into UNKNOWN with the hold.
        fs::write(&file, &good).unwrap();
        let resumed = Journal::open(&dir, &a.wallet).unwrap();
        assert_eq!(resumed.holds(), i.holds);
        drop(resumed);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn journal_write_failure_blocks_send_and_keeps_the_hold() {
        let (dir, a, i, c) = setup();
        let mut j = Journal::open(&dir, &a.wallet).unwrap();
        j.reserve(a.clone(), i.clone(), &c).unwrap();
        j.writer.test_read_only(); // stands for disk full / I/O error at the durability barrier
        let sent = std::cell::Cell::new(0);
        for _ in 0..2 {
            assert!(j
                .handoff(&i.signed_wire_sha256, &c, || {
                    sent.set(sent.get() + 1);
                    Ok(())
                })
                .is_err());
        }
        assert_eq!(sent.get(), 0);
        assert_eq!(j.holds(), i.holds);
        drop(j);
        let mut resumed = Journal::open(&dir, &a.wallet).unwrap();
        assert_eq!(resumed.holds(), i.holds);
        assert_eq!(resumed.saved.as_ref().unwrap().phase, Phase::HandoffUnknown);
        assert!(resumed
            .handoff(&i.signed_wire_sha256, &c, || -> Result<(), String> {
                panic!("MUST NOT SEND")
            })
            .is_err());
        drop(resumed);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn crash_before_and_after_handoff_retains_hold_and_cannot_rearm() {
        for handoff in [false, true] {
            let (path, a, i, c) = setup();
            let mut j = Journal::open(&path, &a.wallet).unwrap();
            j.reserve(a.clone(), i.clone(), &c).unwrap();
            assert!(Journal::open(&path, &a.wallet).is_err());
            if handoff {
                assert!(j
                    .handoff::<()>(&i.signed_wire_sha256, &c, || Err(
                        "timeout after possible acceptance".into()
                    ))
                    .is_err());
            }
            drop(j);
            let mut resumed = Journal::open(&path, &a.wallet).unwrap();
            assert_eq!(resumed.holds(), i.holds);
            assert!(resumed
                .handoff(&i.signed_wire_sha256, &c, || -> Result<(), String> {
                    panic!("MUST NOT SEND")
                })
                .is_err());
            assert!(resumed.reserve(a, i, &c).is_err());
            drop(resumed);
            fs::remove_dir_all(path).unwrap();
        }
    }
    #[test]
    fn double_trigger_signed_bytes_and_all_budget_guards() {
        let (path, a, i, c) = setup();
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        let mut bad = a.clone();
        bad.authorized = false;
        assert!(j.reserve(bad, i.clone(), &c).is_err());
        for field in 0..10 {
            let mut bad = i.clone();
            match field {
                0 => bad.capital_usd_micro = 101,
                1 => bad.tip_lamports = 11,
                2 => bad.rent_lamports = 11,
                3 => bad.max_loss_usd_micro = 51,
                4 => bad.transaction_fee_lamports = 51,
                5 => bad.conservative_net_usd_micro = 0,
                6 => bad.slippage_bps = 101,
                7 => bad.code_sha = "d".repeat(40),
                8 => bad.holds.clear(),
                _ => bad.signatures.push(bad.signatures[0].clone()),
            }
            assert!(j.reserve(a.clone(), bad, &c).is_err(), "guard {field}");
        }
        j.reserve(a.clone(), i.clone(), &c).unwrap();
        assert!(j.reserve(a, i.clone(), &c).is_err());
        assert!(j
            .handoff("changed-wire", &c, || -> Result<(), String> {
                panic!("MUST NOT SEND")
            })
            .is_err());
        let mut calls = 0;
        j.handoff(&i.signed_wire_sha256, &c, || {
            calls += 1;
            Ok(())
        })
        .unwrap();
        assert!(j
            .handoff(&i.signed_wire_sha256, &c, || {
                calls += 1;
                Ok(())
            })
            .is_err());
        assert_eq!(calls, 1);
        drop(j);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn revalidate_stale_fork_upgrade_wallet_and_bundle_at_actual_boundary() {
        let (path, a, i, c) = setup();
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        j.reserve(a, i.clone(), &c).unwrap();
        for field in 0..7 {
            let mut bad = c.clone();
            match field {
                0 => bad.now_unix_ms = 2000,
                1 => bad.state_generation = 2,
                2 => bad.program_data_hash = "upgrade".into(),
                3 => {
                    bad.balances.insert("SOL".into(), 19);
                }
                4 => bad.exact_signed_payload_simulated = false,
                5 => bad.canonical_fresh_no_gap_or_fork = false,
                _ => bad.health_funding_route_fee_exit_known = false,
            }
            assert!(j
                .handoff(&i.signed_wire_sha256, &bad, || -> Result<(), String> {
                    panic!("MUST NOT SEND")
                })
                .is_err());
        }
        drop(j);
        fs::remove_dir_all(path).unwrap();
        let (path, a, mut i, c) = setup();
        i.signatures
            .push(solana_sdk::signature::Signature::new_unique().to_string());
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        assert!(j.reserve(a, i, &c).is_err());
        drop(j);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn all_finalized_members_fees_deltas_and_exit_required_even_on_revert() {
        for success in [false, true] {
            let (path, a, i, c) = setup();
            let mut j = Journal::open(&path, &a.wallet).unwrap();
            j.reserve(a.clone(), i.clone(), &c).unwrap();
            j.handoff(&i.signed_wire_sha256, &c, || Ok(())).unwrap();
            let complete = Receipt {
                signature: i.signatures[0].clone(),
                finalized: true,
                success,
                fee_lamports: Some(20),
                wallet_deltas: Some([("SOL".into(), -20)].into_iter().collect()),
                exit_complete: true,
            };
            for field in 0..5 {
                let mut bad = complete.clone();
                match field {
                    0 => bad.finalized = false,
                    1 => bad.fee_lamports = None,
                    2 => bad.wallet_deltas = None,
                    3 => bad.exit_complete = false,
                    _ => bad.signature = solana_sdk::signature::Signature::new_unique().to_string(),
                }
                assert!(j.reconcile(vec![bad]).is_err());
                assert_eq!(j.holds(), i.holds);
            }
            j.reconcile(vec![complete]).unwrap();
            assert!(j.holds().is_empty());
            drop(j);
            let mut reopened = Journal::open(&path, &a.wallet).unwrap();
            assert!(reopened.reserve(a, i, &c).is_err());
            drop(reopened);
            fs::remove_dir_all(path).unwrap();
        }
    }
    #[test]
    fn bundle_partial_landing_does_not_release_single_flight_hold() {
        let (path, a, mut i, mut c) = setup();
        i.signatures
            .push(solana_sdk::signature::Signature::new_unique().to_string());
        c.bundle_simulated = true;
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        j.reserve(a, i.clone(), &c).unwrap();
        j.handoff(&i.signed_wire_sha256, &c, || Ok(())).unwrap();
        let receipt = |signature: &String| Receipt {
            signature: signature.clone(),
            finalized: true,
            success: false,
            fee_lamports: Some(20),
            wallet_deltas: Some([("SOL".into(), -20)].into_iter().collect()),
            exit_complete: true,
        };
        assert!(j.reconcile(vec![receipt(&i.signatures[0])]).is_err());
        assert_eq!(j.holds(), i.holds);
        assert!(j
            .reconcile(vec![receipt(&i.signatures[0]), receipt(&i.signatures[0])])
            .is_err());
        j.reconcile(i.signatures.iter().map(receipt).collect())
            .unwrap();
        assert!(j.holds().is_empty());
        drop(j);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn torn_or_corrupt_journal_is_a_halt_not_a_new_approval() {
        let (path, a, i, c) = setup();
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        j.reserve(a.clone(), i, &c).unwrap();
        let file = path_of(&j);
        drop(j);
        fs::write(&file, b"{}").unwrap();
        assert!(Journal::open(&path, &a.wallet).is_err());
        fs::remove_file(file).unwrap();
        fs::write(path.join(format!("{}.pending", a.wallet)), b"partial").unwrap();
        assert!(Journal::open(&path, &a.wallet).is_err());
        fs::remove_dir_all(path).unwrap();
    }
}

#[cfg(test)]
mod sync_failure_tests {
    use super::*;
    #[test]
    fn reservation_or_handoff_sync_failure_blocks_send_and_recovers_unknown() {
        for during_handoff in [false, true] {
            for error in [std::io::ErrorKind::Other, std::io::ErrorKind::StorageFull] {
                let (dir, a, i, c) = super::tests::setup();
                let mut j = Journal::open(&dir, &a.wallet).unwrap();
                if during_handoff {
                    j.reserve(a.clone(), i.clone(), &c).unwrap();
                }
                j.writer.test_fail_next_sync(error);
                let sends = std::cell::Cell::new(0);
                let result = if during_handoff {
                    j.handoff(&i.signed_wire_sha256, &c, || {
                        sends.set(sends.get() + 1);
                        Ok(())
                    })
                } else {
                    j.reserve(a.clone(), i.clone(), &c)
                };
                let expected = if error == std::io::ErrorKind::StorageFull {
                    "JOURNAL_FSYNC:NO_SPACE"
                } else {
                    "JOURNAL_FSYNC:IO_OTHER"
                };
                assert_eq!(result.unwrap_err(), expected);
                assert_eq!(sends.get(), 0);
                assert!(j
                    .handoff(&i.signed_wire_sha256, &c, || {
                        sends.set(sends.get() + 1);
                        Ok(())
                    })
                    .is_err());
                assert_eq!(sends.get(), 0);
                // Even if the OS retained the unacknowledged complete frame, restart is
                // conservative: holds survive, the attempt becomes UNKNOWN, no new send.
                drop(j);
                let mut reopened = Journal::open(&dir, &a.wallet).unwrap();
                assert_eq!(reopened.holds(), i.holds);
                assert_eq!(
                    reopened.saved.as_ref().unwrap().phase,
                    Phase::HandoffUnknown
                );
                assert!(reopened
                    .handoff(&i.signed_wire_sha256, &c, || {
                        sends.set(sends.get() + 1);
                        Ok(())
                    })
                    .is_err());
                assert_eq!(sends.get(), 0);
                drop(reopened);
                fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}
