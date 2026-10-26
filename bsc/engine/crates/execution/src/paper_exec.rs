//! The C3 production path in PAPER mode (order V2 §7.4, PHASENPLAN §11/§16):
//! a pending launch goes through the production venue decoder, the C2
//! chain-state snapshot, the venue economics and admission, the production
//! BUY build and signer, the Unit-4 intent lane, and ends at the PAPER sink
//! (`T_shadow_sink`, never `T_send`).
//!
//! Every stage is the production code; only the end differs: the sink has
//! no transport (`paper`), the signer is the PAPER identity.
//!
//! Lanes: a PAPER transaction is never mined, so no inclusion proof will
//! ever resolve its intent, and a Unit-4 lane blocks behind an unresolved
//! head. The executor therefore arms a bounded pool of single-intent lanes
//! when it starts, each behind its own durable fence (created, written and
//! synced at arming, never on the entry path), and every PAPER BUY consumes
//! one lane. The lane semantics stay exactly those of production; an empty
//! pool refuses further BUYs, visibly, until the next start.

use crate::paper::{PaperHandoff, PaperIdentity, PaperSink};
use alloy_primitives::{Address, B256, U256};
use darkbot_bsc_trading::chain_state::SnapshotId;
use darkbot_bsc_trading::economics::flap::native_curve20::{
    QualifiedNativeState, VenueBuyTerms, admit_native_buy,
};
use darkbot_bsc_trading::economics::policy::BuyPolicy;
use darkbot_bsc_trading::intent::{AdapterId, AdapterSet, ArmedFence, IntentLane};
use darkbot_bsc_trading::submission_policy::plan::PlanCosts;
use darkbot_bsc_trading::venues::flap::binding::ExpectedBuy;
use darkbot_bsc_trading::venues::flap::deployment::ValidatedDeployment;
use darkbot_bsc_trading::venues::flap::profile::{UnsupportedLaunch, classify_launch};
use darkbot_bsc_trading::venues::flap::recognize::{Recognized, recognize};
use std::collections::VecDeque;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;

/// The operating values of the PAPER path (`BSC_EXEC_*`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecConfig {
    pub input_wei: u128,
    pub slippage_bps: u16,
    pub gas_price_wei: u64,
    pub gas_limit: u64,
    /// Cumulative exposure cap of one lane's session (Unit 4, E5-1).
    pub session_cap_wei: u128,
    /// Single-intent lanes armed per start.
    pub lanes: usize,
    /// Largest calldata the decoder accepts.
    pub max_calldata: usize,
    /// Latency samples kept for the status quantiles.
    pub latency_samples: usize,
    pub policy_generation: u64,
}

/// The one adapter slot of a PAPER lane: the sink.
pub fn paper_adapter() -> AdapterId {
    // Slot 0 exists in every lane (`MAX_ADAPTERS` > 0).
    AdapterId::new(0).unwrap_or_else(|| unreachable!("adapter slot 0"))
}

/// Where a launch left the path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The decoder did not decode a `newTokenV6` launch.
    NotDecoded,
    /// The launch is not the qualified profile.
    Profile(UnsupportedLaunch),
    /// C2 holds no coherent snapshot for this candidate.
    NoSnapshot,
    /// The fresh-launch state or the admission refused.
    Admission,
    /// The expectation could not be formed.
    Intent,
    /// Every armed lane is used.
    LanesExhausted,
    /// The lane refused (reserve, prepare, begin, write).
    Lane,
    /// The build or signature failed.
    Build,
}

impl Refusal {
    pub fn name(self) -> &'static str {
        match self {
            Self::NotDecoded => "NOT_DECODED",
            Self::Profile(_) => "PROFILE",
            Self::NoSnapshot => "NO_SNAPSHOT",
            Self::Admission => "ADMISSION",
            Self::Intent => "INTENT",
            Self::LanesExhausted => "LANES_EXHAUSTED",
            Self::Lane => "LANE",
            Self::Build => "BUILD",
        }
    }
}

