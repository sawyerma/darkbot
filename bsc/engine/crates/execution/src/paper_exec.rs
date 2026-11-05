//! The C3 production path in PAPER mode (order V2 §7.4, PHASENPLAN §11/§16):
//! a pending launch goes through the production venue decoder, the C2
//! chain-state snapshot, the venue economics and admission, the production
//! BUY build and signer, the Unit-4 intent lane, the production submission
//! plan of the selected adapter (the 48Club builder bundle of the original
//! launch and the BUY, BSC-008 = A), its request frame and the production
//! dispatch; it ends at the PAPER sink (`T_shadow_sink`, never `T_send`).
//!
//! Every stage is the production code; only the end differs: the adapter's
//! transport is the PAPER sink, which takes the complete request and has
//! no socket, stream or client (`paper`), and the signer is the PAPER
//! identity. The dispatch is the diagnostic one: a PAPER request is never a
//! release.
//!
//! Lanes: a PAPER transaction is never mined, so no inclusion proof will
//! ever resolve its intent, and a Unit-4 lane blocks behind an unresolved
//! head. The executor therefore arms a bounded pool of single-intent lanes
//! when it starts, behind one durable fence (created, written and synced at
//! arming, never on the entry path), and every PAPER BUY consumes one lane.
//! What production enforces across one session is enforced across the
//! pool: one session exposure cap for all of it, and one BUY per token.
//! Each PAPER BUY is the next BUY from the same canonical state, so every
//! lane starts at the same canonical nonce.
//!
//! Phase I: Strategy C's decisions take the same path. A PASS is a BUY of
//! the policy's input on the exact curve the strategy replayed
//! (`CurveState`, the C2 canonical snapshot), an exit trigger the approval
//! of the Portal and the native-output SELL of the position's tokens on its
//! shadow curve; each is admitted, signed by the PAPER identity, bound in a
//! lane and planned as the single raw transaction of the local node's
//! adapter (`SingleRawBuy`), whose transport is the sink. Their lanes are
//! their own pool behind the same fence: BUYs and approvals at the next
//! nonce, SELLs at the one after.
//!
//! Plan 28.10. (package 3): the executor is generic over its [`End`], the
//! one place PAPER and LIVE differ. Every stage up to the lane's final
//! binding check is this executor's; the end holds the sender's signer and
//! plans, frames and dispatches the prepared transaction. [`PaperEnd`] is
//! the PAPER identity and the sink over the diagnostic dispatch, as
//! before; the LIVE end (the operator's key, the release dispatch over
//! qualified transports) lives outside this module, which has no transport
//! but the sink.

use crate::paper::{PaperHandoff, PaperIdentity, PaperSink};
use crate::submission::adapter_frame;
use crate::submission::send::{
    Armed, DispatchContext, DispatchError, FanoutReport, SendOutcome, dispatch_diagnostic,
};
use crate::submission::tls::{CLUB48_DOCUMENTED_HOST, HTTPS_PORT, ProviderTarget, TlsEndpoint};
use crate::submission::transport::{Clock, ConnectionState, Transport};
use alloy_primitives::{Address, B256, U256};
use darkbot_bsc_trading::chain_state::SnapshotId;
use darkbot_bsc_trading::chain_state::canonical::{FinalityProof, InclusionProof, OrphanProof};
use darkbot_bsc_trading::economics::flap::AdmittedBuy;
use darkbot_bsc_trading::economics::flap::native_curve20::{
    CurveState, QUALIFIED_MAINNET_INPUT_WEI, QualifiedNativeState, VenueBuyTerms, admit_curve_buy,
    admit_curve_sell, admit_native_buy,
};
use darkbot_bsc_trading::economics::four_classic::{
    AdmittedClassicBuy, AdmittedClassicSell, QUALIFIED_CLASSIC_INPUTS_WEI, QualifiedClassicHolding,
    QualifiedClassicState, admit_classic_buy, admit_classic_sell, floor_gwei,
};
use darkbot_bsc_trading::economics::openfour::{
    AdmittedOpenFourBuy, AdmittedOpenFourSell, QUALIFIED_OPENFOUR_INPUTS_WEI,
    QualifiedOpenFourHolding, QualifiedOpenFourState, admit_openfour_buy, admit_openfour_sell,
};
use darkbot_bsc_trading::economics::policy::{BuyPolicy, ExitPolicy};
use darkbot_bsc_trading::intent::{
    AdapterId, AdapterSet, ArmedFence, IntentLane, IntentState, Ticket,
};
use darkbot_bsc_trading::signer::Signer;
use darkbot_bsc_trading::strategy::book::FourCurve;
use darkbot_bsc_trading::submission_policy::capability::{
    AdapterBinding, AdapterKind, AdapterTable, CapabilityId, SubmissionMode,
};
use darkbot_bsc_trading::submission_policy::plan::{
    BudgetEnvelope, ChainHead, Launch, LocalBlockStartDeadline, MergePolicy, PlanInputs,
    PreSubmission, ProviderValidity, SubmissionPlan, TransportStartDeadline, TxRef,
};
use darkbot_bsc_trading::submission_policy::release::ReleaseProfile;
use darkbot_bsc_trading::venues::flap::binding::{
    ExpectedBurn, ExpectedBuy, ExpectedExit, ExpectedTx, TRANSFER_GAS,
};
use darkbot_bsc_trading::venues::flap::deployment::ValidatedDeployment;
use darkbot_bsc_trading::venues::flap::profile::{UnsupportedLaunch, classify_launch};
use darkbot_bsc_trading::venues::flap::recognize::{Recognized, recognize};
use darkbot_bsc_trading::venues::fourmeme::binding::{
    ExpectedClassicBuy, ExpectedClassicExit, ExpectedOpenFourBuy, ExpectedOpenFourExit,
};
use darkbot_bsc_trading::venues::fourmeme::deployment::ValidatedFourDeployment;
use darkbot_bsc_trading::venues::fourmeme::profile::{
    UnsupportedClassicLaunch, UnsupportedOpenFourLaunch, classify_classic_launch,
    classify_openfour_launch,
};
use darkbot_bsc_trading::venues::fourmeme::recognize::{
    Recognized as FourRecognized, recognize as four_recognize,
};
use std::collections::VecDeque;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// Audit E 4.2: the profiles a hand-over is planned under
/// ([`Submission::profile`]); the executor's block window and transport
/// start limit must fit each.
pub const EXEC_PROFILES: [ReleaseProfile; 2] =
    [ReleaseProfile::BirthEntryV1, ReleaseProfile::SingleRawV1];

/// The widest `BSC_EXEC_VALIDITY_BLOCKS` every profile takes.
pub fn max_validity_blocks() -> u64 {
    EXEC_PROFILES
        .iter()
        .map(|p| p.max_block_window())
        .min()
        .unwrap_or(0)
}

/// The widest `BSC_EXEC_START_LIMIT_NS` every profile takes.
pub fn max_start_limit_ns() -> u64 {
    EXEC_PROFILES
        .iter()
        .map(|p| p.max_transport_start_limit_ns())
        .min()
        .unwrap_or(0)
}

/// Which PAPER paths take the PAPER sender's nonce sequence in one arm
/// (`BSC_EXEC_SENDER_PATHS`, Phase I gate M2). C3's BUYs are alternatives
/// at the next nonce n; Strategy C's intents are a sequence from n. With
/// both, the first to disclose n owns it: after a C3 BUY the strategy
/// starts at n+1; after a strategy intent every later C3 BUY of the arm is
/// refused NONCE_TAKEN. A path left out is refused PATH_OFF.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SenderPaths {
    C3,
    StrategyC,
    Both,
}

impl SenderPaths {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "C3" => Ok(Self::C3),
            "STRATEGY_C" => Ok(Self::StrategyC),
            "C3+STRATEGY_C" => Ok(Self::Both),
            other => Err(format!(
                "BSC_EXEC_SENDER_PATHS {other:?}: C3, STRATEGY_C or C3+STRATEGY_C"
            )),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::C3 => "C3",
            Self::StrategyC => "STRATEGY_C",
            Self::Both => "C3+STRATEGY_C",
        }
    }

    fn c3(self) -> bool {
        self != Self::StrategyC
    }

    /// Whether Strategy C's intents (its BUYs and every exit) go this way.
    pub fn strategy(self) -> bool {
        self != Self::C3
    }
}

/// The operating values of the PAPER path (`BSC_EXEC_*`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecConfig {
    /// R12c: the Four Classic BUY of the C3 path (`BSC_EXEC_FOUR_*`); none:
    /// no Four arm, Four launches are not decoded by the path.
    pub four: Option<FourExecConfig>,
    /// R12d: the OpenFour BUY of the C3 path (`BSC_EXEC_OPENFOUR_*`) on the
    /// executed bonding preset; none: no OpenFour arm.
    pub openfour: Option<FourExecConfig>,
    pub input_wei: u128,
    pub slippage_bps: u16,
    /// An exit's (the SELL's) slippage, its own limit (BSC3-069).
    pub exit_slippage_bps: u16,
    pub gas_price_wei: u64,
    pub gas_limit: u64,
    /// Cumulative exposure cap of the whole PAPER session (Unit 4, E5-1),
    /// shared by every lane of the pool.
    pub session_cap_wei: u128,
    /// Single-intent lanes armed per start.
    pub lanes: usize,
    /// Largest calldata the decoder accepts.
    pub max_calldata: usize,
    /// Latency samples kept for the status quantiles.
    pub latency_samples: usize,
    pub policy_generation: u64,
    /// Generation of the adapter capability table.
    pub capability_generation: u64,
    /// Blocks after the head the provider may include the bundle in
    /// (`maxBlockNumber` = head + this), within the profile's window.
    pub validity_blocks: u64,
    /// The transport start deadline after `T_ingress`, within the profile's
    /// limit.
    pub start_limit_ns: u64,
    /// Largest adapter request, in bytes.
    pub max_frame: usize,
    /// Phase I: single-intent lanes for Strategy C's PAPER intents, one per
    /// nonce of the sender's sequence (BUY n, approval n+1, SELL n+2).
    pub strategy_lanes: usize,
    /// The exposure cap of Strategy C's PAPER intents in one session (no
    /// money is involved; the strategy's own limits bound its entries).
    pub strategy_session_cap_wei: u128,
    /// Gas limits of an exit's approval and SELL.
    pub approve_gas_limit: u64,
    pub sell_gas_limit: u64,
    /// Phase I gate L-a: the oldest canonical head (its timestamp against
    /// now) a strategy intent is formed on; a follower catching up does
    /// not act on old blocks as if they were fresh.
    pub strategy_max_head_age_ms: u64,
    /// BSC3-036: the oldest canonical head (its timestamp against the wall
    /// clock) a LIVE release plans on, BUY or exit: older, the canonical
    /// source is stalled (or cut off) and nothing leaves.
    pub live_max_head_age_ms: u64,
    /// BSC3-030: reverted exit steps of one position before its exits
    /// stop for the operator (`BSC_EXEC_LIVE_EXIT_REVERTS_MAX`).
    pub live_exit_reverts_max: u64,
    /// BSC3-076: the blocks a C3 BUY's lease of the owner holds
    /// (`BSC_EXEC_LIVE_LEASE_BLOCKS`), renewed before half are gone.
    pub live_lease_blocks: u64,
    /// REV.1.0 R3: an operator's EXIT_REQUESTED of a lot wants its exit
    /// now (`BSC_EXEC_LIVE_OPERATOR_EXIT`, OFF by default: held back).
    pub live_operator_exit: bool,
    /// REV.1.0 R3b: an operator's BURN_REQUESTED of an unresolved
    /// transaction burns its nonce (`BSC_EXEC_LIVE_NONCE_BURN`, OFF by
    /// default: held back, the CLI refuses it).
    pub live_nonce_burn: bool,
    /// REV.1.0 R4 (PHASENPLAN §20): a public WSS source may be a C3 BUY's
    /// only LIVE signal (`BSC_EXEC_LIVE_PUBLIC_ONLY`, OFF by default: a C3
    /// BUY needs a LIVE source that is not public).
    pub live_public_only: bool,
    /// REV.1.0 R4 (BSC3-059): the longest a grant may still run for a BUY
    /// to go (`BSC_EXEC_LIVE_GRANT_MAX_DURATION_MS`; 0: no bound); its
    /// exits go on.
    pub live_grant_max_duration_ms: u64,
    /// REV.1.0 R8 (BSC3-049): the oldest a LIVE qualification may be for a
    /// BUY to go (`BSC_EXEC_LIVE_QUALIFICATION_MAX_AGE_MS`; 0: no bound);
    /// its exits go on.
    pub live_qualification_max_age_ms: u64,
    /// REV.1.0 R9 (BSC3-001): the operator's presence key
    /// (`BSC_EXEC_LIVE_PRESENCE_ANCHOR`; none, the default: OFF): a BUY
    /// needs the grant signed by it; its exits go on.
    pub live_presence_anchor: Option<[u8; 20]>,
    /// REV.1.0 R4: the campaign's cumulative worst-case budget every LIVE
    /// BUY with its exits is admitted within, all grants together
    /// (`BSC_EXEC_LIVE_SPEND_TOTAL_WEI`; 0: no BUY).
    pub live_spend_total_wei: u128,
    /// REV.1.0 R4 (BSC3-069): the soft bounds of a grant's risk (its BUY's
    /// and its exits' slippage, its gas price); a grant above them is an
    /// outlier: it admits BUYs only confirmed as one and with
    /// `BSC_EXEC_LIVE_RISK_OUTLIER` ON (exits go on).
    pub live_risk_soft_buy_slippage_bps: u16,
    pub live_risk_soft_exit_slippage_bps: u16,
    pub live_risk_soft_gas_price_wei: u64,
    pub live_risk_outlier: bool,
    /// REV.1.0 R4 (BSC3-042): a C3 BUY whose nonce the chain took renews
    /// the arm for the next C3 BUY (`BSC_EXEC_LIVE_C3_REARM`, OFF by
    /// default: one C3 BUY per arm).
    pub live_c3_rearm: bool,
    /// REV.1.0 R4: the bound builder policy (repository-relative) a LIVE
    /// arm rests on: fresh, its builders verified, the signed gas price at
    /// its floor or above (`BSC_EXEC_LIVE_BUILDER_POLICY`).
    pub live_builder_policy: String,
    /// REV.1.0 R5 (BSC3-030): an exit step released at a nonce whose window
    /// is over unresolved goes again at its price raised by these bps, at
    /// most the grant's risk maximum (`BSC_EXEC_LIVE_EXIT_REPRICE_BPS`; 0,
    /// the default: never re-priced, the same bytes are refused).
    pub live_exit_reprice_bps: u16,
    /// REV.1.0 R5 (BSC3-030): a LIVE exit step, once handed to the
    /// builders, also goes to the public JSON-RPC endpoint
    /// `live_public_exit_rpc` (`BSC_EXEC_LIVE_PUBLIC_EXIT`, OFF by default;
    /// `BSC_EXEC_LIVE_PUBLIC_EXIT_RPC`, an `https://` URL or empty).
    pub live_public_exit: bool,
    pub live_public_exit_rpc: String,
    /// REV.1.0 R5 (BSC3-030): a LIVE lot whose token migrated to its DEX
    /// exits at the Portal's quote, routed through its DEX router
    /// (`BSC_EXEC_LIVE_DEX_EXIT`, OFF by default: it stays stranded for the
    /// operator).
    pub live_dex_exit: bool,
    /// REV.1.0 R7 (BSC3-056): while the second source holds another block
    /// at a height of the followed chain (DISAGREES) nothing LIVE goes,
    /// exits neither (`BSC_EXEC_LIVE_DIVERGENCE_QUARANTINE`, OFF by default:
    /// only BUYs wait, as before; the operator's decision).
    pub live_divergence_quarantine: bool,
    /// Phase I gate M2: the PAPER paths that take the sender.
    pub sender_paths: SenderPaths,
}

