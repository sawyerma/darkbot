//! The LIVE end of the one executor (plan 28.10., package 4).
//!
//! The buyer's [`LiveKey`] signs. Each prepared transaction is planned
//! under its release profile through the granted builders (C3: the birth
//! entry bundle behind the original launch; Strategy C: the transaction
//! alone, a one-transaction bundle), qualified for release with the
//! verified LIVE qualification, framed by the builders' own framing, and
//! only then admitted by the native owner as it holds at that moment
//! ([`Store::admit_live`]: the switches and the gate, a BUY's daily and
//! campaign slot, the grant's budget, all durable before any byte can
//! leave). The release dispatch sends it over qualified transports.
//!
//! Nothing is retried. The owner's admission names the transaction's token
//! and nonce and is durable before any byte can leave (its write-ahead
//! record, BSC3-024); right after the dispatch its outcome is a fact of the
//! LIVE ledger: released (handed off), UNKNOWN (its handoff did not
//! complete) or withdrawn (no byte left). The executor stops the sender's
//! sequence on an UNKNOWN. The route is the builders' TLS connections or,
//! for a non-sending run, the loopback adapter on 127.0.0.1.

use crate::live_key::LiveKey;
use crate::live_ledger::{Fact, FactKind, Ledger};
use crate::live_qualification::{Verified, capability};
use crate::paper_exec::{End, Handed, HandoffTime, Handover, Refusal, Submission, consumed_by};
use crate::sources::{ComponentId, LiveAdapter, LiveIntent, LiveIntentKind, LiveNow, Store};
use crate::submission::frame::Frame;
use crate::submission::loopback::{LoopbackTransport, qualify_loopback};
use crate::submission::send::{
    DispatchContext, FanoutReport, ReleaseArmed, SendOutcome, dispatch_release,
};
use crate::submission::tls::{
    AuthorizationToken, BLOCKRAZOR_DOCUMENTED_HOST, CLUB48_DOCUMENTED_HOST, HTTPS_PORT,
    ProviderTarget, Q5A_TLS_POLICY, TlsConnectionOwner, TlsEndpoint, TlsSetup, qualify_transport,
};
use crate::submission::transport::TransportQualifications;
use crate::submission::{blockrazor, club48};
use alloy_primitives::U256;
use darkbot_bsc_trading::intent::{AdapterId, AdapterSet};
use darkbot_bsc_trading::signer::Signer;
use darkbot_bsc_trading::submission_policy::capability::{
    AdapterBinding, AdapterTable, SubmissionMode,
};
use darkbot_bsc_trading::submission_policy::plan::{
    Launch, LocalBlockStartDeadline, MergePolicy, PlanInputs, PreSubmission, ProviderValidity,
    SubmissionPlan, TransportStartDeadline, TxRef,
};
use darkbot_bsc_trading::submission_policy::release::ReleaseProfile;
use darkbot_bsc_trading::venues::flap::binding::ExpectedTx;
use std::time::Duration;

/// Where the LIVE end's requests go.
pub enum LiveRoute {
    /// The granted builders over their qualified TLS connections; the
    /// BlockRazor builder needs its authorization.
    Builders {
        setup: TlsSetup,
        blockrazor: Option<AuthorizationToken>,
    },
    /// The builders' frames to a stand-in on 127.0.0.1:`port`: nothing
    /// leaves the host.
    Loopback { port: u16 },
}

enum Link {
    Tls(Box<TlsConnectionOwner>),
    Loopback(LoopbackTransport),
}

struct Slot {
    adapter: AdapterId,
    live: LiveAdapter,
    target: ProviderTarget,
    link: Link,
}

/// When the release took a transaction: the first qualified handoff
/// boundary among the granted builders, and how many took it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveHandoff {
    pub tx_hash: [u8; 32],
    pub at_ns: u64,
    pub adapters: u8,
}

impl HandoffTime for LiveHandoff {
    fn at_ns(&self) -> u64 {
        self.at_ns
    }
}

/// One LIVE transaction whose bytes may have left: what reconciliation
/// (responses, receipts, inclusion) resolves. Never sent again. Its frames
/// (they may carry an authorization) never show in `Debug`.
pub struct Released {
    pub tx_hash: [u8; 32],
    pub kind: LiveIntentKind,
    pub component: ComponentId,
    pub token: [u8; 20],
    pub nonce: u64,
    /// The signed native value, and with the gas limit at the signed price
    /// the most it can cost.
    pub input_wei: U256,
    pub max_wei: U256,
    /// Whether a builder took it whole; otherwise UNKNOWN.
    pub handed_off: bool,
    pub report: Option<FanoutReport>,
    pub frames: Vec<Frame>,
}

impl core::fmt::Debug for Released {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Released")
            .field("tx_hash", &self.tx_hash)
            .field("kind", &self.kind)
            .field("component", &self.component)
            .field("handed_off", &self.handed_off)
            .field("frames", &self.frames.len())
            .finish_non_exhaustive()
    }
}

/// The armed LIVE end.
pub struct LiveEnd {
    key: LiveKey,
    armed: LiveNow,
    store: Store,
    qualification: Verified,
    table: AdapterTable,
    slots: Vec<Slot>,
    /// Unix milliseconds, the owner's clock for each admission.
    wall_ms: Box<dyn Fn() -> u64 + Send>,
    released: Vec<Released>,
    last_refusal: Option<String>,
    /// The LIVE ledger under the owner's state, the facts this end wrote
    /// there and why the last one could not be written.
    ledger: Ledger,
    facts: u64,
    ledger_failure: Option<String>,
}

impl core::fmt::Debug for LiveEnd {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LiveEnd")
            .field("key", &self.key)
            .field("grant_generation", &self.armed.grant_generation())
            .field("released", &self.released.len())
            .finish_non_exhaustive()
    }
}