/// The stage times of one PAPER BUY on the runtime's monotonic clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StageTimes {
    /// The source handed the verified pending launch to the runtime.
    pub ingress_ns: u64,
    pub decoded_ns: u64,
    pub admitted_ns: u64,
    pub signed_ns: u64,
    /// The sink took the bytes: `T_shadow_sink`.
    pub sink_ns: u64,
}

/// One PAPER BUY that reached the sink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaperBuy {
    pub launch_hash: [u8; 32],
    pub token: [u8; 20],
    pub handoff: PaperHandoff,
    pub times: StageTimes,
    /// The signed bytes (a local test artefact, never sent).
    pub raw: Vec<u8>,
}

/// The executor's counters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecCounters {
    pub launches: u64,
    pub handoffs: u64,
    /// Refusals by stage and reason, in first-seen order.
    pub refusals: Vec<(Refusal, u64)>,
}

impl ExecCounters {
    fn refuse(&mut self, r: Refusal) {
        match self.refusals.iter_mut().find(|(k, _)| *k == r) {
            Some((_, n)) => *n += 1,
            None => self.refusals.push((r, 1)),
        }
    }

    /// Refusals of one stage, all profile reasons together.
    pub fn refused(&self, name: &str) -> u64 {
        self.refusals
            .iter()
            .filter(|(k, _)| k.name() == name)
            .map(|(_, n)| n)
            .sum()
    }
}

/// The latest `capacity` ingress→sink latencies (ns), for quantiles.
#[derive(Clone, Debug)]
pub struct Latencies {
    capacity: usize,
    samples: VecDeque<u64>,
    max: u64,
}

impl Latencies {
    fn new(capacity: usize) -> Self {
        Latencies {
            capacity: capacity.max(1),
            samples: VecDeque::with_capacity(capacity.max(1)),
            max: 0,
        }
    }

    fn push(&mut self, ns: u64) {
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(ns);
        self.max = self.max.max(ns);
    }

    /// (p50, p99, max over the run) of the kept samples; `None` without.
    pub fn quantiles(&self) -> Option<(u64, u64, u64)> {
        if self.samples.is_empty() {
            return None;
        }
        let mut sorted: Vec<u64> = self.samples.iter().copied().collect();
        sorted.sort_unstable();
        let at = |p: usize| sorted[(sorted.len() - 1) * p / 100];
        Some((at(50), at(99), self.max))
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// The armed PAPER path.
pub struct PaperExecutor {
    identity: PaperIdentity,
    deployment: ValidatedDeployment,
    terms: VenueBuyTerms,
    policy: BuyPolicy,
    config: ExecConfig,
    lanes: Vec<IntentLane>,
    used: usize,
    sink: PaperSink,
    scratch: Vec<u8>,
    raw: Vec<u8>,
    counters: ExecCounters,
    latencies: Latencies,
}

/// Why the executor could not be armed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArmError {
    Policy(String),
    Fence(String),
    Lane(String),
}

impl core::fmt::Display for ArmError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Policy(m) => write!(f, "paper exec policy: {m}"),
            Self::Fence(m) => write!(f, "paper exec fence: {m}"),
            Self::Lane(m) => write!(f, "paper exec lane: {m}"),
        }
    }
}

impl std::error::Error for ArmError {}