impl ExecConfig {
    /// SC4 d: a BUY's and an exit's limits on `venue`: input, BUY gas,
    /// approval gas, SELL gas (`BSC_EXEC_*`, a Four.meme family's own; a
    /// family not configured is never armed, its lots count at Flap's).
    pub fn on_venue(&self, venue: crate::sources::VenueId) -> (u128, u64, u64, u64) {
        use crate::sources::VenueId;
        let four = match venue {
            VenueId::Flap => None,
            VenueId::FourClassic => self.four,
            VenueId::OpenFour => self.openfour,
        };
        four.map_or(
            (
                self.input_wei,
                self.gas_limit,
                self.approve_gas_limit,
                self.sell_gas_limit,
            ),
            |f| {
                (
                    f.input_wei,
                    f.gas_limit,
                    f.approve_gas_limit,
                    f.sell_gas_limit,
                )
            },
        )
    }

    /// The cross-key rules: an armed path must be able to admit a BUY.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(four) = &self.four {
            four.validate(FourVenue::Classic, self.gas_price_wei, self.session_cap_wei)?;
        }
        if let Some(openfour) = &self.openfour {
            openfour.validate(
                FourVenue::OpenFour,
                self.gas_price_wei,
                self.session_cap_wei,
            )?;
        }
        if self.input_wei != u128::from(QUALIFIED_MAINNET_INPUT_WEI) {
            return Err(format!(
                "BSC_EXEC_INPUT_WEI {} is not the witnessed input {QUALIFIED_MAINNET_INPUT_WEI}",
                self.input_wei
            ));
        }
        cap_holds_one(
            self.session_cap_wei,
            self.input_wei,
            self.gas_price_wei,
            self.gas_limit,
            ("BSC_EXEC_INPUT_WEI", "BUY"),
        )?;
        if self.lanes == 0 {
            return Err("BSC_EXEC_LANES must be at least 1".into());
        }
        if !(1..=max_validity_blocks()).contains(&self.validity_blocks) {
            return Err(format!(
                "BSC_EXEC_VALIDITY_BLOCKS {} outside 1..={}",
                self.validity_blocks,
                max_validity_blocks()
            ));
        }
        if !(1..=max_start_limit_ns()).contains(&self.start_limit_ns) {
            return Err(format!(
                "BSC_EXEC_START_LIMIT_NS {} outside 1..={}",
                self.start_limit_ns,
                max_start_limit_ns()
            ));
        }
        if self.capability_generation == 0 || self.policy_generation == 0 {
            return Err("BSC_EXEC generations start at 1".into());
        }
        if self.strategy_lanes == 0 {
            return Err("BSC_EXEC_STRATEGY_LANES must be at least 1".into());
        }
        if self.approve_gas_limit == 0 || self.sell_gas_limit == 0 {
            return Err(
                "BSC_EXEC_APPROVE_GAS_LIMIT and BSC_EXEC_SELL_GAS_LIMIT must be positive".into(),
            );
        }
        if self.strategy_max_head_age_ms == 0 {
            return Err("BSC_EXEC_STRATEGY_MAX_HEAD_AGE_MS must be positive".into());
        }
        if self.live_max_head_age_ms == 0 {
            return Err("BSC_EXEC_LIVE_MAX_HEAD_AGE_MS must be positive".into());
        }
        if self.live_exit_reverts_max == 0 {
            return Err("BSC_EXEC_LIVE_EXIT_REVERTS_MAX must be at least 1".into());
        }
        // A fresh lease covers a BUY's window with half its blocks left.
        if self.live_lease_blocks < self.validity_blocks.saturating_mul(2) {
            return Err(format!(
                "BSC_EXEC_LIVE_LEASE_BLOCKS {} is below twice BSC_EXEC_VALIDITY_BLOCKS {}",
                self.live_lease_blocks, self.validity_blocks
            ));
        }
        Ok(())
    }

    /// One Flap BUY's budget.
    fn budget(&self) -> BudgetEnvelope {
        one_buy_budget(self.input_wei, self.gas_price_wei, self.gas_limit)
    }
}

/// One BUY's budget: the admitted input and the signed gas, nothing for a
/// launch (the original is a third party's) or a provider.
fn one_buy_budget(input_wei: u128, gas_price_wei: u64, gas_limit: u64) -> BudgetEnvelope {
    let gas = U256::from(gas_price_wei) * U256::from(gas_limit);
    let input = U256::from(input_wei);
    BudgetEnvelope {
        max_buy_input_wei: input,
        max_buy_gas_wei: gas,
        max_own_launch_wei: U256::ZERO,
        max_provider_payment_wei: U256::ZERO,
        max_total_wei: input + gas,
    }
}

/// The session cap holds one BUY of `input_wei` and its gas (`names`: the
/// input's key and what the BUY is called).
fn cap_holds_one(
    session_cap_wei: u128,
    input_wei: u128,
    gas_price_wei: u64,
    gas_limit: u64,
    (input_key, what): (&str, &str),
) -> Result<(), String> {
    let one = input_wei
        .checked_add(u128::from(gas_price_wei) * u128::from(gas_limit))
        .ok_or_else(|| format!("{input_key} + gas overflows"))?;
    if session_cap_wei < one {
        return Err(format!(
            "BSC_EXEC_SESSION_CAP_WEI {session_cap_wei} is below one {what} ({one})"
        ));
    }
    Ok(())
}

/// The one adapter slot of a PAPER lane: the selected builder adapter,
/// whose transport is the sink.
pub fn paper_adapter() -> AdapterId {
    // Slot 0 exists in every lane (`MAX_ADAPTERS` > 0).
    AdapterId::new(0).unwrap_or_else(|| unreachable!("adapter slot 0"))
}

/// Where a launch left the path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The decoder did not decode a `newTokenV6` launch (or the signed
    /// bytes were not a transaction).
    NotDecoded,
    /// The launch is not the qualified profile.
    Profile(UnsupportedLaunch),
    /// R12c: a Four Classic launch outside the priced profile (counted as
    /// `PROFILE`).
    FourProfile(UnsupportedClassicLaunch),
    /// R12d: an OpenFour launch outside the executed bonding preset's
    /// profile (counted as `PROFILE`).
    OpenFourProfile(UnsupportedOpenFourLaunch),
    /// C2 holds no coherent snapshot for this candidate, or no head.
    NoSnapshot,
    /// The fresh-launch state or the admission refused.
    Admission,
    /// This session already bought the token.
    DuplicateToken,
    /// REV.1.0 R6: another launch transaction predicts a token this arm
    /// bought on its own launch (a CONFLICT: the chain decides which one
    /// lands; nothing more goes on the token).
    TokenConflict,
    /// The BUY would exceed the session's exposure cap.
    SessionCap,
    /// The expectation could not be formed.
    Intent,
    /// Every armed lane is used.
    LanesExhausted,
    /// The lane refused (reserve, prepare).
    Lane,
    /// The build or signature failed.
    Build,
    /// The submission plan or its request frame was refused.
    Plan,
    /// The transport start deadline after `T_ingress` passed: before the
    /// economic admission (only the lane and the token are used) or at the
    /// pre-permit check after it (the exposure counts as well).
    Deadline,
    /// The dispatch did not hand the request to the sink.
    Dispatch,
    /// Phase I gate M1: an exit of a token without a handed-off PAPER BUY
    /// in this session.
    NotHeld,
    /// Phase I gate M3: a strategy nonce was reserved and not handed off;
    /// no later PAPER transaction of the sender could pass that hole.
    NonceGap,
    /// Phase I gate L-a: the canonical head is older than
    /// `strategy_max_head_age_ms`.
    StaleHead,
    /// Phase I gate L-d: a C3 BUY at the next nonce while a strategy
    /// intent holds it.
    NonceTaken,
    /// Phase I gate M2: the path does not take the sender in this arm
    /// (`BSC_EXEC_SENDER_PATHS`).
    PathOff,
    /// SC4: a Strategy C intent on a Four.meme curve whose venue has no
    /// strategy execution armed.
    VenueUnarmed,
    /// Plan 28.10.: the plan or a transport is not qualified for a release.
    Unqualified,
    /// Plan 28.10.: the native owner did not admit the LIVE intent now.
    Owner,
    /// BSC3-055: an end whose transactions reach the chain takes a nonce
    /// only when it is the sender's canonical next one (C2, clean): every
    /// lower nonce is consumed on chain. Nothing was consumed; the intent
    /// may come again once the lower nonce is.
    SenderNonce,
    /// BSC3-048: an open position's exit waits for the sender's nonce; no
    /// new BUY takes it first.
    ExitFirst,
    /// BSC3-027: the sender's balance does not hold a BUY with its exit
    /// and the exits of what is open; the exits go, no BUY.
    Unfunded,
    /// BSC3-030: the position's exit steps reverted as often as allowed,
    /// or the last one used all its gas: its exits wait for the operator.
    ExitStopped,
    /// 31.10. order: a BUY beyond the grant's positions, losses or BUY
    /// interval (exits go on).
    GrantLimit,
    /// BSC3-056: the independent second source does not confirm the
    /// followed chain now (stale, behind, another hash, no common height):
    /// no BUY on it (exits go on; R7: under the divergence quarantine not
    /// while it holds another block).
    Unconfirmed,
}

impl Refusal {
    pub fn name(self) -> &'static str {
        match self {
            Self::NotDecoded => "NOT_DECODED",
            Self::Profile(_) | Self::FourProfile(_) | Self::OpenFourProfile(_) => "PROFILE",
            Self::NoSnapshot => "NO_SNAPSHOT",
            Self::Admission => "ADMISSION",
            Self::DuplicateToken => "DUPLICATE_TOKEN",
            Self::TokenConflict => "TOKEN_CONFLICT",
            Self::SessionCap => "SESSION_CAP",
            Self::Intent => "INTENT",
            Self::LanesExhausted => "LANES_EXHAUSTED",
            Self::Lane => "LANE",
            Self::Build => "BUILD",
            Self::Plan => "PLAN",
            Self::Deadline => "DEADLINE",
            Self::Dispatch => "DISPATCH",
            Self::NotHeld => "NOT_HELD",
            Self::NonceGap => "NONCE_GAP",
            Self::StaleHead => "STALE_HEAD",
            Self::NonceTaken => "NONCE_TAKEN",
            Self::PathOff => "PATH_OFF",
            Self::VenueUnarmed => "VENUE_UNARMED",
            Self::Unqualified => "UNQUALIFIED",
            Self::Owner => "OWNER",
            Self::SenderNonce => "SENDER_NONCE",
            Self::ExitFirst => "EXIT_FIRST",
            Self::Unfunded => "UNFUNDED",
            Self::ExitStopped => "EXIT_STOPPED",
            Self::GrantLimit => "GRANT_LIMIT",
            Self::Unconfirmed => "UNCONFIRMED",
        }
    }
}

/// The Four.meme family a [`FourExecConfig`] is of: its keys and the
/// inputs its economics are witnessed at by execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FourVenue {
    Classic,
    OpenFour,
}

impl FourVenue {
    /// The configuration keys' prefix.
    pub fn keys(self) -> &'static str {
        match self {
            Self::Classic => "BSC_EXEC_FOUR",
            Self::OpenFour => "BSC_EXEC_OPENFOUR",
        }
    }

    fn what(self) -> &'static str {
        match self {
            Self::Classic => "Four BUY",
            Self::OpenFour => "OpenFour BUY",
        }
    }

    fn qualified_inputs(self) -> &'static [u64] {
        match self {
            Self::Classic => &QUALIFIED_CLASSIC_INPUTS_WEI,
            Self::OpenFour => &QUALIFIED_OPENFOUR_INPUTS_WEI,
        }
    }
}

/// R12c: a Four.meme BUY's operating values (`BSC_EXEC_FOUR_*`, R12d
/// `BSC_EXEC_OPENFOUR_*`). The gas limit is the operator's bound, held to
/// the frozen BUY receipts of its family (Classic: two, of the G0.1-era
/// TokenManager2 implementation; OpenFour: two `buyByBudget`, above the
/// executed `Core.buy` estimates of the fixture; no launch-block BUY of
/// either and none of the pinned implementations: the Four network cost is
/// UNSUPPORTED).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FourExecConfig {
    pub input_wei: u128,
    pub slippage_bps: u16,
    pub gas_limit: u64,
    /// SC4 c (B1): its exits' approval and SELL gas limits
    /// (`*_APPROVE_GAS_LIMIT`, `*_SELL_GAS_LIMIT`), held to its family's
    /// frozen APPROVE and SELL receipts.
    pub approve_gas_limit: u64,
    pub sell_gas_limit: u64,
}

impl FourExecConfig {
    fn validate(
        &self,
        venue: FourVenue,
        gas_price_wei: u64,
        session_cap_wei: u128,
    ) -> Result<(), String> {
        let (keys, qualified) = (venue.keys(), venue.qualified_inputs());
        if !qualified.iter().any(|w| u128::from(*w) == self.input_wei) {
            return Err(format!(
                "{keys}_INPUT_WEI {} is none of the witnessed inputs {qualified:?}",
                self.input_wei
            ));
        }
        cap_holds_one(
            session_cap_wei,
            self.input_wei,
            gas_price_wei,
            self.gas_limit,
            (&format!("{keys}_INPUT_WEI"), venue.what()),
        )
    }

    /// One Four BUY's budget.
    fn budget(&self, gas_price_wei: u64) -> BudgetEnvelope {
        one_buy_budget(self.input_wei, gas_price_wei, self.gas_limit)
    }
}

/// The armed Four Classic BUY: the deployment the drift probe validated
/// (its code generation, its fee terms), the policy and budget.
#[derive(Clone, Copy, Debug)]
struct FourArm {
    validated: ValidatedFourDeployment,
    policy: BuyPolicy,
    budget: BudgetEnvelope,
    /// SC4 c (B1): its positions' exit gas limits (approval, SELL).
    exit_gas: (u64, u64),
}

/// A C3 BUY admitted on one venue, before the lane, signature and plan.
struct C3Admitted {
    intent: BuyAdmitted,
    /// The arm's policy of its venue.
    policy: BuyPolicy,
    token: [u8; 20],
    budget: BudgetEnvelope,
    decoded_ns: u64,
    admitted_ns: u64,
    snapshot: SnapshotId,
    head: ChainHead,
}

/// A BUY admitted on one venue (C3 or Strategy C), before its nonce: what
/// [`Executor::expect_buy`] binds and signs.
enum BuyAdmitted {
    Flap(AdmittedBuy),
    Classic(AdmittedClassicBuy),
    OpenFour(AdmittedOpenFourBuy),
}

