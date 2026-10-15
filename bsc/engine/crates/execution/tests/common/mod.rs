//! Shared fixtures of the runner's tests.
//!
//! The intent lane, the expectation, the signed BUY and the plans are the
//! real ones of `darkbot-bsc-trading`, built through its public API and its
//! `test-support` helpers. Only the network and the clock are fakes. The
//! tests therefore prove the runner's semantics against the real core,
//! and nothing about any provider's behaviour.

#![allow(dead_code)]

use std::cell::Cell;
use std::io;
use std::rc::Rc;

use alloy_primitives::{Address, B256, U256};
use darkbot_bsc_execution::submission::frame::{Endpoint, Frame};
use darkbot_bsc_execution::submission::send::DispatchContext;
use darkbot_bsc_execution::submission::transport::{Clock, ConnectionState, Transport};
use darkbot_bsc_execution::submission::{blockrazor, club48, local_geth};
use darkbot_bsc_trading::chain_state::SnapshotId;
use darkbot_bsc_trading::economics::flap::native_curve20::{
    QUALIFIED_MAINNET_INPUT_WEI, QualifiedNativeState, WITNESSED_MAINNET_TERMS, admit_native_buy,
};
use darkbot_bsc_trading::economics::policy::BuyPolicy;
use darkbot_bsc_trading::intent::{
    AdapterId, AdapterSet, ArmedFence, IntentLane, PREPARE_SCRATCH_CAPACITY, Ticket,
};
use darkbot_bsc_trading::signer::Signer;
use darkbot_bsc_trading::submission_policy::capability::{
    AdapterBinding, AdapterTable, CapabilityId, CapabilityRecord, SubmissionMode,
};
use darkbot_bsc_trading::submission_policy::plan::{
    BudgetEnvelope, ChainHead, Launch, LocalBlockStartDeadline, MergePolicy, PlanInputs,
    PreSubmission, ProviderValidity, SubmissionPlan, TransportStartDeadline, TxRef,
};
use darkbot_bsc_trading::submission_policy::release::ReleaseProfile;
use darkbot_bsc_trading::transactions::legacy::LegacyTx;
use darkbot_bsc_trading::venues::flap::abi::LaunchParamsV6;
use darkbot_bsc_trading::venues::flap::binding::ExpectedBuy;
use darkbot_bsc_trading::venues::flap::deployment::{
    ObservedDeployment, ValidatedDeployment, bsc_mainnet_v5_24_0,
};
use darkbot_bsc_trading::venues::flap::profile::classify_launch;

pub const CHAIN: u64 = 56;
pub const FIRST_NONCE: u64 = 7;
pub const CODE_GENERATION: u64 = 7;
pub const POLICY_GENERATION: u64 = 3;
pub const CAPABILITY_GENERATION: u64 = 11;
pub const GAS_PRICE: u64 = 310_000_000;
pub const GAS_LIMIT: u64 = 1_500_000;
/// Last block the providers are asked to include in.
pub const MAX_BLOCK: u64 = 1_000;
/// Head at dispatch, and the last head at which a write may start.
pub const HEAD: u64 = 999;
pub const CLOCK: u32 = 4;
pub const LIMIT_NS: u64 = 1_000_000;
pub const FRAME_CAPACITY: usize = 8_192;
pub const RESPONSE_CAPACITY: usize = 2_048;

/// Adapter indices of the tests.
pub const CLUB48: u8 = 0;
pub const BLOCKRAZOR: u8 = 1;
pub const LOCAL: u8 = 2;

/// Endpoints of the tests. No real host is contacted.
pub const CLUB48_ENDPOINT: Endpoint<'static> = Endpoint {
    host: "club48.test",
    path: "/",
    authorization: None,
};
pub const BLOCKRAZOR_ENDPOINT: Endpoint<'static> = Endpoint {
    host: "blockrazor.test",
    path: "/",
    authorization: Some("test-token"),
};