/// Creates, writes and syncs one fence file that did not exist: the
/// durable attestation behind one lane.
fn durable_fence(path: &Path, bytes: &[u8]) -> Result<(), ArmError> {
    let e = |err: std::io::Error| ArmError::Fence(format!("{}: {err}", path.display()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(e)?;
    file.write_all(bytes).map_err(e)?;
    file.sync_all().map_err(e)?;
    Ok(())
}

impl PaperExecutor {
    /// Arms the PAPER path: the policy, then `config.lanes` single-intent
    /// lanes at the sender's canonical `next_nonce` (a C2 nonce fact), each
    /// behind its own new fence in `fence_dir` named after `session`.
    pub fn arm(
        identity: PaperIdentity,
        deployment: ValidatedDeployment,
        terms: VenueBuyTerms,
        config: ExecConfig,
        next_nonce: u64,
        fence_dir: &Path,
        session: u64,
    ) -> Result<Self, ArmError> {
        let policy = BuyPolicy::new(
            config.policy_generation,
            U256::from(config.input_wei),
            config.slippage_bps,
            config.gas_price_wei,
            config.gas_limit,
        )
        .map_err(|e| ArmError::Policy(format!("{e:?}")))?;
        fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(fence_dir)
            .map_err(|e| ArmError::Fence(format!("{}: {e}", fence_dir.display())))?;
        let sender = Address::from(identity.address());
        let chain_id = deployment.deployment().chain_id;
        let mut lanes = Vec::with_capacity(config.lanes);
        for i in 0..config.lanes {
            let mut body = Vec::with_capacity(8 + 8 + 20 + 8 + 16);
            body.extend_from_slice(b"DBPAPFN1");
            body.extend_from_slice(&chain_id.to_le_bytes());
            body.extend_from_slice(sender.as_slice());
            body.extend_from_slice(&next_nonce.to_le_bytes());
            body.extend_from_slice(&config.session_cap_wei.to_le_bytes());
            durable_fence(&fence_dir.join(format!("{session}-{i}.fence")), &body)?;
            let fence = ArmedFence::from_durable_startup(
                chain_id,
                sender,
                next_nonce,
                U256::from(config.session_cap_wei),
            );
            lanes.push(IntentLane::armed(fence, 1).map_err(|e| ArmError::Lane(format!("{e:?}")))?);
        }
        let samples = config.latency_samples;
        Ok(PaperExecutor {
            identity,
            deployment,
            terms,
            policy,
            config,
            lanes,
            used: 0,
            sink: PaperSink::default(),
            scratch: Vec::with_capacity(512),
            raw: Vec::with_capacity(512),
            counters: ExecCounters::default(),
            latencies: Latencies::new(samples),
        })
    }

    /// One verified pending launch (`hash`, signed `raw` calldata `input`
    /// to `to`), observed at `ingress_ns`. `snapshot` asks C2 for the
    /// coherent snapshot of this candidate under the given policy and code
    /// generations; `now_ns` reads the runtime clock.
    pub fn on_launch(
        &mut self,
        hash: [u8; 32],
        to: Option<[u8; 20]>,
        input: &[u8],
        ingress_ns: u64,
        snapshot: impl FnOnce(B256, u64, u64) -> Option<SnapshotId>,
        now_ns: impl Fn() -> u64,
    ) -> Result<PaperBuy, Refusal> {
        self.counters.launches += 1;
        let result = self.path(hash, to, input, ingress_ns, snapshot, &now_ns);
        match &result {
            Ok(buy) => {
                self.counters.handoffs += 1;
                self.latencies
                    .push(buy.times.sink_ns.saturating_sub(buy.times.ingress_ns));
            }
            Err(r) => self.counters.refuse(*r),
        }
        result
    }

    fn path(
        &mut self,
        hash: [u8; 32],
        to: Option<[u8; 20]>,
        input: &[u8],
        ingress_ns: u64,
        snapshot: impl FnOnce(B256, u64, u64) -> Option<SnapshotId>,
        now_ns: &impl Fn() -> u64,
    ) -> Result<PaperBuy, Refusal> {
        let portal = self.deployment.deployment().portal;
        let Recognized::LaunchV6(launch) = recognize(
            portal,
            to.map(Address::from),
            input,
            self.config.max_calldata,
        ) else {
            return Err(Refusal::NotDecoded);
        };
        let decoded_ns = now_ns();
        let candidate = B256::from(hash);
        let classified =
            classify_launch(&self.deployment, candidate, &launch).map_err(Refusal::Profile)?;
        let snapshot = snapshot(
            candidate,
            self.policy.generation(),
            self.deployment.codehash_generation(),
        )
        .ok_or(Refusal::NoSnapshot)?;
        // A pending launch: no trade of its token precedes it.
        let state = QualifiedNativeState::fresh_launch(&classified, &self.terms, 0, snapshot)
            .map_err(|_| Refusal::Admission)?;
        let admitted = admit_native_buy(&state, &self.policy).map_err(|_| Refusal::Admission)?;
        let admitted_ns = now_ns();
        let Some(lane) = self.lanes.get_mut(self.used) else {
            return Err(Refusal::LanesExhausted);
        };
        // The lane is consumed by this attempt, whatever happens next.
        self.used += 1;
        let ticket = lane.reserve(admitted.token()).map_err(|_| Refusal::Lane)?;
        let sender = Address::from(self.identity.address());
        let expected = ExpectedBuy::from_admitted(
            &self.deployment,
            &admitted,
            &self.policy,
            sender,
            ticket.nonce(),
        )
        .map_err(|_| Refusal::Intent)?;
        let tx_hash = expected
            .sign(self.identity.signer(), &mut self.scratch, &mut self.raw)
            .map_err(|_| Refusal::Build)?;
        let signed_ns = now_ns();
        lane.prepare(ticket, &self.raw, &expected, &snapshot, &mut self.scratch)
            .map_err(|_| Refusal::Lane)?;
        let buy_gas_wei = U256::from(expected.gas_price()) * U256::from(expected.gas_limit());
        let costs = PlanCosts {
            buy_input_wei: expected.input_amount(),
            buy_gas_wei,
            own_launch_wei: U256::ZERO,
            provider_payment_wei: U256::ZERO,
            total_wei: expected.input_amount() + buy_gas_wei,
        };
        let adapter = paper_adapter();
        let adapters = AdapterSet::new(&[adapter]).ok_or(Refusal::Lane)?;
        let dispatch = lane
            .begin_send(&snapshot, adapters, &costs)
            .map_err(|_| Refusal::Lane)?;
        let write = lane
            .start_write(dispatch.ticket, adapter, tx_hash)
            .map_err(|_| Refusal::Lane)?;
        let sink_ns = now_ns();
        let handoff = self.sink.handoff(tx_hash.0, write.bytes(), sink_ns);
        let raw = write.bytes().to_vec();
        lane.write_completed(dispatch.ticket, adapter)
            .map_err(|_| Refusal::Lane)?;
        Ok(PaperBuy {
            launch_hash: hash,
            token: admitted.token().into_array(),
            handoff,
            times: StageTimes {
                ingress_ns,
                decoded_ns,
                admitted_ns,
                signed_ns,
                sink_ns,
            },
            raw,
        })
    }

    pub fn sender(&self) -> [u8; 20] {
        self.identity.address()
    }

    pub fn counters(&self) -> &ExecCounters {
        &self.counters
    }

    pub fn latencies(&self) -> &Latencies {
        &self.latencies
    }

    /// Lanes armed and lanes left.
    pub fn lanes(&self) -> (usize, usize) {
        (self.lanes.len(), self.lanes.len() - self.used)
    }

    pub fn sink(&self) -> &PaperSink {
        &self.sink
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::hex;
    use darkbot_bsc_trading::economics::flap::native_curve20::WITNESSED_MAINNET_TERMS;
    use darkbot_bsc_trading::transactions::legacy::decode_signed;
    use darkbot_bsc_trading::venues::flap::abi::{
        LaunchParamsV6, decode_swap_exact_input_native, encode_new_token_v6,
    };
    use darkbot_bsc_trading::venues::flap::deployment::{
        ObservedDeployment, bsc_mainnet_v5_24_0, bsc_mainnet_v5_27_0,
    };
    use std::cell::Cell;
    use std::path::PathBuf;

    const POLICY_GENERATION: u64 = 3;
    const CODEHASH_GENERATION: u64 = 5;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bsc-paper-exec-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    /// The deployment whose economics G0.6a witnessed (the live v5.27.0 is
    /// decode-only until reconfirmed; the decode path is the same Portal).
    fn validated() -> ValidatedDeployment {
        let d = bsc_mainnet_v5_24_0();
        d.validate(
            &ObservedDeployment {
                chain_id: d.chain_id,
                portal_version: d.portal_version,
                portal_runtime_sha256: d.portal_runtime_sha256,
                portal_implementation: d.portal_implementation,
                portal_implementation_runtime_sha256: d.portal_implementation_runtime_sha256,
                tax_v3_implementation_runtime_sha256: d.tax_v3_implementation_runtime_sha256,
            },
            CODEHASH_GENERATION,
        )
        .unwrap()
    }

    fn config(lanes: usize) -> ExecConfig {
        ExecConfig {
            // The amount covered by witnessed evidence.
            input_wei: 1_000_000_000_000_000,
            slippage_bps: 500,
            gas_price_wei: 1_000_000_000,
            gas_limit: 300_000,
            session_cap_wei: 10_000_000_000_000_000,
            lanes,
            max_calldata: 4_096,
            latency_samples: 64,
            policy_generation: POLICY_GENERATION,
        }
    }

    /// The witnessed native self-launch: the one qualified profile.
    fn witnessed_launch() -> Vec<u8> {
        let mut beneficiary = [0u8; 20];
        beneficiary[18] = 0xa1;
        beneficiary[19] = 0x1c;
        let launch = LaunchParamsV6 {
            name: b"DarkBot G06A Economics",
            symbol: b"DBG06E",
            uri: b"ipfs://darkbot-g06a-economics",
            dex_thresh: 1,
            salt: B256::from(hex!(
                "09c4b20c7956530d4bf91b3569ed173abfce47d16d412d94dde5e6f4034c6eeb"
            )),
            migrator_type: 1,
            quote_token: Address::ZERO,
            quote_amt: U256::ZERO,
            beneficiary: Address::from(beneficiary),
            permit: &[],
            extension_id: B256::ZERO,
            extension_data: &[],
            dex_id: 0,
            lp_fee_profile: 0,
            buy_tax: 100,
            sell_tax: 100,
            tax_duration: 31_536_000,
            anti_farmer_duration: 86_400,
            mkt_bps: 10_000,
            deflation_bps: 0,
            dividend_bps: 0,
            lp_bps: 0,
            minimum_share_balance: U256::ZERO,
            dividend_token: Address::ZERO,
            commission_receiver: Address::ZERO,
            token_version: 6,
        };
        let mut out = Vec::new();
        encode_new_token_v6(&launch, &mut out);
        out
    }

    fn snapshot(candidate: B256, policy: u64, codehash: u64) -> Option<SnapshotId> {
        Some(SnapshotId {
            canonical_block_hash: B256::repeat_byte(0xb1),
            ordered_state_cursor: 9,
            pending_candidate: candidate,
            policy_generation: policy,
            codehash_generation: codehash,
        })
    }

    fn armed(name: &str, lanes: usize) -> (PaperExecutor, PathBuf) {
        let d = dir(name);
        let key = d.join("identity/paper.key");
        PaperIdentity::create(&key, Path::new("/nonexistent-repo")).unwrap();
        let identity = PaperIdentity::load(&key, Path::new("/nonexistent-repo"), &[]).unwrap();
        let exec = PaperExecutor::arm(
            identity,
            validated(),
            WITNESSED_MAINNET_TERMS,
            config(lanes),
            0,
            &d.join("fences"),
            1,
        )
        .unwrap();
        (exec, d)
    }

    /// C3: the witnessed launch runs the whole production path and ends at
    /// the sink as a BUY the send boundary would accept: the PAPER sender,
    /// the canonical nonce, the Portal, the derived token, the admitted
    /// amounts; stage times ordered on the one clock.
    #[test]
    fn the_witnessed_launch_reaches_the_sink_as_the_admitted_buy() {
        let (mut exec, d) = armed("path", 2);
        let portal = bsc_mainnet_v5_27_0().portal.into_array();
        let clock = Cell::new(100u64);
        let tick = || {
            clock.set(clock.get() + 7);
            clock.get()
        };
        let buy = exec
            .on_launch(
                [0xa1; 32],
                Some(portal),
                &witnessed_launch(),
                100,
                snapshot,
                tick,
            )
            .unwrap();
        let signed = decode_signed(&buy.raw).unwrap();
        assert_eq!(signed.tx.chain_id, 56);
        assert_eq!(signed.tx.nonce, 0);
        assert_eq!(signed.tx.to.into_array(), portal);
        assert_eq!(signed.tx.value, U256::from(1_000_000_000_000_000u64));
        let call = decode_swap_exact_input_native(signed.tx.input).unwrap();
        // CREATE2 of the witnessed salt (G0.6a evm crosscheck token).
        assert_eq!(
            call.output_token.into_array(),
            hex!("e51c4deb1038afcd95ef12d4a649cdbcf6fa7777")
        );
        assert_eq!(buy.token, call.output_token.into_array());
        assert!(!call.min_output_amount.is_zero());
        let t = buy.times;
        assert!(t.ingress_ns < t.decoded_ns && t.decoded_ns < t.admitted_ns);
        assert!(t.admitted_ns < t.signed_ns && t.signed_ns < t.sink_ns);
        assert_eq!(
            (buy.handoff.sequence, buy.handoff.raw_len),
            (1, buy.raw.len())
        );
        assert_eq!(exec.counters().handoffs, 1);
        assert_eq!(exec.lanes(), (2, 1));
        assert_eq!(
            exec.latencies().quantiles().map(|q| q.2),
            Some(t.sink_ns - 100)
        );
        // Two fences, one per lane, written at arming.
        assert_eq!(fs::read_dir(d.join("fences")).unwrap().count(), 2);
        fs::remove_dir_all(&d).unwrap();
    }

    /// Every other outcome is a counted refusal at its stage: another
    /// destination, a foreign launch, no coherent snapshot, no lane left.
    #[test]
    fn refusals_are_counted_at_their_stage() {
        let (mut exec, d) = armed("refuse", 1);
        let portal = bsc_mainnet_v5_27_0().portal.into_array();
        let now = || 1u64;
        let launch = witnessed_launch();
        assert_eq!(
            exec.on_launch([1; 32], Some([7; 20]), &launch, 0, snapshot, now),
            Err(Refusal::NotDecoded)
        );
        let foreign = hex::decode(
            include_str!("../../trading/fixtures/flap/launch_v6_native_split_tax_pending.hex")
                .trim(),
        )
        .unwrap();
        assert!(matches!(
            exec.on_launch([2; 32], Some(portal), &foreign, 0, snapshot, now),
            Err(Refusal::Profile(_))
        ));
        assert_eq!(
            exec.on_launch([3; 32], Some(portal), &launch, 0, |_, _, _| None, now),
            Err(Refusal::NoSnapshot)
        );
        exec.on_launch([4; 32], Some(portal), &launch, 0, snapshot, now)
            .unwrap();
        assert_eq!(
            exec.on_launch([5; 32], Some(portal), &launch, 0, snapshot, now),
            Err(Refusal::LanesExhausted)
        );
        let c = exec.counters();
        assert_eq!((c.launches, c.handoffs), (5, 1));
        for stage in ["NOT_DECODED", "PROFILE", "NO_SNAPSHOT", "LANES_EXHAUSTED"] {
            assert_eq!(c.refused(stage), 1, "{stage}");
        }
        // The same session cannot arm its fences twice.
        let key = d.join("identity/paper.key");
        let again = PaperExecutor::arm(
            PaperIdentity::load(&key, Path::new("/nonexistent-repo"), &[]).unwrap(),
            validated(),
            WITNESSED_MAINNET_TERMS,
            config(1),
            0,
            &d.join("fences"),
            1,
        );
        assert!(matches!(again, Err(ArmError::Fence(_))));
        fs::remove_dir_all(&d).unwrap();
    }
}