impl BuyAdmitted {
    fn token(&self) -> Address {
        match self {
            Self::Flap(a) => a.token(),
            Self::Classic(a) => a.token(),
            Self::OpenFour(a) => a.token(),
        }
    }

    /// The tokens the BUY is bound to yield at least (OpenFour: exactly).
    fn min_tokens(&self) -> U256 {
        match self {
            Self::Flap(a) => a.min_output_amount(),
            Self::Classic(a) => a.min_tokens(),
            Self::OpenFour(a) => a.amount(),
        }
    }
}

/// An admitted BUY bound to its nonce and signed (the raw transaction in
/// the executor's buffer).
enum ExpectedBuyTx {
    Flap(ExpectedBuy),
    Classic(ExpectedClassicBuy),
    OpenFour(ExpectedOpenFourBuy),
}

impl ExpectedBuyTx {
    fn tx(&self) -> ExpectedTx<'_> {
        match self {
            Self::Flap(e) => ExpectedTx::Buy(e),
            Self::Classic(e) => ExpectedTx::ClassicBuy(e),
            Self::OpenFour(e) => ExpectedTx::OpenFourBuy(e),
        }
    }
}

/// A native value as signed (big-endian, no leading zero), in wei.
fn native_value(bytes: &[u8]) -> Option<U256> {
    (bytes.len() <= 32 && bytes.first() != Some(&0)).then(|| U256::from_be_slice(bytes))
}

/// The local node's adapter slot of the PAPER table (Phase I): the single
/// raw transaction of a Strategy C intent, whose transport is the sink.
pub fn paper_raw_adapter() -> AdapterId {
    // Slot 1 exists in every lane (`MAX_ADAPTERS` > 1).
    AdapterId::new(1).unwrap_or_else(|| unreachable!("adapter slot 1"))
}

/// What a Strategy C PAPER transaction is (Phase I); (R3b) the LIVE
/// path's nonce burn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentKind {
    Buy,
    Approve,
    Sell,
    Burn,
}

impl IntentKind {
    pub fn code(self) -> u8 {
        match self {
            Self::Buy => 1,
            Self::Approve => 2,
            Self::Sell => 3,
            Self::Burn => 4,
        }
    }
}

/// The exact curve a Strategy C intent is admitted on
/// (`strategy::engine::Engine::curve_of`).
pub use darkbot_bsc_trading::strategy::engine::StrategyCurve;

/// How an exit's SELL is priced: on the token's curve, or (REV.1.0 R5) by
/// the Portal's quote of a token that migrated to its DEX. A transient
/// `Copy` value of one exit: a boxed curve would allocate on the exit path.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitSale {
    Curve(StrategyCurve),
    Quoted(U256),
}

/// An exit's SELL as admitted: on its curve, or (R5) at the Portal's quote;
/// SC4 on its Four.meme venue's curve.
enum SellAdmitted {
    Curve(darkbot_bsc_trading::economics::flap::AdmittedSell),
    Quoted(U256),
    Classic(AdmittedClassicSell),
    OpenFour(AdmittedOpenFourSell),
}

/// An exit's approval and SELL as bound on its venue: what signs and what
/// the lane and plan take of each step.
enum ExitExpected {
    Flap(ExpectedExit),
    Classic(ExpectedClassicExit),
    OpenFour(ExpectedOpenFourExit),
}

impl ExitExpected {
    fn sign(
        &self,
        kind: IntentKind,
        signer: &Signer,
        scratch: &mut Vec<u8>,
        out: &mut Vec<u8>,
    ) -> bool {
        let approve = kind == IntentKind::Approve;
        match (self, approve) {
            (Self::Flap(e), true) => e.sign_approve(signer, scratch, out).is_ok(),
            (Self::Flap(e), false) => e.sign_sell(signer, scratch, out).is_ok(),
            (Self::Classic(e), true) => e.sign_approve(signer, scratch, out).is_ok(),
            (Self::Classic(e), false) => e.sign_sell(signer, scratch, out).is_ok(),
            (Self::OpenFour(e), true) => e.sign_approve(signer, scratch, out).is_ok(),
            (Self::OpenFour(e), false) => e.sign_sell(signer, scratch, out).is_ok(),
        }
    }

    fn tx(&self, kind: IntentKind) -> ExpectedTx<'_> {
        match (self, kind == IntentKind::Approve) {
            (Self::Flap(e), true) => ExpectedTx::Approve(e),
            (Self::Flap(e), false) => ExpectedTx::Sell(e),
            (Self::Classic(e), true) => ExpectedTx::ClassicApprove(e),
            (Self::Classic(e), false) => ExpectedTx::ClassicSell(e),
            (Self::OpenFour(e), true) => ExpectedTx::OpenFourApprove(e),
            (Self::OpenFour(e), false) => ExpectedTx::OpenFourSell(e),
        }
    }
}

/// A Strategy C position whose BUY reached the sink: the tokens it is bound
/// to have bought (its minimum output) and its input (open exposure).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Held {
    token: [u8; 20],
    min_tokens: U256,
    input_wei: U256,
    /// Its input left the open exposure with its SELL's hand-over; an
    /// exit reopened (its SELL reverted or replaced) counts it again.
    released: bool,
    /// SC4 c (B1): its venue's exit gas limits (approval, SELL): what its
    /// exit is signed with and reserves.
    exit_gas: (u64, u64),
}

/// The C2 facts of a Strategy C intent: the canonical snapshot under
/// (policy, codehash) generations, and the head; and the wall time its age
/// is measured at (unix ms).
pub struct CanonicalContext<'a> {
    pub snapshot: &'a dyn Fn(u64, u64) -> Option<SnapshotId>,
    pub head: Option<ChainHead>,
    pub now_unix_ms: u64,
    /// The sender's canonical next nonce, when C2 knows it exactly (clean):
    /// an end whose transactions reach the chain takes no other.
    pub next_nonce: Option<u64>,
    /// The most the exits of the sender's open positions may still cost,
    /// when the end's ledger knows it ([`Executor::exits_after_buy`]).
    pub exit_reserve_wei: Option<U256>,
    /// REV.1.0 R5: the gas price an exit goes at when not the arm's (a
    /// stalled step re-priced by the LIVE path).
    pub exit_gas_price_wei: Option<u64>,
}

impl CanonicalContext<'_> {
    /// The head, if it is at most `max_ms` old (its timestamp is unix
    /// seconds, so the age is overstated by less than a second).
    fn fresh_head(&self, max_ms: u64) -> Result<ChainHead, Refusal> {
        let head = self.head.ok_or(Refusal::NoSnapshot)?;
        let age = self
            .now_unix_ms
            .saturating_sub(head.timestamp.saturating_mul(1_000));
        if age > max_ms {
            return Err(Refusal::StaleHead);
        }
        Ok(head)
    }
}

/// One Strategy C transaction that reached the executor's end (Phase I).
/// Its `times.ingress_ns` is the decision's record time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecIntent<H> {
    pub kind: IntentKind,
    pub token: [u8; 20],
    /// The canonical head block it was formed on.
    pub block: u64,
    pub handoff: H,
    pub times: StageTimes,
    /// The signed transaction (PAPER: a local test artefact, never sent).
    pub raw: Vec<u8>,
}

/// One Strategy C PAPER transaction that reached the sink.
pub type PaperIntent = ExecIntent<PaperHandoff>;

/// The stage times of one PAPER BUY on the runtime's monotonic clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StageTimes {
    /// The source received the frame of the pending launch.
    pub ingress_ns: u64,
    pub decoded_ns: u64,
    pub admitted_ns: u64,
    pub signed_ns: u64,
    /// The lane holds the BUY (its final binding check done).
    pub prepared_ns: u64,
    /// Plan formed and the adapter request built.
    pub planned_ns: u64,
    /// The end took the complete request: `T_shadow_sink` (PAPER) or the
    /// release handoff boundary (LIVE).
    pub sink_ns: u64,
}

/// One C3 BUY that reached the executor's end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecBuy<H> {
    pub launch_hash: [u8; 32],
    pub token: [u8; 20],
    pub handoff: H,
    pub times: StageTimes,
    /// The signed BUY (PAPER: a local test artefact, never sent).
    pub raw: Vec<u8>,
}

/// One PAPER BUY that reached the sink.
pub type PaperBuy = ExecBuy<PaperHandoff>;

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
    /// Selection, not a full sort: linear in the kept samples, read only
    /// when the status is written.
    pub fn quantiles(&self) -> Option<(u64, u64, u64)> {
        if self.samples.is_empty() {
            return None;
        }
        let mut kept: Vec<u64> = self.samples.iter().copied().collect();
        let last = kept.len() - 1;
        let mut at = |p: usize| *kept.select_nth_unstable(last * p / 100).1;
        Some((at(50), at(99), self.max))
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// The PAPER sink as the adapter's transport: `write` hands the complete
/// request (the production frame with the signed BUY in its slot) to the
/// sink, read on the runtime clock. It owns no socket, stream or client;
/// nothing is read back.
struct SinkTransport<'a> {
    sink: &'a mut PaperSink,
    clock: &'a dyn Clock,
    tx_hash: [u8; 32],
    max_request_len: usize,
    state: ConnectionState,
    handoff: Option<PaperHandoff>,
}

impl Transport for SinkTransport<'_> {
    fn state(&self) -> ConnectionState {
        self.state
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // One request, taken whole.
        self.handoff = Some(self.sink.handoff(self.tx_hash, buf, self.clock.now_ns()));
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::ErrorKind::WouldBlock.into())
    }

    fn mark(&mut self, state: ConnectionState) {
        self.state = state;
    }

    fn max_request_len(&self) -> usize {
        self.max_request_len
    }
}

/// BSC3-085: the held positions still to exit, and the approved ones of
/// them; SC4 c (B1): the gas their exits still need at their venues'
/// limits (each open position's SELL, the approval of each not approved).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct OpenExits {
    open: u64,
    approved: u64,
    sell_gas: u128,
    approve_gas: u128,
}

/// The armed path towards its end.
pub struct Executor<E: End> {
    end: E,
    deployment: ValidatedDeployment,
    /// R12c: the Four Classic arm (`arm_four`).
    four: Option<FourArm>,
    /// R12d: the OpenFour arm (`arm_openfour`).
    openfour: Option<FourArm>,
    terms: VenueBuyTerms,
    policy: BuyPolicy,
    config: ExecConfig,
    budget: BudgetEnvelope,
    lanes: Vec<IntentLane>,
    used: usize,
    /// Exposure dispatched in this session, against `session_cap_wei`.
    spent_wei: U256,
    /// Tokens bought in this session (at most one per lane).
    bought: Vec<[u8; 20]>,
    /// R6: each C3 token with the launch transaction it was bought on.
    bought_on: Vec<([u8; 20], [u8; 32])>,
    fence: PathBuf,
    scratch: Vec<u8>,
    raw: Vec<u8>,
    counters: ExecCounters,
    latencies: Latencies,
    /// Phase I: Strategy C's lanes, one per nonce of the PAPER sender's
    /// sequence from the arming's next nonce (gate M3: BUY n, approval
    /// n+1, SELL n+2, across positions), their open exposure, positions
    /// held and exits taken.
    strategy_lanes: Vec<IntentLane>,
    strategy_first_nonce: u64,
    strategy_used: usize,
    strategy_nonce_gap: bool,
    /// Gate L-d: one sender, one sequence. A C3 BUY disclosed at the next
    /// nonce n (its alternatives all share it) makes the strategy start
    /// at n+1 (its first lane skipped); a strategy intent holding n
    /// refuses C3's later BUYs (NONCE_TAKEN).
    c3_took_nonce: bool,
    strategy_skipped_first: bool,
    strategy_spent_wei: U256,
    strategy_bought: Vec<Held>,
    /// Approvals at the sink and the token amounts they approved.
    approved: Vec<([u8; 20], U256)>,
    exited: Vec<[u8; 20]>,
    /// BSC3-085: of `strategy_bought`, the positions not exited and of
    /// those the approved ones, recounted for the one token at each change
    /// of a position (linear in the arm's lists): a BUY reads its exit
    /// reserve in O(1) (before: every position against every exit and
    /// approval, quadratic over an arm).
    open_exits: OpenExits,
    exit_policy: ExitPolicy,
    strategy_counters: ExecCounters,
    /// The arming's next nonce: every C3 lane's nonce.
    first_nonce: u64,
    evidence: EvidenceCounters,
}

/// The armed PAPER path.
pub type PaperExecutor = Executor<PaperEnd>;

/// C2's evidence about one of the sender's nonces (plan 28.10.).
#[derive(Clone, Copy, Debug)]
pub enum NonceEvidence<'a> {
    Inclusion(&'a InclusionProof),
    Finality(&'a FinalityProof),
    Orphan(&'a OrphanProof),
}

/// What C2's evidence did to the executor's intents.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EvidenceCounters {
    pub included: u64,
    pub reverted: u64,
    /// Another transaction consumed an intent's nonce.
    pub replaced: u64,
    pub finalized: u64,
    pub orphaned: u64,
    /// A nonce of the sender no lane of this arm assigned (consumed by a
    /// transaction this executor never made).
    pub unassigned: u64,
}

/// Why the executor could not be armed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArmError {
    Config(String),
    Policy(String),
    Fence(String),
    Lane(String),
}

impl core::fmt::Display for ArmError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Config(m) => write!(f, "paper exec config: {m}"),
            Self::Policy(m) => write!(f, "paper exec policy: {m}"),
            Self::Fence(m) => write!(f, "paper exec fence: {m}"),
            Self::Lane(m) => write!(f, "paper exec lane: {m}"),
        }
    }
}

impl std::error::Error for ArmError {}

/// Creates, writes and syncs one fence file that did not exist, and syncs
/// its directory: the durable attestation behind the armed lanes.
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
    if let Some(dir) = path.parent() {
        fs::File::open(dir).and_then(|d| d.sync_all()).map_err(e)?;
    }
    Ok(())
}

/// What one hand-over takes to its end.
#[derive(Clone, Copy, Debug)]
pub enum Submission<'a> {
    /// C3: the original launch's signed bytes, then the BUY.
    LaunchBundle(&'a [u8]),
    /// Strategy C: the transaction alone (a BUY, an approval or a SELL).
    Single,
}

impl<'a> Submission<'a> {
    /// Audit E 4.2: the profile the hand-over is planned (and its transport
    /// start deadline set) under, PAPER's and LIVE's alike: a launch's
    /// bundle is a birth entry behind it, a transaction alone one alone.
    pub fn profile(self) -> ReleaseProfile {
        match self {
            Self::LaunchBundle(_) => ReleaseProfile::BirthEntryV1,
            Self::Single => ReleaseProfile::SingleRawV1,
        }
    }

    /// The launch bytes a frame carries ahead of the transaction (none
    /// alone).
    pub fn launch_raw(self) -> &'a [u8] {
        match self {
            Self::LaunchBundle(launch_raw) => launch_raw,
            Self::Single => &[],
        }
    }
}

/// Why a hand-over's plan inputs cannot be formed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputsError {
    NoAdapters,
    WindowOverflow,
}

impl core::fmt::Display for InputsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::NoAdapters => "no adapter set",
            Self::WindowOverflow => "the block window overflows",
        })
    }
}