fn wei(value: U256) -> Option<u128> {
    u128::try_from(value).ok()
}

impl LiveEnd {
    /// The LIVE end of the grant `armed`: one adapter slot per granted
    /// builder, each qualified by `qualification` (else refused), its
    /// connection over `route` not yet established.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        key: LiveKey,
        armed: LiveNow,
        store: Store,
        qualification: Verified,
        route: LiveRoute,
        chain_id: u64,
        capability_generation: u64,
        wall_ms: Box<dyn Fn() -> u64 + Send>,
    ) -> Result<Self, String> {
        let grant = armed.grant().clone();
        let mut bindings = Vec::with_capacity(grant.adapters.len());
        let mut slots = Vec::with_capacity(grant.adapters.len());
        for (index, live) in grant.adapters.iter().enumerate() {
            let adapter = u8::try_from(index)
                .ok()
                .and_then(AdapterId::new)
                .ok_or("more granted adapters than adapter slots")?;
            let id = capability(*live);
            if !qualification.view.adapters.contains(live)
                || !qualification.adapters.iter().any(|q| q.id() == id)
            {
                return Err(format!(
                    "{}: {}",
                    crate::sources::LIVE_ADAPTER_UNQUALIFIED,
                    live.name()
                ));
            }
            bindings.push(AdapterBinding {
                adapter,
                capability: id,
            });
            let e = |e| format!("{}: {e:?}", live.name());
            let target = match live {
                LiveAdapter::Club48BuilderV1 => ProviderTarget::club48(
                    TlsEndpoint::new(CLUB48_DOCUMENTED_HOST, HTTPS_PORT).map_err(e)?,
                ),
                LiveAdapter::BlockRazorBuilderV1 => {
                    let token = match &route {
                        LiveRoute::Builders { blockrazor, .. } => blockrazor.clone(),
                        // The stand-in takes the frame as BlockRazor's;
                        // no credential reaches it.
                        LiveRoute::Loopback { .. } => {
                            AuthorizationToken::new("loopback".into()).ok()
                        }
                    }
                    .ok_or("BlockRazor needs its authorization")?;
                    ProviderTarget::blockrazor(
                        TlsEndpoint::new(BLOCKRAZOR_DOCUMENTED_HOST, HTTPS_PORT).map_err(e)?,
                        token,
                    )
                }
            };
            let link = match &route {
                LiveRoute::Builders { setup, .. } => Link::Tls(Box::new(
                    TlsConnectionOwner::new(
                        target.tls_endpoint().clone(),
                        setup.clone(),
                        Q5A_TLS_POLICY,
                    )
                    .map_err(|e| format!("{}: {e:?}", live.name()))?,
                )),
                LiveRoute::Loopback { port } => Link::Loopback(LoopbackTransport::new(*port)),
            };
            slots.push(Slot {
                adapter,
                live: *live,
                target,
                link,
            });
        }
        if slots.is_empty() {
            return Err("a LIVE grant names at least one adapter".into());
        }
        let table = AdapterTable::new(chain_id, capability_generation, &bindings)
            .map_err(|e| e.to_string())?;
        let ledger = Ledger::open(store.dir());
        Ok(LiveEnd {
            key,
            armed,
            store,
            qualification,
            table,
            slots,
            wall_ms,
            released: Vec::new(),
            last_refusal: None,
            ledger,
            facts: 0,
            ledger_failure: None,
        })
    }

    /// Establishes every adapter's connection (startup and maintenance,
    /// never in a dispatch): which adapter failed and why.
    pub fn connect(&mut self, timeout: Duration) -> Vec<(LiveAdapter, String)> {
        let mut failed = Vec::new();
        for slot in &mut self.slots {
            let result = match &mut slot.link {
                Link::Tls(owner) => owner.connect().map(|_| ()).map_err(|e| e.to_string()),
                Link::Loopback(t) => t.connect(timeout).map_err(|e| e.to_string()),
            };
            if let Err(e) = result {
                failed.push((slot.live, e));
            }
        }
        failed
    }

    /// One maintenance pass outside any dispatch: a TLS owner probes,
    /// gives up and replaces its connection as its policy says; a loopback
    /// link that is not ready connects again. Which adapter failed and why.
    pub fn maintain(&mut self, timeout: Duration) -> Vec<(LiveAdapter, String)> {
        use crate::submission::transport::{ConnectionState, Transport};
        let mut failed = Vec::new();
        for slot in &mut self.slots {
            let result = match &mut slot.link {
                Link::Tls(owner) => match owner.maintain() {
                    crate::submission::tls::Maintenance::Failed(e) => Err(e.to_string()),
                    _ => Ok(()),
                },
                Link::Loopback(t) if t.state() != ConnectionState::Ready => {
                    t.connect(timeout).map_err(|e| e.to_string())
                }
                Link::Loopback(_) => Ok(()),
            };
            if let Err(e) = result {
                failed.push((slot.live, e));
            }
        }
        failed
    }

    /// `BUILDERS` or `LOOPBACK`.
    pub fn route(&self) -> &'static str {
        match self.slots.first().map(|s| &s.link) {
            Some(Link::Loopback(_)) => "LOOPBACK",
            _ => "BUILDERS",
        }
    }

    /// The grant this end was armed under.
    pub fn armed(&self) -> &LiveNow {
        &self.armed
    }

    /// Every transaction whose bytes may have left, in dispatch order.
    pub fn released(&self) -> &[Released] {
        &self.released
    }

    /// The owner's or the release's last reason to refuse an intent.
    pub fn last_refusal(&self) -> Option<&str> {
        self.last_refusal.as_deref()
    }

    /// The ledger facts this end wrote (one per admitted intent).
    pub fn facts_written(&self) -> u64 {
        self.facts
    }

    /// Why the last admitted intent's fact could not be written: its
    /// admission entry recovers it once nothing is armed.
    pub fn ledger_failure(&self) -> Option<&str> {
        self.ledger_failure.as_deref()
    }

    fn refuse(&mut self, refusal: Refusal, why: String) -> Handed<LiveHandoff> {
        self.last_refusal = Some(why);
        Handed::refused(refusal)
    }
}

