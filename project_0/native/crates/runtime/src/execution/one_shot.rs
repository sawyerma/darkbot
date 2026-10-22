//! FE-04 durable single-wallet hold at the existing submission boundary.
//! No signer, provider, state/price engine or automatic LIVE activation.
pub mod receipts;
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SafetyReplay {
    pub held_capital_usd_micro: Option<u64>,
}

/// A durable FE-04 hold that is not finalized (P8-RES-02).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveHold {
    pub attempt: String,
    pub target: String,
    pub unknown_outcome: bool,
    pub holds: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Phase {
    Reserved,
    HandoffUnknown,
    HaltFinalized,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Saved {
    approval: Approval,
    intent: Intent,
    phase: Phase,
    finalized: Option<Vec<Receipt>>,
    #[serde(default)]
    observations: BTreeMap<String, receipts::FinalizedObservation>,
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
/// (`FE04_TRANSITION`, schema 2). Schema-1 JSON frames of older journals stay readable.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Version {
    transition: Transition,
    state: Saved,
}
impl crate::analytics::payload::Payload for Version {
    const KIND: u16 = journal::kind::FE04_TRANSITION;
}
/// F2: the import of a pre-binary JSON wallet journal as a typed frame (`FE04_LEGACY_IMPORT`,
/// schema 2); the JSON file itself stays unchanged and is only read.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyImport {
    legacy_sha256: String,
    state: Saved,
}
impl crate::analytics::payload::Payload for LegacyImport {
    const KIND: u16 = journal::kind::FE04_LEGACY_IMPORT;
}
/// A wallet journal holds a handful of state versions; anything larger is not ours.
const MAX_JOURNAL_BYTES: u64 = 16 * 1024 * 1024;
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
fn digest(v: &[u8]) -> String {
    solana_sdk::hash::hash(v).to_string()
}
fn hex(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn key(s: &str) -> bool {
    s.parse::<solana_sdk::pubkey::Pubkey>().is_ok()
}
/// Pre-binary wallet journal (`<wallet>.json`, digest-wrapped). Read only, never rewritten.
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
    let same = |p: &Saved| p.approval == next.approval && p.intent == next.intent;
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
        _ => false,
    }
}
/// F2: a typed frame (schema 2) is decoded natively; a schema-1 frame is a journal written
/// before F2 and is read through the historical value reader only (never written again).
fn decode_frame<P: crate::analytics::payload::Payload>(
    frame: &journal::Frame,
) -> Result<P, String> {
    if frame.schema == 1 {
        let value = frame
            .legacy_value()
            .map_err(|e| cause("JOURNAL_CORRUPT", &e))?;
        return serde_json::from_value(value).map_err(|e| json_cause("JOURNAL_CORRUPT", &e));
    }
    frame.typed().map_err(|e| cause("JOURNAL_CORRUPT", &e))
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

impl Journal {
    pub fn open(directory: &Path, wallet: &str) -> Result<Self, String> {
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
        let lock = WalletLock(lock);
        let legacy = directory.join(format!("{wallet}.json"));
        // An interrupted pre-binary write blocks further actions; never silently truncate it.
        if legacy.with_extension("pending").exists() {
            return Err(err("JOURNAL_INTERRUPTED_WRITE_HALT"));
        }
        let legacy = if legacy.exists() {
            Some(read_legacy(&legacy, wallet)?)
        } else {
            None
        };
        let path = directory.join(format!("{wallet}.p0j"));
        let (saved, writer) = if path.exists() {
            if fs::metadata(&path)
                .map_err(|e| cause("JOURNAL_READ", &e))?
                .len()
                > MAX_JOURNAL_BYTES
            {
                return Err(err("JOURNAL_SIZE_LIMIT"));
            }
            let (saved, imported) = replay(&path, wallet)?;
            // The import binds the unchanged JSON bytes; a missing or edited JSON file is a halt.
            match (&legacy, imported, &saved) {
                (Some((sha, _)), Some(imported), _) if *sha == imported => {}
                (None, None, _) | (Some(_), None, None) => {}
                _ => return Err(err("JOURNAL_LEGACY_MISMATCH")),
            }
            let writer =
                journal::Writer::resume(&path).map_err(|e| cause("JOURNAL_CORRUPT", &e))?;
            (saved, writer)
        } else {
            (
                None,
                journal::Writer::create(&path).map_err(|e| cause("JOURNAL_IO", &e))?,
            )
        };
        let mut this = Self {
            wallet: wallet.into(),
            saved,
            writer,
            _lock: lock,
        };
        if this.saved.is_none() {
            if let Some((legacy_sha256, state)) = legacy {
                this.append(&LegacyImport {
                    legacy_sha256,
                    state: state.clone(),
                })?;
                this.saved = Some(state);
            }
        }
        if let Some(mut prior) = this.saved.clone() {
            if prior.phase == Phase::Reserved {
                prior.phase = Phase::HandoffUnknown;
                this.persist(prior, Transition::ReopenUnknown)?;
            }
        }
        Ok(this)
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
    fn reserve(&mut self, approval: Approval, intent: Intent, now: &Current) -> Result<(), String> {
        if self.saved.is_some() {
            return Err(err("ONE_SHOT_ALREADY_CONSUMED_OR_UNKNOWN"));
        }
        if approval.wallet != self.wallet {
            return Err(err("JOURNAL_WALLET_MISMATCH"));
        }
        validate(&approval, &intent, now)?;
        self.persist(
            Saved {
                approval,
                intent,
                phase: Phase::Reserved,
                finalized: None,
                observations: BTreeMap::new(),
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
    /// outcome — to rebuild the global reservation book after a restart; `None` once finalized.
    pub fn active_hold(&self) -> Option<ActiveHold> {
        self.saved
            .as_ref()
            .filter(|s| s.phase != Phase::HaltFinalized)
            .map(|s| ActiveHold {
                attempt: s.approval.approval_id.clone(),
                target: s.intent.target.clone(),
                unknown_outcome: s.phase == Phase::HandoffUnknown,
                holds: s.intent.holds.clone(),
            })
    }
    /// B3: what the safety layer replays after a restart — `None` without any attempt; with one,
    /// the one-shot is consumed and its realized PnL is not booked (C3), so its HALT is sticky,
    /// and until it is finalized it still binds its capital.
    pub fn safety_replay(&self) -> Option<SafetyReplay> {
        self.saved.as_ref().map(|s| SafetyReplay {
            held_capital_usd_micro: (s.phase != Phase::HaltFinalized)
                .then_some(s.intent.capital_usd_micro),
        })
    }
    pub fn holds(&self) -> BTreeMap<String, u64> {
        self.saved
            .as_ref()
            .filter(|s| s.phase != Phase::HaltFinalized)
            .map(|s| s.intent.holds.clone())
            .unwrap_or_default()
    }
}

fn validate(a: &Approval, i: &Intent, c: &Current) -> Result<(), String> {
    if a.schema != 1
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
        || a.max_slippage_bps > 10_000
    {
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
        };
        let version = Version {
            transition: Transition::Finalize,
            state: state.clone(),
        };
        let bytes = crate::analytics::payload::encode(&version).unwrap();
        let back: Version = crate::analytics::payload::decode(
            journal::kind::FE04_TRANSITION,
            crate::analytics::payload::TYPED,
            &bytes,
        )
        .unwrap();
        assert_eq!((back.transition, back.state), (Transition::Finalize, state));
        assert_eq!(
            (bytes.len(), solana_sdk::hash::hash(&bytes).to_string()),
            (
                1108,
                "HwhygqtWc6VthHdUKxendZqFqfWF593a6mo8EFvxmWtn".to_string()
            ),
            "FE04_TRANSITION schema-2 layout changed"
        );
    }

    /// F2: every new wallet-journal frame is a typed binary frame (schema 2, no JSON value
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
        assert_eq!(
            (frames[0].kind, frames[0].schema),
            (
                journal::kind::FE04_TRANSITION,
                crate::analytics::payload::TYPED
            )
        );
        assert!(frames[0].legacy_value().is_err());
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
            .append_legacy(
                journal::kind::FE04_TRANSITION,
                Default::default(),
                &historical
            )
            .is_err());
        w.test_append_historical(journal::kind::FE04_TRANSITION, &historical)
            .unwrap();
        w.sync().unwrap();
        drop(w);
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
            [1, crate::analytics::payload::TYPED],
            "historical frame kept, the reopen transition is typed"
        );
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
        assert_eq!(
            book.reserve_target("b", &hold.target, &i.holds, &c.balances),
            Err("RESERVATION_TARGET_CONTENDED")
        );
        assert_eq!(
            book.reserve_target("b", "another target", &i.holds, &c.balances),
            Ok(())
        );
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
                s.restore_attempt(r.held_capital_usd_micro).unwrap();
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
        ) -> Result<darkbot_solana::submit::SubmitAck, darkbot_solana::submit::SubmitError>
        {
            self.submit_bundle(std::slice::from_ref(tx), t)
        }
        fn submit_bundle(
            &self,
            txs: &[solana_sdk::transaction::VersionedTransaction],
            t: u64,
        ) -> Result<darkbot_solana::submit::SubmitAck, darkbot_solana::submit::SubmitError>
        {
            self.calls.set(self.calls.get() + 1);
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
            },
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
                let simulated = payload.simulate(&adapter, test_deadline()).unwrap();
                let mut j = Journal::open(&dir, &a.wallet).unwrap();
                j.reserve_payload(a.clone(), i.clone(), &c, &simulated)
                    .unwrap();
                let result = j.submit_payload(&simulated, &c, 123);
                assert_eq!(result.is_ok(), failure == 0);
                assert_eq!(j.saved.as_ref().unwrap().phase, Phase::HandoffUnknown);
                assert_eq!(j.holds(), i.holds);
                assert!(j.submit_payload(&simulated, &c, 124).is_err());
                assert_eq!(adapter.calls.get(), 1);
                drop(j);
                let mut reopened = Journal::open(&dir, &a.wallet).unwrap();
                assert!(reopened.submit_payload(&simulated, &c, 125).is_err());
                assert_eq!(reopened.holds(), i.holds);
                drop(reopened);
                fs::remove_dir_all(dir).unwrap();
            }
        }
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
        let original = original.simulate(&adapter, test_deadline()).unwrap();
        let reversed = reversed.simulate(&adapter, test_deadline()).unwrap();
        let mut j = Journal::open(&dir, &a.wallet).unwrap();
        assert!(j
            .reserve_payload(a.clone(), i.clone(), &c, &reversed)
            .is_err());
        j.reserve_payload(a, i, &c, &original).unwrap();
        assert!(j.submit_payload(&reversed, &c, 1).is_err());
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
            assert!(p.simulate(&a, test_deadline()).is_err());
            assert_eq!(a.calls.get(), 0);
        }
        let a = adapter(SubmissionPath::Rpc, 0);
        let p = transport::SignedPayload::new(
            vec![signed(&payer)],
            SubmissionPath::JitoBundle,
            payer.pubkey(),
        )
        .unwrap();
        assert!(p.simulate(&a, test_deadline()).is_err());
        assert!(a.simulated.borrow().is_empty());
    }
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
    fn reserved(a: &Approval, i: &Intent) -> Saved {
        Saved {
            approval: a.clone(),
            intent: i.clone(),
            phase: Phase::Reserved,
            finalized: None,
            observations: BTreeMap::new(),
        }
    }
    fn path_of(j: &Journal) -> PathBuf {
        j.writer.path.clone()
    }
    fn kinds(path: &Path) -> Vec<u16> {
        let mut r = journal::Reader::open(path).unwrap();
        std::iter::from_fn(|| r.next_frame().unwrap())
            .map(|f| f.kind)
            .collect()
    }
    #[test]
    fn legacy_journal_without_observations_reopens_with_hold() {
        let (dir, a, i, _) = setup();
        let legacy = write_legacy(&dir, &reserved(&a, &i), true);
        let bytes = fs::read(&legacy).unwrap();
        let reopened = Journal::open(&dir, &a.wallet).unwrap();
        assert_eq!(reopened.holds(), i.holds);
        assert!(reopened.saved.as_ref().unwrap().observations.is_empty());
        assert_eq!(
            reopened.saved.as_ref().unwrap().phase,
            Phase::HandoffUnknown
        );
        let binary = dir.join(format!("{}.p0j", a.wallet));
        drop(reopened);
        // The JSON journal is historical evidence: never rewritten, its bytes are bound once.
        assert_eq!(fs::read(&legacy).unwrap(), bytes);
        assert_eq!(
            kinds(&binary),
            [
                journal::kind::FE04_LEGACY_IMPORT,
                journal::kind::FE04_TRANSITION
            ]
        );
        assert_eq!(Journal::open(&dir, &a.wallet).unwrap().holds(), i.holds);
        let mut edited = bytes.clone();
        edited.push(b' ');
        fs::write(&legacy, edited).unwrap();
        assert_eq!(
            Journal::open(&dir, &a.wallet).err().as_deref(),
            Some("JOURNAL_LEGACY_MISMATCH")
        );
        fs::remove_file(&legacy).unwrap();
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
        assert_eq!(
            Journal::open(&dir, &a.wallet).err().as_deref(),
            Some("JOURNAL_CORRUPT:KIND")
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