/// Audit E 4.2: the plan inputs of one hand-over through `adapters`,
/// PAPER's and LIVE's (each end plans them under its qualification: LIVE
/// its verified one, PAPER none, never a release): the submission's mode,
/// launch, merge policy and profile; the local block deadline at the head
/// plus `validity_blocks`; and the same last block as the provider
/// validity every provider is told (a plan through the local node alone
/// has none: the node enforces no expiry).
pub fn plan_inputs(
    table: &AdapterTable,
    adapters: &[AdapterId],
    ticket: Ticket,
    raw: &[u8],
    submission: Submission<'_>,
    head: ChainHead,
    validity_blocks: u64,
) -> Result<PlanInputs, InputsError> {
    let set = AdapterSet::new(adapters).ok_or(InputsError::NoAdapters)?;
    let last_block = head
        .number
        .checked_add(validity_blocks)
        .ok_or(InputsError::WindowOverflow)?;
    let local_only = adapters.iter().all(|a| {
        table
            .record(*a)
            .is_some_and(|r| r.kind() == AdapterKind::LocalGeth)
    });
    let (mode, launch, merge) = match submission {
        Submission::LaunchBundle(launch_raw) => (
            SubmissionMode::PublicFullLaunchBundle,
            Launch::PublicOriginal(TxRef::of(launch_raw)),
            MergePolicy::NoMerge,
        ),
        Submission::Single => (
            SubmissionMode::SingleRawBuy,
            Launch::None,
            MergePolicy::ProviderDefault,
        ),
    };
    Ok(PlanInputs {
        ticket,
        buy: TxRef::of(raw),
        launch,
        mode,
        profile: submission.profile(),
        adapters: set,
        local_block_deadline: LocalBlockStartDeadline {
            last_head_block: last_block,
        },
        provider_validity: (!local_only).then_some(ProviderValidity {
            max_block_number: last_block,
            min_timestamp: None,
            max_timestamp: None,
        }),
        merge,
        pre_submission: PreSubmission::No,
        provider_payment_wei: U256::ZERO,
    })
}

/// One transaction its lane holds, bound to `expected` and `snapshot`, for
/// the end to plan, frame and dispatch.
pub struct Handover<'a> {
    pub lane: &'a mut IntentLane,
    pub ticket: Ticket,
    pub raw: &'a [u8],
    pub tx_hash: [u8; 32],
    pub expected: ExpectedTx<'a>,
    pub budget: &'a BudgetEnvelope,
    pub snapshot: &'a SnapshotId,
    pub head: ChainHead,
    pub clock: &'a dyn Clock,
    /// Where the transport start deadline runs from.
    pub start_ns: u64,
    pub submission: Submission<'a>,
    pub config: &'a ExecConfig,
    /// A BUY: the most the exits of the sender's open positions and of its
    /// own may still cost (BSC3-057); an exit step: zero.
    pub exit_reserve_wei: U256,
    /// REV.1.0 R2: a Strategy C BUY's launch creator (the end keeps it with
    /// the release); none for C3 (its block names it) and the exits.
    pub launcher: Option<[u8; 20]>,
}

/// What an end made of one hand-over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handed<H> {
    pub result: Result<H, Refusal>,
    /// Plan formed and the adapter request built (0: not reached).
    pub planned_ns: u64,
    /// The economic admission was consumed: the exposure counts.
    pub admitted: bool,
    /// A permit's write was started (or the dispatch broke after the
    /// economic admission): the bytes may be out, their nonce is taken and
    /// their exposure counts. Otherwise the nonce is taken again (gate N1)
    /// and nothing is spent (gate L2).
    pub disclosed: bool,
}

impl<H> Handed<H> {
    /// Refused before the dispatch: nothing consumed.
    pub fn refused(r: Refusal) -> Self {
        Handed {
            result: Err(r),
            planned_ns: 0,
            admitted: false,
            disclosed: false,
        }
    }
}

/// What a dispatch consumed: (the economic admission, a disclosure).
pub fn consumed_by(dispatched: &Result<FanoutReport, DispatchError>) -> (bool, bool) {
    match dispatched {
        Ok(report) => (
            report.admitted(),
            report.admitted() && !report.admitted_without_disclosure,
        ),
        Err(DispatchError::DispatchBinding(_)) => (true, true),
        Err(_) => (false, false),
    }
}

/// When an end took a hand-over, on the runtime's monotonic clock.
pub trait HandoffTime {
    fn at_ns(&self) -> u64;
}

impl HandoffTime for PaperHandoff {
    fn at_ns(&self) -> u64 {
        self.at_ns
    }
}

/// Where the executor's path ends (plan 28.10.): the sender's signer, and
/// the plan, frame and dispatch of each prepared transaction.
pub trait End {
    type Handoff: HandoffTime + Copy + core::fmt::Debug + PartialEq + Eq;
    /// The fence file prefix and the fence body's magic of this end.
    const FENCE: (&'static str, [u8; 8]);
    /// Whether this end's transactions reach the chain (BSC3-055). Then
    /// the sender's lanes together keep the single-sender order of the
    /// intent contract (Unit 4 invariant 3): a transaction is handed over
    /// only at the sender's canonical next nonce (C2), so at most one nonce
    /// of the sender has bytes out that the chain has not consumed, and an
    /// exit's SELL waits for its approval to be canonical. An end whose
    /// transactions never leave the process (PAPER) plans its sequence
    /// ahead.
    const CHAIN_ORDERED: bool;

    fn address(&self) -> [u8; 20];
    fn signer(&self) -> &Signer;
    fn hand_over(&mut self, h: Handover<'_>) -> Handed<Self::Handoff>;

    /// Reads, without waiting, what arrived of the providers' answers to
    /// the transactions of `lane` this end released (BSC3-051), and records
    /// them in the lane. An end without provider answers has none.
    fn poll_responses(&mut self, _lane: &mut IntentLane, _clock: &dyn Clock) {}
}

/// The PAPER end: the PAPER identity signs; the 48Club bundle (C3) and the
/// local node's single raw transaction (Strategy C) are planned under
/// [`Submission::profile`] (a birth entry, one alone) and dispatched
/// diagnostically to the sink. Never a release.
pub struct PaperEnd {
    identity: PaperIdentity,
    table: AdapterTable,
    target: ProviderTarget,
    sink: PaperSink,
}

impl PaperEnd {
    /// The PAPER end of `chain_id` under the capability generation.
    pub fn new(
        identity: PaperIdentity,
        chain_id: u64,
        capability_generation: u64,
    ) -> Result<Self, ArmError> {
        let table = AdapterTable::new(
            chain_id,
            capability_generation,
            &[
                AdapterBinding {
                    adapter: paper_adapter(),
                    capability: CapabilityId::Club48BuilderV1,
                },
                // Phase I: Strategy C's single raw transactions.
                AdapterBinding {
                    adapter: paper_raw_adapter(),
                    capability: CapabilityId::LocalGethV1,
                },
            ],
        )
        .map_err(|e| ArmError::Config(e.to_string()))?;
        let target = ProviderTarget::club48(
            TlsEndpoint::new(CLUB48_DOCUMENTED_HOST, HTTPS_PORT)
                .map_err(|e| ArmError::Config(format!("{e:?}")))?,
        );
        Ok(PaperEnd {
            identity,
            table,
            target,
            sink: PaperSink::default(),
        })
    }

    pub fn sink(&self) -> &PaperSink {
        &self.sink
    }
}

impl End for PaperEnd {
    type Handoff = PaperHandoff;
    const FENCE: (&'static str, [u8; 8]) = ("paper", *b"DBPAPFN3");
    // A PAPER transaction never leaves the process.
    const CHAIN_ORDERED: bool = false;

    fn address(&self) -> [u8; 20] {
        self.identity.address()
    }

    fn signer(&self) -> &Signer {
        self.identity.signer()
    }

    fn hand_over(&mut self, h: Handover<'_>) -> Handed<PaperHandoff> {
        let Handover {
            lane,
            ticket,
            raw,
            tx_hash,
            expected,
            budget,
            snapshot,
            head,
            clock,
            start_ns,
            submission,
            config,
            exit_reserve_wei: _,
            launcher: _,
        } = h;
        // PAPER's adapters: the 48Club builder's bundle; a transaction alone
        // through the local node (a builder's single is enabled only by the
        // LIVE qualification, which PAPER never simulates).
        let adapter = match submission {
            Submission::LaunchBundle(_) => paper_adapter(),
            Submission::Single => paper_raw_adapter(),
        };
        let table = &self.table;
        let target = &self.target;
        let planned = (|| {
            // Audit E 4.2: the shared plan inputs and framing, planned with
            // no qualification.
            let inputs = plan_inputs(
                table,
                &[adapter],
                ticket,
                raw,
                submission,
                head,
                config.validity_blocks,
            )
            .map_err(|_| Refusal::Plan)?;
            let plan =
                SubmissionPlan::new(table, expected, budget, inputs).map_err(|_| Refusal::Plan)?;
            let frame = adapter_frame(
                &plan,
                table,
                adapter,
                &target.frame_endpoint(),
                submission.launch_raw(),
                config.max_frame,
            )
            .map_err(|_| Refusal::Plan)?;
            let deadline = TransportStartDeadline::for_profile(
                submission.profile(),
                clock.id(),
                start_ns,
                config.start_limit_ns,
            )
            .map_err(|_| Refusal::Plan)?;
            Ok((plan, frame, deadline))
        })();
        let (plan, mut frame, deadline) = match planned {
            Ok(p) => p,
            Err(r) => return Handed::refused(r),
        };
        let planned_ns = clock.now_ns();
        let mut transport = SinkTransport {
            sink: &mut self.sink,
            clock,
            tx_hash,
            max_request_len: config.max_frame,
            state: ConnectionState::Ready,
            handoff: None,
        };
        let dispatched = dispatch_diagnostic(
            lane,
            &plan,
            &DispatchContext {
                table,
                current: snapshot,
                head,
            },
            clock,
            &deadline,
            &mut [Armed {
                adapter,
                transport: &mut transport,
                frame: &mut frame,
            }],
        );
        let handoff = transport.handoff;
        let (admitted, disclosed) = consumed_by(&dispatched);
        let result = match dispatched {
            Err(_) => Err(Refusal::Dispatch),
            Ok(report) => match report.adapter(adapter).map(|r| r.outcome) {
                Some(SendOutcome::HandedOff) => handoff.ok_or(Refusal::Dispatch),
                Some(SendOutcome::SkippedDeadline(_)) => Err(Refusal::Deadline),
                _ => Err(Refusal::Dispatch),
            },
        };
        Handed {
            result,
            planned_ns,
            admitted,
            disclosed,
        }
    }
}

/// The executor parts one hand-over needs.
struct Parts<'a, E: End> {
    end: &'a mut E,
    lane: &'a mut IntentLane,
    config: &'a ExecConfig,
    scratch: &'a mut Vec<u8>,
}

/// Binds a signed transaction in its lane (its final binding check) and
/// hands it to the end. Returns the handoff, the prepared and planned
/// times, and what the dispatch consumed: (admission, disclosure).
#[allow(clippy::too_many_arguments)]
fn to_end<E: End>(
    parts: Parts<'_, E>,
    ticket: Ticket,
    raw: &[u8],
    expected: ExpectedTx<'_>,
    budget: &BudgetEnvelope,
    exit_reserve_wei: U256,
    snapshot: &SnapshotId,
    head: ChainHead,
    clock: &dyn Clock,
    start_ns: u64,
    submission: Submission<'_>,
    launcher: Option<[u8; 20]>,
) -> (Result<E::Handoff, Refusal>, u64, u64, (bool, bool)) {
    let Parts {
        end,
        lane,
        config,
        scratch,
    } = parts;
    let tx_hash = match lane.prepare(ticket, raw, expected, snapshot, scratch) {
        Ok(h) => h,
        Err(_) => return (Err(Refusal::Lane), 0, 0, (false, false)),
    };
    let prepared_ns = clock.now_ns();
    let handed = end.hand_over(Handover {
        lane,
        ticket,
        raw,
        tx_hash: tx_hash.0,
        expected,
        budget,
        snapshot,
        head,
        clock,
        start_ns,
        submission,
        config,
        exit_reserve_wei,
        launcher,
    });
    (
        handed.result,
        prepared_ns,
        handed.planned_ns,
        (handed.admitted, handed.disclosed),
    )
}

/// One single-intent lane at `nonce` behind the arm's durable fence.
fn fenced_lane(
    chain_id: u64,
    sender: Address,
    nonce: u64,
    cap: u128,
) -> Result<IntentLane, ArmError> {
    let armed = ArmedFence::from_durable_startup(chain_id, sender, nonce, U256::from(cap));
    IntentLane::armed(armed, 1).map_err(|e| ArmError::Lane(format!("{e:?}")))
}

/// A runtime clock reading as a submission [`Clock`].
pub struct FnClock<F: Fn() -> u64> {
    pub id: u32,
    pub now: F,
}

impl<F: Fn() -> u64> Clock for FnClock<F> {
    fn id(&self) -> u32 {
        self.id
    }

    fn now_ns(&self) -> u64 {
        (self.now)()
    }
}

/// Where and as what one arm fences its lanes: the canonical next nonce
/// of the PAPER sender (a C2 nonce fact), the fence directory, the
/// process's session and this arm's number in it.
#[derive(Clone, Copy, Debug)]
pub struct Arming<'a> {
    pub next_nonce: u64,
    pub fence_dir: &'a Path,
    pub session: u64,
    pub arm: u64,
}

/// One verified pending launch: the hash of its signed bytes `raw`, its
/// calldata `input` to `to`, received at `ingress_ns`.
#[derive(Clone, Copy, Debug)]
pub struct PendingLaunch<'a> {
    pub hash: [u8; 32],
    pub raw: &'a [u8],
    /// The verified signer (R12d: an OpenFour token's derivation names
    /// it). Taken as given: the ingress recovered it from these signed
    /// bytes (`geth_source::sender_of`, the WSS sources' verification), no
    /// second recovery on the launch path.
    pub from: [u8; 20],
    pub to: Option<[u8; 20]>,
    /// The native value as signed (big-endian bytes).
    pub value: &'a [u8],
    pub input: &'a [u8],
    pub ingress_ns: u64,
}

/// The C2 facts one launch needs.
pub struct LaunchContext<'a> {
    /// The coherent snapshot of a candidate under (policy, codehash)
    /// generations.
    pub snapshot: &'a dyn Fn(B256, u64, u64) -> Option<SnapshotId>,
    /// The canonical head: number and timestamp (unix seconds).
    pub head: Option<ChainHead>,
    /// The sender's canonical next nonce, when C2 knows it exactly (clean):
    /// an end whose transactions reach the chain takes no other.
    pub next_nonce: Option<u64>,
    /// As [`CanonicalContext::exit_reserve_wei`].
    pub exit_reserve_wei: Option<U256>,
}

impl Executor<PaperEnd> {
    /// Arms the PAPER path: the PAPER end (its adapter table and the
    /// selected builder target), then [`Executor::arm_end`].
    pub fn arm(
        identity: PaperIdentity,
        deployment: ValidatedDeployment,
        terms: VenueBuyTerms,
        config: ExecConfig,
        arming: Arming<'_>,
    ) -> Result<Self, ArmError> {
        config.validate().map_err(ArmError::Config)?;
        let chain_id = deployment.deployment().chain_id;
        let end = PaperEnd::new(identity, chain_id, config.capability_generation)?;
        Self::arm_end(end, deployment, terms, config, arming)
    }

