//! Phase H: Strategy C in the runtime (component `BSC_STRATEGY_C`, SHADOW:
//! a PAPER ledger; nothing is signed or sent).
//!
//! The engine (`trading::strategy::engine`) is fed from the canonical
//! records the runtime journals, decoded back by [`input_of_record`]: the
//! blocks (`B`), the Flap launches with their creators (`N`), the token
//! terms (`F`), the trades (`T`), the migrations (`M`), and every record
//! that makes the Flap events incomplete (`W`, a reorg `R`, a resync `G`).
//! Its outputs are journaled as `Y` records. A replay of a journal feeds
//! the same function the same records, so it reproduces the runtime's
//! decisions exactly or shows where it does not.

use alloy_primitives::U256;
use darkbot_bsc_journal::RecordKind;
use darkbot_bsc_trading::economics::flap::position::Outcome;
use darkbot_bsc_trading::economics::network::GasEvidence;
use darkbot_bsc_trading::strategy::book::{Term, Trade, Untracked};
use darkbot_bsc_trading::strategy::engine::{
    Cancel, Config, CreatorSoldBeforeFill, Decision, Engine, ExitReason, Input, Output,
};

use crate::chain_follower::FlapTerm;
use crate::paper_exec::{IntentKind, PaperIntent, StrategyCurve};
use crate::runtime_journal::{RuntimeJournal, flap_term_of, flap_trade_of};

/// A canonical block (`B | number u64 | hash [32] | time_ms u64`).
pub fn block_payload(number: u64, hash: &[u8; 32], time_ms: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(BLOCK_LEN);
    out.push(b'B');
    out.extend_from_slice(&number.to_le_bytes());
    out.extend_from_slice(hash);
    out.extend_from_slice(&time_ms.to_le_bytes());
    out
}

pub const BLOCK_LEN: usize = 49;

/// A Flap launch with its creator (`N | block u64 | tx_index u64 | token
/// [20] | creator [20]`).
pub fn flap_launch_payload(
    block: u64,
    tx_index: u64,
    token: &[u8; 20],
    creator: &[u8; 20],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(LAUNCH_LEN);
    out.push(b'N');
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&tx_index.to_le_bytes());
    out.extend_from_slice(token);
    out.extend_from_slice(creator);
    out
}

pub const LAUNCH_LEN: usize = 57;

/// A resync the strategy must see (`I | new head u64`): no curve it holds
/// stays exact.
pub fn incomplete_payload(new_head: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    out.push(b'I');
    out.extend_from_slice(&new_head.to_le_bytes());
    out
}

fn u64_at(p: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(p.get(at..at + 8)?.try_into().ok()?))
}

fn word(w: &[u8; 32]) -> U256 {
    U256::from_be_bytes(*w)
}

/// The strategy input of one journaled canonical record, if it is one.
pub fn input_of_record(kind: RecordKind, p: &[u8]) -> Option<Input> {
    match (kind, p.first()?) {
        (RecordKind::Canonical, b'B') if p.len() == BLOCK_LEN => Some(Input::Block {
            number: u64_at(p, 1)?,
            time_ms: u64_at(p, 41)?,
        }),
        (RecordKind::Canonical, b'N') if p.len() == LAUNCH_LEN => Some(Input::Launch {
            tx_index: u64_at(p, 9)?,
            token: p.get(17..37)?.try_into().ok()?,
            creator: p.get(37..57)?.try_into().ok()?,
        }),
        (RecordKind::Canonical, b'F') => {
            let t = flap_term_of(p)?;
            let term = match t.term {
                FlapTerm::Version(v) => Term::Version(v),
                FlapTerm::QuoteToken(a) => Term::QuoteToken(a),
                FlapTerm::Curve { r, h, k } => Term::Curve {
                    r: word(&r),
                    h: word(&h),
                    k: word(&k),
                },
                FlapTerm::DexThreshold(w) => Term::DexThreshold(word(&w)),
                FlapTerm::Migrator(m) => Term::Migrator(m),
                FlapTerm::Tax { buy, sell } => Term::Tax {
                    buy: word(&buy),
                    sell: word(&sell),
                },
                FlapTerm::NftGate(nft) => Term::NftGate(nft),
                FlapTerm::HoldingCap { bps, max_amount } => Term::HoldingCap {
                    bps,
                    max_amount: word(&max_amount),
                },
            };
            Some(Input::Term {
                token: t.token,
                term,
            })
        }
        (RecordKind::Canonical, b'T') => {
            let t = flap_trade_of(p)?;
            Some(Input::Trade {
                token: t.token,
                trade: Trade {
                    block_number: t.block_number,
                    tx_index: t.tx_index,
                    log_index: t.log_index,
                    sell: t.sell,
                    trader: t.trader,
                    amount: word(&t.amount),
                    eth: word(&t.eth),
                    fee: word(&t.fee),
                },
            })
        }
        (RecordKind::Canonical, b'M') if p.len() == 77 => Some(Input::Migrated {
            token: p.get(57..77)?.try_into().ok()?,
        }),
        // A refused Flap log, a reorg, a resync: no curve stays exact.
        (RecordKind::Canonical, b'W') if p.len() == 57 => Some(Input::Incomplete),
        (RecordKind::Reorg, b'R') if p.len() == 45 => Some(Input::Incomplete),
        (RecordKind::Reorg, b'G') if p.len() == 17 => Some(Input::Incomplete),
        (RecordKind::Canonical, b'I') if p.len() == 9 => Some(Input::Incomplete),
        _ => None,
    }
}

/// The strategy switched on (`U | 1`) or off (`U | 0`), a `Runtime`
/// record: a replay starts and ends its engine at the same place.
pub fn switch_payload(on: bool) -> Vec<u8> {
    vec![b'U', u8::from(on)]
}

/// The kinds of a `Y` record.
const DECIDED: u8 = 1;
const ENTERED: u8 = 2;
const CANCELLED: u8 = 3;
const TRIGGERED: u8 = 4;
const EXITED: u8 = 5;
const UNRESOLVED: u8 = 6;

pub const OUTPUT_LEN: usize = 1 + 1 + 8 + 20 + 1 + 1 + 4 * 32;