/// Salt of the G0.6a EVM crosscheck launch.
const WITNESSED_SALT: &str = "09c4b20c7956530d4bf91b3569ed173abfce47d16d412d94dde5e6f4034c6eeb";

pub fn unhex(text: &str) -> Vec<u8> {
    let digits = text.strip_prefix("0x").unwrap_or(text).as_bytes();
    digits
        .chunks(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).unwrap();
            let low = (pair[1] as char).to_digit(16).unwrap();
            u8::try_from(high * 16 + low).unwrap()
        })
        .collect()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn adapter(index: u8) -> AdapterId {
    AdapterId::new(index).unwrap()
}

pub fn set(indices: &[u8]) -> AdapterSet {
    let ids: Vec<AdapterId> = indices.iter().map(|index| adapter(*index)).collect();
    AdapterSet::new(&ids).unwrap()
}

/// The buyer: signer of the BUY.
pub fn buyer() -> Signer {
    Signer::from_secret_bytes(&[0x51u8; 32]).unwrap()
}

/// The witnessed mainnet deployment, validated through the production
/// path: the observation matches the frozen constants.
pub fn deployment() -> ValidatedDeployment {
    let frozen = bsc_mainnet_v5_24_0();
    let observed = ObservedDeployment {
        chain_id: frozen.chain_id,
        portal_version: frozen.portal_version,
        portal_runtime_sha256: frozen.portal_runtime_sha256,
        portal_implementation: frozen.portal_implementation,
        portal_implementation_runtime_sha256: frozen.portal_implementation_runtime_sha256,
        tax_v3_implementation_runtime_sha256: frozen.tax_v3_implementation_runtime_sha256,
    };
    frozen.validate(&observed, CODE_GENERATION).unwrap()
}

/// Launch parameters of the G0.6a EVM crosscheck.
pub fn witnessed_launch(salt: &[u8]) -> LaunchParamsV6<'static> {
    let mut launcher = [0u8; 20];
    launcher[18] = 0xa1;
    launcher[19] = 0x1c;
    LaunchParamsV6 {
        name: b"DarkBot G06A Economics",
        symbol: b"DBG06E",
        uri: b"ipfs://darkbot-g06a-economics",
        dex_thresh: 1,
        salt: B256::from_slice(salt),
        migrator_type: 1,
        quote_token: Address::ZERO,
        quote_amt: U256::ZERO,
        beneficiary: Address::from_slice(&launcher),
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
    }
}

/// Stand-in for the original signed launch of a third party. The runner
/// never decodes it; it sends exactly these bytes.
pub fn foreign_launch_raw() -> Vec<u8> {
    let mut raw = vec![0x02u8];
    raw.extend_from_slice(&[0x1au8; 1_123]);
    raw
}

pub fn snapshot(candidate: B256) -> SnapshotId {
    SnapshotId {
        canonical_block_hash: B256::from_slice(&[0xb1u8; 32]),
        ordered_state_cursor: 41,
        pending_candidate: candidate,
        policy_generation: POLICY_GENERATION,
        codehash_generation: CODE_GENERATION,
    }
}

pub fn head() -> ChainHead {
    ChainHead {
        number: HEAD,
        timestamp: 1_800_000_000,
    }
}

/// The admitted BUY behind `candidate` for `nonce`: expectation and
/// calldata, through the production path of the core.
fn admitted_buy(candidate: B256, nonce: u64) -> (ExpectedBuy, Vec<u8>) {
    let deployment = deployment();
    let salt = unhex(WITNESSED_SALT);
    let launch = witnessed_launch(&salt);
    let classified = classify_launch(&deployment, candidate, &launch).unwrap();
    let terms = WITNESSED_MAINNET_TERMS;
    let state =
        QualifiedNativeState::fresh_launch(&classified, &terms, 0, snapshot(candidate)).unwrap();
    let policy = BuyPolicy::new(
        POLICY_GENERATION,
        U256::from(QUALIFIED_MAINNET_INPUT_WEI),
        100,
        GAS_PRICE,
        GAS_LIMIT,
    )
    .unwrap();
    let admitted = admit_native_buy(&state, &policy).unwrap();
    let expected =
        ExpectedBuy::from_admitted(&deployment, &admitted, &policy, buyer().address(), nonce)
            .unwrap();
    (expected, admitted.calldata().unwrap().to_vec())
}