    pub fn sink(&self) -> &PaperSink {
        self.end.sink()
    }
}

impl<E: End> Executor<E> {
    /// Arms the path towards `end`: the policy, then one durable fence
    /// `<end>-<session>-<arm>.fence` in `fence_dir` (unique per arm) and
    /// behind it `config.lanes` single-intent lanes at the sender's
    /// canonical `next_nonce` (a C2 nonce fact).
    pub fn arm_end(
        end: E,
        deployment: ValidatedDeployment,
        terms: VenueBuyTerms,
        config: ExecConfig,
        arming: Arming<'_>,
    ) -> Result<Self, ArmError> {
        let Arming {
            next_nonce,
            fence_dir,
            session,
            arm,
        } = arming;
        config.validate().map_err(ArmError::Config)?;
        let policy = BuyPolicy::new(
            config.policy_generation,
            U256::from(config.input_wei),
            config.slippage_bps,
            config.gas_price_wei,
            config.gas_limit,
        )
        .map_err(|e| ArmError::Policy(format!("{e:?}")))?;
        let exit_policy = ExitPolicy::new(
            config.policy_generation,
            config.exit_slippage_bps,
            config.gas_price_wei,
            config.approve_gas_limit,
            config.sell_gas_limit,
        )
        .map_err(|e| ArmError::Policy(format!("{e:?}")))?;
        let chain_id = deployment.deployment().chain_id;
        fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(fence_dir)
            .map_err(|e| ArmError::Fence(format!("{}: {e}", fence_dir.display())))?;
        let sender = Address::from(end.address());
        // The strategy's nonces: one lane per nonce of the sequence.
        next_nonce
            .checked_add(config.strategy_lanes as u64)
            .ok_or_else(|| ArmError::Config("the strategy nonces overflow".into()))?;
        let mut body = Vec::with_capacity(8 + 8 + 20 + 8 + 16 + 8 + 8 + 8 + 8 + 16);
        body.extend_from_slice(&E::FENCE.1);
        body.extend_from_slice(&chain_id.to_le_bytes());
        body.extend_from_slice(sender.as_slice());
        body.extend_from_slice(&next_nonce.to_le_bytes());
        body.extend_from_slice(&config.session_cap_wei.to_le_bytes());
        body.extend_from_slice(&(config.lanes as u64).to_le_bytes());
        body.extend_from_slice(&config.policy_generation.to_le_bytes());
        body.extend_from_slice(&config.capability_generation.to_le_bytes());
        body.extend_from_slice(&(config.strategy_lanes as u64).to_le_bytes());
        body.extend_from_slice(&config.strategy_session_cap_wei.to_le_bytes());
        let fence = fence_dir.join(format!("{}-{session}-{arm}.fence", E::FENCE.0));
        durable_fence(&fence, &body)?;
        let lane_at = |nonce: u64, cap: u128| fenced_lane(chain_id, sender, nonce, cap);
        // C3's pool: alternative BUYs from the same canonical state, all
        // at the next nonce. Strategy C's: its sequence, one nonce each.
        let pools = (
            (0..config.lanes)
                .map(|_| lane_at(next_nonce, config.session_cap_wei))
                .collect::<Result<Vec<_>, _>>(),
            (0..config.strategy_lanes as u64)
                .map(|i| lane_at(next_nonce + i, config.strategy_session_cap_wei))
                .collect::<Result<Vec<_>, _>>(),
        );
        let (lanes, strategy_lanes) = match pools {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => {
                let _ = fs::remove_file(&fence);
                return Err(e);
            }
        };
        let samples = config.latency_samples;
        let budget = config.budget();
        Ok(Executor {
            end,
            deployment,
            four: None,
            openfour: None,
            terms,
            policy,
            budget,
            lanes,
            used: 0,
            spent_wei: U256::ZERO,
            bought: Vec::with_capacity(config.lanes),
            bought_on: Vec::with_capacity(config.lanes),
            fence,
            scratch: Vec::with_capacity(512),
            raw: Vec::with_capacity(512),
            counters: ExecCounters::default(),
            latencies: Latencies::new(samples),
            strategy_used: 0,
            strategy_first_nonce: next_nonce,
            strategy_nonce_gap: false,
            c3_took_nonce: false,
            strategy_skipped_first: false,
            strategy_spent_wei: U256::ZERO,
            strategy_bought: Vec::with_capacity(strategy_lanes.len()),
            approved: Vec::with_capacity(strategy_lanes.len()),
            exited: Vec::with_capacity(strategy_lanes.len()),
            open_exits: OpenExits::default(),
            strategy_lanes,
            exit_policy,
            strategy_counters: ExecCounters::default(),
            first_nonce: next_nonce,
            evidence: EvidenceCounters::default(),
            config,
        })
    }

    /// R12c: arms the Four Classic BUY on the deployment the drift probe
    /// validated, with the configured `BSC_EXEC_FOUR_*` values; its launches
    /// then take the same lanes, caps and end as the Flap BUY.
    pub fn arm_four(&mut self, validated: ValidatedFourDeployment) -> Result<(), ArmError> {
        self.four = Some(self.four_arm(FourVenue::Classic, validated)?);
        Ok(())
    }

    /// R12d: arms the OpenFour BUY on the deployment the drift probe
    /// validated with its executed bonding preset, with the configured
    /// `BSC_EXEC_OPENFOUR_*` values; its launches take the same lanes, caps
    /// and end.
    pub fn arm_openfour(&mut self, validated: ValidatedFourDeployment) -> Result<(), ArmError> {
        if validated.openfour_bonding().is_none() {
            return Err(ArmError::Config(
                "the validated deployment pins no executed OpenFour preset".into(),
            ));
        }
        self.openfour = Some(self.four_arm(FourVenue::OpenFour, validated)?);
        Ok(())
    }

    fn four_arm(
        &self,
        venue: FourVenue,
        validated: ValidatedFourDeployment,
    ) -> Result<FourArm, ArmError> {
        let keys = venue.keys();
        let four = match venue {
            FourVenue::Classic => self.config.four,
            FourVenue::OpenFour => self.config.openfour,
        }
        .ok_or_else(|| ArmError::Config(format!("{keys}_* is not configured")))?;
        let policy = BuyPolicy::new(
            self.config.policy_generation,
            U256::from(four.input_wei),
            four.slippage_bps,
            self.config.gas_price_wei,
            four.gas_limit,
        )
        .map_err(|e| ArmError::Config(format!("{keys}_*: {e}")))?;
        Ok(FourArm {
            validated,
            policy,
            budget: four.budget(self.config.gas_price_wei),
            exit_gas: (four.approve_gas_limit, four.sell_gas_limit),
        })
    }

    /// Whether the Four Classic BUY is armed.
    pub fn four_armed(&self) -> bool {
        self.four.is_some()
    }

    /// R12d: whether the OpenFour BUY is armed.
    pub fn openfour_armed(&self) -> bool {
        self.openfour.is_some()
    }

    /// Disarms: the lanes go and the arm's fence is removed (a PAPER BUY
    /// never leaves the process, so nothing waits for reconciliation; the
    /// LIVE owner disarms only what its reconciliation resolved).
    pub fn disarm(self) -> Result<(), String> {
        fs::remove_file(&self.fence).map_err(|e| format!("{}: {e}", self.fence.display()))
    }

    /// Tests: the configuration it was armed with, changed in place.
    #[cfg(test)]
    pub(crate) fn config_mut(&mut self) -> &mut ExecConfig {
        &mut self.config
    }

    /// A launch the runtime could not hand over (its signed bytes are not a
    /// transaction): counted, never silent.
    pub fn refuse_undecodable(&mut self) {
        self.counters.launches += 1;
        self.counters.refuse(Refusal::NotDecoded);
    }

    /// One verified pending launch, its `T_ingress` on `clock`.
    pub fn on_launch(
        &mut self,
        launch: PendingLaunch<'_>,
        c2: &LaunchContext<'_>,
        clock: &dyn Clock,
    ) -> Result<ExecBuy<E::Handoff>, Refusal> {
        self.counters.launches += 1;
        let result = self.path(launch, c2, clock);
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
        launch: PendingLaunch<'_>,
        c2: &LaunchContext<'_>,
        clock: &dyn Clock,
    ) -> Result<ExecBuy<E::Handoff>, Refusal> {
        if !self.config.sender_paths.c3() {
            return Err(Refusal::PathOff);
        }
        // R12c: a call to the armed TokenManager2 is a Four Classic launch;
        // R12d: one to the armed OpenFour Core an OpenFour launch.
        let four_target = self
            .four
            .as_ref()
            .map(|f| f.validated.deployment().token_manager2);
        let openfour_target = self
            .openfour
            .as_ref()
            .map(|f| f.validated.deployment().openfour_core);
        let admitted = match launch.to.map(Address::from) {
            Some(to) if Some(to) == four_target => self.admit_four(&launch, c2, clock)?,
            Some(to) if Some(to) == openfour_target => self.admit_openfour(&launch, c2, clock)?,
            _ => self.admit_flap(&launch, c2, clock)?,
        };
        self.c3_send(admitted, &launch, c2, clock)
    }

    /// The Flap BUY a pending `newTokenV6` launch admits.
    fn admit_flap(
        &self,
        launch: &PendingLaunch<'_>,
        c2: &LaunchContext<'_>,
        clock: &dyn Clock,
    ) -> Result<C3Admitted, Refusal> {
        let portal = self.deployment.deployment().portal;
        let Recognized::LaunchV6(decoded) = recognize(
            portal,
            launch.to.map(Address::from),
            launch.input,
            self.config.max_calldata,
        ) else {
            return Err(Refusal::NotDecoded);
        };
        let decoded_ns = clock.now_ns();
        let candidate = B256::from(launch.hash);
        let classified =
            classify_launch(&self.deployment, candidate, &decoded).map_err(Refusal::Profile)?;
        let snapshot = (c2.snapshot)(
            candidate,
            self.policy.generation(),
            self.deployment.codehash_generation(),
        )
        .ok_or(Refusal::NoSnapshot)?;
        let head = c2.head.ok_or(Refusal::NoSnapshot)?;
        // A pending launch: no trade of its token precedes it.
        let state = QualifiedNativeState::fresh_launch(&classified, &self.terms, 0, snapshot)
            .map_err(|_| Refusal::Admission)?;
        let admitted = admit_native_buy(&state, &self.policy).map_err(|_| Refusal::Admission)?;
        Ok(C3Admitted {
            token: admitted.token().into_array(),
            intent: BuyAdmitted::Flap(admitted),
            policy: self.policy,
            budget: self.budget,
            decoded_ns,
            admitted_ns: clock.now_ns(),
            snapshot,
            head,
        })
    }

    /// R12c: the Four Classic BUY a pending `createToken` admits: the
    /// launch classified on the probe-validated deployment, the state
    /// directly after it, the Four policy.
    fn admit_four(
        &self,
        launch: &PendingLaunch<'_>,
        c2: &LaunchContext<'_>,
        clock: &dyn Clock,
    ) -> Result<C3Admitted, Refusal> {
        let four = self.four.as_ref().ok_or(Refusal::NotDecoded)?;
        let FourRecognized::ClassicLaunch(decoded) = four_recognize(
            four.validated.deployment(),
            launch.to.map(Address::from),
            launch.input,
            self.config.max_calldata,
        ) else {
            return Err(Refusal::NotDecoded);
        };
        let decoded_ns = clock.now_ns();
        let value = native_value(launch.value).ok_or(Refusal::NotDecoded)?;
        let candidate = B256::from(launch.hash);
        let classified = classify_classic_launch(&four.validated, &decoded, value, candidate)
            .map_err(Refusal::FourProfile)?;
        let snapshot = (c2.snapshot)(
            candidate,
            four.policy.generation(),
            four.validated.codehash_generation(),
        )
        .ok_or(Refusal::NoSnapshot)?;
        let head = c2.head.ok_or(Refusal::NoSnapshot)?;
        // A pending launch: no trade of its token precedes it.
        let state = QualifiedClassicState::fresh_launch(&classified, 0, snapshot)
            .map_err(|_| Refusal::Admission)?;
        let admitted = admit_classic_buy(&state, &four.policy).map_err(|_| Refusal::Admission)?;
        Ok(C3Admitted {
            token: admitted.token().into_array(),
            intent: BuyAdmitted::Classic(admitted),
            policy: four.policy,
            budget: four.budget,
            decoded_ns,
            admitted_ns: clock.now_ns(),
            snapshot,
            head,
        })
    }

    /// R12d: the OpenFour BUY a pending `createToken` admits: the launch
    /// classified on the probe-validated deployment's executed bonding
    /// preset (its token derived from its verified sender), the state
    /// directly after it and its creator BUY, the OpenFour policy.
    fn admit_openfour(
        &self,
        launch: &PendingLaunch<'_>,
        c2: &LaunchContext<'_>,
        clock: &dyn Clock,
    ) -> Result<C3Admitted, Refusal> {
        let arm = self.openfour.as_ref().ok_or(Refusal::NotDecoded)?;
        let FourRecognized::OpenFourLaunch(decoded) = four_recognize(
            arm.validated.deployment(),
            launch.to.map(Address::from),
            launch.input,
            self.config.max_calldata,
        ) else {
            return Err(Refusal::NotDecoded);
        };
        let decoded_ns = clock.now_ns();
        let value = native_value(launch.value).ok_or(Refusal::NotDecoded)?;
        let candidate = B256::from(launch.hash);
        let classified = classify_openfour_launch(
            &arm.validated,
            &decoded,
            Address::from(launch.from),
            value,
            candidate,
        )
        .map_err(Refusal::OpenFourProfile)?;
        let snapshot = (c2.snapshot)(
            candidate,
            arm.policy.generation(),
            arm.validated.codehash_generation(),
        )
        .ok_or(Refusal::NoSnapshot)?;
        let head = c2.head.ok_or(Refusal::NoSnapshot)?;
        // A pending launch: no trade of its token precedes it.
        let state = QualifiedOpenFourState::fresh_launch(&classified, 0, snapshot)
            .map_err(|_| Refusal::Admission)?;
        let admitted = admit_openfour_buy(&state, &arm.policy).map_err(|_| Refusal::Admission)?;
        Ok(C3Admitted {
            token: admitted.token().into_array(),
            intent: BuyAdmitted::OpenFour(admitted),
            policy: arm.policy,
            budget: arm.budget,
            decoded_ns,
            admitted_ns: clock.now_ns(),
            snapshot,
            head,
        })
    }