/// A decision carries its 16 features in place of the four words: a tag
/// (0 NULL, 1 number, 2 flag false, 3 flag true) and the value (i128,
/// little-endian) each, so a replay compares them too.
pub const DECIDED_LEN: usize = 1 + 1 + 8 + 20 + 1 + 1 + 16 * 17;

fn decision_code(d: Decision) -> (u8, u8) {
    match d {
        Decision::Pass => (0, 0),
        Decision::Reject(f) => (1, f as u8),
        Decision::DataIncomplete => (2, 0),
        Decision::NotTracked(u) => (3, untracked_code(u)),
        Decision::SlotLimit => (4, 0),
        Decision::CreatorSold => (5, 0),
    }
}

/// Why a token is not tracked: the reason in the high nibble, for a term
/// outside the scope the term in the low one (1 version, 2 quote token,
/// 3 NFT gate, 4 holding cap, 5 curve, 6 tax, 7 threshold, 8 not
/// tradable, 15 another).
fn untracked_code(u: Untracked) -> u8 {
    let (reason, term) = match u {
        Untracked::Unsupported(why) => (
            1,
            match why {
                "version" => 1,
                "quote token" => 2,
                "nft gate" => 3,
                "holding cap" => 4,
                "curve" => 5,
                "tax" => 6,
                "threshold" => 7,
                "not tradable" => 8,
                _ => 15,
            },
        ),
        Untracked::TermsIncomplete => (2, 0),
        Untracked::TradeMismatch => (3, 0),
        Untracked::Migrated => (4, 0),
        Untracked::Gap => (5, 0),
        Untracked::ShadowReplay => (6, 0),
    };
    (reason << 4) | term
}

fn reason_code(r: ExitReason) -> u8 {
    match r {
        ExitReason::CreatorSell => 1,
        ExitReason::Target => 2,
        ExitReason::Stop => 3,
        ExitReason::Timeout => 4,
    }
}

fn decided_payload(o: &Output) -> Vec<u8> {
    let Output::Decided {
        token,
        block,
        decision,
        features,
    } = o
    else {
        return Vec::new();
    };
    use darkbot_bsc_trading::strategy::features::Value;
    let (code, detail) = decision_code(*decision);
    let mut out = Vec::with_capacity(DECIDED_LEN);
    out.push(b'Y');
    out.push(DECIDED);
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(token);
    out.push(code);
    out.push(detail);
    for v in features.iter() {
        let (tag, value) = match v {
            Value::Null => (0u8, 0i128),
            Value::Num(n) => (1, *n),
            Value::Flag(false) => (2, 0),
            Value::Flag(true) => (3, 0),
        };
        out.push(tag);
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// A strategy output (`Y | kind u8 | block u64 | token [20] | code u8 |
/// detail u8 | four words`): a decision (verdict, failed feature or
/// reason; its 16 features in place of the words, [`DECIDED_LEN`]), an
/// entry (tokens, input, network fee, spent), a cancel, an exit trigger, an
/// exit (proceeds, network fee, the result, 0 gain / 1 loss), an
/// unresolved position.
pub fn output_payload(o: &Output) -> Vec<u8> {
    let (kind, block, token, code, detail, words): (u8, u64, [u8; 20], u8, u8, [U256; 4]) = match o
    {
        Output::Decided { .. } => return decided_payload(o),
        Output::Entered {
            token,
            block,
            entry,
        } => (
            ENTERED,
            *block,
            *token,
            0,
            0,
            [
                entry.tokens,
                entry.input_wei,
                entry.network_fee_wei,
                entry.spent_wei,
            ],
        ),
        Output::EntryCancelled {
            token,
            block,
            cancel,
        } => {
            let (c, d) = match cancel {
                Cancel::CreatorSold => (1, 0),
                Cancel::NotTracked(u) => (2, untracked_code(*u)),
                Cancel::Quote(_) => (3, 0),
                Cancel::Cost(_) => (4, 0),
            };
            (CANCELLED, *block, *token, c, d, [U256::ZERO; 4])
        }
        Output::ExitTriggered {
            token,
            block,
            reason,
        } => (
            TRIGGERED,
            *block,
            *token,
            reason_code(*reason),
            0,
            [U256::ZERO; 4],
        ),
        Output::Exited {
            token,
            block,
            reason,
            exit,
            outcome,
        } => {
            let (sign, amount) = match outcome {
                Outcome::Gain(a) => (0, *a),
                Outcome::Loss(a) => (1, *a),
            };
            (
                EXITED,
                *block,
                *token,
                reason_code(*reason),
                sign,
                [exit.proceeds_wei, exit.network_fee_wei, amount, U256::ZERO],
            )
        }
        Output::Unresolved {
            token,
            block,
            reason,
        } => (
            UNRESOLVED,
            *block,
            *token,
            untracked_code(*reason),
            0,
            [U256::ZERO; 4],
        ),
    };
    let mut out = Vec::with_capacity(OUTPUT_LEN);
    out.push(b'Y');
    out.push(kind);
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&token);
    out.push(code);
    out.push(detail);
    for w in words {
        out.extend_from_slice(&w.to_be_bytes::<32>());
    }
    out
}

/// The strategy's counters for the runtime status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub launches: u64,
    pub passed: u64,
    pub rejected: u64,
    pub data_incomplete: u64,
    pub not_tracked: u64,
    pub slot_limit: u64,
    pub creator_sold: u64,
    pub entered: u64,
    pub cancelled: u64,
    pub exits_creator_sell: u64,
    pub exits_target: u64,
    pub exits_stop: u64,
    pub exits_timeout: u64,
    pub gains: u64,
    pub losses: u64,
    pub gain_wei: U256,
    pub loss_wei: U256,
    pub unresolved: u64,
    /// The inputs taken: trades, terms, migrations and the records that
    /// made every followed curve incomplete.
    pub trades: u64,
    pub terms: u64,
    pub migrations: u64,
    pub incomplete: u64,
    /// Phase I: decisions for the PAPER path that found no armed C3
    /// executor (`BSC_EXEC` not ARMED): counted, never executed.
    pub paper_unarmed: u64,
}