/// A lane with one prepared intent: the BUY behind a third party's launch.
pub struct World {
    pub lane: IntentLane,
    pub ticket: Ticket,
    pub expected: ExpectedBuy,
    pub snapshot: SnapshotId,
    pub raw_buy: Vec<u8>,
    pub launch_raw: Vec<u8>,
}

impl World {
    pub fn prepared() -> Self {
        Self::prepared_with_launch(foreign_launch_raw())
    }

    /// A world whose third-party launch is `launch_raw`, for requests of a
    /// chosen size.
    pub fn prepared_with_launch(launch_raw: Vec<u8>) -> Self {
        let candidate = TxRef::of(&launch_raw).hash();
        let fence = ArmedFence::for_tests(CHAIN, buyer().address(), FIRST_NONCE);
        let mut lane = IntentLane::armed(fence, 4).unwrap();
        let token = admitted_buy(candidate, FIRST_NONCE).0.token();
        let ticket = lane.reserve(token).unwrap();
        let (expected, calldata) = admitted_buy(candidate, ticket.nonce());
        let tx = LegacyTx {
            chain_id: expected.chain_id(),
            nonce: expected.nonce(),
            gas_price: expected.gas_price(),
            gas_limit: expected.gas_limit(),
            to: expected.portal(),
            value: expected.input_amount(),
            input: &calldata,
        };
        let mut scratch = Vec::with_capacity(PREPARE_SCRATCH_CAPACITY);
        let mut raw_buy = Vec::new();
        buyer()
            .sign_legacy(&tx, &mut scratch, &mut raw_buy)
            .unwrap();
        let snapshot = snapshot(candidate);
        lane.prepare(ticket, &raw_buy, &expected, &snapshot, &mut scratch)
            .unwrap();
        Self {
            lane,
            ticket,
            expected,
            snapshot,
            raw_buy,
            launch_raw,
        }
    }
}

pub fn context<'a>(table: &'a AdapterTable, current: &'a SnapshotId) -> DispatchContext<'a> {
    DispatchContext {
        table,
        current,
        head: head(),
    }
}

fn bind(index: u8, capability: CapabilityId) -> AdapterBinding {
    AdapterBinding {
        adapter: adapter(index),
        capability,
    }
}

/// The frozen records: 48Club at 0, BlockRazor builder at 1, local node
/// at 2.
pub fn frozen_table() -> AdapterTable {
    AdapterTable::new(
        CHAIN,
        CAPABILITY_GENERATION,
        &[
            bind(CLUB48, CapabilityId::Club48BuilderV1),
            bind(BLOCKRAZOR, CapabilityId::BlockRazorBuilderV1),
            bind(LOCAL, CapabilityId::LocalGethV1),
        ],
    )
    .unwrap()
}

/// A table of synthetic records, from the core's test kit.
pub fn table_of(records: &[(u8, CapabilityRecord)]) -> AdapterTable {
    let records: Vec<(AdapterId, CapabilityRecord)> = records
        .iter()
        .map(|(index, record)| (adapter(*index), *record))
        .collect();
    AdapterTable::for_tests(CHAIN, CAPABILITY_GENERATION, &records).unwrap()
}

pub fn budget() -> BudgetEnvelope {
    BudgetEnvelope {
        max_buy_input_wei: U256::MAX,
        max_buy_gas_wei: U256::MAX,
        max_own_launch_wei: U256::ZERO,
        max_provider_payment_wei: U256::ZERO,
        max_total_wei: U256::MAX,
    }
}