    /// An admitted C3 BUY of either venue: the token's duplicate and
    /// conflict check, the caps, the arm's nonce, the lane, the signed
    /// intent and its plan to the end.
    fn c3_send(
        &mut self,
        a: C3Admitted,
        launch: &PendingLaunch<'_>,
        c2: &LaunchContext<'_>,
        clock: &dyn Clock,
    ) -> Result<ExecBuy<E::Handoff>, Refusal> {
        let (hash, token) = (launch.hash, a.token);
        if self.holds(&token) {
            // R6: another launch transaction of a token bought on its own
            // launch is a CONFLICT, not the same launch seen again.
            return Err(
                match self
                    .bought_on
                    .iter()
                    .any(|(t, h)| *t == token && *h != hash)
                {
                    true => Refusal::TokenConflict,
                    false => Refusal::DuplicateToken,
                },
            );
        }
        let exposure = a.budget.max_total_wei;
        if self.spent_wei + exposure > U256::from(self.config.session_cap_wei) {
            return Err(Refusal::SessionCap);
        }
        // BSC3-072: on a chain ordered end its position is adopted once
        // included and exits through the strategy: the strategy's cap keeps
        // room for its exit (its venue's, B1) and for the exits of what is
        // open.
        let exits = self.exits_after_buy(c2.exit_reserve_wei, self.exit_gas_of(&a.intent)?);
        if E::CHAIN_ORDERED
            && self.config.sender_paths.strategy()
            && self.strategy_spent_wei + exits > U256::from(self.config.strategy_session_cap_wei)
        {
            return Err(Refusal::SessionCap);
        }
        if self.strategy_used > 0 && !self.strategy_skipped_first {
            return Err(Refusal::NonceTaken);
        }
        // BSC3-055: every C3 lane is at the arm's nonce; it goes only while
        // that is the sender's canonical next nonce.
        if E::CHAIN_ORDERED && c2.next_nonce != Some(self.first_nonce) {
            return Err(Refusal::SenderNonce);
        }
        let index = self.used;
        let Some(lane) = self.lanes.get_mut(index) else {
            return Err(Refusal::LanesExhausted);
        };
        // The lane is consumed by this attempt, whatever happens next.
        self.used += 1;
        let ticket = lane
            .reserve(Address::from(token))
            .map_err(|_| Refusal::Lane)?;
        // The token is taken at the reservation, as the production lane
        // burns it there: a replacement launch of it is refused.
        self.bought.push(token);
        self.bought_on.push((token, hash));
        let expected = self.expect_buy(&a.intent, &a.policy, ticket.nonce())?;
        let signed_ns = clock.now_ns();
        let (handoff, prepared_ns, planned_ns, (admitted, disclosed)) = to_end(
            Parts {
                end: &mut self.end,
                lane: &mut self.lanes[index],
                config: &self.config,
                scratch: &mut self.scratch,
            },
            ticket,
            &self.raw,
            expected.tx(),
            &a.budget,
            exits,
            &a.snapshot,
            a.head,
            clock,
            launch.ingress_ns,
            Submission::LaunchBundle(launch.raw),
            None,
        );
        // The exposure is the session's once the economic admission was
        // consumed (`begin_send`), whatever followed.
        if admitted {
            self.spent_wei += exposure;
        }
        self.c3_took_nonce |= disclosed;
        let handoff = handoff?;
        Ok(ExecBuy {
            launch_hash: hash,
            token,
            handoff,
            times: StageTimes {
                ingress_ns: launch.ingress_ns,
                decoded_ns: a.decoded_ns,
                admitted_ns: a.admitted_ns,
                signed_ns,
                prepared_ns,
                planned_ns,
                sink_ns: handoff.at_ns(),
            },
            raw: self.raw.clone(),
        })
    }

    /// A5: the expected BUY of `admitted` under `policy` at `nonce`, bound
    /// to its venue's deployment and signed into the raw buffer: the one
    /// intent + nonce to transaction step of C3 and Strategy C.
    fn expect_buy(
        &mut self,
        admitted: &BuyAdmitted,
        policy: &BuyPolicy,
        nonce: u64,
    ) -> Result<ExpectedBuyTx, Refusal> {
        let sender = Address::from(self.end.address());
        let expected = match admitted {
            BuyAdmitted::Flap(a) => {
                ExpectedBuy::from_admitted(&self.deployment, a, policy, sender, nonce)
                    .map(ExpectedBuyTx::Flap)
                    .map_err(|_| Refusal::Intent)?
            }
            BuyAdmitted::Classic(a) => {
                let arm = self.four.as_ref().ok_or(Refusal::Intent)?;
                ExpectedClassicBuy::from_admitted(&arm.validated, a, policy, sender, nonce)
                    .map(ExpectedBuyTx::Classic)
                    .map_err(|_| Refusal::Intent)?
            }
            BuyAdmitted::OpenFour(a) => {
                let arm = self.openfour.as_ref().ok_or(Refusal::Intent)?;
                ExpectedOpenFourBuy::from_admitted(&arm.validated, a, policy, sender, nonce)
                    .map(ExpectedBuyTx::OpenFour)
                    .map_err(|_| Refusal::Intent)?
            }
        };
        let (signer, scratch, raw) = (self.end.signer(), &mut self.scratch, &mut self.raw);
        let signed = match &expected {
            ExpectedBuyTx::Flap(e) => e.sign(signer, scratch, raw).is_ok(),
            ExpectedBuyTx::Classic(e) => e.sign(signer, scratch, raw).is_ok(),
            ExpectedBuyTx::OpenFour(e) => e.sign(signer, scratch, raw).is_ok(),
        };
        match signed {
            true => Ok(expected),
            false => Err(Refusal::Build),
        }
    }

    /// Phase I: a Strategy C PASS on `token` (`decided_ns`: the decision
    /// record's time, kept with the intent): a BUY of `input_wei` on the
    /// strategy's exact curve, through admission, the PAPER signature, the
    /// next strategy lane and the single-raw plan to the sink. The
    /// transport start deadline runs from now, when the decision is
    /// processed (gate H1), not from the record.
    #[allow(clippy::too_many_arguments)]
    pub fn on_strategy_buy(
        &mut self,
        token: [u8; 20],
        creator: [u8; 20],
        curve: StrategyCurve,
        input_wei: U256,
        c2: &CanonicalContext<'_>,
        clock: &dyn Clock,
        decided_ns: u64,
    ) -> Result<ExecIntent<E::Handoff>, Refusal> {
        self.strategy_counters.launches += 1;
        let start_ns = clock.now_ns();
        let result = self.strategy_buy(
            token, creator, curve, input_wei, c2, clock, decided_ns, start_ns,
        );
        match &result {
            Ok(_) => self.strategy_counters.handoffs += 1,
            Err(r) => self.strategy_counters.refuse(*r),
        }
        result
    }

    /// Phase I: a Strategy C exit trigger on `token`: the approval of the
    /// Portal and the native-output SELL on the position's shadow curve,
    /// at the next two strategy nonces. Only for a BUY of the token this
    /// session handed off (gate M1); the SELL sells at most that BUY's
    /// bound minimum output, the tokens it is sure to hold. The intents
    /// that reached the sink are returned even when a later one was
    /// refused (gate L3), with that refusal. A chain ordered end hands over
    /// the approval alone (no refusal: the exit is not over, see
    /// [`Executor::exited`]); the exit again, once the approval is
    /// canonical, hands over the SELL (BSC3-055).
    pub fn on_strategy_exit(
        &mut self,
        token: [u8; 20],
        tokens: U256,
        curve: StrategyCurve,
        c2: &CanonicalContext<'_>,
        clock: &dyn Clock,
        triggered_ns: u64,
    ) -> (Vec<ExecIntent<E::Handoff>>, Option<Refusal>) {
        self.strategy_counters.launches += 1;
        let start_ns = clock.now_ns();
        let mut out = Vec::with_capacity(2);
        let refused = self
            .strategy_exit(
                token,
                tokens,
                ExitSale::Curve(curve),
                c2,
                clock,
                triggered_ns,
                start_ns,
                &mut out,
            )
            .err();
        self.strategy_counters.handoffs += out.len() as u64;
        if let Some(r) = refused {
            self.strategy_counters.refuse(r);
        }
        (out, refused)
    }

    /// REV.1.0 R5 (BSC3-030): the exit of a position whose token migrated
    /// to its DEX, as [`Executor::on_strategy_exit`]: the SELL's expected
    /// output the Portal's quote `quoted` of `tokens` (its minimum under
    /// the exit slippage); the Portal routes it through its DEX router.
    pub fn on_dex_exit(
        &mut self,
        token: [u8; 20],
        tokens: U256,
        quoted: U256,
        c2: &CanonicalContext<'_>,
        clock: &dyn Clock,
    ) -> (Vec<ExecIntent<E::Handoff>>, Option<Refusal>) {
        self.strategy_counters.launches += 1;
        let start_ns = clock.now_ns();
        let mut out = Vec::with_capacity(2);
        let refused = self
            .strategy_exit(
                token,
                tokens,
                ExitSale::Quoted(quoted),
                c2,
                clock,
                start_ns,
                start_ns,
                &mut out,
            )
            .err();
        self.strategy_counters.handoffs += out.len() as u64;
        if let Some(r) = refused {
            self.strategy_counters.refuse(r);
        }
        (out, refused)
    }

    /// REV.1.0 R3b: the operator's nonce burn at C2's clean next nonce (the
    /// nonce of an unresolved transaction whose bytes may still land): the
    /// sender's plain transfer of nothing to itself on the next strategy
    /// lane, handed to the end as an exit step is (the owner admits it;
    /// its bundle, window and facts). Its gas counts in the session cap.
    pub fn on_burn(
        &mut self,
        c2: &CanonicalContext<'_>,
        clock: &dyn Clock,
    ) -> (Option<ExecIntent<E::Handoff>>, Option<Refusal>) {
        let start_ns = clock.now_ns();
        match self.burn(c2, clock, start_ns) {
            Ok(intent) => {
                self.strategy_counters.handoffs += 1;
                (Some(intent), None)
            }
            Err(r) => {
                self.strategy_counters.refuse(r);
                (None, Some(r))
            }
        }
    }

    fn burn(
        &mut self,
        c2: &CanonicalContext<'_>,
        clock: &dyn Clock,
        start_ns: u64,
    ) -> Result<ExecIntent<E::Handoff>, Refusal> {
        if !self.config.sender_paths.strategy() {
            return Err(Refusal::PathOff);
        }
        let snapshot = (c2.snapshot)(
            self.config.policy_generation,
            self.deployment.codehash_generation(),
        )
        .ok_or(Refusal::NoSnapshot)?;
        let head = c2.fresh_head(self.config.strategy_max_head_age_ms)?;
        let gas = U256::from(self.config.gas_price_wei) * U256::from(TRANSFER_GAS);
        if self.strategy_spent_wei + gas > U256::from(self.config.strategy_session_cap_wei) {
            return Err(Refusal::SessionCap);
        }
        if self.strategy_lanes.len() == self.strategy_used {
            return Err(Refusal::LanesExhausted);
        }
        let index = self.next_strategy_lane(c2.next_nonce)?;
        let ticket = match self.strategy_lanes[index].reserve(Address::ZERO) {
            Ok(t) => t,
            Err(_) => {
                self.strategy_refused(index, true);
                return Err(Refusal::Lane);
            }
        };
        let admitted_ns = clock.now_ns();
        let expected = ExpectedBurn::new(
            self.deployment.deployment().chain_id,
            Address::from(self.end.address()),
            ticket.nonce(),
            self.config.gas_price_wei,
            snapshot,
        );
        if expected
            .sign(self.end.signer(), &mut self.scratch, &mut self.raw)
            .is_err()
        {
            self.strategy_refused(index, false);
            return Err(Refusal::Build);
        }
        let signed_ns = clock.now_ns();
        let budget = BudgetEnvelope {
            max_buy_input_wei: U256::ZERO,
            max_buy_gas_wei: gas,
            max_own_launch_wei: U256::ZERO,
            max_provider_payment_wei: U256::ZERO,
            max_total_wei: gas,
        };
        let (handoff, prepared_ns, planned_ns, (_, disclosed)) = to_end(
            Parts {
                end: &mut self.end,
                lane: &mut self.strategy_lanes[index],
                config: &self.config,
                scratch: &mut self.scratch,
            },
            ticket,
            &self.raw,
            ExpectedTx::Burn(&expected),
            &budget,
            U256::ZERO,
            &snapshot,
            head,
            clock,
            start_ns,
            Submission::Single,
            None,
        );
        if disclosed {
            self.strategy_spent_wei += gas;
        }
        let handoff = match handoff {
            Ok(h) => h,
            Err(r) => {
                self.strategy_refused(index, disclosed);
                return Err(r);
            }
        };
        Ok(ExecIntent {
            kind: IntentKind::Burn,
            token: [0; 20],
            block: head.number,
            handoff,
            times: StageTimes {
                ingress_ns: start_ns,
                decoded_ns: start_ns,
                admitted_ns,
                signed_ns,
                prepared_ns,
                planned_ns,
                sink_ns: handoff.at_ns(),
            },
            raw: self.raw.clone(),
        })
    }

    /// Whether this arm holds `token` already, by either strategy: one
    /// sender buys a token once (BSC3-055, Unit 4 invariant 2).
    fn holds(&self, token: &[u8; 20]) -> bool {
        self.bought.contains(token) || self.strategy_bought.iter().any(|h| h.token == *token)
    }

    /// One position's exit at the signed gas price: its approval's gas and
    /// its SELL's.
    /// The Flap exit's gas limits (approval, SELL).
    fn flap_exit_gas(&self) -> (u64, u64) {
        (self.config.approve_gas_limit, self.config.sell_gas_limit)
    }

    /// SC4 d: the exit gas limits of a lot on `venue` (its family's
    /// configured ones; one not configured is never armed, its lot at
    /// Flap's).
    fn venue_exit_gas(&self, venue: crate::sources::VenueId) -> (u64, u64) {
        let (_, _, approve, sell) = self.config.on_venue(venue);
        (approve, sell)
    }

    /// SC4 c (B1): the exit gas limits of an admitted BUY's venue; a Four
    /// BUY is admitted only on its venue's arm.
    fn exit_gas_of(&self, admitted: &BuyAdmitted) -> Result<(u64, u64), Refusal> {
        match admitted {
            BuyAdmitted::Flap(_) => Ok(self.flap_exit_gas()),
            BuyAdmitted::Classic(_) => self.four.map(|a| a.exit_gas).ok_or(Refusal::VenueUnarmed),
            BuyAdmitted::OpenFour(_) => self
                .openfour
                .map(|a| a.exit_gas)
                .ok_or(Refusal::VenueUnarmed),
        }
    }

    /// The wei of `gas` at the arm's signing price.
    fn wei_of_gas(&self, gas: u128) -> U256 {
        U256::from(self.config.gas_price_wei) * U256::from(gas)
    }

    /// The most the exits of what this arm holds for the strategy may still
    /// cost: per position not sold its SELL and, unless approved, its
    /// approval, each at its venue's limits (a C3 position is the
    /// strategy's once adopted).
    fn held_exit_reserve(&self) -> U256 {
        self.wei_of_gas(self.open_exits.sell_gas + self.open_exits.approve_gas)
    }

    /// The reserve as a scan of every position (the counted one's
    /// reference).
    #[cfg(test)]
    fn held_exit_reserve_scan(&self) -> U256 {
        let gas = self
            .strategy_bought
            .iter()
            .filter(|h| !self.exited.contains(&h.token))
            .map(|h| {
                let (approval, sell) = (u128::from(h.exit_gas.0), u128::from(h.exit_gas.1));
                match self.approved.iter().any(|(a, _)| *a == h.token) {
                    true => sell,
                    false => approval + sell,
                }
            })
            .sum();
        self.wei_of_gas(gas)
    }