/// A Strategy C decision for the PAPER path (Phase I): a PASS is a BUY of
/// the configured input on the token's exact curve, an exit trigger the
/// SELL of the position's tokens on its shadow curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrategyAction {
    Buy {
        token: [u8; 20],
        curve: StrategyCurve,
        input_wei: U256,
    },
    Exit {
        token: [u8; 20],
        tokens: U256,
        curve: StrategyCurve,
    },
}

/// A Strategy C PAPER transaction at the sink (Phase I), a `Runtime`
/// record: `J | kind u8 (1 BUY, 2 approval, 3 SELL) | sink sequence u64 |
/// tx_hash [32] | token [20] | decided, admitted, signed, sink ns u64 |
/// the signed bytes` (a local artefact, never sent).
pub fn paper_intent_payload(i: &PaperIntent) -> Vec<u8> {
    let t = i.times;
    let mut out = Vec::with_capacity(1 + 1 + 8 + 32 + 20 + 4 * 8 + i.raw.len());
    out.push(b'J');
    out.push(i.kind.code());
    out.extend_from_slice(&i.handoff.sequence.to_le_bytes());
    out.extend_from_slice(&i.handoff.tx_hash);
    out.extend_from_slice(&i.token);
    for ns in [t.ingress_ns, t.admitted_ns, t.signed_ns, t.sink_ns] {
        out.extend_from_slice(&ns.to_le_bytes());
    }
    out.extend_from_slice(&i.raw);
    out
}

/// The kind of a `J` record, if it is one.
pub fn paper_intent_kind(p: &[u8]) -> Option<IntentKind> {
    match (p.first()?, p.get(1)?) {
        (b'J', 1) => Some(IntentKind::Buy),
        (b'J', 2) => Some(IntentKind::Approve),
        (b'J', 3) => Some(IntentKind::Sell),
        _ => None,
    }
}

/// The strategy component of one run.
pub struct StrategyRuntime {
    engine: Engine,
    pub counters: Counters,
    input_wei: U256,
    /// Decisions for the PAPER path since the last `take_actions`
    /// (collected on the runtime path only, not in a replay).
    actions: Vec<StrategyAction>,
}

/// The status section of a strategy that runs, or of one refused (no
/// runtime, with its reason).
pub fn status_draft(
    strategy: Option<&StrategyRuntime>,
    refused: Option<&str>,
    config: &Config,
) -> darkbot_bsc_record::Draft {
    use darkbot_bsc_record::Value;
    use darkbot_bsc_record::schema::bsc_runtime::runtime_status::strategy as st;
    let c = strategy.map(|s| s.counters).unwrap_or_default();
    let mut d = darkbot_bsc_record::Draft::new(&st::SCHEMA);
    d.set(
        st::STATE,
        if strategy.is_some() {
            "SHADOW"
        } else {
            "GAS_EVIDENCE_REFUSED"
        },
    )
    .set(st::FOLLOWED, strategy.map_or(0, |s| s.followed() as u64))
    .set(st::LAUNCHES, c.launches)
    .set(st::PASSED, c.passed)
    .set(st::REJECTED, c.rejected)
    .set(st::DATA_INCOMPLETE, c.data_incomplete)
    .set(st::NOT_TRACKED, c.not_tracked)
    .set(st::SLOT_LIMIT, c.slot_limit)
    .set(st::CREATOR_SOLD, c.creator_sold)
    .set(st::ENTERED, c.entered)
    .set(st::CANCELLED, c.cancelled)
    .set(st::EXITS_CREATOR_SELL, c.exits_creator_sell)
    .set(st::EXITS_TARGET, c.exits_target)
    .set(st::EXITS_STOP, c.exits_stop)
    .set(st::EXITS_TIMEOUT, c.exits_timeout)
    .set(st::GAINS, c.gains)
    .set(st::LOSSES, c.losses)
    .set(st::GAIN_WEI, Value::U256(c.gain_wei.to_be_bytes()))
    .set(st::LOSS_WEI, Value::U256(c.loss_wei.to_be_bytes()))
    .set(st::UNRESOLVED, c.unresolved)
    .set(st::OBSERVATION_MS, config.observation_ms)
    .set(st::MAX_OPEN, config.max_open as u64)
    .set(st::INPUT_WEI, Value::U256(config.input_wei.to_be_bytes()))
    .set(st::TRADES, c.trades)
    .set(st::TERMS, c.terms)
    .set(st::MIGRATIONS, c.migrations)
    .set(st::INCOMPLETE, c.incomplete)
    .set(
        st::OPEN,
        strategy.map_or(0, |s| s.engine.open_positions() as u64),
    )
    .set(st::PAPER_UNARMED, c.paper_unarmed);
    if let (None, Some(why)) = (strategy, refused) {
        d.set(st::REASON, why);
    }
    d
}

impl StrategyRuntime {
    pub fn new(config: Config, gas: GasEvidence) -> Self {
        Self {
            input_wei: config.input_wei,
            engine: Engine::new(config, gas),
            counters: Counters::default(),
            actions: Vec::new(),
        }
    }

    /// The decisions for the PAPER path since the last call (Phase I).
    pub fn take_actions(&mut self) -> Vec<StrategyAction> {
        std::mem::take(&mut self.actions)
    }

    /// The PAPER action of one output, on the engine's state right after
    /// the record that produced it.
    fn action_of(&self, o: &Output) -> Option<StrategyAction> {
        let curve = |token: &[u8; 20]| {
            self.engine
                .curve_of(token)
                .map(|(terms, reserve, circulating)| StrategyCurve {
                    terms,
                    reserve,
                    circulating,
                })
        };
        match o {
            Output::Decided {
                token,
                decision: Decision::Pass,
                ..
            } => Some(StrategyAction::Buy {
                token: *token,
                curve: curve(token)?,
                input_wei: self.input_wei,
            }),
            Output::ExitTriggered { token, .. } => Some(StrategyAction::Exit {
                token: *token,
                tokens: self.engine.position_tokens(token)?,
                curve: curve(token)?,
            }),
            _ => None,
        }
    }

    pub fn followed(&self) -> usize {
        self.engine.followed()
    }