/// The third party's launch and the BUY, through `adapters`.
pub fn bundle_inputs(world: &World, adapters: &[u8], merge: MergePolicy) -> PlanInputs {
    PlanInputs {
        ticket: world.ticket,
        buy: TxRef::of(&world.raw_buy),
        launch: Launch::PublicOriginal(TxRef::of(&world.launch_raw)),
        mode: SubmissionMode::PublicFullLaunchBundle,
        profile: ReleaseProfile::BirthEntryV1,
        adapters: set(adapters),
        local_block_deadline: LocalBlockStartDeadline {
            last_head_block: HEAD,
        },
        provider_validity: Some(ProviderValidity {
            max_block_number: MAX_BLOCK,
            min_timestamp: None,
            max_timestamp: None,
        }),
        merge,
        pre_submission: PreSubmission::No,
        provider_payment_wei: U256::ZERO,
    }
}

/// The BUY alone, through the local node.
pub fn single_inputs(world: &World) -> PlanInputs {
    PlanInputs {
        launch: Launch::None,
        mode: SubmissionMode::SingleRawBuy,
        adapters: set(&[LOCAL]),
        provider_validity: None,
        merge: MergePolicy::ProviderDefault,
        ..bundle_inputs(world, &[LOCAL], MergePolicy::ProviderDefault)
    }
}

pub fn plan(world: &World, table: &AdapterTable, inputs: PlanInputs) -> SubmissionPlan {
    SubmissionPlan::new(table, &world.expected, &budget(), inputs).unwrap()
}

/// The public bundle through both builders, as the birth-entry profile
/// wants it.
pub fn both_builders(world: &World, table: &AdapterTable) -> SubmissionPlan {
    plan(
        world,
        table,
        bundle_inputs(world, &[CLUB48, BLOCKRAZOR], MergePolicy::NoMerge),
    )
}

pub fn deadline() -> TransportStartDeadline {
    TransportStartDeadline::for_profile(ReleaseProfile::BirthEntryV1, CLOCK, 0, LIMIT_NS).unwrap()
}

pub fn club48_frame(world: &World, plan: &SubmissionPlan, table: &AdapterTable) -> Frame {
    club48::frame(
        plan,
        table,
        adapter(CLUB48),
        &CLUB48_ENDPOINT,
        &world.launch_raw,
        FRAME_CAPACITY,
    )
    .unwrap()
}

pub fn blockrazor_frame(world: &World, plan: &SubmissionPlan, table: &AdapterTable) -> Frame {
    blockrazor::frame(
        plan,
        table,
        adapter(BLOCKRAZOR),
        &BLOCKRAZOR_ENDPOINT,
        &world.launch_raw,
        FRAME_CAPACITY,
    )
    .unwrap()
}

pub fn local_frame(plan: &SubmissionPlan, table: &AdapterTable) -> Frame {
    local_geth::frame(plan, table, adapter(LOCAL), FRAME_CAPACITY).unwrap()
}

/// A connection that records what is written to it and answers from a
/// script. It never connects: there is nothing to connect.
pub struct FakeTransport {
    pub state: ConnectionState,
    pub written: Vec<u8>,
    pub write_calls: usize,
    pub flush_calls: usize,
    pub read_calls: usize,
    /// Total bytes accepted before every further write fails; `None`
    /// accepts everything.
    pub accept_limit: Option<usize>,
    pub fail_flush: bool,
    pub response: Vec<u8>,
    pub response_pos: usize,
    /// End of stream once the scripted response is consumed.
    pub close_after_response: bool,
    pub marks: Vec<ConnectionState>,
    /// The dispatch's clock, if the test wants to know how many reads of
    /// it happened before the first write.
    pub clock: Option<Rc<ScriptClock>>,
    pub clock_reads_at_first_write: Option<usize>,
}

impl FakeTransport {
    pub fn with_state(state: ConnectionState) -> Self {
        Self {
            state,
            written: Vec::with_capacity(4 * FRAME_CAPACITY),
            write_calls: 0,
            flush_calls: 0,
            read_calls: 0,
            accept_limit: None,
            fail_flush: false,
            response: Vec::new(),
            response_pos: 0,
            close_after_response: false,
            marks: Vec::with_capacity(8),
            clock: None,
            clock_reads_at_first_write: None,
        }
    }