    /// BSC3-085: whether `token` is a held position still to exit, and
    /// whether it is approved too.
    fn exit_state(&self, token: &[u8; 20]) -> (bool, bool) {
        let open =
            !self.exited.contains(token) && self.strategy_bought.iter().any(|h| h.token == *token);
        (open, open && self.approved.iter().any(|(t, _)| t == token))
    }

    /// BSC3-085: counts the change of `token`'s exit state from `before`.
    fn recount(&mut self, token: &[u8; 20], before: (bool, bool)) {
        let after = self.exit_state(token);
        let n = |b: bool| u64::from(b);
        let (approve, sell) = self
            .strategy_bought
            .iter()
            .find(|h| h.token == *token)
            .map_or((0, 0), |h| {
                (u128::from(h.exit_gas.0), u128::from(h.exit_gas.1))
            });
        // The gas still owed: the SELL while open, the approval while open
        // and not approved.
        let owed = |(open, approved): (bool, bool)| {
            (
                if open { sell } else { 0 },
                if open && !approved { approve } else { 0 },
            )
        };
        let (was, now) = (owed(before), owed(after));
        let o = &mut self.open_exits;
        o.open = o.open + n(after.0) - n(before.0);
        o.approved = o.approved + n(after.1) - n(before.1);
        o.sell_gas = o.sell_gas + now.0 - was.0;
        o.approve_gas = o.approve_gas + now.1 - was.1;
        #[cfg(test)]
        assert_eq!(self.held_exit_reserve(), self.held_exit_reserve_scan());
    }

    /// What a BUY leaves room for (BSC3-057/072): the exits of the open
    /// positions (`known`, the end's ledger's; else what this arm holds)
    /// and its own.
    fn exits_after_buy(&self, known: Option<U256>, exit_gas: (u64, u64)) -> U256 {
        let new = u128::from(exit_gas.0) + u128::from(exit_gas.1);
        known.unwrap_or_else(|| self.held_exit_reserve()) + self.wei_of_gas(new)
    }

    /// What an exit of `tokens` sells: at most what the handed-off BUY is
    /// bound to have bought and what an approval out approved.
    fn sellable(tokens: U256, held: Held, approved: Option<U256>) -> U256 {
        let amount = tokens.min(held.min_tokens);
        approved.map_or(amount, |a| amount.min(a))
    }

    /// REV.1.0 R5: what an exit of `token`'s `tokens` sells now (none: not
    /// held), the amount a DEX exit's quote is of.
    pub fn exit_amount(&self, token: [u8; 20], tokens: U256) -> Option<U256> {
        let held = self.strategy_bought.iter().find(|h| h.token == token)?;
        let approved = self
            .approved
            .iter()
            .find(|(t, _)| *t == token)
            .map(|(_, amount)| *amount);
        Some(Self::sellable(tokens, *held, approved))
    }

    /// Whether a C3 BUY of this arm was disclosed at the arm's nonce.
    pub fn c3_took_nonce(&self) -> bool {
        self.c3_took_nonce
    }

    /// The arm's nonce: every C3 lane's.
    pub fn c3_nonce(&self) -> u64 {
        self.first_nonce
    }

    /// The nonce the next strategy lane is at.
    pub fn strategy_next_nonce(&self) -> u64 {
        let skip = self.strategy_used == 0 && self.c3_took_nonce;
        self.strategy_first_nonce + (self.strategy_used + usize::from(skip)) as u64
    }

    /// Whether this arm's SELL of `token` may have left (its exit is over
    /// for this arm).
    pub fn exited(&self, token: &[u8; 20]) -> bool {
        self.exited.contains(token)
    }

    /// An approval of `token` for `amount` the chain included before this
    /// arm (BSC3-048): the exit sells without approving again.
    pub fn adopt_approval(&mut self, token: [u8; 20], amount: U256) {
        if !self.approved.iter().any(|(t, _)| *t == token) {
            let before = self.exit_state(&token);
            self.approved.push((token, amount));
            self.recount(&token, before);
        }
    }

    /// Another transaction took the nonce of this arm's exit transaction
    /// of `token` (BSC3-048): it never happened, the exit is open again.
    /// A replaced approval approved nothing.
    pub fn reopen_exit(&mut self, token: [u8; 20], approval: bool) {
        let before = self.exit_state(&token);
        self.exited.retain(|t| *t != token);
        // Open again: its input is open exposure again (BSC3-030 review).
        if let Some(h) = self.strategy_bought.iter_mut().find(|h| h.token == token)
            && h.released
        {
            self.strategy_spent_wei = self.strategy_spent_wei.saturating_add(h.input_wei);
            h.released = false;
        }
        if approval {
            self.approved.retain(|(t, _)| *t != token);
        }
        self.recount(&token, before);
    }

    /// Tokens whose exit's approval went and whose SELL did not (a chain
    /// ordered end's SELL waits for its approval to be canonical).
    pub fn exits_awaiting_sell(&self) -> Vec<[u8; 20]> {
        self.approved
            .iter()
            .map(|(t, _)| *t)
            .filter(|t| !self.exited.contains(t))
            .collect()
    }

    /// The next strategy lane: its nonce is the next of the PAPER
    /// sender's sequence (each lane one nonce above the last). Refused
    /// once a taken nonce was not handed off (a hole no later PAPER
    /// transaction could pass on chain). An end whose transactions reach
    /// the chain takes it only at the sender's canonical next nonce
    /// `canonical`; refused otherwise with nothing taken (BSC3-055).
    fn next_strategy_lane(&mut self, canonical: Option<u64>) -> Result<usize, Refusal> {
        if self.strategy_nonce_gap {
            return Err(Refusal::NonceGap);
        }
        if E::CHAIN_ORDERED {
            let next = self.strategy_next_nonce();
            let lane_left = next - self.strategy_first_nonce < self.strategy_lanes.len() as u64;
            if lane_left && canonical != Some(next) {
                return Err(Refusal::SenderNonce);
            }
        }
        if self.strategy_used == 0 && self.c3_took_nonce {
            self.strategy_used = 1;
            self.strategy_skipped_first = true;
        }
        if self.strategy_used >= self.strategy_lanes.len() {
            return Err(Refusal::LanesExhausted);
        }
        self.strategy_used += 1;
        Ok(self.strategy_used - 1)
    }

    /// A strategy intent refused with the lanes from `from` taken (gate
    /// N1). Nothing of it disclosed (no permit's write started): their
    /// nonces are free again, the lanes re-armed fresh behind the same
    /// fence. Its bytes may be out, or a lane broke: a hole, the
    /// strategy's intents stop.
    fn strategy_refused(&mut self, from: usize, disclosed: bool) {
        if disclosed {
            self.strategy_nonce_gap = true;
            return;
        }
        let chain_id = self.deployment.deployment().chain_id;
        let sender = Address::from(self.end.address());
        for index in from..self.strategy_used {
            let nonce = self.strategy_first_nonce + index as u64;
            match fenced_lane(
                chain_id,
                sender,
                nonce,
                self.config.strategy_session_cap_wei,
            ) {
                Ok(lane) => self.strategy_lanes[index] = lane,
                Err(_) => {
                    self.strategy_nonce_gap = true;
                    return;
                }
            }
        }
        self.strategy_used = from;
    }