    /// One journaled record: its input to the engine, the outputs journaled
    /// (when a journal is given) and counted.
    pub fn record(
        &mut self,
        kind: RecordKind,
        payload: &[u8],
        at: u64,
        journal: Option<&mut RuntimeJournal>,
    ) -> Vec<Vec<u8>> {
        let Some(input) = input_of_record(kind, payload) else {
            return Vec::new();
        };
        let c = &mut self.counters;
        match input {
            Input::Launch { .. } => c.launches += 1,
            Input::Trade { .. } => c.trades += 1,
            Input::Term { .. } => c.terms += 1,
            Input::Migrated { .. } => c.migrations += 1,
            Input::Incomplete => c.incomplete += 1,
            Input::Block { .. } => {}
        }
        let typed = self.engine.step(input);
        let outputs: Vec<Vec<u8>> = typed
            .iter()
            .map(|o| {
                self.count(o);
                output_payload(o)
            })
            .collect();
        if journal.is_some() {
            let actions: Vec<StrategyAction> =
                typed.iter().filter_map(|o| self.action_of(o)).collect();
            self.actions.extend(actions);
        }
        if let Some(j) = journal {
            for p in &outputs {
                j.record(RecordKind::Runtime, at, p.clone());
            }
        }
        outputs
    }

    fn count(&mut self, o: &Output) {
        let c = &mut self.counters;
        match o {
            Output::Decided { decision, .. } => match decision {
                Decision::Pass => c.passed += 1,
                Decision::Reject(_) => c.rejected += 1,
                Decision::DataIncomplete => c.data_incomplete += 1,
                Decision::NotTracked(_) => c.not_tracked += 1,
                Decision::SlotLimit => c.slot_limit += 1,
                Decision::CreatorSold => c.creator_sold += 1,
            },
            Output::Entered { .. } => c.entered += 1,
            Output::EntryCancelled { .. } => c.cancelled += 1,
            Output::ExitTriggered { .. } => {}
            Output::Exited {
                reason, outcome, ..
            } => {
                match reason {
                    ExitReason::CreatorSell => c.exits_creator_sell += 1,
                    ExitReason::Target => c.exits_target += 1,
                    ExitReason::Stop => c.exits_stop += 1,
                    ExitReason::Timeout => c.exits_timeout += 1,
                }
                match outcome {
                    Outcome::Gain(a) => {
                        c.gains += 1;
                        c.gain_wei = c.gain_wei.saturating_add(*a);
                    }
                    Outcome::Loss(a) => {
                        c.losses += 1;
                        c.loss_wei = c.loss_wei.saturating_add(*a);
                    }
                }
            }
            Output::Unresolved { .. } => c.unresolved += 1,
        }
    }
}

/// The replay of one run's journal through Strategy C: the outputs it
/// produces from the journaled records against the `Y` records the runtime
/// journaled, and the PAPER result of the replay.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Replay {
    pub run_id: String,
    pub records: u64,
    pub starts: u64,
    pub journaled: u64,
    pub replayed: u64,
    pub first_difference: Option<u64>,
    pub config_match: Option<bool>,
    pub read_error: Option<String>,
    pub counters: Counters,
    /// PAPER positions still open when the strategy's periods ended.
    pub open_at_end: u64,
}

/// Replays the run `run_id` under `journal_dir`; `config_sha256` is the
/// configuration replayed with, compared with the run manifest's.
pub fn replay(
    journal_dir: &std::path::Path,
    run_id: &str,
    max_payload: usize,
    config: &Config,
    gas: &GasEvidence,
    config_sha256: Option<[u8; 32]>,
) -> Result<Replay, String> {
    use darkbot_bsc_journal::storage::run::RunReader;
    use darkbot_bsc_record::schema::bsc_capture::capture_run as cr;
    let run = journal_dir.join(run_id);
    if !run.is_dir() {
        return Err(format!("{}: no run directory", run.display()));
    }
    let mut out = Replay {
        run_id: run_id.to_string(),
        ..Replay::default()
    };
    if let Ok(bytes) = std::fs::read(run.join(crate::runtime_journal::RUN_MANIFEST))
        && let Ok(m) = cr::decode_run(&bytes)
    {
        out.config_match = m
            .opt_b256(cr::CONFIG_SHA256)
            .ok()
            .flatten()
            .zip(config_sha256)
            .map(|(a, b)| a == b);
    }
    let mut reader = RunReader::open(&run, max_payload).map_err(|e| e.to_string())?;
    let mut strategy: Option<StrategyRuntime> = None;
    let mut journaled: Vec<Vec<u8>> = Vec::new();
    let mut replayed: Vec<Vec<u8>> = Vec::new();
    loop {
        let r = match reader.next_record() {
            Ok(Some(r)) => r,
            Ok(None) => break,
            Err(e) => {
                out.read_error = Some(e.to_string());
                break;
            }
        };
        out.records += 1;
        match (r.kind, &*r.payload) {
            (RecordKind::Runtime, [b'U', on]) => {
                // The counters of every period the strategy ran add up.
                if let Some(s) = strategy.take() {
                    add(&mut out.counters, &s.counters);
                    out.open_at_end += s.engine.open_positions() as u64;
                }
                if *on == 1 {
                    out.starts += 1;
                    strategy = Some(StrategyRuntime::new(config.clone(), gas.clone()));
                }
            }
            (RecordKind::Runtime, p) if p.first() == Some(&b'Y') => journaled.push(p.to_vec()),
            (kind, p) => {
                if let Some(s) = strategy.as_mut() {
                    replayed.extend(s.record(kind, p, r.timestamp_ns, None));
                }
            }
        }
    }
    if let Some(s) = strategy {
        add(&mut out.counters, &s.counters);
        out.open_at_end += s.engine.open_positions() as u64;
    }
    out.journaled = journaled.len() as u64;
    out.replayed = replayed.len() as u64;
    out.first_difference = journaled
        .iter()
        .zip(&replayed)
        .position(|(a, b)| a != b)
        .or_else(|| {
            (journaled.len() != replayed.len()).then(|| journaled.len().min(replayed.len()))
        })
        .map(|i| i as u64);
    Ok(out)
}