    pub fn ready() -> Self {
        Self::with_state(ConnectionState::Ready)
    }

    pub fn untouched(&self) -> bool {
        self.write_calls == 0 && self.flush_calls == 0 && self.read_calls == 0
    }
}

impl Transport for FakeTransport {
    fn state(&self) -> ConnectionState {
        self.state
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_calls += 1;
        if self.write_calls == 1 {
            self.clock_reads_at_first_write = self.clock.as_ref().map(|clock| clock.reads());
        }
        let room = match self.accept_limit {
            None => buf.len(),
            Some(limit) => limit.saturating_sub(self.written.len()).min(buf.len()),
        };
        if room == 0 {
            return Err(io::Error::from(io::ErrorKind::BrokenPipe));
        }
        self.written.extend_from_slice(&buf[..room]);
        Ok(room)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_calls += 1;
        if self.fail_flush {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        } else {
            Ok(())
        }
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.read_calls += 1;
        let available = &self.response[self.response_pos..];
        if available.is_empty() {
            if self.close_after_response && !self.response.is_empty() {
                return Ok(0);
            }
            return Err(io::Error::from(io::ErrorKind::WouldBlock));
        }
        let count = available.len().min(buf.len());
        buf[..count].copy_from_slice(&available[..count]);
        self.response_pos += count;
        Ok(count)
    }

    fn mark(&mut self, state: ConnectionState) {
        self.state = state;
        self.marks.push(state);
    }
}

/// A clock that returns scripted times, one per read, and then repeats the
/// last one.
pub struct ScriptClock {
    id: u32,
    times: Vec<u64>,
    next: Cell<usize>,
}

impl ScriptClock {
    pub fn new(id: u32, times: &[u64]) -> Self {
        assert!(!times.is_empty());
        Self {
            id,
            times: times.to_vec(),
            next: Cell::new(0),
        }
    }

    pub fn reads(&self) -> usize {
        self.next.get()
    }
}

impl Clock for ScriptClock {
    fn id(&self) -> u32 {
        self.id
    }

    fn now_ns(&self) -> u64 {
        let index = self.next.get();
        self.next.set(index + 1);
        self.times[index.min(self.times.len() - 1)]
    }
}

/// An HTTP/1.1 response with a JSON body framed by `Content-Length`.
pub fn http_json(status_line: &str, body: &str) -> Vec<u8> {
    format!(
        "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// The JSON-RPC id of the world's requests: the BUY's hash.
pub fn request_id(world: &World) -> String {
    format!("0x{}", hex(TxRef::of(&world.raw_buy).hash().as_slice()))
}

/// A result in the form both builders document: a bundle hash.
pub const BUNDLE_HASH: &str = "0xa06ba06ba06ba06ba06ba06ba06ba06ba06ba06ba06ba06ba06ba06ba06ba06b";

pub fn ack_body(id: &str) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"{BUNDLE_HASH}"}}"#)
}

pub fn reject_body(id: &str) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":"{id}","error":{{"code":-38000,"message":"refused"}}}}"#)
}

/// Clock reads of a two-adapter dispatch in which both are sent: select,
/// then per adapter pre-permit check, write check and boundary.
pub fn quick_clock() -> ScriptClock {
    ScriptClock::new(CLOCK, &[100, 150, 200, 300, 350, 400, 500])
}

/// The body of a written request.
pub fn request_body(request: &[u8]) -> &[u8] {
    let end = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap();
    &request[end + 4..]
}

/// The two transactions of a written `eth_sendBundle` request, in order.
pub fn bundle_txs(request: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let body = std::str::from_utf8(request_body(request)).unwrap();
    let start = body.find("\"txs\":[\"0x").unwrap() + "\"txs\":[\"0x".len();
    let end = start + body[start..].find("\"]").unwrap();
    let (first, second) = body[start..end].split_once("\",\"0x").unwrap();
    (unhex(first), unhex(second))
}