    #[allow(clippy::too_many_arguments)]
    fn strategy_buy(
        &mut self,
        token: [u8; 20],
        creator: [u8; 20],
        curve: StrategyCurve,
        input_wei: U256,
        c2: &CanonicalContext<'_>,
        clock: &dyn Clock,
        decided_ns: u64,
        start_ns: u64,
    ) -> Result<ExecIntent<E::Handoff>, Refusal> {
        if !self.config.sender_paths.strategy() {
            return Err(Refusal::PathOff);
        }
        // SC4: a Four.meme curve's BUY is admitted on its venue's arm: its
        // code generation, its BUY slippage and gas limit.
        let four = match curve {
            StrategyCurve::Flap { .. } => None,
            StrategyCurve::Four(FourCurve::Classic { .. }) => {
                Some(self.four.ok_or(Refusal::VenueUnarmed)?)
            }
            StrategyCurve::Four(FourCurve::OpenFour { .. }) => {
                Some(self.openfour.ok_or(Refusal::VenueUnarmed)?)
            }
        };
        let (slippage_bps, gas_limit, generation) = match four {
            Some(arm) => (
                arm.policy.slippage_bps(),
                arm.policy.gas_limit(),
                arm.validated.codehash_generation(),
            ),
            None => (
                self.config.slippage_bps,
                self.config.gas_limit,
                self.deployment.codehash_generation(),
            ),
        };
        let policy = BuyPolicy::new(
            self.config.policy_generation,
            input_wei,
            slippage_bps,
            self.config.gas_price_wei,
            gas_limit,
        )
        .map_err(|_| Refusal::Admission)?;
        let snapshot =
            (c2.snapshot)(self.config.policy_generation, generation).ok_or(Refusal::NoSnapshot)?;
        let head = c2.fresh_head(self.config.strategy_max_head_age_ms)?;
        let at = Address::from(token);
        let admitted = match (curve, four) {
            (
                StrategyCurve::Flap {
                    terms,
                    reserve,
                    circulating,
                },
                _,
            ) => BuyAdmitted::Flap(
                CurveState::tracked(at, terms, reserve, circulating, &self.terms, snapshot)
                    .map_err(|_| Refusal::Admission)
                    .and_then(|s| admit_curve_buy(&s, &policy).map_err(|_| Refusal::Admission))?,
            ),
            (StrategyCurve::Four(FourCurve::Classic { now, fee, .. }), Some(arm)) => {
                BuyAdmitted::Classic(
                    QualifiedClassicState::tracked(&arm.validated, at, now, fee, snapshot)
                        .ok()
                        .and_then(|s| admit_classic_buy(&s, &policy).ok())
                        .ok_or(Refusal::Admission)?,
                )
            }
            (StrategyCurve::Four(FourCurve::OpenFour { now, fee, .. }), Some(arm)) => {
                BuyAdmitted::OpenFour(
                    QualifiedOpenFourState::tracked(&arm.validated, at, now, fee, snapshot)
                        .ok()
                        .and_then(|s| admit_openfour_buy(&s, &policy).ok())
                        .ok_or(Refusal::Admission)?,
                )
            }
            (StrategyCurve::Four(_), None) => return Err(Refusal::VenueUnarmed),
        };
        let min_tokens = admitted.min_tokens();
        let exit_gas = self.exit_gas_of(&admitted)?;
        let admitted_ns = clock.now_ns();
        if self.holds(&token) {
            return Err(Refusal::DuplicateToken);
        }
        let gas = U256::from(policy.gas_price()) * U256::from(policy.gas_limit());
        let budget = BudgetEnvelope {
            max_buy_input_wei: input_wei,
            max_buy_gas_wei: gas,
            max_own_launch_wei: U256::ZERO,
            max_provider_payment_wei: U256::ZERO,
            max_total_wei: input_wei + gas,
        };
        // BSC3-072: the cap keeps room for its exit (its venue's, B1) and
        // for the exits of what is open.
        let exits = self.exits_after_buy(c2.exit_reserve_wei, exit_gas);
        if self.strategy_spent_wei + budget.max_total_wei + exits
            > U256::from(self.config.strategy_session_cap_wei)
        {
            return Err(Refusal::SessionCap);
        }
        // A chain ordered end keeps a lane for every SELL its approval
        // waits for.
        if E::CHAIN_ORDERED {
            let taken = self.strategy_next_nonce() - self.strategy_first_nonce;
            let left = (self.strategy_lanes.len() as u64).saturating_sub(taken);
            if left <= self.exits_awaiting_sell().len() as u64 {
                return Err(Refusal::LanesExhausted);
            }
        }
        let index = self.next_strategy_lane(c2.next_nonce)?;
        let Ok(ticket) = self.strategy_lanes[index].reserve(admitted.token()) else {
            self.strategy_refused(index, true);
            return Err(Refusal::Lane);
        };
        let expected = match self.expect_buy(&admitted, &policy, ticket.nonce()) {
            Ok(e) => e,
            Err(r) => {
                self.strategy_refused(index, false);
                return Err(r);
            }
        };
        let signed_ns = clock.now_ns();
        let (handoff, prepared_ns, planned_ns, (_, disclosed)) = to_end(
            Parts {
                end: &mut self.end,
                lane: &mut self.strategy_lanes[index],
                config: &self.config,
                scratch: &mut self.scratch,
            },
            ticket,
            &self.raw,
            expected.tx(),
            &budget,
            exits,
            &snapshot,
            head,
            clock,
            start_ns,
            Submission::Single,
            (creator != [0; 20]).then_some(creator),
        );
        // Gate L2: the exposure counts once the BUY may be out; a nonce
        // taken again (nothing disclosed) spends nothing.
        if disclosed {
            self.strategy_spent_wei += budget.max_total_wei;
        }
        let handoff = match handoff {
            Ok(h) => h,
            Err(r) => {
                self.strategy_refused(index, disclosed);
                return Err(r);
            }
        };
        let before = self.exit_state(&token);
        self.strategy_bought.push(Held {
            released: false,
            token,
            min_tokens,
            input_wei,
            exit_gas,
        });
        self.recount(&token, before);
        Ok(ExecIntent {
            kind: IntentKind::Buy,
            token,
            block: head.number,
            handoff,
            times: StageTimes {
                ingress_ns: decided_ns,
                decoded_ns: start_ns,
                admitted_ns,
                signed_ns,
                prepared_ns,
                planned_ns,
                sink_ns: handoff.at_ns(),
            },
            raw: self.raw.clone(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn strategy_exit(
        &mut self,
        token: [u8; 20],
        tokens: U256,
        sale: ExitSale,
        c2: &CanonicalContext<'_>,
        clock: &dyn Clock,
        triggered_ns: u64,
        start_ns: u64,
        out: &mut Vec<ExecIntent<E::Handoff>>,
    ) -> Result<(), Refusal> {
        if !self.config.sender_paths.strategy() {
            return Err(Refusal::PathOff);
        }
        let held = self
            .strategy_bought
            .iter()
            .find(|h| h.token == token)
            .copied()
            .ok_or(Refusal::NotHeld)?;
        if self.exited.contains(&token) {
            return Err(Refusal::DuplicateToken);
        }
        // Gate L2: an approval already out (its SELL refused before) is not
        // signed again; the SELL sells at most what it approved.
        let approved = self
            .approved
            .iter()
            .find(|(t, _)| *t == token)
            .map(|(_, amount)| *amount);
        // SC4: a Four.meme position exits on its venue's armed deployment.
        let four = match sale {
            ExitSale::Curve(StrategyCurve::Four(FourCurve::Classic { .. })) => {
                Some(self.four.ok_or(Refusal::VenueUnarmed)?)
            }
            ExitSale::Curve(StrategyCurve::Four(FourCurve::OpenFour { .. })) => {
                Some(self.openfour.ok_or(Refusal::VenueUnarmed)?)
            }
            ExitSale::Curve(StrategyCurve::Flap { .. }) | ExitSale::Quoted(_) => None,
        };
        let snapshot = (c2.snapshot)(
            self.config.policy_generation,
            four.map_or(self.deployment.codehash_generation(), |a| {
                a.validated.codehash_generation()
            }),
        )
        .ok_or(Refusal::NoSnapshot)?;
        let head = c2.fresh_head(self.config.strategy_max_head_age_ms)?;
        let amount = match sale {
            // Whole gwei units, as every witnessed Classic trade's.
            ExitSale::Curve(StrategyCurve::Four(FourCurve::Classic { .. })) => {
                floor_gwei(Self::sellable(tokens, held, approved))
            }
            _ => Self::sellable(tokens, held, approved),
        };
        // R5: a re-priced exit is signed and budgeted at its own price; B1:
        // at its venue's gas limits.
        let price = c2.exit_gas_price_wei.unwrap_or(self.config.gas_price_wei);
        let (approve_limit, sell_limit) = held.exit_gas;
        let exit_policy =
            match price == self.config.gas_price_wei && held.exit_gas == self.flap_exit_gas() {
                true => self.exit_policy,
                false => ExitPolicy::new(
                    self.config.policy_generation,
                    self.config.exit_slippage_bps,
                    price,
                    approve_limit,
                    sell_limit,
                )
                .map_err(|_| Refusal::Intent)?,
            };
        let at = Address::from(token);
        let admitted = match sale {
            ExitSale::Curve(StrategyCurve::Four(curve)) => {
                let arm = four.ok_or(Refusal::VenueUnarmed)?;
                match curve {
                    FourCurve::Classic { now, fee, .. } => SellAdmitted::Classic(
                        QualifiedClassicHolding::tracked(&arm.validated, at, now, fee, snapshot)
                            .ok()
                            .and_then(|h| admit_classic_sell(&h, amount, &exit_policy).ok())
                            .ok_or(Refusal::Admission)?,
                    ),
                    FourCurve::OpenFour { now, fee, .. } => SellAdmitted::OpenFour(
                        QualifiedOpenFourHolding::tracked(&arm.validated, at, now, fee, snapshot)
                            .ok()
                            .and_then(|h| admit_openfour_sell(&h, amount, &exit_policy).ok())
                            .ok_or(Refusal::Admission)?,
                    ),
                }
            }
            ExitSale::Curve(StrategyCurve::Flap {
                terms,
                reserve,
                circulating,
            }) => SellAdmitted::Curve(
                CurveState::tracked(at, terms, reserve, circulating, &self.terms, snapshot)
                    .map_err(|_| Refusal::Admission)
                    .and_then(|state| {
                        admit_curve_sell(&state, amount, &exit_policy)
                            .map_err(|_| Refusal::Admission)
                    })?,
            ),
            // R5: a migrated token: the Portal's quote at the snapshot, of
            // exactly the tokens sold (else its minimum is another amount's).
            ExitSale::Quoted(_) if amount != tokens => return Err(Refusal::Admission),
            ExitSale::Quoted(quoted) => SellAdmitted::Quoted(quoted),
        };
        let admitted_ns = clock.now_ns();
        let (approval_gas, sell_gas) = (
            U256::from(price) * U256::from(approve_limit),
            U256::from(price) * U256::from(sell_limit),
        );
        let approve_gas = if approved.is_some() {
            U256::ZERO
        } else {
            approval_gas
        };
        let budget = |gas: U256| BudgetEnvelope {
            max_buy_input_wei: U256::ZERO,
            max_buy_gas_wei: gas,
            max_own_launch_wei: U256::ZERO,
            max_provider_payment_wei: U256::ZERO,
            max_total_wei: gas,
        };
        if self.strategy_spent_wei + approve_gas + sell_gas
            > U256::from(self.config.strategy_session_cap_wei)
        {
            return Err(Refusal::SessionCap);
        }
        // BSC3-055: a chain ordered end's SELL waits for its approval to be
        // canonical; it comes back as an exit with the approval out.
        let defer_sell = E::CHAIN_ORDERED && approved.is_none();
        let needed = if approved.is_some() { 1 } else { 2 };
        if self.strategy_lanes.len() - self.strategy_used < needed {
            return Err(Refusal::LanesExhausted);
        }
        let approve_index = match approved {
            Some(_) => None,
            None => Some(self.next_strategy_lane(c2.next_nonce)?),
        };
        let sell_index = match defer_sell {
            true => None,
            false => Some(self.next_strategy_lane(c2.next_nonce)?),
        };
        let Some(first) = approve_index.or(sell_index) else {
            return Err(Refusal::Lane);
        };
        let mut tickets = [None, None];
        for (slot, index) in [approve_index, sell_index].into_iter().enumerate() {
            let Some(index) = index else { continue };
            match self.strategy_lanes[index].reserve(Address::from(token)) {
                Ok(t) => tickets[slot] = Some(t),
                Err(_) => {
                    self.strategy_refused(first, true);
                    return Err(Refusal::Lane);
                }
            }
        }
        let [approve_ticket, sell_ticket] = tickets;
        // The approval at n, the SELL at n+1 (a SELL alone: n is its own
        // nonce minus one, the approval already out; an approval alone: the
        // SELL comes at n+1 later).
        let base = match (approve_ticket, sell_ticket) {
            (Some(a), Some(s)) if s.nonce() == a.nonce() + 1 => Some(a.nonce()),
            (Some(a), None) => Some(a.nonce()),
            (None, Some(s)) => s.nonce().checked_sub(1),
            _ => None,
        };
        let Some(base) = base else {
            self.strategy_refused(first, true);
            return Err(Refusal::Intent);
        };
        let sender = Address::from(self.end.address());
        let expected = match admitted {
            SellAdmitted::Curve(admitted) => {
                ExpectedExit::from_admitted(&self.deployment, &admitted, &exit_policy, sender, base)
                    .map(ExitExpected::Flap)
                    .ok()
            }
            SellAdmitted::Quoted(quoted) => ExpectedExit::from_quote(
                &self.deployment,
                at,
                amount,
                quoted,
                &exit_policy,
                snapshot,
                sender,
                base,
            )
            .map(ExitExpected::Flap)
            .ok(),
            SellAdmitted::Classic(admitted) => four.and_then(|arm| {
                ExpectedClassicExit::from_admitted(
                    &arm.validated,
                    &admitted,
                    &exit_policy,
                    sender,
                    base,
                )
                .map(ExitExpected::Classic)
                .ok()
            }),
            SellAdmitted::OpenFour(admitted) => four.and_then(|arm| {
                ExpectedOpenFourExit::from_admitted(
                    &arm.validated,
                    &admitted,
                    &exit_policy,
                    sender,
                    base,
                )
                .map(ExitExpected::OpenFour)
                .ok()
            }),
        };
        let Some(expected) = expected else {
            self.strategy_refused(first, false);
            return Err(Refusal::Intent);
        };
        let steps = [
            approve_index
                .zip(approve_ticket)
                .map(|(i, t)| (IntentKind::Approve, i, t, approve_gas)),
            sell_index
                .zip(sell_ticket)
                .map(|(i, t)| (IntentKind::Sell, i, t, sell_gas)),
        ];
        for (kind, index, ticket, gas) in steps.into_iter().flatten() {
            if !expected.sign(kind, self.end.signer(), &mut self.scratch, &mut self.raw) {
                self.strategy_refused(index, false);
                return Err(Refusal::Build);
            }
            let signed_ns = clock.now_ns();
            let tx = expected.tx(kind);
            let (handoff, prepared_ns, planned_ns, (_, disclosed)) = to_end(
                Parts {
                    end: &mut self.end,
                    lane: &mut self.strategy_lanes[index],
                    config: &self.config,
                    scratch: &mut self.scratch,
                },
                ticket,
                &self.raw,
                tx,
                &budget(gas),
                U256::ZERO,
                &snapshot,
                head,
                clock,
                start_ns,
                Submission::Single,
                None,
            );
            // Gate L2: gas counts once something may be out; a nonce taken
            // again spends nothing.
            if disclosed {
                self.strategy_spent_wei += gas;
            }
            let handoff = match handoff {
                Ok(h) => h,
                Err(r) => {
                    self.strategy_refused(index, disclosed);
                    // A SELL that may be out ends the position's exits.
                    if kind == IntentKind::Sell && disclosed {
                        let before = self.exit_state(&token);
                        self.exited.push(token);
                        self.recount(&token, before);
                    }
                    return Err(r);
                }
            };
            if kind == IntentKind::Approve {
                let before = self.exit_state(&token);
                self.approved.push((token, amount));
                self.recount(&token, before);
            }
            out.push(ExecIntent {
                kind,
                token,
                block: head.number,
                handoff,
                times: StageTimes {
                    ingress_ns: triggered_ns,
                    decoded_ns: start_ns,
                    admitted_ns,
                    signed_ns,
                    prepared_ns,
                    planned_ns,
                    sink_ns: handoff.at_ns(),
                },
                raw: self.raw.clone(),
            });
        }
        if defer_sell {
            // The approval is out; its SELL waits for it to be canonical
            // (the exit again then hands it over).
            return Ok(());
        }
        let before = self.exit_state(&token);
        self.exited.push(token);
        self.recount(&token, before);
        // The position is closed: its input leaves the open exposure, the
        // gas spent stays (gate L1); once (a SELL again after a reverted or
        // replaced one releases nothing more, BSC3-030 review).
        if let Some(h) = self.strategy_bought.iter_mut().find(|h| h.token == token)
            && !h.released
        {
            self.strategy_spent_wei = self.strategy_spent_wei.saturating_sub(h.input_wei);
            h.released = true;
        }
        Ok(())
    }

    /// Applies C2's evidence about one of the sender's nonces to the lanes
    /// that hold that nonce (plan 28.10.): every C3 lane and the first
    /// strategy lane at the arming's nonce, strategy lane `i` at its own.
    /// An inclusion of the intent's own transaction makes it `Included` or
    /// `Reverted`, another transaction `Replaced` (the lane stops); an
    /// orphan rewinds and stops it. Returns the state an inclusion gave the
    /// intent that was dispatched, if one was.
    pub fn apply_evidence(&mut self, evidence: NonceEvidence<'_>) -> Option<IntentState> {
        let sender = Address::from(self.end.address());
        let (proof_sender, nonce) = match evidence {
            NonceEvidence::Inclusion(p) => (p.sender(), p.nonce()),
            NonceEvidence::Finality(p) => (p.sender(), p.nonce()),
            NonceEvidence::Orphan(p) => (p.sender(), p.nonce()),
        };
        if proof_sender != sender {
            return None;
        }
        let strategy = nonce
            .checked_sub(self.strategy_first_nonce)
            .and_then(|i| usize::try_from(i).ok())
            .filter(|i| *i < self.strategy_lanes.len());
        let c3 = nonce == self.first_nonce;
        if strategy.is_none() && !c3 {
            if nonce >= self.first_nonce {
                self.evidence.unassigned += 1;
            }
            return None;
        }
        let lanes = self
            .lanes
            .iter_mut()
            .filter(|_| c3)
            .chain(strategy.and_then(|i| self.strategy_lanes.get_mut(i)));
        let mut ours = None;
        let mut applied = false;
        for lane in lanes {
            match evidence {
                NonceEvidence::Inclusion(p) => match lane.apply_inclusion(p) {
                    Ok(s @ (IntentState::Included | IntentState::Reverted)) => ours = Some(s),
                    Ok(s) => {
                        applied = true;
                        ours = ours.or(Some(s));
                    }
                    Err(_) => {}
                },
                NonceEvidence::Finality(p) => applied |= lane.apply_finality(p).is_ok(),
                NonceEvidence::Orphan(p) => applied |= lane.apply_orphan(p).is_ok(),
            }
        }
        let e = &mut self.evidence;
        match (evidence, ours) {
            (NonceEvidence::Inclusion(_), Some(IntentState::Included)) => e.included += 1,
            (NonceEvidence::Inclusion(_), Some(IntentState::Reverted)) => e.reverted += 1,
            (NonceEvidence::Inclusion(_), _) if applied => e.replaced += 1,
            (NonceEvidence::Finality(_), _) if applied => e.finalized += 1,
            (NonceEvidence::Orphan(_), _) if applied => e.orphaned += 1,
            _ => {}
        }
        ours
    }

    /// What C2's evidence did to this arm's intents.
    pub fn evidence(&self) -> EvidenceCounters {
        self.evidence
    }

    /// Plan 28.10.: a real position the chain confirmed, `tokens` received:
    /// its exit sells at most these tokens, and a C3 position (or one from
    /// before a restart) becomes exitable the same way as a Strategy C one.
    /// Its input is not this arm's strategy exposure (no strategy BUY of
    /// this arm charged it): its SELL releases none (BSC3-072).
    pub fn adopt(&mut self, token: [u8; 20], tokens: U256, venue: crate::sources::VenueId) {
        match self.strategy_bought.iter_mut().find(|h| h.token == token) {
            Some(h) => h.min_tokens = tokens,
            None => {
                let before = self.exit_state(&token);
                // SC4 d: its exit at its venue's limits.
                let exit_gas = self.venue_exit_gas(venue);
                self.strategy_bought.push(Held {
                    released: false,
                    token,
                    min_tokens: tokens,
                    input_wei: U256::ZERO,
                    exit_gas,
                });
                self.recount(&token, before);
            }
        }
    }

    /// Strategy C's PAPER counters (Phase I): intents asked for, handed
    /// off, refused by stage.
    pub fn strategy_counters(&self) -> &ExecCounters {
        &self.strategy_counters
    }

    /// Strategy lanes armed and left, the next strategy nonce, the open
    /// exposure plus the gas spent, and whether a nonce hole stopped the
    /// strategy's intents.
    pub fn strategy_lanes(&self) -> (usize, usize, u64, U256, bool) {
        (
            self.strategy_lanes.len(),
            self.strategy_lanes.len() - self.strategy_used,
            self.strategy_first_nonce + self.strategy_used as u64,
            self.strategy_spent_wei,
            self.strategy_nonce_gap,
        )
    }

    pub fn sender(&self) -> [u8; 20] {
        self.end.address()
    }

    /// The Portal of the validated deployment this path is armed on.
    pub fn portal(&self) -> Address {
        self.deployment.deployment().portal
    }

    /// The end the path hands over to.
    pub fn end(&self) -> &E {
        &self.end
    }

    /// The providers' answers to what this arm's lanes released, read
    /// without waiting (BSC3-051): outside any hand-over.
    pub fn poll_responses(&mut self, clock: &dyn Clock) {
        for lane in self.lanes.iter_mut().chain(self.strategy_lanes.iter_mut()) {
            self.end.poll_responses(lane, clock);
        }
    }

    /// The end, for its maintenance outside any hand-over.
    pub fn end_mut(&mut self) -> &mut E {
        &mut self.end
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

    /// Exposure dispatched in this session.
    pub fn spent_wei(&self) -> U256 {
        self.spent_wei
    }

    /// No further BUY can be admitted: every lane is used, or the session
    /// cap has no room for one more BUY.
    pub fn exhausted(&self) -> bool {
        self.used == self.lanes.len()
            || self.spent_wei + self.budget.max_total_wei > U256::from(self.config.session_cap_wei)
    }

    pub fn fence(&self) -> &Path {
        &self.fence
    }
}

#[cfg(test)]
#[path = "paper_exec_tests.rs"]
pub(crate) mod tests;