fn add(sum: &mut Counters, c: &Counters) {
    for (a, b) in [
        (&mut sum.launches, c.launches),
        (&mut sum.passed, c.passed),
        (&mut sum.rejected, c.rejected),
        (&mut sum.data_incomplete, c.data_incomplete),
        (&mut sum.not_tracked, c.not_tracked),
        (&mut sum.slot_limit, c.slot_limit),
        (&mut sum.creator_sold, c.creator_sold),
        (&mut sum.entered, c.entered),
        (&mut sum.cancelled, c.cancelled),
        (&mut sum.exits_creator_sell, c.exits_creator_sell),
        (&mut sum.exits_target, c.exits_target),
        (&mut sum.exits_stop, c.exits_stop),
        (&mut sum.exits_timeout, c.exits_timeout),
        (&mut sum.gains, c.gains),
        (&mut sum.losses, c.losses),
        (&mut sum.unresolved, c.unresolved),
        (&mut sum.trades, c.trades),
        (&mut sum.terms, c.terms),
        (&mut sum.migrations, c.migrations),
        (&mut sum.incomplete, c.incomplete),
    ] {
        *a = a.saturating_add(b);
    }
    sum.gain_wei = sum.gain_wei.saturating_add(c.gain_wei);
    sum.loss_wei = sum.loss_wei.saturating_add(c.loss_wei);
}

impl Replay {
    /// `PASS` only when every journaled output was reproduced and nothing
    /// else, from a journal read to its end with the run's configuration.
    pub fn verdict(&self) -> &'static str {
        if self.read_error.is_some() {
            "INCOMPLETE"
        } else if self.starts == 0 {
            "NOT_RUN"
        } else if self.config_match.is_none() {
            // No run manifest names the configuration: nothing says the
            // replay ran with the run's (H gate L6).
            "INCOMPLETE"
        } else if self.first_difference.is_some() || self.config_match == Some(false) {
            "DIFFERS"
        } else {
            "PASS"
        }
    }

    pub fn reasons(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(e) = &self.read_error {
            out.push(format!("the journal does not read to its end: {e}"));
        }
        if self.config_match == Some(false) {
            out.push("the run's configuration differs from the one replayed with".into());
        }
        if self.config_match.is_none() {
            out.push("no run manifest names the run's configuration".into());
        }
        if let Some(i) = self.first_difference {
            out.push(format!(
                "output {i} differs (journaled {}, replayed {})",
                self.journaled, self.replayed
            ));
        }
        out
    }

    pub fn encode(&self, observed_unix_ms: u64) -> Result<Vec<u8>, String> {
        use darkbot_bsc_record::Value;
        use darkbot_bsc_record::schema::bsc_runtime::strategy_replay as sr;
        let c = &self.counters;
        let mut d = darkbot_bsc_record::Draft::new(&sr::V2);
        d.set(sr::RUN_ID, self.run_id.as_str())
            .set(sr::CHAIN_ID, 56u64)
            .set(sr::PROVENANCE_TOOL, "bsc_runtime strategy-replay")
            .set(sr::PROVENANCE_OBSERVED_UNIX_MS, observed_unix_ms)
            .set(sr::VERDICT, self.verdict())
            .set(sr::RECORDS, self.records)
            .set(sr::STARTS, self.starts)
            .set(sr::JOURNALED, self.journaled)
            .set(sr::REPLAYED, self.replayed)
            .set(sr::LAUNCHES, c.launches)
            .set(sr::PASSED, c.passed)
            .set(sr::REJECTED, c.rejected)
            .set(sr::DATA_INCOMPLETE, c.data_incomplete)
            .set(sr::NOT_TRACKED, c.not_tracked)
            .set(sr::SLOT_LIMIT, c.slot_limit)
            .set(sr::CREATOR_SOLD, c.creator_sold)
            .set(sr::ENTERED, c.entered)
            .set(sr::CANCELLED, c.cancelled)
            .set(sr::EXITS_CREATOR_SELL, c.exits_creator_sell)
            .set(sr::EXITS_TARGET, c.exits_target)
            .set(sr::EXITS_STOP, c.exits_stop)
            .set(sr::EXITS_TIMEOUT, c.exits_timeout)
            .set(sr::GAINS, c.gains)
            .set(sr::LOSSES, c.losses)
            .set(sr::GAIN_WEI, Value::U256(c.gain_wei.to_be_bytes()))
            .set(sr::LOSS_WEI, Value::U256(c.loss_wei.to_be_bytes()))
            .set(sr::UNRESOLVED, c.unresolved)
            .set(sr::TRADES, c.trades)
            .set(sr::TERMS, c.terms)
            .set(sr::MIGRATIONS, c.migrations)
            .set(sr::INCOMPLETE, c.incomplete)
            .set(sr::OPEN_AT_END, self.open_at_end);
        if let Some(i) = self.first_difference {
            d.set(sr::FIRST_DIFFERENCE, i);
        }
        if let Some(m) = self.config_match {
            d.set(sr::CONFIG_MATCH, m);
        }
        for r in self.reasons() {
            d.push(sr::REASONS, r.as_str());
        }
        d.encode().map_err(|e| format!("strategy replay: {e}"))
    }

    pub fn summary(&self) -> String {
        let c = &self.counters;
        format!(
            "strategy replay of run {}: {} (records {}, starts {}, outputs journaled {} replayed {}; inputs: trades {}, terms {}, migrations {}, incomplete {}; launches {}, passed {}, rejected {}, data incomplete {}, not tracked {}, slot limit {}, creator sold {}; PAPER entered {}, cancelled {}, exits creator-sell {} target {} stop {} timeout {}; gains {} ({} wei), losses {} ({} wei), unresolved {}, open at the end {} (not valued))\n{}",
            self.run_id,
            self.verdict(),
            self.records,
            self.starts,
            self.journaled,
            self.replayed,
            c.trades,
            c.terms,
            c.migrations,
            c.incomplete,
            c.launches,
            c.passed,
            c.rejected,
            c.data_incomplete,
            c.not_tracked,
            c.slot_limit,
            c.creator_sold,
            c.entered,
            c.cancelled,
            c.exits_creator_sell,
            c.exits_target,
            c.exits_stop,
            c.exits_timeout,
            c.gains,
            c.gain_wei,
            c.losses,
            c.loss_wei,
            c.unresolved,
            self.open_at_end,
            self.reasons().join("\n"),
        )
    }
}

