//! FE-04 durable single-wallet hold at the existing submission boundary.
//! No signer, provider, state/price engine or automatic LIVE activation.
pub mod receipts;
pub mod transport;

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
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
    /// Each existing gate must independently be known and passed. In particular
    /// a single-TX simulation cannot satisfy the bundle_simulated gate.
    pub health_funding_route_fee_exit_known: bool,
    pub canonical_fresh_no_gap_or_fork: bool,
    pub exact_signed_payload_simulated: bool,
    pub bundle_simulated: bool,
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

pub struct Journal {
    _lock: WalletLock,
    path: PathBuf,
    wallet: String,
    saved: Option<Saved>,
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
fn err(s: &str) -> String {
    s.into()
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

impl Journal {
    pub fn open(directory: &Path, wallet: &str) -> Result<Self, String> {
        if !key(wallet) {
            return Err(err("WALLET_INVALID"));
        }
        fs::create_dir_all(directory).map_err(|_| err("JOURNAL_IO"))?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join(format!("{wallet}.lock")))
            .map_err(|_| err("JOURNAL_LOCK_IO"))?;
        lock.try_lock().map_err(|_| err("WALLET_ALREADY_LOCKED"))?;
        let lock = WalletLock(lock);
        let path = directory.join(format!("{wallet}.json"));
        if path.with_extension("pending").exists() {
            return Err(err("JOURNAL_INTERRUPTED_WRITE_HALT"));
        }
        let saved = if path.exists() {
            if fs::metadata(&path).map_err(|_| err("JOURNAL_READ"))?.len() > 256 * 1024 {
                return Err(err("JOURNAL_SIZE_LIMIT"));
            }
            let bytes = fs::read(&path).map_err(|_| err("JOURNAL_READ"))?;
            let wrapper: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|_| err("JOURNAL_CORRUPT"))?;
            let data =
                serde_json::to_vec(&wrapper["record"]).map_err(|_| err("JOURNAL_CORRUPT"))?;
            if wrapper["digest"] != digest(&data) {
                return Err(err("JOURNAL_CORRUPT"));
            }
            let s: Saved = serde_json::from_value(wrapper["record"].clone())
                .map_err(|_| err("JOURNAL_CORRUPT"))?;
            if s.approval.wallet != wallet || s.intent.wallet != wallet {
                return Err(err("JOURNAL_WALLET_MISMATCH"));
            }
            Some(s)
        } else {
            None
        };
        let mut journal = Self {
            _lock: lock,
            path,
            wallet: wallet.into(),
            saved,
        };
        if let Some(mut prior) = journal.saved.clone() {
            if prior.phase == Phase::Reserved {
                prior.phase = Phase::HandoffUnknown;
                journal.persist(prior)?;
            }
        }
        Ok(journal)
    }
    fn persist(&mut self, saved: Saved) -> Result<(), String> {
        let record = serde_json::to_value(&saved).map_err(|_| err("JOURNAL_ENCODE"))?;
        let bytes = serde_json::to_vec(&record).map_err(|_| err("JOURNAL_ENCODE"))?;
        let wrapper = serde_json::json!({"record":record,"digest":digest(&bytes)});
        let tmp = self.path.with_extension("pending");
        // An interrupted write blocks further actions; never silently truncate it.
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp)
            .map_err(|_| err("JOURNAL_PENDING_OR_IO"))?;
        serde_json::to_writer(&mut file, &wrapper).map_err(|_| err("JOURNAL_WRITE"))?;
        file.flush()
            .and_then(|_| file.sync_all())
            .map_err(|_| err("JOURNAL_FSYNC"))?;
        fs::rename(&tmp, &self.path).map_err(|_| err("JOURNAL_RENAME"))?;
        File::open(self.path.parent().unwrap())
            .and_then(|f| f.sync_all())
            .map_err(|_| err("JOURNAL_DIR_FSYNC"))?;
        self.saved = Some(saved);
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
        self.persist(Saved {
            approval,
            intent,
            phase: Phase::Reserved,
            finalized: None,
            observations: BTreeMap::new(),
        })
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
        self.persist(s)?; // durable BEFORE any possible acceptance; errors retain holds
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
        self.persist(s)
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
        || i.holds
            .iter()
            .any(|(mint, n)| c.balances.get(mint).is_none_or(|balance| balance < n))
    {
        return Err(err("WALLET_UNDERFUNDED_OR_HOLD_MISSING"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
            health_funding_route_fee_exit_known: true,
            canonical_fresh_no_gap_or_fork: true,
            exact_signed_payload_simulated: true,
            bundle_simulated: false,
        };
        (path, a, i, c)
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
        fn simulate(
            &self,
            tx: &solana_sdk::transaction::VersionedTransaction,
        ) -> darkbot_solana::submit::SimResult {
            self.simulate_bundle(std::slice::from_ref(tx))
        }
        fn simulate_bundle(
            &self,
            txs: &[solana_sdk::transaction::VersionedTransaction],
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
                let simulated = payload.simulate(&adapter).unwrap();
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
        let original = original.simulate(&adapter).unwrap();
        let reversed = reversed.simulate(&adapter).unwrap();
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
            assert!(p.simulate(&a).is_err());
            assert_eq!(a.calls.get(), 0);
        }
        let a = adapter(SubmissionPath::Rpc, 0);
        let p = transport::SignedPayload::new(
            vec![signed(&payer)],
            SubmissionPath::JitoBundle,
            payer.pubkey(),
        )
        .unwrap();
        assert!(p.simulate(&a).is_err());
        assert!(a.simulated.borrow().is_empty());
    }
    #[test]
    fn legacy_journal_without_observations_reopens_with_hold() {
        let (dir, a, i, c) = setup();
        let mut journal = Journal::open(&dir, &a.wallet).unwrap();
        journal.reserve(a.clone(), i.clone(), &c).unwrap();
        let path = journal.path.clone();
        drop(journal);
        let mut wrapper: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        wrapper["record"]
            .as_object_mut()
            .unwrap()
            .remove("observations");
        wrapper["digest"] =
            serde_json::json!(digest(&serde_json::to_vec(&wrapper["record"]).unwrap()));
        fs::write(&path, serde_json::to_vec(&wrapper).unwrap()).unwrap();
        let reopened = Journal::open(&dir, &a.wallet).unwrap();
        assert_eq!(reopened.holds(), i.holds);
        assert!(reopened.saved.as_ref().unwrap().observations.is_empty());
        assert_eq!(
            reopened.saved.as_ref().unwrap().phase,
            Phase::HandoffUnknown
        );
        drop(reopened);
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
        let file = j.path.clone();
        drop(j);
        fs::write(&file, b"{}").unwrap();
        assert!(Journal::open(&path, &a.wallet).is_err());
        fs::remove_file(file).unwrap();
        fs::write(path.join(format!("{}.pending", a.wallet)), b"partial").unwrap();
        assert!(Journal::open(&path, &a.wallet).is_err());
        fs::remove_dir_all(path).unwrap();
    }
}