impl End for LiveEnd {
    type Handoff = LiveHandoff;
    const FENCE: (&'static str, [u8; 8]) = ("live", *b"DBLIVFN1");
    const CHAIN_ORDERED: bool = true;

    fn address(&self) -> [u8; 20] {
        self.key.address()
    }

    fn signer(&self) -> &Signer {
        self.key.signer()
    }

    fn hand_over(&mut self, h: Handover<'_>) -> Handed<LiveHandoff> {
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
        } = h;
        // BSC3-045: a fact the ledger did not take may be missing from the
        // book: nothing more is handed over (before the owner's admission:
        // no attempt, no spend).
        if let Some(e) = &self.ledger_failure {
            let why = format!("{}: {e}", crate::sources::LIVE_LEDGER_UNREADABLE);
            return self.refuse(Refusal::Owner, why);
        }
        let kind = match expected {
            ExpectedTx::Buy(_) => LiveIntentKind::Buy,
            ExpectedTx::Approve(_) => LiveIntentKind::Approve,
            ExpectedTx::Sell(_) => LiveIntentKind::Sell,
        };
        let (component, profile, mode, launch, launch_raw, merge) = match submission {
            Submission::LaunchBundle(launch_raw) => (
                ComponentId::ExecC3,
                ReleaseProfile::BirthEntryV1,
                SubmissionMode::PublicFullLaunchBundle,
                Launch::PublicOriginal(TxRef::of(launch_raw)),
                launch_raw,
                MergePolicy::NoMerge,
            ),
            Submission::Single => (
                ComponentId::StrategyC,
                ReleaseProfile::SingleRawV1,
                SubmissionMode::SingleRawBuy,
                Launch::None,
                &[][..],
                MergePolicy::ProviderDefault,
            ),
        };
        let ids: Vec<AdapterId> = self.slots.iter().map(|s| s.adapter).collect();
        let Some(adapters) = AdapterSet::new(&ids) else {
            return self.refuse(Refusal::Plan, "no adapter set".into());
        };
        let Some(last_block) = head.number.checked_add(config.validity_blocks) else {
            return self.refuse(Refusal::Plan, "the block window overflows".into());
        };
        let plan = match SubmissionPlan::new_with(
            &self.table,
            &self.qualification.adapters,
            expected,
            budget,
            PlanInputs {
                ticket,
                buy: TxRef::of(raw),
                launch,
                mode,
                profile,
                adapters,
                local_block_deadline: LocalBlockStartDeadline {
                    last_head_block: last_block,
                },
                provider_validity: Some(ProviderValidity {
                    max_block_number: last_block,
                    min_timestamp: None,
                    max_timestamp: None,
                }),
                merge,
                pre_submission: PreSubmission::No,
                provider_payment_wei: U256::ZERO,
            },
        ) {
            Ok(p) => p,
            Err(e) => return self.refuse(Refusal::Plan, format!("{e:?}")),
        };
        let release = match plan.qualify_for_release_with(&self.table, &self.qualification.adapters)
        {
            Ok(r) => r,
            Err(r) => return self.refuse(Refusal::Unqualified, format!("{:?}", r.blocker)),
        };
        let mut frames = Vec::with_capacity(self.slots.len());
        for slot in &self.slots {
            let endpoint = slot.target.frame_endpoint();
            let frame = match slot.live {
                LiveAdapter::Club48BuilderV1 => club48::frame(
                    &plan,
                    &self.table,
                    slot.adapter,
                    &endpoint,
                    launch_raw,
                    config.max_frame,
                ),
                LiveAdapter::BlockRazorBuilderV1 => blockrazor::frame(
                    &plan,
                    &self.table,
                    slot.adapter,
                    &endpoint,
                    launch_raw,
                    config.max_frame,
                ),
            };
            match frame {
                Ok(f) => frames.push(f),
                Err(e) => return self.refuse(Refusal::Plan, e.to_string()),
            }
        }
        let deadline = match TransportStartDeadline::for_profile(
            profile,
            clock.id(),
            start_ns,
            config.start_limit_ns,
        ) {
            Ok(d) => d,
            Err(e) => return self.refuse(Refusal::Plan, format!("{e:?}")),
        };
        let planned_ns = clock.now_ns();
        let (Some(input_wei), Some(max_wei)) =
            (wei(budget.max_buy_input_wei), wei(budget.max_total_wei))
        else {
            return self.refuse(Refusal::Plan, "the budget exceeds 128 bits".into());
        };
        let id: String = tx_hash.iter().map(|b| format!("{b:02x}")).collect();
        let mut qualifications = TransportQualifications::default();
        let mut transports = Vec::with_capacity(self.slots.len());
        let mut unqualified = None;
        for slot in &mut self.slots {
            let qualified = match &mut slot.link {
                Link::Tls(owner) => qualify_transport(owner, &self.table, slot.adapter)
                    .map(|q| q.into_parts())
                    .map_err(|e| e.to_string()),
                Link::Loopback(t) => {
                    qualify_loopback(t, &self.table, slot.adapter).map_err(|e| format!("{e:?}"))
                }
            };
            match qualified {
                Ok((transport, q)) => {
                    qualifications = qualifications.with(slot.adapter, q);
                    transports.push(transport);
                }
                Err(e) => {
                    unqualified = Some(format!("{}: {e}", slot.live.name()));
                    break;
                }
            }
        }
        if let Some(why) = unqualified {
            drop(transports);
            return self.refuse(Refusal::Unqualified, why);
        }
        // The owner, as it holds now: nothing before this consumed an
        // attempt or a budget, and nothing after it can leave unadmitted.
        // Its entry, with the token and nonce, is the transaction's
        // write-ahead record (BSC3-024).
        let token = ticket.token().into_array();
        let nonce = ticket.nonce();
        let admitted = self.store.admit_live(
            &self.armed,
            &LiveIntent {
                id: &id,
                component,
                kind,
                input_wei,
                max_wei,
                token,
                nonce,
                max_block: last_block,
            },
            &self.qualification.view,
            (self.wall_ms)(),
        );
        if let Err(why) = admitted {
            drop(transports);
            return self.refuse(Refusal::Owner, why);
        }
        let mut armed: Vec<ReleaseArmed<'_>> = transports
            .into_iter()
            .zip(frames.iter_mut())
            .map(|(transport, frame)| ReleaseArmed { transport, frame })
            .collect();
        let dispatched = dispatch_release(
            lane,
            &release,
            &DispatchContext {
                table: &self.table,
                current: snapshot,
                head,
            },
            clock,
            &deadline,
            &qualifications,
            &mut armed,
        );
        drop(armed);
        let (admission, disclosed) = consumed_by(&dispatched);
        let result = match &dispatched {
            Err(e) => Err((Refusal::Dispatch, format!("{e:?}"))),
            Ok(report) => {
                let handed: Vec<u64> = ids
                    .iter()
                    .filter_map(|a| report.adapter(*a))
                    .filter(|r| r.outcome == SendOutcome::HandedOff)
                    .filter_map(|r| r.t_boundary_ns)
                    .collect();
                let late = ids
                    .iter()
                    .filter_map(|a| report.adapter(*a))
                    .any(|r| matches!(r.outcome, SendOutcome::SkippedDeadline(_)));
                match handed.iter().min() {
                    Some(at_ns) => Ok(LiveHandoff {
                        tx_hash,
                        at_ns: *at_ns,
                        adapters: handed.len() as u8,
                    }),
                    None if late && !disclosed => Err((
                        Refusal::Deadline,
                        "the transport start deadline passed".into(),
                    )),
                    None => Err((Refusal::Dispatch, "no builder took it whole".into())),
                }
            }
        };
        // The admitted intent's outcome, a ledger fact at once: off the
        // dispatch, before the executor goes on.
        let fact = Fact {
            kind: match (disclosed, result.is_ok()) {
                (false, _) => FactKind::Withdrawn,
                (true, true) => FactKind::Released,
                (true, false) => FactKind::Unknown,
            },
            tx_hash,
            intent: kind,
            component,
            token,
            nonce,
            grant_generation: self.armed.grant_generation(),
            input_wei: Some(budget.max_buy_input_wei),
            max_wei: Some(budget.max_total_wei),
            block: None,
            gas_wei: None,
            tokens: None,
            eth_wei: None,
            max_block: Some(last_block),
            by_tx: None,
        };
        match self.ledger.append(&fact, (self.wall_ms)()) {
            Ok(_) => self.facts += 1,
            Err(e) => self.ledger_failure = Some(e),
        }
        if disclosed {
            self.released.push(Released {
                tx_hash,
                kind,
                component,
                token,
                nonce,
                input_wei: budget.max_buy_input_wei,
                max_wei: budget.max_total_wei,
                handed_off: result.is_ok(),
                report: dispatched.as_ref().ok().copied(),
                frames,
            });
        }
        let result = result.map_err(|(refusal, why)| {
            self.last_refusal = Some(why);
            refusal
        });
        Handed {
            result,
            planned_ns,
            admitted: admission,
            disclosed,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::paper_exec::tests::{
        HEAD, c2_next, clock, config, curve_after, dir, launch, strategy_c2_next, validated,
    };
    use crate::paper_exec::{Arming, Executor, IntentKind, PendingLaunch, StrategyCurve};
    use crate::sources::tests::{grant_of, live_ready_granting, qualified};
    use crate::sources::{
        Entity, LIVE_GRANT_CHANGED, LiveGrant, LiveStrategies, Mode, buy_usage, spent_by_grant,
    };
    use alloy_primitives::keccak256;
    use darkbot_bsc_trading::economics::flap::native_curve20::{
        HoldingCap, NativeBuyInputs, WITNESSED_MAINNET_TERMS, quote_native_buy,
    };
    use darkbot_bsc_trading::signer::recover_legacy_sender;
    use darkbot_bsc_trading::submission_policy::capability::{CapabilityId, Knowledge};
    use darkbot_bsc_trading::submission_policy::qualification::{
        AdapterQualification, FeatureName, Finding, PropertyName,
    };
    use darkbot_bsc_trading::transactions::legacy::decode_signed;
    use darkbot_bsc_trading::venues::flap::abi::decode_swap_exact_input_native;
    use darkbot_bsc_trading::venues::flap::deployment::bsc_mainnet_v5_27_0;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::{Ipv4Addr, TcpListener};
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;

    /// The builder stand-in on 127.0.0.1: every complete HTTP request it
    /// reads, whole. It answers nothing.
    pub(crate) fn stand_in() -> (u16, mpsc::Receiver<Vec<u8>>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let tx = tx.clone();
                std::thread::spawn(move || {
                    let (mut buf, mut chunk) = (Vec::new(), [0u8; 4096]);
                    loop {
                        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&buf[..end]).to_string();
                            let len: usize = head
                                .lines()
                                .find_map(|l| l.strip_prefix("Content-Length: "))
                                .and_then(|v| v.trim().parse().ok())
                                .unwrap_or(0);
                            if buf.len() >= end + 4 + len {
                                let rest = buf.split_off(end + 4 + len);
                                if tx.send(std::mem::replace(&mut buf, rest)).is_err() {
                                    return;
                                }
                                continue;
                            }
                        }
                        match stream.read(&mut chunk) {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    }
                });
            }
        });
        (port, rx)
    }

    /// 48Club measured for the birth entry and the single transaction.
    pub(crate) fn club48_findings() -> Vec<Finding> {
        let properties = [
            (PropertyName::PreservesOrder, Knowledge::Yes),
            (PropertyName::Atomic, Knowledge::Yes),
            (PropertyName::SameBlock, Knowledge::Yes),
            (PropertyName::ExtractsTransactions, Knowledge::No),
            (PropertyName::ProviderRebroadcast, Knowledge::No),
            (PropertyName::ProviderPublicFallback, Knowledge::No),
            (PropertyName::ReachesOtherBuilders, Knowledge::No),
            (PropertyName::PublicMempoolExposure, Knowledge::No),
        ];
        properties
            .into_iter()
            .map(|(p, k)| Finding::Property(p, k))
            .chain([
                Finding::Feature(FeatureName::NoMerge),
                Finding::Feature(FeatureName::PositionFirst),
                Finding::Feature(FeatureName::MaxBlockNumber),
                Finding::Mode(SubmissionMode::SingleRawBuy),
                Finding::SendTime,
            ])
            .collect()
    }

    pub(crate) fn verified() -> Verified {
        let findings = club48_findings();
        Verified {
            view: qualified(),
            adapters: vec![
                AdapterQualification::from_findings(CapabilityId::Club48BuilderV1, &findings)
                    .unwrap(),
            ],
            sha256: [0; 32],
        }
    }

    /// A private buyer key file in `d`, and its address.
    pub(crate) fn key_file(d: &Path) -> (PathBuf, [u8; 20]) {
        let keys = d.join("keys");
        fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(&keys)
            .unwrap();
        let path = keys.join("buyer.key");
        let secret = [0x33u8; 32];
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .and_then(|mut f| f.write_all(&secret))
            .unwrap();
        let buyer = Signer::from_secret_bytes(&secret).unwrap().address();
        (path, buyer.into_array())
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Plan 28.10.: under an effective LIVE the one executor's C3 BUY leaves
    /// as the builder bundle behind the launch, a Strategy C BUY and its
    /// approval and SELL each as a one-transaction bundle, all signed by
    /// the buyer key and each admitted by the owner first (attempt slots,
    /// spend). A request goes only to the loopback stand-in. Once the grant
    /// is revoked, the owner admits nothing and nothing leaves.
    #[test]
    fn live_intents_leave_through_the_release_only_as_the_owner_admits_them() {
        let (state, store, now) = live_ready_granting("live-end", |now| LiveGrant {
            max_input_wei: 10_000_000_000_000_000,
            max_spend_wei: 100_000_000_000_000_000,
            ..grant_of(now + 3_600_000, LiveStrategies::Both)
        });
        let q = verified();
        let armed = store
            .live_now(&[ComponentId::ExecC3, ComponentId::StrategyC], &q.view, now)
            .unwrap();
        let generation = armed.grant_generation();
        let d = dir("live-end");
        let (key_path, buyer) = key_file(&d);
        let key = LiveKey::load(
            &key_path,
            Path::new("/nonexistent-repo"),
            buyer,
            &[],
            &armed,
        )
        .unwrap();
        let (port, requests) = stand_in();
        let mut cfg = config(2, 2);
        // The BUY, its approval and SELL, and one more BUY.
        cfg.strategy_lanes = 5;
        let end = LiveEnd::new(
            key,
            armed,
            store,
            q,
            LiveRoute::Loopback { port },
            56,
            cfg.capability_generation,
            Box::new(move || now),
        )
        .unwrap();
        let mut exec = Executor::arm_end(
            end,
            validated(),
            WITNESSED_MAINNET_TERMS,
            cfg,
            Arming {
                next_nonce: 0,
                fence_dir: &d.join("fences"),
                session: 1,
                arm: 0,
            },
        )
        .unwrap();
        let wait = std::time::Duration::from_secs(2);
        let reconnect = |e: &mut Executor<LiveEnd>| {
            assert!(e.end_mut().connect(wait).is_empty());
        };
        reconnect(&mut exec);
        // C3: the bundle behind the launch.
        let portal = bsc_mainnet_v5_27_0().portal.into_array();
        let (hash, raw, input) = launch(0, 7);
        let pending = PendingLaunch {
            hash,
            raw: &raw,
            to: Some(portal),
            input: &input,
            ingress_ns: 100,
        };
        let buy = exec
            .on_launch(pending, &c2_next(Some(HEAD), 0), &clock(100, 7))
            .unwrap();
        let request = String::from_utf8(requests.recv_timeout(wait).unwrap()).unwrap();
        assert!(request.contains(&format!(
            "\"txs\":[\"0x{}\",\"0x{}\"]",
            hex(&raw),
            hex(&buy.raw)
        )));
        assert!(request.contains("\"noMerge\":true,\"positionFirst\":true"));
        assert_eq!(buy.handoff.tx_hash, keccak256(&buy.raw).0);
        assert_eq!(buy.handoff.adapters, 1);
        let signed = decode_signed(&buy.raw).unwrap();
        let from = recover_legacy_sender(&signed, 56, &mut Vec::new()).unwrap();
        assert_eq!(from.into_array(), buyer);
        let c3_most = 1_000_000_000_000_000u128 + 1_000_000_000 * 300_000;
        assert_eq!(buy_usage(&state, now).unwrap().used, 1);
        assert_eq!(spent_by_grant(&state, generation).unwrap(), c3_most);
        // Strategy C, once nonce 0 is canonical: the BUY alone, then its
        // approval and SELL.
        reconnect(&mut exec);
        let token = [0xAB; 20];
        let input = U256::from(10_000_000_000_000_000u64);
        let curve = curve_after(1_000_000_000_000_000_000);
        let entry = exec
            .on_strategy_buy(
                token,
                curve,
                input,
                &strategy_c2_next(Some(HEAD), 1),
                &clock(500, 3),
                500,
            )
            .unwrap();
        let request = String::from_utf8(requests.recv_timeout(wait).unwrap()).unwrap();
        assert!(request.contains(&format!("\"txs\":[\"0x{}\"]", hex(&entry.raw))));
        assert_eq!(buy_usage(&state, now).unwrap().used, 2);
        let call =
            decode_swap_exact_input_native(decode_signed(&entry.raw).unwrap().tx.input).unwrap();
        let tokens = call.min_output_amount;
        let q = quote_native_buy(&NativeBuyInputs {
            token: curve.terms,
            reserve_before: curve.reserve,
            circulating_before: curve.circulating,
            input_amount: input,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
            holding_cap: HoldingCap::None,
        })
        .unwrap();
        let held = StrategyCurve {
            reserve: q.reserve_after,
            circulating: q.supply_after,
            ..curve
        };
        reconnect(&mut exec);
        // The day's BUYs are taken; the exit needs none. Once the BUY is
        // canonical its approval goes; its SELL waits for the approval to
        // be canonical, then goes alone.
        let ctx = strategy_c2_next(Some(HEAD), 2);
        let (out, refused) = exec.on_strategy_exit(token, tokens, held, &ctx, &clock(900, 3), 900);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, IntentKind::Approve);
        assert_eq!(refused, None);
        assert!(!exec.exited(&token));
        let approval = requests.recv_timeout(wait).unwrap();
        assert!(
            String::from_utf8(approval)
                .unwrap()
                .contains(&hex(&out[0].raw))
        );
        reconnect(&mut exec);
        let ctx = strategy_c2_next(Some(HEAD), 3);
        let (out, refused) = exec.on_strategy_exit(token, tokens, held, &ctx, &clock(950, 3), 950);
        assert_eq!((out.len(), refused), (1, None));
        assert_eq!(out[0].kind, IntentKind::Sell);
        let sell = requests.recv_timeout(wait).unwrap();
        assert!(String::from_utf8(sell).unwrap().contains(&hex(&out[0].raw)));
        let released = exec.end().released();
        assert_eq!(released.len(), 4);
        assert!(released.iter().all(|r| r.handed_off));
        // Revoked: the owner admits nothing more, nothing leaves.
        let owner = Store::open_in_session(&state, 8, Ok("boot-1".into())).unwrap();
        let g = owner.current().unwrap().generation;
        let r = owner
            .request("revoke", Entity::LiveGrant, Mode::Disabled, g, now)
            .unwrap();
        assert!(r.applied());
        reconnect(&mut exec);
        let refused = exec.on_strategy_buy(
            [0xCD; 20],
            curve,
            input,
            &strategy_c2_next(Some(HEAD), 4),
            &clock(1_000, 3),
            1_000,
        );
        assert_eq!(refused.unwrap_err(), Refusal::Owner);
        assert_eq!(exec.end().last_refusal(), Some(LIVE_GRANT_CHANGED));
        assert!(
            requests
                .recv_timeout(std::time::Duration::from_millis(300))
                .is_err()
        );
        assert_eq!(buy_usage(&state, now).unwrap().used, 2);
        assert_eq!(exec.end().released().len(), 4);
        // The key file was only read.
        assert_eq!(fs::read(&key_path).unwrap(), [0x33u8; 32]);
        exec.disarm().unwrap();
        fs::remove_dir_all(d).unwrap();
        fs::remove_dir_all(state).unwrap();
    }

    /// An executor armed towards the LIVE end of a grant for both
    /// strategies at nonce 0, on the loopback stand-in: the owner's state,
    /// the run directory, the executor and what the stand-in receives.
    fn armed_both(tag: &str) -> (PathBuf, PathBuf, Executor<LiveEnd>, mpsc::Receiver<Vec<u8>>) {
        let (state, store, now) = live_ready_granting(tag, |now| LiveGrant {
            max_input_wei: 10_000_000_000_000_000,
            max_spend_wei: 100_000_000_000_000_000,
            ..grant_of(now + 3_600_000, LiveStrategies::Both)
        });
        let q = verified();
        let armed = store
            .live_now(&[ComponentId::ExecC3, ComponentId::StrategyC], &q.view, now)
            .unwrap();
        let d = dir(tag);
        let (key_path, buyer) = key_file(&d);
        let key = LiveKey::load(
            &key_path,
            Path::new("/nonexistent-repo"),
            buyer,
            &[],
            &armed,
        )
        .unwrap();
        let (port, requests) = stand_in();
        let mut cfg = config(2, 2);
        cfg.strategy_lanes = 4;
        let end = LiveEnd::new(
            key,
            armed,
            store,
            q,
            LiveRoute::Loopback { port },
            56,
            cfg.capability_generation,
            Box::new(move || now),
        )
        .unwrap();
        let mut exec = Executor::arm_end(
            end,
            validated(),
            WITNESSED_MAINNET_TERMS,
            cfg,
            Arming {
                next_nonce: 0,
                fence_dir: &d.join("fences"),
                session: 1,
                arm: 0,
            },
        )
        .unwrap();
        assert!(
            exec.end_mut()
                .connect(std::time::Duration::from_secs(2))
                .is_empty()
        );
        (state, d, exec, requests)
    }

    fn reconnect(e: &mut Executor<LiveEnd>) {
        assert!(
            e.end_mut()
                .connect(std::time::Duration::from_secs(2))
                .is_empty()
        );
    }

    /// BSC3-055: one sender's LIVE transactions reach the chain one nonce
    /// after the other (Unit 4 invariant 3 across the sender's lanes).
    /// While the C3 BUY at nonce 0 is not canonical, the strategy's BUY at
    /// nonce 1 is refused and nothing of it leaves; once C2 shows nonce 0
    /// consumed it goes. An exit's approval goes at its nonce; its SELL
    /// waits for the approval to be canonical, and then goes alone. A
    /// refusal for the order consumes nothing.
    #[test]
    fn one_sender_releases_one_nonce_after_the_other() {
        let (state, d, mut exec, requests) = armed_both("live-order");
        let wait = std::time::Duration::from_secs(2);
        let silent = std::time::Duration::from_millis(300);
        let portal = bsc_mainnet_v5_27_0().portal.into_array();
        let (hash, raw, input) = launch(0, 7);
        let pending = PendingLaunch {
            hash,
            raw: &raw,
            to: Some(portal),
            input: &input,
            ingress_ns: 100,
        };
        // C2 not clean: no nonce is known, nothing goes.
        assert_eq!(
            exec.on_launch(
                pending,
                &crate::paper_exec::tests::c2(Some(HEAD)),
                &clock(100, 7)
            )
            .unwrap_err(),
            Refusal::SenderNonce
        );
        assert!(requests.recv_timeout(silent).is_err());
        exec.on_launch(pending, &c2_next(Some(HEAD), 0), &clock(100, 7))
            .unwrap();
        requests.recv_timeout(wait).unwrap();
        reconnect(&mut exec);
        let token = [0xAB; 20];
        let input = U256::from(10_000_000_000_000_000u64);
        let curve = curve_after(1_000_000_000_000_000_000);
        let buy = |exec: &mut Executor<LiveEnd>, next: u64| {
            exec.on_strategy_buy(
                token,
                curve,
                input,
                &strategy_c2_next(Some(HEAD), next),
                &clock(500, 3),
                500,
            )
        };
        // Nonce 0 is not canonical yet: nonce 1 does not go.
        assert_eq!(buy(&mut exec, 0).unwrap_err(), Refusal::SenderNonce);
        assert!(requests.recv_timeout(silent).is_err());
        assert_eq!(exec.strategy_next_nonce(), 1);
        // Nonce 0 consumed: the BUY at nonce 1.
        let entry = buy(&mut exec, 1).unwrap();
        assert_eq!(decode_signed(&entry.raw).unwrap().tx.nonce, 1);
        requests.recv_timeout(wait).unwrap();
        reconnect(&mut exec);
        let tokens = decode_swap_exact_input_native(decode_signed(&entry.raw).unwrap().tx.input)
            .unwrap()
            .min_output_amount;
        let q = quote_native_buy(&NativeBuyInputs {
            token: curve.terms,
            reserve_before: curve.reserve,
            circulating_before: curve.circulating,
            input_amount: input,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
            holding_cap: HoldingCap::None,
        })
        .unwrap();
        let held = StrategyCurve {
            reserve: q.reserve_after,
            circulating: q.supply_after,
            ..curve
        };
        let exit = |exec: &mut Executor<LiveEnd>, next: u64| {
            exec.on_strategy_exit(
                token,
                tokens,
                held,
                &strategy_c2_next(Some(HEAD), next),
                &clock(900, 3),
                900,
            )
        };
        // The BUY at 1 is not canonical: the approval at 2 does not go.
        let (out, refused) = exit(&mut exec, 1);
        assert_eq!((out.len(), refused), (0, Some(Refusal::SenderNonce)));
        assert!(requests.recv_timeout(silent).is_err());
        // It is: the approval goes, the SELL waits for it; the exit is not
        // over.
        let (out, refused) = exit(&mut exec, 2);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, IntentKind::Approve);
        assert_eq!(decode_signed(&out[0].raw).unwrap().tx.nonce, 2);
        assert_eq!(refused, None);
        assert!(!exec.exited(&token));
        requests.recv_timeout(wait).unwrap();
        assert!(requests.recv_timeout(silent).is_err());
        assert_eq!(exec.exits_awaiting_sell(), vec![token]);
        // The approval not canonical yet: still no SELL.
        let (out, refused) = exit(&mut exec, 2);
        assert_eq!((out.len(), refused), (0, Some(Refusal::SenderNonce)));
        assert!(requests.recv_timeout(silent).is_err());
        // Canonical, but the connection still awaits the approval's
        // answer: refused before anything is consumed.
        let (out, refused) = exit(&mut exec, 3);
        assert_eq!((out.len(), refused), (0, Some(Refusal::Unqualified)));
        assert!(requests.recv_timeout(silent).is_err());
        assert_eq!(exec.strategy_next_nonce(), 3);
        reconnect(&mut exec);
        // On a ready connection: the SELL alone, at 3.
        let (out, refused) = exit(&mut exec, 3);
        assert_eq!((out.len(), refused), (1, None));
        assert_eq!(out[0].kind, IntentKind::Sell);
        assert_eq!(decode_signed(&out[0].raw).unwrap().tx.nonce, 3);
        requests.recv_timeout(wait).unwrap();
        assert_eq!(exec.exits_awaiting_sell(), Vec::<[u8; 20]>::new());
        // Four transactions, each handed off once, in nonce order.
        let nonces: Vec<u64> = exec.end().released().iter().map(|r| r.nonce).collect();
        assert_eq!(nonces, vec![0, 1, 2, 3]);
        exec.disarm().unwrap();
        fs::remove_dir_all(d).unwrap();
        fs::remove_dir_all(state).unwrap();
    }

    /// BSC3-024 (review): an intent the owner admitted whose dispatch then
    /// found the transport start deadline passed: no byte reaches the
    /// stand-in, its fact is WITHDRAWN (resolved, never on chain, no
    /// position, no cost), so neither recovery nor the next arm takes it
    /// for one that may have left. The owner's slot stays taken (counted
    /// conservatively, BSC3-068).
    #[test]
    fn an_admitted_intent_that_never_left_is_withdrawn() {
        use crate::live_ledger::fold;
        let (state, d, mut exec, requests) = armed_both("live-withdrawn");
        let portal = bsc_mainnet_v5_27_0().portal.into_array();
        let (hash, raw, input) = launch(0, 7);
        let late = clock(100 + 2_000_000, 1);
        let refused = exec.on_launch(
            PendingLaunch {
                hash,
                raw: &raw,
                to: Some(portal),
                input: &input,
                ingress_ns: 100,
            },
            &c2_next(Some(HEAD), 0),
            &late,
        );
        assert_eq!(refused.unwrap_err(), Refusal::Deadline);
        assert!(
            requests
                .recv_timeout(std::time::Duration::from_millis(300))
                .is_err()
        );
        assert!(exec.end().released().is_empty());
        let facts = Ledger::open(&state).facts().unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(
            (facts[0].kind, facts[0].intent, facts[0].nonce),
            (FactKind::Withdrawn, LiveIntentKind::Buy, 0)
        );
        let book = fold(&facts);
        assert_eq!(book.unresolved().count(), 0);
        assert!(book.positions.is_empty());
        assert_eq!(
            Ledger::open(&state)
                .recover(&crate::sources::admitted_intents(&state).unwrap(), 1)
                .unwrap()
                .unknown,
            0
        );
        assert_eq!(crate::sources::admitted_intents(&state).unwrap().len(), 1);
        exec.disarm().unwrap();
        fs::remove_dir_all(d).unwrap();
        fs::remove_dir_all(state).unwrap();
    }

    /// BSC3-055: a token is bought once per sender and arm, whichever
    /// strategy asks: Strategy C's BUY of the token C3 bought is refused.
    #[test]
    fn a_token_is_bought_once_by_one_sender() {
        let (state, d, mut exec, requests) = armed_both("live-dedup");
        let wait = std::time::Duration::from_secs(2);
        let silent = std::time::Duration::from_millis(300);
        let portal = bsc_mainnet_v5_27_0().portal.into_array();
        let (hash, raw, input) = launch(0, 7);
        let pending = PendingLaunch {
            hash,
            raw: &raw,
            to: Some(portal),
            input: &input,
            ingress_ns: 100,
        };
        let buy = exec
            .on_launch(pending, &c2_next(Some(HEAD), 0), &clock(100, 7))
            .unwrap();
        requests.recv_timeout(wait).unwrap();
        reconnect(&mut exec);
        // Nonce 0 canonical (the C3 BUY reverted, say): the strategy's BUY
        // of the same token is still not a second purchase.
        let again = exec.on_strategy_buy(
            buy.token,
            curve_after(1_000_000_000_000_000_000),
            U256::from(10_000_000_000_000_000u64),
            &strategy_c2_next(Some(HEAD), 1),
            &clock(500, 3),
            500,
        );
        assert_eq!(again.err(), Some(Refusal::DuplicateToken));
        assert!(requests.recv_timeout(silent).is_err());
        exec.disarm().unwrap();
        fs::remove_dir_all(d).unwrap();
        fs::remove_dir_all(state).unwrap();
    }

    /// BSC3-045: the ledger did not take a released transaction's fact.
    /// The end hands nothing over after it: the next intent is refused
    /// before the owner admits it (no attempt, no spend), nothing leaves.
    #[test]
    fn an_end_whose_fact_was_not_written_hands_nothing_more_over() {
        use std::os::unix::fs::PermissionsExt;
        let (state, d, mut exec, requests) = armed_both("live-unwritten");
        let wait = std::time::Duration::from_secs(2);
        let silent = std::time::Duration::from_millis(300);
        let ledger = state.join("live-ledger");
        fs::create_dir_all(&ledger).unwrap();
        fs::set_permissions(&ledger, fs::Permissions::from_mode(0o500)).unwrap();
        let portal = bsc_mainnet_v5_27_0().portal.into_array();
        let (hash, raw, input) = launch(0, 7);
        let pending = PendingLaunch {
            hash,
            raw: &raw,
            to: Some(portal),
            input: &input,
            ingress_ns: 100,
        };
        exec.on_launch(pending, &c2_next(Some(HEAD), 0), &clock(100, 7))
            .unwrap();
        requests.recv_timeout(wait).unwrap();
        assert!(exec.end().ledger_failure().is_some());
        reconnect(&mut exec);
        let next = exec.on_strategy_buy(
            [0x5A; 20],
            curve_after(1_000_000_000_000_000_000),
            U256::from(10_000_000_000_000_000u64),
            &strategy_c2_next(Some(HEAD), 1),
            &clock(500, 3),
            500,
        );
        assert_eq!(next.err(), Some(Refusal::Owner));
        assert!(
            exec.end()
                .last_refusal()
                .unwrap()
                .starts_with(crate::sources::LIVE_LEDGER_UNREADABLE)
        );
        assert!(requests.recv_timeout(silent).is_err());
        assert_eq!(crate::sources::admitted_intents(&state).unwrap().len(), 1);
        fs::set_permissions(&ledger, fs::Permissions::from_mode(0o700)).unwrap();
        drop(exec);
        fs::remove_dir_all(d).unwrap();
        fs::remove_dir_all(state).unwrap();
    }
}