/// `FREE_CANCEL` or `TX_LANDS` (`BSC_STRATEGY_C_CREATOR_SOLD_BEFORE_FILL`).
pub fn creator_sold_policy(text: &str) -> Result<CreatorSoldBeforeFill, String> {
    match text {
        "FREE_CANCEL" => Ok(CreatorSoldBeforeFill::FreeCancel),
        "TX_LANDS" => Ok(CreatorSoldBeforeFill::TxLands),
        other => Err(format!(
            "BSC_STRATEGY_C_CREATOR_SOLD_BEFORE_FILL={other} is not FREE_CANCEL or TX_LANDS"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain_follower::{CanonicalTerm, CanonicalTrade};
    use crate::runtime_journal::{flap_term_payload, flap_trade_payload, trade_gap_payload};
    use darkbot_bsc_record::schema::bsc_runtime::runtime_status::strategy as st;
    use darkbot_bsc_trading::economics::flap::native_curve20::{curve20, quote_native_buy};
    use darkbot_bsc_trading::economics::network::{Action, PathClass, Venue};
    use darkbot_bsc_trading::strategy::book::TokenBook;
    use darkbot_bsc_trading::strategy::filters::parse_rules;

    const TOKEN: [u8; 20] = [0xAA; 20];
    const CREATOR: [u8; 20] = [0xCC; 20];
    const HASH: [u8; 32] = [7; 32];

    fn config() -> Config {
        Config {
            observation_ms: 3_000,
            ratio_places: 4,
            first_buyer_window: 5,
            rules: parse_rules("", 4).unwrap(),
            target_net_multiple: 15_000,
            stop_net_multiple: 5_000,
            hold_ms: 60_000,
            max_open: 2,
            input_wei: U256::from(10_000_000_000_000_000u64),
            fill_delay_blocks: 1,
            creator_sold_before_fill: CreatorSoldBeforeFill::FreeCancel,
            gas_price_wei: U256::from(100_000_000u64),
        }
    }

    fn gas() -> GasEvidence {
        let mut gas = GasEvidence::default();
        for (action, used) in [
            (Action::Buy, 284_720),
            (Action::Approve, 49_248),
            (Action::Sell, 356_630),
        ] {
            gas.add(
                PathClass {
                    venue: Venue::Flap,
                    action,
                },
                used,
            )
            .unwrap();
        }
        gas
    }

    fn runtime() -> StrategyRuntime {
        StrategyRuntime::new(config(), gas())
    }

    fn be(v: U256) -> [u8; 32] {
        v.to_be_bytes()
    }

    /// The journaled records of a launch whose creator buys in its block:
    /// the block, the launch, the terms and the trade, each built by the
    /// runtime's own payload function.
    fn launch_records(block: u64) -> Vec<(RecordKind, Vec<u8>)> {
        let c = curve20();
        let terms = [
            FlapTerm::Version(6),
            FlapTerm::QuoteToken([0; 20]),
            FlapTerm::Curve {
                r: be(c.r),
                h: be(c.h),
                k: be(c.k),
            },
            FlapTerm::DexThreshold(be(
                U256::from(8u64) * U256::from(10u64).pow(U256::from(26u64))
            )),
            FlapTerm::Tax {
                buy: be(U256::from(100u8)),
                sell: be(U256::from(100u8)),
            },
        ];
        let mut out = vec![
            (RecordKind::Canonical, block_payload(block, &HASH, 0)),
            (
                RecordKind::Canonical,
                flap_launch_payload(block, 1, &TOKEN, &CREATOR),
            ),
        ];
        let mut book = TokenBook::new();
        for (i, term) in terms.into_iter().enumerate() {
            let t = CanonicalTerm {
                block_number: block,
                block_hash: HASH,
                tx_index: 1,
                log_index: i as u64,
                token: TOKEN,
                term,
            };
            let Some(Input::Term { term, .. }) =
                input_of_record(RecordKind::Canonical, &flap_term_payload(&t))
            else {
                panic!("term {i}")
            };
            book.term(term);
            out.push((RecordKind::Canonical, flap_term_payload(&t)));
        }
        let eth = U256::from(5_000_000_000_000_000u64);
        let q = quote_native_buy(&book.buy_inputs(eth).unwrap()).unwrap();
        let trade = CanonicalTrade {
            block_number: block,
            block_hash: HASH,
            tx_index: 2,
            tx_hash: [9; 32],
            log_index: 0,
            origin: CREATOR,
            sell: false,
            token: TOKEN,
            trader: CREATOR,
            amount: be(q.output_amount),
            eth: be(eth),
            fee: be(q.protocol_fee),
            post_price: [0; 32],
        };
        out.push((RecordKind::Canonical, flap_trade_payload(&trade)));
        out
    }

    fn feed(s: &mut StrategyRuntime, records: &[(RecordKind, Vec<u8>)]) -> Vec<Vec<u8>> {
        records
            .iter()
            .flat_map(|(k, p)| s.record(*k, p, 0, None))
            .collect()
    }

    fn block(number: u64) -> (RecordKind, Vec<u8>) {
        (
            RecordKind::Canonical,
            block_payload(number, &HASH, (number - 100) * 750),
        )
    }

    /// Every runtime record decodes to its input; other records are none.
    #[test]
    fn records_decode_to_their_inputs() {
        for (term, expected) in [
            (FlapTerm::NftGate([5; 20]), Term::NftGate([5; 20])),
            (
                FlapTerm::HoldingCap {
                    bps: 200,
                    max_amount: be(U256::from(77u8)),
                },
                Term::HoldingCap {
                    bps: 200,
                    max_amount: U256::from(77u8),
                },
            ),
        ] {
            let t = CanonicalTerm {
                block_number: 5,
                block_hash: HASH,
                tx_index: 1,
                log_index: 2,
                token: TOKEN,
                term,
            };
            assert_eq!(
                crate::runtime_journal::flap_term_of(&flap_term_payload(&t)),
                Some(t)
            );
            assert_eq!(
                input_of_record(RecordKind::Canonical, &flap_term_payload(&t)),
                Some(Input::Term {
                    token: TOKEN,
                    term: expected
                })
            );
        }
        assert_eq!(
            input_of_record(RecordKind::Canonical, &block_payload(5, &HASH, 750)),
            Some(Input::Block {
                number: 5,
                time_ms: 750
            })
        );
        assert_eq!(
            input_of_record(
                RecordKind::Canonical,
                &flap_launch_payload(5, 3, &TOKEN, &CREATOR)
            ),
            Some(Input::Launch {
                token: TOKEN,
                creator: CREATOR,
                tx_index: 3
            })
        );
        for (kind, p) in [
            (RecordKind::Canonical, trade_gap_payload(5, &HASH, 1, 2)),
            (RecordKind::Canonical, incomplete_payload(9)),
            (
                RecordKind::Reorg,
                crate::runtime_journal::reorg_payload(2, 9, &HASH),
            ),
            (
                RecordKind::Reorg,
                crate::runtime_journal::resync_payload(3, 8),
            ),
        ] {
            assert_eq!(input_of_record(kind, &p), Some(Input::Incomplete));
        }
        for (kind, p) in [
            (
                RecordKind::Canonical,
                block_payload(5, &HASH, 0)[..48].to_vec(),
            ),
            (RecordKind::Canonical, vec![b'I'; 3]),
            (RecordKind::Canonical, vec![b'W'; 9]),
            (RecordKind::Reorg, vec![b'G'; 10]),
            (RecordKind::Runtime, block_payload(5, &HASH, 0)),
            (RecordKind::Canonical, Vec::new()),
        ] {
            assert_eq!(input_of_record(kind, &p), None);
        }
    }

    /// A launch fed from its journaled records: PASS after the window, the
    /// PAPER entry one block later; every output is one `Y` record.
    #[test]
    fn a_launch_from_its_records_is_decided_and_entered() {
        let mut s = runtime();
        let mut out = feed(&mut s, &launch_records(100));
        for b in 101..=106 {
            out.extend(feed(&mut s, &[block(b)]));
        }
        out.extend(feed(&mut s, &[block(107)]));
        let kinds: Vec<(u8, u64, u8)> = out
            .iter()
            .map(|p| (p[1], u64_at(p, 2).unwrap(), p[30]))
            .collect();
        assert_eq!(kinds, [(DECIDED, 105, 0), (ENTERED, 106, 0)]);
        assert_eq!((out[0].len(), out[1].len()), (DECIDED_LEN, OUTPUT_LEN));
        // Without a journal (a replay) no PAPER action is collected.
        assert!(s.take_actions().is_empty());
        assert!(out.iter().all(|p| p[0] == b'Y'));
        assert_eq!(&out[0][10..30], &TOKEN);
        assert_eq!(
            U256::from_be_slice(&out[1][32 + 32..32 + 64]),
            config().input_wei
        );
        assert_eq!(
            (s.counters.launches, s.counters.passed, s.counters.entered),
            (1, 1, 1)
        );
        assert_eq!(s.followed(), 1);
    }

    /// A refused trade log before the decision makes it DATA_INCOMPLETE.
    #[test]
    fn a_refused_log_is_data_incomplete() {
        let mut s = runtime();
        let mut out = feed(&mut s, &launch_records(100));
        out.extend(feed(
            &mut s,
            &[
                block(101),
                (RecordKind::Canonical, trade_gap_payload(101, &HASH, 0, 0)),
            ],
        ));
        for b in 102..=105 {
            out.extend(feed(&mut s, &[block(b)]));
        }
        assert_eq!(out.len(), 1);
        assert_eq!((out[0][1], out[0][30]), (DECIDED, 2));
        assert_eq!(s.counters.data_incomplete, 1);
    }

    /// The status section of a running and of a refused strategy.
    #[test]
    fn the_status_section_encodes() {
        let mut s = runtime();
        feed(&mut s, &launch_records(100));
        let r = darkbot_bsc_record::decode(
            &st::SCHEMA,
            &status_draft(Some(&s), None, &config()).encode().unwrap(),
        )
        .unwrap();
        assert_eq!(
            r.get(st::STATE),
            Some(&darkbot_bsc_record::Value::Text("SHADOW".into()))
        );
        assert_eq!(
            r.get(st::LAUNCHES),
            Some(&darkbot_bsc_record::Value::U64(1))
        );
        assert_eq!(r.get(st::REASON), None);
        let r = darkbot_bsc_record::decode(
            &st::SCHEMA,
            &status_draft(None, Some("no gas evidence"), &config())
                .encode()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            r.get(st::STATE),
            Some(&darkbot_bsc_record::Value::Text(
                "GAS_EVIDENCE_REFUSED".into()
            ))
        );
        assert_eq!(
            r.get(st::REASON),
            Some(&darkbot_bsc_record::Value::Text("no gas evidence".into()))
        );
    }

    /// A run journaled as the runtime does (a record, then its outputs;
    /// the switches as `U`) replays to the same outputs; what ran before
    /// the switch is not replayed; another configuration differs.
    #[test]
    fn a_journaled_run_replays_exactly() {
        use crate::runtime_journal::{EXIT_PAYLOAD, JournalSetup, RunIdentity};
        let base = std::env::temp_dir().join(format!("bsc-strategy-replay-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let max = 16_384;
        let mut j = RuntimeJournal::open(&base, 7, 1, JournalSetup::whole(64, max)).unwrap();
        j.set_identity(RunIdentity {
            host: "test".into(),
            build: "test".into(),
            config_sha256: [0xC0; 32],
            build_sha256: None,
            build_dirty: None,
            capture_pending: "VENUES",
        });
        // Before the switch: journaled only.
        for (k, p) in launch_records(90) {
            assert!(j.record(k, 1, p));
        }
        assert!(j.record(RecordKind::Runtime, 2, switch_payload(true)));
        let mut s = runtime();
        let mut records = launch_records(100);
        records.extend((101..=107).map(block));
        for (k, p) in records {
            assert!(j.record(k, 3, p.clone()));
            s.record(k, &p, 3, Some(&mut j));
        }
        // Phase I: the PASS is one BUY action of the configured input on
        // the token's curve at the decision.
        let actions = s.take_actions();
        assert!(
            matches!(
                actions.as_slice(),
                [StrategyAction::Buy { token, input_wei, .. }]
                    if *token == TOKEN && *input_wei == config().input_wei
            ),
            "{actions:?}"
        );
        assert!(j.record(RecordKind::Runtime, 4, switch_payload(false)));
        // After it: journaled only.
        assert!(j.record(RecordKind::Canonical, 5, block_payload(108, &HASH, 6_000)));
        assert!(j.record(RecordKind::Runtime, 6, EXIT_PAYLOAD.to_vec()));
        j.finish().unwrap();

        let r = replay(&base, "7", max, &config(), &gas(), Some([0xC0; 32])).unwrap();
        assert_eq!(r.verdict(), "PASS", "{}", r.summary());
        assert_eq!((r.starts, r.journaled, r.replayed), (1, 2, 2));
        assert_eq!(r.counters, s.counters);
        // H gate L10: the position entered at 106 was still open at the
        // switch.
        assert_eq!(r.open_at_end, 1);
        let rec = darkbot_bsc_record::schema::bsc_runtime::strategy_replay::decode_replay(
            &r.encode(1).unwrap(),
        )
        .unwrap();
        assert_eq!(
            rec.get(darkbot_bsc_record::schema::bsc_runtime::strategy_replay::OPEN_AT_END),
            Some(&darkbot_bsc_record::Value::U64(1))
        );
        assert_eq!(
            rec.get(darkbot_bsc_record::schema::bsc_runtime::strategy_replay::VERDICT),
            Some(&darkbot_bsc_record::Value::Text("PASS".into()))
        );

        let mut other = config();
        other.input_wei = U256::from(20_000_000_000_000_000u64);
        let r = replay(&base, "7", max, &other, &gas(), Some([0xC0; 32])).unwrap();
        assert_eq!((r.verdict(), r.first_difference), ("DIFFERS", Some(1)));
        let r = replay(&base, "7", max, &config(), &gas(), Some([0xC1; 32])).unwrap();
        assert_eq!((r.verdict(), r.config_match), ("DIFFERS", Some(false)));
        // H gate MEDIUM 3: the decision's features are part of its record,
        // so a feature computed differently is a difference.
        let mut filtered = config();
        // The creator's share rounded at 2 places instead of 4.
        filtered.ratio_places = 2;
        let r = replay(&base, "7", max, &filtered, &gas(), Some([0xC0; 32])).unwrap();
        assert_eq!((r.verdict(), r.first_difference), ("DIFFERS", Some(0)));
        // H gate L6: without the run manifest the configuration is unknown.
        std::fs::remove_file(base.join("7").join(crate::runtime_journal::RUN_MANIFEST)).unwrap();
        let r = replay(&base, "7", max, &config(), &gas(), Some([0xC0; 32])).unwrap();
        assert_eq!((r.verdict(), r.config_match), ("INCOMPLETE", None));
        std::fs::remove_dir_all(&base).unwrap();
    }

    /// H gate L7: a token outside the scope keeps which term put it there.
    #[test]
    fn the_untracked_reason_keeps_the_term() {
        assert_eq!(untracked_code(Untracked::Unsupported("quote token")), 0x12);
        assert_eq!(untracked_code(Untracked::Unsupported("nft gate")), 0x13);
        assert_eq!(untracked_code(Untracked::Unsupported("holding cap")), 0x14);
        assert_eq!(untracked_code(Untracked::Gap), 0x50);
        let d = output_payload(&Output::Decided {
            token: TOKEN,
            block: 9,
            decision: Decision::NotTracked(Untracked::Unsupported("version")),
            features: Box::new([darkbot_bsc_trading::strategy::features::Value::Null; 16]),
        });
        assert_eq!((d.len(), d[30], d[31]), (DECIDED_LEN, 3, 0x11));
    }

    /// The runtime reads only gas evidence the economics snapshot binds:
    /// the configured files load; any other file or none is refused.
    #[test]
    fn only_frozen_gas_evidence_is_read() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let with = |files: &[&str]| crate::runtime::StrategyConfig {
            engine: config(),
            gas_evidence: files.iter().map(|f| f.to_string()).collect(),
        };
        let text = std::fs::read_to_string(root.join("bsc/config/ingress.public.env")).unwrap();
        let configured = crate::runtime::RuntimeConfig::parse(&text)
            .unwrap()
            .strategy;
        assert!(configured.gas(&root).is_ok());
        for bad in [
            &["bsc/config/ingress.public.env"][..],
            &["bsc/engine/crates/trading/fixtures/bsc/../bsc/flap_gas_20261026.txt"][..],
            &[][..],
        ] {
            assert!(with(bad).gas(&root).is_err(), "{bad:?}");
        }
        // H gate L9: the right path with other content is refused.
        let copy = std::env::temp_dir().join(format!("bsc-gas-bound-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&copy);
        for (path, bytes) in crate::runtime::StrategyConfig::FROZEN_GAS_EVIDENCE {
            std::fs::create_dir_all(copy.join(path).parent().unwrap()).unwrap();
            std::fs::write(copy.join(path), bytes).unwrap();
        }
        assert!(configured.gas(&copy).is_ok());
        let flap = crate::runtime::StrategyConfig::FROZEN_GAS_EVIDENCE[1].0;
        let mut changed = std::fs::read(copy.join(flap)).unwrap();
        changed.extend_from_slice(b"FLAP BUY 1 aa bb 1 100 50 1 2 cc dd\n");
        std::fs::write(copy.join(flap), changed).unwrap();
        let refused = configured.gas(&copy).unwrap_err();
        assert!(refused.contains("differs from the frozen"), "{refused}");
        std::fs::remove_dir_all(&copy).unwrap();
    }

    #[test]
    fn the_creator_policy_is_exact() {
        assert_eq!(
            creator_sold_policy("TX_LANDS"),
            Ok(CreatorSoldBeforeFill::TxLands)
        );
        assert!(creator_sold_policy("free_cancel").is_err());
    }
}
