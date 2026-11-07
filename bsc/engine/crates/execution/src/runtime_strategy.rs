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
use darkbot_bsc_trading::strategy::book::{FourCurve, FourPost, Term, Trade, Untracked};
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

/// SC4: why a Four.meme launch's curve is not priced (its code in the `n`
/// record is the index plus one).
pub const FOUR_LAUNCH_REFUSALS: [&str; 5] = [
    "four deployment not validated",
    "four launch unverifiable",
    "four launch not decoded",
    "four launch profile",
    "four token mismatch",
];

/// SC4: a Four.meme launch's curve as its classification on the drift
/// guard's validated deployment names it (the launch curve, its fee
/// terms), or the index of [`FOUR_LAUNCH_REFUSALS`] why not: `raw` the
/// launch's verified signed bytes, `sender` their signer (an OpenFour
/// token's derivation names it), `token` the canonical event's.
pub fn classify_four_launch(
    validated: Option<darkbot_bsc_trading::venues::fourmeme::deployment::ValidatedFourDeployment>,
    venue: crate::canonical_join::Venue,
    raw: Option<&[u8]>,
    sender: Option<[u8; 20]>,
    (token, tx_hash): ([u8; 20], [u8; 32]),
    max_calldata: usize,
) -> Result<FourCurve, usize> {
    use crate::canonical_join::Venue;
    use alloy_primitives::{Address, B256};
    use darkbot_bsc_trading::venues::fourmeme::profile::{
        WITNESSED_MAX_OFFERS, classify_classic_launch, classify_openfour_launch,
    };
    use darkbot_bsc_trading::venues::fourmeme::recognize::{Recognized, recognize};
    let v = validated.ok_or(0usize)?;
    let raw = raw.ok_or(1usize)?;
    let target = darkbot_bsc_geth_ipc::ingress::decode_tx_target(raw).map_err(|_| 1usize)?;
    let value = U256::try_from_be_slice(target.value).ok_or(1usize)?;
    let candidate = B256::from(tx_hash);
    let decoded = recognize(
        v.deployment(),
        target.to.map(Address::from),
        target.data,
        max_calldata,
    );
    let (named, curve) = match (venue, decoded) {
        (Venue::Four, Recognized::ClassicLaunch(d)) => {
            let c = classify_classic_launch(&v, &d, value, candidate).map_err(|_| 3usize)?;
            (c.token(), FourCurve::classic_at(c.curve(), c.fee()))
        }
        (Venue::OpenFour, Recognized::OpenFourLaunch(d)) => {
            let sender = Address::from(sender.ok_or(1usize)?);
            let c =
                classify_openfour_launch(&v, &d, sender, value, candidate).map_err(|_| 3usize)?;
            (
                c.token(),
                FourCurve::openfour_at(c.curve(), WITNESSED_MAX_OFFERS, c.fee()),
            )
        }
        _ => return Err(2),
    };
    if named.into_array() != token {
        return Err(4);
    }
    curve.ok_or(3)
}

/// SC4: a Four.meme launch with its creator and its curve (`n | venue u8 |
/// block u64 | tx_index u64 | token [20] | creator [20] | refused u8 |
/// w0..w3 [4 × 32] | a u16 | b [32] | c u16`): Classic the launch curve's
/// `k t offers 0` and the fee `rate min tax`, OpenFour `k T Q remaining`
/// and `protocol 0 dev`; refused, the code of [`FOUR_LAUNCH_REFUSALS`]
/// (index + 1) and every word zero.
pub fn four_launch_payload(
    venue: crate::canonical_join::Venue,
    (block, tx_index): (u64, u64),
    token: &[u8; 20],
    creator: &[u8; 20],
    curve: Result<FourCurve, usize>,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(FOUR_LAUNCH_LEN);
    out.push(b'n');
    out.push(crate::runtime_journal::venue_byte(venue));
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&tx_index.to_le_bytes());
    out.extend_from_slice(token);
    out.extend_from_slice(creator);
    let (refused, words, (a, b, c)) = match curve {
        Ok(FourCurve::Classic { start, fee, .. }) => (
            0u8,
            [start.k, start.t, start.offers, U256::ZERO],
            (fee.rate_bps, fee.min_fee, fee.tax_bps),
        ),
        Ok(FourCurve::OpenFour { start, fee, .. }) => (
            0u8,
            [
                start.k,
                start.token_reserve,
                start.quote_reserve,
                start.remaining,
            ],
            (fee.protocol_bps, U256::ZERO, fee.dev_bps),
        ),
        Err(i) => (
            u8::try_from(i + 1).unwrap_or(u8::MAX),
            [U256::ZERO; 4],
            (0, U256::ZERO, 0),
        ),
    };
    out.push(refused);
    for w in words {
        out.extend_from_slice(&w.to_be_bytes::<32>());
    }
    out.extend_from_slice(&a.to_le_bytes());
    out.extend_from_slice(&b.to_be_bytes::<32>());
    out.extend_from_slice(&c.to_le_bytes());
    out
}

pub const FOUR_LAUNCH_LEN: usize = 223;

/// An `n` record back: its venue, block, launch index, token, creator and
/// the curve (or why it is not priced); `None` for any other bytes (a Flap
/// venue, an unknown code, a curve the words do not name).
#[allow(clippy::type_complexity)]
pub fn four_launch_of(
    p: &[u8],
) -> Option<(
    crate::canonical_join::Venue,
    u64,
    u64,
    [u8; 20],
    [u8; 20],
    Result<FourCurve, &'static str>,
)> {
    use crate::canonical_join::Venue;
    use darkbot_bsc_trading::economics::four_classic::{ClassicCurve, ClassicFee};
    use darkbot_bsc_trading::economics::openfour::{OpenFourCurve, OpenFourFee};
    if p.len() != FOUR_LAUNCH_LEN || p[0] != b'n' {
        return None;
    }
    let venue = crate::runtime_journal::venue_of_byte(p[1]).filter(|v| *v != Venue::Flap)?;
    let w = |i: usize| U256::from_be_slice(&p[59 + 32 * i..91 + 32 * i]);
    let a = u16::from_le_bytes([p[187], p[188]]);
    let b = U256::from_be_slice(&p[189..221]);
    let c = u16::from_le_bytes([p[221], p[222]]);
    let curve = match (p[58], venue) {
        (0, Venue::Four) => Ok(FourCurve::classic(
            ClassicCurve {
                k: w(0),
                t: w(1),
                offers: w(2),
                sold: w(3),
            },
            ClassicFee {
                rate_bps: a,
                min_fee: b,
                tax_bps: c,
            },
        )?),
        (0, _) => {
            let start = OpenFourCurve {
                k: w(0),
                token_reserve: w(1),
                quote_reserve: w(2),
                remaining: w(3),
            };
            (start.quote_reserve.checked_mul(start.token_reserve)? == start.k && b.is_zero())
                .then_some(())?;
            Ok(FourCurve::openfour(
                start,
                OpenFourFee {
                    protocol_bps: a,
                    dev_bps: c,
                },
            ))
        }
        (code, _) => Err(*FOUR_LAUNCH_REFUSALS.get(usize::from(code) - 1)?),
    };
    Some((
        venue,
        u64::from_le_bytes(p[2..10].try_into().ok()?),
        u64::from_le_bytes(p[10..18].try_into().ok()?),
        p[18..38].try_into().ok()?,
        p[38..58].try_into().ok()?,
        curve,
    ))
}

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

/// Decision 06.11.: a held position's exit rules from its entry, after its
/// `h`, `k` or `q` record (a record without them: the strategy's):
/// `places u32 | target i128 | stop i128 | hold ms u64 | creator-sell exit
/// u8`, little-endian.
const EXITS_LEN: usize = 4 + 16 + 16 + 8 + 1;

fn push_exits(out: &mut Vec<u8>, exits: Option<ExitTerms>) {
    if let Some(x) = exits {
        out.extend_from_slice(&x.ratio_places.to_le_bytes());
        out.extend_from_slice(&x.target_net_multiple.to_le_bytes());
        out.extend_from_slice(&x.stop_net_multiple.to_le_bytes());
        out.extend_from_slice(&x.hold_ms.to_le_bytes());
        out.push(u8::from(x.creator_sell_exit));
    }
}

/// The exit rules of a record of `len` bytes without them: none at `len`,
/// those after it at `len + EXITS_LEN` (as the strategy's check takes
/// them); `None`: neither.
fn exits_after(p: &[u8], len: usize) -> Option<Option<ExitTerms>> {
    if p.len() == len {
        return Some(None);
    }
    let x = p.get(len..).filter(|x| x.len() == EXITS_LEN)?;
    let exits = ExitTerms {
        ratio_places: u32::from_le_bytes(x[0..4].try_into().ok()?),
        target_net_multiple: i128::from_le_bytes(x[4..20].try_into().ok()?),
        stop_net_multiple: i128::from_le_bytes(x[20..36].try_into().ok()?),
        hold_ms: u64::from_le_bytes(x[36..44].try_into().ok()?),
        creator_sell_exit: match x[44] {
            0 => false,
            1 => true,
            _ => return None,
        },
    };
    exits.validate().ok()?;
    Some(Some(exits))
}

/// Plan 28.10.: a real LIVE position held by the strategy, a `Runtime`
/// record: `h | token [20] | tokens [32] | spent wei [32] | filled ms u64`
/// and (decision 06.11.) its entry's exit rules.
pub fn hold_payload(
    token: [u8; 20],
    tokens: U256,
    spent_wei: U256,
    filled_ms: u64,
    exits: Option<ExitTerms>,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(HOLD_LEN + EXITS_LEN);
    out.push(b'h');
    out.extend_from_slice(&token);
    out.extend_from_slice(&tokens.to_be_bytes::<32>());
    out.extend_from_slice(&spent_wei.to_be_bytes::<32>());
    out.extend_from_slice(&filled_ms.to_le_bytes());
    push_exits(&mut out, exits);
    out
}

const HOLD_LEN: usize = 1 + 20 + 32 + 32 + 8;

/// The real LIVE position's end: `u | token [20]`.
pub fn unhold_payload(token: [u8; 20]) -> Vec<u8> {
    let mut out = Vec::with_capacity(21);
    out.push(b'u');
    out.extend_from_slice(&token);
    out
}

/// 01.11. (R1): a real LIVE position held again on its curve as read at
/// the end of the last block applied, a `Runtime` record: `k | token [20] |
/// creator [20] | tradable u8 | version u8 | quote token [20] | NFT gate
/// [20] | r, h, k [32 each] | DEX threshold [32] | buy, sell tax bps u16 |
/// reserve [32] | circulating [32] | tokens [32] | spent wei [32] | filled
/// ms u64` and (decision 06.11.) its entry's exit rules.
pub fn reheld_payload(r: &Reheld) -> Vec<u8> {
    let (t, reserve, circulating) = match r.curve {
        HeldCurve::Flap {
            terms,
            reserve,
            circulating,
        } => (terms, reserve, circulating),
        HeldCurve::Four(curve) => return reheld_four_payload(r, curve),
    };
    let t = &t;
    let mut out = Vec::with_capacity(REHELD_LEN);
    out.push(b'k');
    out.extend_from_slice(&r.token);
    out.extend_from_slice(&r.creator);
    out.push(u8::from(t.tradable));
    out.push(t.version);
    out.extend_from_slice(&t.quote_token);
    out.extend_from_slice(&t.nft_gate);
    for w in [t.curve.r, t.curve.h, t.curve.k, t.dex_supply_threshold] {
        out.extend_from_slice(&w.to_be_bytes::<32>());
    }
    out.extend_from_slice(&t.buy_tax_bps.to_le_bytes());
    out.extend_from_slice(&t.sell_tax_bps.to_le_bytes());
    for w in [reserve, circulating, r.tokens, r.spent_wei] {
        out.extend_from_slice(&w.to_be_bytes::<32>());
    }
    out.extend_from_slice(&r.filled_ms.to_le_bytes());
    push_exits(&mut out, r.exits);
    out
}

/// SC4 d: a real LIVE Four.meme position held again on its curve as read,
/// a `Runtime` record: `q | venue u8 | token [20] | creator [20] | start
/// w0..w3 [4 × 32] | now w0..w3 [4 × 32] | a u16 | b [32] | c u16 | tokens
/// [32] | spent wei [32] | filled ms u64` and (decision 06.11.) its entry's
/// exit rules: Classic `k t offers sold` and the fee `rate min tax`,
/// OpenFour `k T Q remaining` and `protocol 0 dev`.
fn reheld_four_payload(r: &Reheld, curve: FourCurve) -> Vec<u8> {
    use crate::canonical_join::Venue;
    let classic = |c: darkbot_bsc_trading::economics::four_classic::ClassicCurve| {
        [c.k, c.t, c.offers, c.sold]
    };
    let openfour = |c: darkbot_bsc_trading::economics::openfour::OpenFourCurve| {
        [c.k, c.token_reserve, c.quote_reserve, c.remaining]
    };
    let (venue, start, now, (a, b, c)) = match curve {
        FourCurve::Classic { start, now, fee } => (
            Venue::Four,
            classic(start),
            classic(now),
            (fee.rate_bps, fee.min_fee, fee.tax_bps),
        ),
        FourCurve::OpenFour { start, now, fee } => (
            Venue::OpenFour,
            openfour(start),
            openfour(now),
            (fee.protocol_bps, U256::ZERO, fee.dev_bps),
        ),
    };
    let mut out = Vec::with_capacity(REHELD_FOUR_LEN);
    out.push(b'q');
    out.push(crate::runtime_journal::venue_byte(venue));
    out.extend_from_slice(&r.token);
    out.extend_from_slice(&r.creator);
    for w in start.into_iter().chain(now) {
        out.extend_from_slice(&w.to_be_bytes::<32>());
    }
    out.extend_from_slice(&a.to_le_bytes());
    out.extend_from_slice(&b.to_be_bytes::<32>());
    out.extend_from_slice(&c.to_le_bytes());
    for w in [r.tokens, r.spent_wei] {
        out.extend_from_slice(&w.to_be_bytes::<32>());
    }
    out.extend_from_slice(&r.filled_ms.to_le_bytes());
    push_exits(&mut out, r.exits);
    out
}

const REHELD_FOUR_LEN: usize = 1 + 1 + 20 + 20 + 8 * 32 + 2 + 32 + 2 + 2 * 32 + 8;

/// A `q` record back; `None` for any other bytes (a Flap venue, a curve the
/// words do not name).
fn reheld_four_of(p: &[u8]) -> Option<Reheld> {
    use crate::canonical_join::Venue;
    use darkbot_bsc_trading::economics::four_classic::{ClassicCurve, ClassicFee};
    use darkbot_bsc_trading::economics::openfour::{OpenFourCurve, OpenFourFee};
    if p.first() != Some(&b'q') {
        return None;
    }
    let exits = exits_after(p, REHELD_FOUR_LEN)?;
    let venue = crate::runtime_journal::venue_of_byte(p[1])?;
    let w = |i: usize| U256::from_be_slice(&p[42 + 32 * i..74 + 32 * i]);
    let at = 42 + 8 * 32;
    let a = u16::from_le_bytes([p[at], p[at + 1]]);
    let b = U256::from_be_slice(&p[at + 2..at + 34]);
    let c = u16::from_le_bytes([p[at + 34], p[at + 35]]);
    let rest = at + 36;
    let classic = |o: usize| ClassicCurve {
        k: w(o),
        t: w(o + 1),
        offers: w(o + 2),
        sold: w(o + 3),
    };
    let openfour = |o: usize| OpenFourCurve {
        k: w(o),
        token_reserve: w(o + 1),
        quote_reserve: w(o + 2),
        remaining: w(o + 3),
    };
    let curve = match venue {
        Venue::Four => FourCurve::Classic {
            start: classic(0),
            now: classic(4),
            fee: ClassicFee {
                rate_bps: a,
                min_fee: b,
                tax_bps: c,
            },
        },
        Venue::OpenFour if b.is_zero() => FourCurve::OpenFour {
            start: openfour(0),
            now: openfour(4),
            fee: OpenFourFee {
                protocol_bps: a,
                dev_bps: c,
            },
        },
        _ => return None,
    };
    Some(Reheld {
        token: p.get(2..22)?.try_into().ok()?,
        creator: p.get(22..42)?.try_into().ok()?,
        curve: HeldCurve::Four(curve),
        tokens: U256::from_be_slice(p.get(rest..rest + 32)?),
        spent_wei: U256::from_be_slice(p.get(rest + 32..rest + 64)?),
        filled_ms: u64_at(p, rest + 64)?,
        exits,
    })
}

const REHELD_LEN: usize = 1 + 20 + 20 + 1 + 1 + 20 + 20 + 4 * 32 + 2 + 2 + 4 * 32 + 8;

pub use darkbot_bsc_trading::strategy::engine::{ExitTerms, HeldCurve, Reheld};

fn reheld_of(p: &[u8]) -> Option<Reheld> {
    use darkbot_bsc_trading::economics::flap::curve::Curve;
    use darkbot_bsc_trading::economics::flap::native_curve20::TokenTerms;
    let exits = exits_after(p, REHELD_LEN)?;
    let w = |at: usize| p.get(at..at + 32).map(U256::from_be_slice);
    let bps = |at: usize| Some(u16::from_le_bytes(p.get(at..at + 2)?.try_into().ok()?));
    let tradable = match p[41] {
        0 => false,
        1 => true,
        _ => return None,
    };
    let words = 83;
    let after = words + 4 * 32;
    Some(Reheld {
        token: p.get(1..21)?.try_into().ok()?,
        creator: p.get(21..41)?.try_into().ok()?,
        curve: HeldCurve::Flap {
            terms: TokenTerms {
                tradable,
                version: p[42],
                quote_token: p.get(43..63)?.try_into().ok()?,
                nft_gate: p.get(63..83)?.try_into().ok()?,
                curve: Curve {
                    r: w(words)?,
                    h: w(words + 32)?,
                    k: w(words + 64)?,
                },
                dex_supply_threshold: w(words + 96)?,
                buy_tax_bps: bps(after)?,
                sell_tax_bps: bps(after + 2)?,
            },
            reserve: w(after + 4)?,
            circulating: w(after + 36)?,
        },
        tokens: w(after + 68)?,
        spent_wei: w(after + 100)?,
        filled_ms: u64_at(p, after + 132)?,
        exits,
    })
}

/// The strategy input of one journaled canonical record, if it is one.
pub fn input_of_record(kind: RecordKind, p: &[u8]) -> Option<Input> {
    match (kind, p.first()?) {
        (RecordKind::Runtime, b'k') => reheld_of(p).map(Input::Reheld),
        (RecordKind::Runtime, b'q') => reheld_four_of(p).map(Input::Reheld),
        (RecordKind::Runtime, b'h') => Some(Input::Hold {
            exits: exits_after(p, HOLD_LEN)?,
            token: p.get(1..21)?.try_into().ok()?,
            tokens: U256::from_be_slice(p.get(21..53)?),
            spent_wei: U256::from_be_slice(p.get(53..85)?),
            filled_ms: u64_at(p, 85)?,
        }),
        (RecordKind::Runtime, b'u') if p.len() == 21 => Some(Input::Unhold {
            token: p.get(1..21)?.try_into().ok()?,
        }),
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
        // SC4: a Four.meme launch, curve trade, vault phase, refused log.
        (RecordKind::Canonical, b'n') => {
            let (_, _, tx_index, token, creator, curve) = four_launch_of(p)?;
            Some(Input::FourLaunch {
                token,
                creator,
                tx_index,
                curve,
            })
        }
        (RecordKind::Canonical, b'Z') => {
            let t = crate::runtime_journal::four_trade_of(p)?;
            Some(Input::FourTrade {
                token: t.token,
                trade: Trade {
                    block_number: t.block_number,
                    tx_index: t.tx_index,
                    log_index: t.log_index,
                    sell: t.sell,
                    trader: t.trader,
                    amount: word(&t.amount),
                    eth: word(&t.quote),
                    fee: U256::ZERO,
                },
                post: FourPost {
                    remaining: word(&t.remaining),
                    raised: word(&t.raised),
                },
            })
        }
        // The vault entering its trading phase is the launch's; any other
        // phase is off the curve.
        (RecordKind::Canonical, b'y') if p.len() == 79 => {
            let to = p[78];
            (to != darkbot_bsc_trading::venues::fourmeme::profile::OPENFOUR_PHASE_TRADING).then(
                || Input::FourOffCurve {
                    token: p[57..77].try_into().unwrap_or([0; 20]),
                },
            )
        }
        (RecordKind::Canonical, b'z') if p.len() == 57 => Some(Input::FourIncomplete),
        // A refused Flap log: no Flap curve stays exact (SC4: the Four.meme
        // ones are not concerned); a reorg, a resync: no curve stays exact.
        (RecordKind::Canonical, b'W') if p.len() == 57 => Some(Input::FlapIncomplete),
        (RecordKind::Reorg, b'R') if p.len() == 45 => Some(Input::Incomplete),
        (RecordKind::Reorg, b'G') if p.len() == 17 => Some(Input::Incomplete),
        (RecordKind::Canonical, b'I') if p.len() == 9 => Some(Input::Incomplete),
        _ => None,
    }
}

/// The strategy switched on (`U | 1`) or off (`U | 0`), a `Runtime`
/// record: a replay starts and ends its engine at the same place.
/// SC4: the switch on with venues besides Flap carries their mask (`U | 1 |
/// mask`); Flap alone, and off, as before.
pub fn switch_payload(on: bool, venues: StrategyVenues) -> Vec<u8> {
    match on && venues != StrategyVenues::FLAP {
        true => vec![b'U', 1, venues.mask()],
        false => vec![b'U', u8::from(on)],
    }
}

/// SC4: the venues whose launches Strategy C follows: those effective
/// SHADOW (or LIVE) when it was switched on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StrategyVenues {
    pub flap: bool,
    pub four: bool,
    pub openfour: bool,
}

impl StrategyVenues {
    pub const FLAP: Self = Self {
        flap: true,
        four: false,
        openfour: false,
    };

    pub fn any(self) -> bool {
        self.flap || self.four || self.openfour
    }

    fn mask(self) -> u8 {
        u8::from(self.flap) | u8::from(self.four) << 1 | u8::from(self.openfour) << 2
    }

    /// The venues of a switch record's mask; `None` for no venue or an
    /// unknown bit.
    pub fn of_mask(mask: u8) -> Option<Self> {
        (mask != 0 && mask < 8).then_some(Self {
            flap: mask & 1 != 0,
            four: mask & 2 != 0,
            openfour: mask & 4 != 0,
        })
    }

    /// Whether Strategy C follows `venue`'s launches and trades.
    pub fn takes(self, venue: crate::canonical_join::Venue) -> bool {
        use crate::canonical_join::Venue;
        match venue {
            Venue::Flap => self.flap,
            Venue::Four => self.four,
            Venue::OpenFour => self.openfour,
        }
    }
}

/// The venues a switch-on record names (`U | 1`, Flap alone; `U | 1 |
/// mask`); `None` for any other bytes.
pub fn switched_on(p: &[u8]) -> Option<StrategyVenues> {
    match p {
        [b'U', 1] => Some(StrategyVenues::FLAP),
        [b'U', 1, mask] => StrategyVenues::of_mask(*mask),
        _ => None,
    }
}

/// The kinds of a `Y` record.
const DECIDED: u8 = 1;
const ENTERED: u8 = 2;
const CANCELLED: u8 = 3;
const TRIGGERED: u8 = 4;
const EXITED: u8 = 5;
const UNRESOLVED: u8 = 6;
/// A4: a PAPER position the real one took the place of (its entry, code 1;
/// 0 while it waited for its fill).
const SUPERSEDED: u8 = 7;

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

/// Why a token is not tracked: the reason in the high nibble (SC4: 7 the
/// venue's quote refused), for a term outside the scope the term in the low
/// one (1 version, 2 quote token, 3 NFT gate, 4 holding cap, 5 curve, 6
/// tax, 7 threshold, 8 not tradable, SC4: 9 a Four.meme curve, 10..14
/// [`FOUR_LAUNCH_REFUSALS`], 15 another).
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
                // SC4: a Four.meme curve (no Flap quote), and why a Four.meme
                // launch is not priced.
                "four.meme curve" => 9,
                why => FOUR_LAUNCH_REFUSALS
                    .iter()
                    .position(|r| *r == why)
                    .map_or(15, |i| 10 + i as u8),
            },
        ),
        Untracked::TermsIncomplete => (2, 0),
        Untracked::TradeMismatch => (3, 0),
        Untracked::Migrated => (4, 0),
        Untracked::Gap => (5, 0),
        Untracked::ShadowReplay => (6, 0),
        Untracked::Quote => (7, 0),
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
        Output::Superseded {
            token,
            block,
            paper,
        } => (
            SUPERSEDED,
            *block,
            *token,
            u8::from(paper.is_some()),
            0,
            paper.map_or([U256::ZERO; 4], |e| {
                [e.tokens, e.input_wei, e.network_fee_wei, e.spent_wei]
            }),
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
    /// Phase I gate L4: decisions whose token had no exact curve (or no
    /// position) when the block was complete: counted, never executed.
    pub paper_unpriced: u64,
    /// A4: PAPER positions a real one took the place of.
    pub superseded: u64,
    /// SC4 c (B5): `cancelled` by reason.
    pub cancelled_creator_sold: u64,
    pub cancelled_not_tracked: u64,
    pub cancelled_quote: u64,
    pub cancelled_cost: u64,
}

/// A Strategy C decision for the PAPER path (Phase I): a PASS is a BUY of
/// the configured input on the token's exact curve, an exit trigger the
/// SELL of the position's tokens on its shadow curve; both on the state
/// of the block once all its events were applied (gate M2), `decided_ns`
/// the decision record's time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrategyAction {
    Buy {
        token: [u8; 20],
        /// The creator of the token's launch (REV.1.0 R2: its LIVE lot's
        /// exit keeps the creator rule after a restart).
        creator: [u8; 20],
        curve: StrategyCurve,
        input_wei: U256,
        decided_ns: u64,
        /// Decision 06.11.: the exit rules its lot keeps (the deciding
        /// strategy's; a strategy's action always names them).
        exits: Option<darkbot_bsc_trading::strategy::engine::ExitTerms>,
    },
    Exit {
        token: [u8; 20],
        tokens: U256,
        curve: StrategyCurve,
        decided_ns: u64,
    },
}

/// A decision waiting for its block to be complete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    Buy([u8; 20], u64),
    Exit([u8; 20], u64),
}

/// A Strategy C PAPER transaction at the sink (Phase I), a `Runtime`
/// record: `J | kind u8 (1 BUY, 2 approval, 3 SELL) | sink sequence u64 |
/// tx_hash [32] | token [20] | head block u64 | decided, admitted, signed,
/// sink ns u64 | the signed bytes` (a local artefact, never sent).
pub fn paper_intent_payload(i: &PaperIntent) -> Vec<u8> {
    let t = i.times;
    let mut out = Vec::with_capacity(1 + 1 + 8 + 32 + 20 + 8 + 4 * 8 + i.raw.len());
    out.push(b'J');
    out.push(i.kind.code());
    out.extend_from_slice(&i.handoff.sequence.to_le_bytes());
    out.extend_from_slice(&i.handoff.tx_hash);
    out.extend_from_slice(&i.token);
    out.extend_from_slice(&i.block.to_le_bytes());
    for ns in [t.ingress_ns, t.admitted_ns, t.signed_ns, t.sink_ns] {
        out.extend_from_slice(&ns.to_le_bytes());
    }
    out.extend_from_slice(&i.raw);
    out
}

/// Decisions acted on at block `block`'s complete state (Phase I gate M1),
/// a `Runtime` record: `A | block u64`, journaled when the action is taken.
pub fn action_payload(block: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(ACTION_LEN);
    out.push(b'A');
    out.extend_from_slice(&block.to_le_bytes());
    out
}

pub const ACTION_LEN: usize = 9;

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
    /// SC4: the venues whose launches it follows.
    venues: StrategyVenues,
    pub counters: Counters,
    input_wei: U256,
    /// Decisions for the PAPER path since the last `take_actions`
    /// (collected on the runtime path only, not in a replay).
    pending: Vec<Pending>,
}

/// The status section of a strategy that runs, or of one refused (no
/// runtime, with its reason).
pub fn status_draft(
    strategy: Option<&StrategyRuntime>,
    refused: Option<&(&'static str, String)>,
    config: &Config,
    settings: Option<&crate::strategy_settings::Settings>,
) -> darkbot_bsc_record::Draft {
    use darkbot_bsc_record::Value;
    use darkbot_bsc_record::schema::bsc_runtime::runtime_status::strategy as st;
    let c = strategy.map(|s| s.counters).unwrap_or_default();
    let mut d = darkbot_bsc_record::Draft::new(&st::SCHEMA);
    d.set(
        st::STATE,
        match (strategy, refused) {
            (Some(_), _) => "SHADOW",
            (None, Some((state, _))) => state,
            (None, None) => "GAS_EVIDENCE_REFUSED",
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
    .set(st::PAPER_UNARMED, c.paper_unarmed)
    .set(st::PAPER_UNPRICED, c.paper_unpriced)
    .set(st::CANCELLED_CREATOR_SOLD, c.cancelled_creator_sold)
    .set(st::CANCELLED_NOT_TRACKED, c.cancelled_not_tracked)
    .set(st::CANCELLED_QUOTE, c.cancelled_quote)
    .set(st::CANCELLED_COST, c.cancelled_cost)
    .set(st::SUPERSEDED, c.superseded);
    if let (None, Some((_, why))) = (strategy, refused) {
        d.set(st::REASON, why.as_str());
    }
    // Frontend package (06.11.): the settings the running strategy took.
    if let (Some(_), Some(s)) = (strategy, settings) {
        d.set(st::SETTINGS_GENERATION, s.generation);
        for (k, v) in &s.values {
            let mut e = darkbot_bsc_record::Draft::new(&st::setting::SCHEMA);
            e.set(st::setting::ID, *k)
                .set(st::setting::VALUE, v.as_str());
            d.push(st::SETTINGS, e.into_nested());
        }
    }
    d
}

impl StrategyRuntime {
    pub fn new(config: Config, gas: GasEvidence, venues: StrategyVenues) -> Self {
        Self {
            venues,
            input_wei: config.input_wei,
            engine: Engine::new(config, gas),
            counters: Counters::default(),
            pending: Vec::new(),
        }
    }

    /// The decisions for the PAPER path since the last call (Phase I), on
    /// the engine's state now: called once the block's events are all
    /// applied, so the curve is the block's complete one (gate M2). A
    /// decision without an exact curve or position now is counted
    /// (`paper_unpriced`, gate L4), never executed.
    pub fn take_actions(&mut self) -> Vec<StrategyAction> {
        let pending = std::mem::take(&mut self.pending);
        let mut out = Vec::with_capacity(pending.len());
        for p in pending {
            let action = match p {
                Pending::Buy(token, decided_ns) => {
                    self.curve(&token).map(|curve| StrategyAction::Buy {
                        token,
                        creator: self.engine.creator_of(&token).unwrap_or_default(),
                        curve,
                        input_wei: self.input_wei,
                        decided_ns,
                        exits: Some(self.engine.exits()),
                    })
                }
                Pending::Exit(token, decided_ns) => self.exit_action(token, decided_ns),
            };
            match action {
                Some(a) => out.push(a),
                None => self.counters.paper_unpriced += 1,
            }
        }
        out
    }

    /// The exit of the position on `token` as the engine holds it now (its
    /// tokens, its curve): a LIVE exit whose SELL waited for its approval to
    /// be canonical continues with it (BSC3-055). None when the position or
    /// its curve is gone.
    pub fn exit_action(&self, token: [u8; 20], decided_ns: u64) -> Option<StrategyAction> {
        self.curve(&token)
            .zip(self.engine.position_tokens(&token))
            .map(|(curve, tokens)| StrategyAction::Exit {
                token,
                tokens,
                curve,
                decided_ns,
            })
    }

    fn curve(&self, token: &[u8; 20]) -> Option<StrategyCurve> {
        self.engine.curve_of(token)
    }

    /// SC4: the venues whose launches it follows.
    pub fn venues(&self) -> StrategyVenues {
        self.venues
    }

    pub fn followed(&self) -> usize {
        self.engine.followed()
    }

    /// Plan 28.10.: the strategy holds a real LIVE position (its exit rules
    /// apply on the real curve). The input is journaled first, so a replay
    /// steps the same engine.
    #[allow(clippy::too_many_arguments)]
    pub fn hold(
        &mut self,
        token: [u8; 20],
        tokens: U256,
        spent_wei: U256,
        filled_ms: u64,
        exits: Option<ExitTerms>,
        at: u64,
        journal: &mut RuntimeJournal,
    ) {
        let p = hold_payload(token, tokens, spent_wei, filled_ms, exits);
        journal.record(RecordKind::Runtime, at, p.clone());
        self.record(RecordKind::Runtime, &p, at, Some(journal));
    }

    /// 01.11. (R1): a real LIVE position the engine does not hold, held
    /// again on its curve as read (journaled first, as `hold`).
    pub fn reheld(&mut self, r: &Reheld, at: u64, journal: &mut RuntimeJournal) {
        let p = reheld_payload(r);
        journal.record(RecordKind::Runtime, at, p.clone());
        self.record(RecordKind::Runtime, &p, at, Some(journal));
    }

    /// Whether the strategy holds the real LIVE position on `token`: its
    /// exit is decided.
    pub fn holds(&self, token: &[u8; 20]) -> bool {
        self.engine.holds(token)
    }

    /// The real position ended (its SELL included).
    pub fn unhold(&mut self, token: [u8; 20], at: u64, journal: &mut RuntimeJournal) {
        let p = unhold_payload(token);
        journal.record(RecordKind::Runtime, at, p.clone());
        self.record(RecordKind::Runtime, &p, at, Some(journal));
    }

    /// Real LIVE positions the strategy holds.
    pub fn held(&self) -> usize {
        self.engine.held_positions()
    }

    /// Decisions wait for their block's completion.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
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
        // SC4: a launch or a Four.meme curve record of a venue it does not
        // follow is no input (nor counted).
        use crate::canonical_join::Venue;
        let four = self.venues.four || self.venues.openfour;
        let followed = match (kind, payload.first()) {
            (RecordKind::Canonical, Some(b'N')) => self.venues.flap,
            (RecordKind::Canonical, Some(b'n' | b'Z')) => payload
                .get(1)
                .and_then(|b| crate::runtime_journal::venue_of_byte(*b))
                .is_some_and(|v| self.venues.takes(v)),
            (RecordKind::Canonical, Some(b'y')) => self.venues.takes(Venue::OpenFour),
            (RecordKind::Canonical, Some(b'z')) => four,
            _ => true,
        };
        if !followed {
            return Vec::new();
        }
        let Some(input) = input_of_record(kind, payload) else {
            return Vec::new();
        };
        let c = &mut self.counters;
        match input {
            Input::Launch { .. } | Input::FourLaunch { .. } => c.launches += 1,
            Input::Trade { .. } | Input::FourTrade { .. } => c.trades += 1,
            Input::Term { .. } => c.terms += 1,
            Input::Migrated { .. } | Input::FourOffCurve { .. } => c.migrations += 1,
            Input::Incomplete | Input::FlapIncomplete | Input::FourIncomplete => c.incomplete += 1,
            Input::Block { .. } | Input::Hold { .. } | Input::Unhold { .. } | Input::Reheld(_) => {}
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
            for o in &typed {
                match o {
                    Output::Decided {
                        token,
                        decision: Decision::Pass,
                        ..
                    } => self.pending.push(Pending::Buy(*token, at)),
                    Output::ExitTriggered { token, .. } => {
                        self.pending.push(Pending::Exit(*token, at))
                    }
                    _ => {}
                }
            }
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
            Output::EntryCancelled { cancel, .. } => {
                c.cancelled += 1;
                *match cancel {
                    Cancel::CreatorSold => &mut c.cancelled_creator_sold,
                    Cancel::NotTracked(_) => &mut c.cancelled_not_tracked,
                    Cancel::Quote(_) => &mut c.cancelled_quote,
                    Cancel::Cost(_) => &mut c.cancelled_cost,
                } += 1;
            }
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
            Output::Superseded { .. } => c.superseded += 1,
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
    /// Each entry's position against the foreign BUYs of its token.
    pub positions: Vec<Position>,
    /// bugs_v4 D5: records the capture lost (the run manifest's count, or
    /// the journal's gap spans without one): such a replay is incomplete.
    pub lost: u64,
}

/// Phase I (PHASENPLAN §17 J input): one entry, a strategy PASS (SHADOW,
/// at its decided block and the journaled time it is acted on) or a BUY at
/// the PAPER sink, against the foreign BUYs of its token. The action's
/// time is a wall-clock estimate (the run's start plus the record's
/// monotonic time), compared with the headers'
/// millisecond block times: nothing is sent, no inclusion is shown, and a
/// header's time is not the producer's cut-off.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Position {
    pub paper: bool,
    pub token: [u8; 20],
    /// The block whose complete state the entry is priced on.
    pub decided_block: u64,
    pub decided_unix_us: Option<u64>,
    /// The first block whose header time is after the decision: the
    /// earliest a BUY decided then could be in.
    pub eligible_block: Option<u64>,
    /// The header time of the block after the decided one minus the
    /// decision (negative: that block's time had passed).
    pub lead_us: Option<i64>,
    /// Foreign BUYs of the token after the decided block, before the
    /// eligible one: ahead of any BUY of the entry.
    pub buys_before: u64,
    /// Foreign BUYs of the token in the eligible block (their order
    /// against the entry's is not known).
    pub buys_in_eligible: u64,
    /// Blocks from the decided block to the first foreign BUY after it.
    pub first_buy_gap_blocks: Option<u64>,
    /// The action time was not journaled (an older journal): the deciding
    /// record's time stands in, a lower bound.
    pub time_lower_bound: bool,
}

/// A canonical BUY of a token (Flap, SC4 Four.meme) and its block: what an
/// entry's position is placed against.
fn foreign_buy(input: &Input) -> Option<([u8; 20], u64)> {
    match input {
        Input::Trade { token, trade } | Input::FourTrade { token, trade, .. } if !trade.sell => {
            Some((*token, trade.block_number))
        }
        _ => None,
    }
}

/// The positions of `entries` (paper, token, decided block, action time,
/// whether that time is the action's own) in a run started at
/// `started_ms`, against the journaled block times and the foreign BUYs
/// (token, block): none of the run's own transactions is ever on chain.
fn positions(
    entries: &[(bool, [u8; 20], u64, u64, bool)],
    started_ms: Option<u64>,
    blocks: &std::collections::BTreeMap<u64, u64>,
    buys: &[([u8; 20], u64)],
) -> Vec<Position> {
    entries
        .iter()
        .map(|&(paper, token, decided_block, at_ns, exact)| {
            let decided_unix_us =
                started_ms.and_then(|ms| ms.checked_mul(1_000)?.checked_add(at_ns / 1_000));
            let eligible_block = decided_unix_us.and_then(|t| {
                blocks
                    .range(decided_block + 1..)
                    .find(|(_, ms)| ms.saturating_mul(1_000) > t)
                    .map(|(n, _)| *n)
            });
            let lead_us = decided_unix_us
                .zip(blocks.get(&(decided_block + 1)))
                .and_then(|(t, ms)| {
                    let us = i128::from(*ms) * 1_000 - i128::from(t);
                    i64::try_from(us).ok()
                });
            let foreign = buys
                .iter()
                .filter(|(t, b)| *t == token && *b > decided_block);
            let (before, within) = match eligible_block {
                Some(e) => foreign
                    .clone()
                    .fold((0, 0), |(a, w), (_, b)| match b.cmp(&e) {
                        std::cmp::Ordering::Less => (a + 1, w),
                        std::cmp::Ordering::Equal => (a, w + 1),
                        std::cmp::Ordering::Greater => (a, w),
                    }),
                None => (0, 0),
            };
            Position {
                paper,
                token,
                decided_block,
                decided_unix_us,
                eligible_block,
                lead_us,
                buys_before: before,
                buys_in_eligible: within,
                first_buy_gap_blocks: foreign.map(|(_, b)| b - decided_block).min(),
                time_lower_bound: !exact,
            }
        })
        .collect()
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
    config_text: Option<&str>,
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
        out.lost = m.u64(cr::LOST_RECORDS).unwrap_or(0);
    }
    let mut reader = RunReader::open(&run, max_payload).map_err(|e| e.to_string())?;
    let mut gap_spans = 0u64;
    let mut strategy: Option<StrategyRuntime> = None;
    // Frontend package (06.11.): the operator's settings the next start
    // takes (its `p` record), on the configuration `config_text`.
    let mut settings: Option<Config> = None;
    let mut journaled: Vec<Vec<u8>> = Vec::new();
    let mut replayed: Vec<Vec<u8>> = Vec::new();
    // Phase I positioning: block times, foreign BUYs, entries (PASS
    // decisions and PAPER BUYs), the journaled action times by block.
    let mut blocks = std::collections::BTreeMap::new();
    let mut buys = Vec::new();
    let mut entries = Vec::new();
    let mut acted = std::collections::BTreeMap::new();
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
            (RecordKind::GapSpan, p) => {
                let span = <[u8; 8]>::try_from(p).map_or(1, u64::from_le_bytes);
                gap_spans = gap_spans.saturating_add(span);
            }
            (RecordKind::Runtime, p @ ([b'U', _] | [b'U', 1, _])) => {
                // The counters of every period the strategy ran add up.
                if let Some(s) = strategy.take() {
                    add(&mut out.counters, &s.counters);
                    out.open_at_end += s.engine.open_positions() as u64;
                }
                if let Some(venues) = switched_on(p) {
                    out.starts += 1;
                    let config = settings.take().unwrap_or_else(|| config.clone());
                    strategy = Some(StrategyRuntime::new(config, gas.clone(), venues));
                }
            }
            (RecordKind::Runtime, p)
                if p.first() == Some(&crate::strategy_settings::JOURNAL_TAG) =>
            {
                let taken = crate::strategy_settings::Settings::of_journal(p)
                    .ok_or_else(|| "an unreadable settings record".to_string())
                    .and_then(|s| {
                        let text = config_text
                            .ok_or("a settings record without the run's configuration")?;
                        crate::strategy_settings::effective(text, &s)
                    });
                match taken {
                    Ok(c) => settings = Some(c.strategy.engine),
                    Err(e) => {
                        out.read_error = Some(format!("Strategy C settings: {e}"));
                        break;
                    }
                }
            }
            (RecordKind::Runtime, p) if p.first() == Some(&b'Y') => journaled.push(p.to_vec()),
            (RecordKind::Runtime, p) if paper_intent_kind(p) == Some(IntentKind::Buy) => {
                if let (Some(token), Some(block)) = (
                    p.get(42..62).and_then(|t| <[u8; 20]>::try_from(t).ok()),
                    u64_at(p, 62),
                ) {
                    entries.push((true, token, block, r.timestamp_ns, true));
                }
            }
            (RecordKind::Runtime, p) if p.len() == ACTION_LEN && p[0] == b'A' => {
                if let Some(block) = u64_at(p, 1) {
                    acted.entry(block).or_insert(r.timestamp_ns);
                }
            }
            (kind, p) => {
                match input_of_record(kind, p) {
                    Some(Input::Block { number, time_ms }) => {
                        blocks.insert(number, time_ms);
                    }
                    Some(input) if foreign_buy(&input).is_some() => {
                        buys.extend(foreign_buy(&input));
                    }
                    _ => {}
                }
                if let Some(s) = strategy.as_mut() {
                    let outputs = s.record(kind, p, r.timestamp_ns, None);
                    // A PASS (gate M1: the decision's block, not the fill's).
                    for o in &outputs {
                        if o.get(1) == Some(&DECIDED)
                            && o.get(30..32) == Some(&[0, 0][..])
                            && let (Some(block), Some(token)) = (
                                u64_at(o, 2),
                                o.get(10..30).and_then(|t| <[u8; 20]>::try_from(t).ok()),
                            )
                        {
                            entries.push((false, token, block, r.timestamp_ns, false));
                        }
                    }
                    replayed.extend(outputs);
                }
            }
        }
    }
    // A PASS is acted on at its block's completion: the journaled action
    // time, else (an older journal) the deciding record's, a lower bound.
    for e in &mut entries {
        if !e.0
            && let Some(at) = acted.get(&e.2)
        {
            (e.3, e.4) = (*at, true);
        }
    }
    // One position per PASS: a BUY at the PAPER sink (its J record)
    // replaces the PASS's SHADOW entry (gate pass-4 LOW: counted twice).
    let paper: Vec<[u8; 20]> = entries.iter().filter(|e| e.0).map(|e| e.1).collect();
    entries.retain(|e| e.0 || !paper.contains(&e.1));
    out.positions = positions(&entries, run_id.parse().ok(), &blocks, &buys);
    if let Some(s) = strategy {
        add(&mut out.counters, &s.counters);
        out.open_at_end += s.engine.open_positions() as u64;
    }
    out.lost = out.lost.max(gap_spans);
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
        (&mut sum.cancelled_creator_sold, c.cancelled_creator_sold),
        (&mut sum.cancelled_not_tracked, c.cancelled_not_tracked),
        (&mut sum.cancelled_quote, c.cancelled_quote),
        (&mut sum.cancelled_cost, c.cancelled_cost),
        (&mut sum.superseded, c.superseded),
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
        } else if self.lost > 0 {
            // bugs_v4 D5: a lost input or output was never compared.
            "INCOMPLETE"
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
        if self.lost > 0 {
            out.push(format!(
                "{} journal records lost: not every input and output was compared",
                self.lost
            ));
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
        let mut d = darkbot_bsc_record::Draft::new(&sr::V3);
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
        let (next, contested, unknown) = self.position_totals();
        d.set(sr::POSITIONS_NEXT_BLOCK, next)
            .set(sr::POSITIONS_CONTESTED, contested)
            .set(sr::POSITIONS_UNKNOWN, unknown);
        for p in &self.positions {
            use sr::position as sp;
            let mut e = darkbot_bsc_record::Draft::new(&sp::SCHEMA);
            e.set(sp::SOURCE, if p.paper { "PAPER" } else { "SHADOW" })
                .set(sp::TOKEN, Value::Address(p.token))
                .set(sp::DECIDED_BLOCK, p.decided_block)
                .set(sp::BUYS_BEFORE, p.buys_before)
                .set(sp::BUYS_IN_ELIGIBLE, p.buys_in_eligible);
            if let Some(v) = p.decided_unix_us {
                e.set(sp::DECIDED_UNIX_US, v);
            }
            if let Some(v) = p.eligible_block {
                e.set(sp::ELIGIBLE_BLOCK, v);
            }
            if let Some(v) = p.lead_us {
                e.set(sp::LEAD_US, Value::I64(v));
            }
            if let Some(v) = p.first_buy_gap_blocks {
                e.set(sp::FIRST_BUY_GAP_BLOCKS, v);
            }
            if p.time_lower_bound {
                e.set(sp::TIME_LOWER_BOUND, true);
            }
            d.push(sr::POSITIONS, e.into_nested());
        }
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

    /// Entries eligible for the block after the decided one, entries with
    /// a foreign BUY before or in their eligible block, entries without an
    /// eligible block.
    pub fn position_totals(&self) -> (u64, u64, u64) {
        let p = &self.positions;
        let count = |f: &dyn Fn(&Position) -> bool| p.iter().filter(|x| f(x)).count() as u64;
        (
            count(&|x| x.eligible_block == Some(x.decided_block + 1)),
            count(&|x| x.eligible_block.is_some() && x.buys_before + x.buys_in_eligible > 0),
            count(&|x| x.eligible_block.is_none()),
        )
    }

    pub fn summary(&self) -> String {
        let c = &self.counters;
        let (next, contested, unknown) = self.position_totals();
        format!(
            "strategy replay of run {}: {} (records {}, starts {}, outputs journaled {} replayed {}; inputs: trades {}, terms {}, migrations {}, incomplete {}; launches {}, passed {}, rejected {}, data incomplete {}, not tracked {}, slot limit {}, creator sold {}; PAPER entered {}, cancelled {}, exits creator-sell {} target {} stop {} timeout {}; gains {} ({} wei), losses {} ({} wei), unresolved {}, open at the end {} (not valued); positions {} (eligible for the next block {}, a foreign BUY before or in the eligible block {}, no eligible block {}))\n{}",
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
            self.positions.len(),
            next,
            contested,
            unknown,
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

/// BSC-V8-02: how a training replay ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrainingEnd {
    /// The recorded run's last record (or an unreadable one: `read_error`).
    JournalEnd,
    /// Its bound (a duration of journal time, a number of launches): no
    /// launch past it, the ones taken decided and their positions closed
    /// (or the journal ended first: `followed_at_end`).
    Bound,
    /// A stop request.
    Stopped,
}

/// BSC-V8-02: a training replay's position, for its bound and its stop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrainingProgress {
    pub records: u64,
    /// Journal time since the run's first record.
    pub elapsed_ns: u64,
    /// The launches of its venues the strategy took so far (each is then
    /// decided: a bounded run ends only once every one taken is).
    pub launches: u64,
}

/// BSC-V8-02: what a training replay asks its owner after every record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrainingStep {
    Continue,
    /// The bound is reached: no further launch.
    Bounded,
    Stop,
}

/// BSC-V8-02/03: the result of one training replay.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Training {
    pub records: u64,
    pub counters: Counters,
    /// Positions still open at its end (not valued).
    pub open_at_end: u64,
    /// Tokens still followed at its end (launches not decided yet,
    /// entries waiting for their fill, open positions): review H2.
    pub followed_at_end: u64,
    /// Records the capture lost (its manifest's count or its gap spans).
    pub lost: u64,
    /// The recorded run had a manifest (its counts known).
    pub manifest: bool,
    pub read_error: Option<String>,
    pub end: Option<TrainingEnd>,
    /// The journal time it covered (the run's monotonic record times).
    pub first_ns: Option<u64>,
    pub last_ns: Option<u64>,
}

impl Training {
    /// The launches it decided (each launch taken is decided once).
    pub fn decided(&self) -> u64 {
        let c = &self.counters;
        c.passed + c.rejected + c.data_incomplete + c.not_tracked + c.slot_limit + c.creator_sold
    }
}

/// BSC-V8-02: Strategy C with `config` on `venues` over the market records
/// of the recorded run `run_id` under `journal_dir`, from its first record
/// whatever the recorded runtime ran: every record the runtime feeds its
/// strategy (the canonical chain and its reorgs and resyncs, review H1)
/// but its own (decisions, switches, settings, real lots: a training is a
/// simulation of its own). After every record `step` says whether to go
/// on; once it is bounded no launch is taken, and the run ends when no
/// token is followed any more (every launch taken decided, every position
/// closed; review H2) or at the journal's end.
pub fn train(
    journal_dir: &std::path::Path,
    run_id: &str,
    max_payload: usize,
    config: &Config,
    gas: &GasEvidence,
    venues: StrategyVenues,
    step: &mut dyn FnMut(&TrainingProgress) -> TrainingStep,
) -> Result<Training, String> {
    use darkbot_bsc_journal::storage::run::RunReader;
    use darkbot_bsc_record::schema::bsc_capture::capture_run as cr;
    let run = journal_dir.join(run_id);
    if !run.is_dir() {
        return Err(format!("{}: no run directory", run.display()));
    }
    let mut out = Training::default();
    if let Ok(bytes) = std::fs::read(run.join(crate::runtime_journal::RUN_MANIFEST))
        && let Ok(m) = cr::decode_run(&bytes)
    {
        out.lost = m.u64(cr::LOST_RECORDS).unwrap_or(0);
        out.manifest = true;
    }
    let mut reader = RunReader::open(&run, max_payload).map_err(|e| e.to_string())?;
    let mut strategy = StrategyRuntime::new(config.clone(), gas.clone(), venues);
    let mut gap_spans = 0u64;
    let mut bounded = false;
    loop {
        // A journal that ends while a bounded run drains: it ran to its bound.
        let at_end = match bounded {
            true => TrainingEnd::Bound,
            false => TrainingEnd::JournalEnd,
        };
        let r = match reader.next_record() {
            Ok(Some(r)) => r,
            Ok(None) => {
                out.end = Some(at_end);
                break;
            }
            Err(e) => {
                out.read_error = Some(e.to_string());
                out.end = Some(at_end);
                break;
            }
        };
        out.records += 1;
        let first = *out.first_ns.get_or_insert(r.timestamp_ns);
        out.last_ns = Some(r.timestamp_ns);
        match (r.kind, &*r.payload) {
            (RecordKind::GapSpan, p) => {
                let span = <[u8; 8]>::try_from(p).map_or(1, u64::from_le_bytes);
                gap_spans = gap_spans.saturating_add(span);
            }
            (RecordKind::Runtime, _) => {}
            (kind, p) => {
                let launch = matches!(
                    input_of_record(kind, p),
                    Some(Input::Launch { .. } | Input::FourLaunch { .. })
                );
                if !(launch && bounded) {
                    strategy.record(kind, p, r.timestamp_ns, None);
                }
            }
        }
        if bounded && strategy.followed() == 0 {
            out.end = Some(TrainingEnd::Bound);
            break;
        }
        let progress = TrainingProgress {
            records: out.records,
            elapsed_ns: r.timestamp_ns.saturating_sub(first),
            launches: strategy.counters.launches,
        };
        match step(&progress) {
            TrainingStep::Continue => {}
            TrainingStep::Bounded => bounded = true,
            TrainingStep::Stop => {
                out.end = Some(TrainingEnd::Stopped);
                break;
            }
        }
    }
    out.counters = strategy.counters;
    out.open_at_end = strategy.engine.open_positions() as u64;
    out.followed_at_end = strategy.followed() as u64;
    out.lost = out.lost.max(gap_spans);
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// BSC-V8-02: a recorded run of `launches` Flap launches (blocks 100,
    /// 200, …, each followed by six blocks: the PASS at its fifth) under
    /// `journal_dir/<run>`, never switched on: what a training replays.
    pub(crate) fn recorded_run(journal_dir: &std::path::Path, run: u64, launches: u64) -> Config {
        use crate::runtime_journal::{EXIT_PAYLOAD, JournalSetup, RunIdentity};
        let max = 16_384;
        let mut j =
            RuntimeJournal::open(journal_dir, run, 1, JournalSetup::whole(64, max)).unwrap();
        j.set_identity(RunIdentity {
            host: "test".into(),
            build: "test".into(),
            config_sha256: [0xC0; 32],
            build_sha256: None,
            build_dirty: None,
            capture_pending: "VENUES",
        });
        for n in 1..=launches {
            let at = n * 100;
            let mut records = launch_records(at);
            records.extend((at + 1..=at + 7).map(block));
            for (k, p) in records {
                // A launch's records 1 ms apart in the run's time.
                assert!(j.record(k, at * 1_000_000, p));
            }
        }
        let end = (launches * 100 + 8) * 1_000_000;
        assert!(j.record(RecordKind::Runtime, end, EXIT_PAYLOAD.to_vec()));
        j.finish().unwrap();
        config()
    }

    /// BSC-V8-02: a training replays every market record of a run the
    /// runtime never switched Strategy C on for, with the configuration it
    /// is given; its bound takes no launch past it, a stop ends it at once.
    #[test]
    fn a_training_replays_a_run_strategy_c_never_ran_on() {
        let base = std::env::temp_dir().join(format!("bsc-train-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let cfg = recorded_run(&base, 9, 3);
        let max = 16_384;
        let go = &mut |_: &TrainingProgress| TrainingStep::Continue;
        let t = train(&base, "9", max, &cfg, &gas(), StrategyVenues::FLAP, go).unwrap();
        assert_eq!(t.end, Some(TrainingEnd::JournalEnd));
        assert_eq!((t.counters.launches, t.counters.passed), (3, 3));
        assert!(t.records > 0 && t.read_error.is_none());
        assert_eq!((t.first_ns, t.lost), (Some(100_000_000), 0));
        // The recorded run's replay has nothing to compare: it never ran.
        let r = replay(&base, "9", max, &cfg, &gas(), None, None).unwrap();
        assert_eq!(r.starts, 0);
        // A venue the profile does not train on takes none of its launches.
        let four = StrategyVenues {
            flap: false,
            four: true,
            openfour: false,
        };
        let t = train(&base, "9", max, &cfg, &gas(), four, go).unwrap();
        assert_eq!(t.counters.launches, 0);
        // Bounded after the first launch: no further one, its PASS decided.
        let first = &mut |p: &TrainingProgress| match p.launches {
            0 => TrainingStep::Continue,
            _ => TrainingStep::Bounded,
        };
        let t = train(&base, "9", max, &cfg, &gas(), StrategyVenues::FLAP, first).unwrap();
        assert_eq!((t.counters.launches, t.counters.passed), (1, 1));
        assert_eq!(t.end, Some(TrainingEnd::Bound));
        // Review H2: the PASS entered (its fill a block later), not cut off
        // by the bound: as far as the unbounded run took its first launch.
        let all = train(&base, "9", max, &cfg, &gas(), StrategyVenues::FLAP, go).unwrap();
        assert!(t.counters.entered >= 1 && all.counters.entered >= t.counters.entered);
        assert_eq!(t.followed_at_end, t.open_at_end);
        // A stop at the 5th record: nothing after it.
        let stop = &mut |p: &TrainingProgress| match p.records {
            5 => TrainingStep::Stop,
            _ => TrainingStep::Continue,
        };
        let t = train(&base, "9", max, &cfg, &gas(), StrategyVenues::FLAP, stop).unwrap();
        assert_eq!((t.records, t.end), (5, Some(TrainingEnd::Stopped)));
        assert!(train(&base, "8", max, &cfg, &gas(), StrategyVenues::FLAP, go).is_err());
        std::fs::remove_dir_all(&base).unwrap();
    }

    /// Review H1: a training takes the chain's reorgs and resyncs as the
    /// runtime does: a resync inside the observation window leaves the
    /// launch's curve not exact (DATA_INCOMPLETE), never a PASS.
    #[test]
    fn a_training_takes_the_reorgs_and_resyncs() {
        use crate::runtime_journal::{EXIT_PAYLOAD, JournalSetup, RunIdentity, resync_payload};
        let base = std::env::temp_dir().join(format!("bsc-train-resync-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let max = 16_384;
        for (run, resync) in [(1u64, false), (2, true)] {
            let mut j = RuntimeJournal::open(&base, run, 1, JournalSetup::whole(64, max)).unwrap();
            j.set_identity(RunIdentity {
                host: "test".into(),
                build: "test".into(),
                config_sha256: [0xC0; 32],
                build_sha256: None,
                build_dirty: None,
                capture_pending: "VENUES",
            });
            for (k, p) in launch_records(100) {
                assert!(j.record(k, 1, p));
            }
            if resync {
                assert!(j.record(RecordKind::Reorg, 2, resync_payload(101, 101)));
            }
            for (k, p) in (101..=107).map(block) {
                assert!(j.record(k, 3, p));
            }
            assert!(j.record(RecordKind::Runtime, 4, EXIT_PAYLOAD.to_vec()));
            j.finish().unwrap();
        }
        let go = &mut |_: &TrainingProgress| TrainingStep::Continue;
        let plain = train(&base, "1", max, &config(), &gas(), StrategyVenues::FLAP, go).unwrap();
        assert_eq!(
            (plain.counters.passed, plain.counters.data_incomplete),
            (1, 0)
        );
        let cut = train(&base, "2", max, &config(), &gas(), StrategyVenues::FLAP, go).unwrap();
        assert_eq!((cut.counters.passed, cut.counters.data_incomplete), (0, 1));
        assert_eq!(cut.counters.incomplete, 1);
        std::fs::remove_dir_all(&base).unwrap();
    }

    /// 01.11. (R1): the re-held position's record reads back as the input
    /// it was journaled for (a replay steps the same engine); any other
    /// length or a flag other than 0/1 is no input.
    #[test]
    fn a_reheld_record_reads_back_as_its_input() {
        use darkbot_bsc_trading::economics::flap::native_curve20::profile_terms;
        let r = Reheld {
            token: [0xE1; 20],
            creator: [0xC1; 20],
            curve: HeldCurve::Flap {
                terms: profile_terms(100, 200),
                reserve: U256::from(7u8),
                circulating: U256::from(8u8),
            },
            tokens: U256::from(9u8),
            spent_wei: U256::from(10u8),
            filled_ms: 11,
            exits: None,
        };
        let p = reheld_payload(&r);
        assert_eq!(p.len(), REHELD_LEN);
        assert_eq!(
            input_of_record(RecordKind::Runtime, &p),
            Some(Input::Reheld(r))
        );
        assert_eq!(input_of_record(RecordKind::Canonical, &p), None);
        assert_eq!(
            input_of_record(RecordKind::Runtime, &p[..p.len() - 1]),
            None
        );
        let mut flag = p.clone();
        flag[41] = 2;
        assert_eq!(input_of_record(RecordKind::Runtime, &flag), None);
    }

    /// Decision 06.11. (review M1): a held position's records (`h`, `k`,
    /// `q`) carry its entry's exit rules and read back with them; without
    /// them (a record before them) they read as the strategy's; rules its
    /// check refuses, a flag other than 0/1 or another length are no input.
    #[test]
    fn a_held_positions_records_carry_its_exit_rules() {
        use darkbot_bsc_trading::economics::flap::native_curve20::profile_terms;
        use darkbot_bsc_trading::economics::openfour::{OpenFourCurve, OpenFourFee};
        let rules = ExitTerms {
            ratio_places: 4,
            target_net_multiple: 30_000,
            stop_net_multiple: 2_500,
            hold_ms: 90_000,
            creator_sell_exit: false,
        };
        let flap = Reheld {
            token: [0xE1; 20],
            creator: [0xC1; 20],
            curve: HeldCurve::Flap {
                terms: profile_terms(100, 200),
                reserve: U256::from(7u8),
                circulating: U256::from(8u8),
            },
            tokens: U256::from(9u8),
            spent_wei: U256::from(10u8),
            filled_ms: 11,
            exits: Some(rules),
        };
        let curve = OpenFourCurve {
            k: U256::from(1u8),
            token_reserve: U256::from(2u8),
            quote_reserve: U256::from(3u8),
            remaining: U256::from(4u8),
        };
        let four = Reheld {
            curve: HeldCurve::Four(FourCurve::OpenFour {
                start: curve,
                now: curve,
                fee: OpenFourFee {
                    protocol_bps: 5,
                    dev_bps: 6,
                },
            }),
            ..flap
        };
        for r in [flap, four] {
            let p = reheld_payload(&r);
            assert_eq!(
                input_of_record(RecordKind::Runtime, &p),
                Some(Input::Reheld(r))
            );
            let without = reheld_payload(&Reheld { exits: None, ..r });
            assert_eq!(p.len(), without.len() + EXITS_LEN);
            assert_eq!(
                input_of_record(RecordKind::Runtime, &without),
                Some(Input::Reheld(Reheld { exits: None, ..r }))
            );
        }
        let hold = hold_payload(
            [0xE1; 20],
            U256::from(9u8),
            U256::from(10u8),
            11,
            Some(rules),
        );
        assert_eq!(
            input_of_record(RecordKind::Runtime, &hold),
            Some(Input::Hold {
                token: [0xE1; 20],
                tokens: U256::from(9u8),
                spent_wei: U256::from(10u8),
                filled_ms: 11,
                exits: Some(rules),
            })
        );
        assert_eq!(hold.len(), HOLD_LEN + EXITS_LEN);
        let mut flag = hold.clone();
        *flag.last_mut().unwrap() = 2;
        assert_eq!(input_of_record(RecordKind::Runtime, &flag), None);
        let mut stop = hold.clone();
        // A stop at 1x is no rule of the strategy's.
        stop[HOLD_LEN + 20..HOLD_LEN + 36].copy_from_slice(&10_000i128.to_le_bytes());
        assert_eq!(input_of_record(RecordKind::Runtime, &stop), None);
        assert_eq!(
            input_of_record(RecordKind::Runtime, &hold[..hold.len() - 1]),
            None
        );
    }

    /// SC4 d: a Four.meme position held again reads back as its input
    /// (a `q` record: either family's start and current curve and fees);
    /// a Flap venue byte or an OpenFour record with a Classic minimum fee
    /// is no input.
    #[test]
    fn a_four_reheld_record_reads_back_as_its_input() {
        use darkbot_bsc_trading::economics::four_classic::{ClassicCurve, ClassicFee};
        use darkbot_bsc_trading::economics::openfour::{OpenFourCurve, OpenFourFee};
        let w = |n: u64| U256::from(n);
        let classic = |sold: u64| ClassicCurve {
            k: w(1),
            t: w(2) + w(sold),
            offers: w(3) + w(sold),
            sold: w(sold),
        };
        let openfour = |remaining: u64| OpenFourCurve {
            k: w(4),
            token_reserve: w(5),
            quote_reserve: w(6),
            remaining: w(remaining),
        };
        for curve in [
            FourCurve::Classic {
                start: classic(0),
                now: classic(7),
                fee: ClassicFee {
                    rate_bps: 100,
                    min_fee: w(8),
                    tax_bps: 1_000,
                },
            },
            FourCurve::OpenFour {
                start: openfour(9),
                now: openfour(10),
                fee: OpenFourFee {
                    protocol_bps: 50,
                    dev_bps: 60,
                },
            },
        ] {
            let r = Reheld {
                token: [0xE1; 20],
                creator: [0xC1; 20],
                curve: HeldCurve::Four(curve),
                tokens: w(11),
                spent_wei: w(12),
                filled_ms: 13,
                exits: None,
            };
            let p = reheld_payload(&r);
            assert_eq!((p[0], p.len()), (b'q', REHELD_FOUR_LEN));
            assert_eq!(
                input_of_record(RecordKind::Runtime, &p),
                Some(Input::Reheld(r))
            );
            assert_eq!(input_of_record(RecordKind::Canonical, &p), None);
            assert_eq!(
                input_of_record(RecordKind::Runtime, &p[..p.len() - 1]),
                None
            );
            let mut flap = p.clone();
            flap[1] = crate::runtime_journal::venue_byte(crate::canonical_join::Venue::Flap);
            assert_eq!(input_of_record(RecordKind::Runtime, &flap), None);
        }
        let openfour_with_min_fee = |p: &mut Vec<u8>| {
            let at = 42 + 8 * 32 + 2;
            p[at + 31] = 1;
        };
        let mut p = reheld_payload(&Reheld {
            token: [0xE1; 20],
            creator: [0xC1; 20],
            curve: HeldCurve::Four(FourCurve::OpenFour {
                start: openfour(9),
                now: openfour(10),
                fee: OpenFourFee {
                    protocol_bps: 50,
                    dev_bps: 60,
                },
            }),
            tokens: w(11),
            spent_wei: w(12),
            filled_ms: 13,
            exits: None,
        });
        openfour_with_min_fee(&mut p);
        assert_eq!(input_of_record(RecordKind::Runtime, &p), None);
    }
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
            creator_sell_exit: true,
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
        StrategyRuntime::new(config(), gas(), StrategyVenues::FLAP)
    }

    /// A4: a PAPER position the real one took the place of is journaled (its
    /// entry, code 1; 0 while it waited for its fill), counted and in the
    /// status.
    #[test]
    fn a_superseded_paper_position_is_journaled_counted_and_shown() {
        use darkbot_bsc_record::Value;
        use darkbot_bsc_record::schema::bsc_runtime::runtime_status::strategy as st;
        use darkbot_bsc_trading::economics::flap::position::Entry;
        let entry = Entry {
            tokens: U256::from(7u64),
            input_wei: U256::from(8u64),
            network_fee_wei: U256::from(9u64),
            spent_wei: U256::from(17u64),
        };
        let mut s = runtime();
        let mut payloads = Vec::new();
        for paper in [Some(entry), None] {
            let o = Output::Superseded {
                token: TOKEN,
                block: 42,
                paper,
            };
            s.count(&o);
            payloads.push(output_payload(&o));
        }
        assert_eq!(s.counters.superseded, 2);
        let [with, waiting] = payloads.try_into().unwrap();
        assert_eq!((with.len(), waiting.len()), (OUTPUT_LEN, OUTPUT_LEN));
        assert_eq!((with[0], with[1], with[30], waiting[30]), (b'Y', 7, 1, 0));
        assert_eq!(&with[32..64], &U256::from(7u64).to_be_bytes::<32>());
        assert_eq!(&with[128..160], &U256::from(17u64).to_be_bytes::<32>());
        assert!(waiting[32..].iter().all(|b| *b == 0));
        let d = status_draft(Some(&s), None, &config(), None);
        assert_eq!(d.values(st::SUPERSEDED), [Value::U64(2)]);
    }

    /// SC4 c (B5): a cancelled entry counts under its reason, and the status
    /// names each reason besides the total.
    #[test]
    fn a_cancelled_entry_counts_under_its_reason() {
        use darkbot_bsc_record::Value;
        use darkbot_bsc_record::schema::bsc_runtime::runtime_status::strategy as st;
        use darkbot_bsc_trading::economics::flap::QuoteError;
        use darkbot_bsc_trading::economics::network::{Action, CostError, PathClass, Venue};
        let mut s = runtime();
        let class = PathClass {
            venue: Venue::FourClassic,
            action: Action::Sell,
        };
        for cancel in [
            Cancel::CreatorSold,
            Cancel::NotTracked(Untracked::Gap),
            Cancel::NotTracked(Untracked::Quote),
            Cancel::Quote(QuoteError::ZeroInput),
            Cancel::Cost(CostError::NoEvidence(class)),
            Cancel::Cost(CostError::Overflow),
            Cancel::Cost(CostError::NoAttempt),
        ] {
            s.count(&Output::EntryCancelled {
                token: TOKEN,
                block: 1,
                cancel,
            });
        }
        let c = s.counters;
        assert_eq!(
            (
                c.cancelled,
                c.cancelled_creator_sold,
                c.cancelled_not_tracked,
                c.cancelled_quote,
                c.cancelled_cost
            ),
            (7, 1, 2, 1, 3)
        );
        let d = status_draft(Some(&s), None, &config(), None);
        for (tag, n) in [
            (st::CANCELLED, 7),
            (st::CANCELLED_CREATOR_SOLD, 1),
            (st::CANCELLED_NOT_TRACKED, 2),
            (st::CANCELLED_QUOTE, 1),
            (st::CANCELLED_COST, 3),
        ] {
            assert_eq!(d.values(tag), [Value::U64(n)], "{tag}");
        }
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
        // SC4: a refused Flap log concerns the Flap curves only.
        assert_eq!(
            input_of_record(RecordKind::Canonical, &trade_gap_payload(5, &HASH, 1, 2)),
            Some(Input::FlapIncomplete)
        );
        for (kind, p) in [
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

    /// Phase I gate M2: the PASS is decided when its block's record
    /// arrives, its PAPER action taken once the block is complete: a trade
    /// of that block is in the action's curve.
    #[test]
    fn the_action_takes_the_complete_blocks_curve() {
        use crate::runtime_journal::JournalSetup;
        let base = std::env::temp_dir().join(format!("bsc-strategy-m2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mut j = RuntimeJournal::open(&base, 7, 1, JournalSetup::whole(64, 16_384)).unwrap();
        let mut s = runtime();
        let mut records = launch_records(100);
        records.extend((101..=105).map(block));
        for (k, p) in &records {
            s.record(*k, p, 1, Some(&mut j));
        }
        assert_eq!(s.counters.passed, 1);
        // A buy in the decision block 105, after its record.
        let Some(darkbot_bsc_trading::strategy::engine::StrategyCurve::Flap {
            terms,
            reserve,
            circulating,
        }) = s.engine.curve_of(&TOKEN)
        else {
            panic!("a Flap curve")
        };
        let eth = U256::from(100_000_000_000_000_000u64);
        let q = quote_native_buy(
            &darkbot_bsc_trading::economics::flap::native_curve20::NativeBuyInputs {
                token: terms,
                reserve_before: reserve,
                circulating_before: circulating,
                input_amount: eth,
                protocol_fee_bps:
                    darkbot_bsc_trading::economics::flap::native_curve20::WITNESSED_MAINNET_TERMS
                        .protocol_buy_fee_bps,
                tax_on_bonding_curve_enabled: true,
                caller_fee_exempt: false,
                holding_cap: darkbot_bsc_trading::economics::flap::native_curve20::HoldingCap::None,
            },
        )
        .unwrap();
        let trade = CanonicalTrade {
            block_number: 105,
            block_hash: HASH,
            tx_index: 3,
            tx_hash: [8; 32],
            log_index: 0,
            origin: [0x55; 20],
            sell: false,
            token: TOKEN,
            trader: [0x55; 20],
            amount: be(q.output_amount),
            eth: be(eth),
            fee: be(q.protocol_fee),
            post_price: [0; 32],
        };
        s.record(
            RecordKind::Canonical,
            &flap_trade_payload(&trade),
            2,
            Some(&mut j),
        );
        let actions = s.take_actions();
        let [
            StrategyAction::Buy {
                curve, decided_ns, ..
            },
        ] = actions.as_slice()
        else {
            panic!("{actions:?}")
        };
        let StrategyCurve::Flap {
            reserve: after,
            circulating,
            ..
        } = *curve
        else {
            panic!("a Flap curve")
        };
        assert_eq!((after, circulating), (q.reserve_after, q.supply_after));
        assert_ne!(after, reserve);
        assert_eq!(*decided_ns, 1);
        j.finish().unwrap();
        std::fs::remove_dir_all(&base).unwrap();
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
            &status_draft(Some(&s), None, &config(), None)
                .encode()
                .unwrap(),
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
            &status_draft(
                None,
                Some(&("GAS_EVIDENCE_REFUSED", "no gas evidence".to_string())),
                &config(),
                None,
            )
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

    /// R1: a LIVE position held again on its read curve (`k`, journaled
    /// before its step as the runtime does) replays as it ran: held on the
    /// same curve, its exit triggers at the same block.
    #[test]
    fn a_reheld_position_replays_exactly() {
        use crate::runtime_journal::{EXIT_PAYLOAD, JournalSetup, RunIdentity};
        use darkbot_bsc_trading::economics::flap::native_curve20::profile_terms;
        let base = std::env::temp_dir().join(format!("bsc-strategy-reheld-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let max = 16_384;
        let mut j = RuntimeJournal::open(&base, 7, 1, JournalSetup::whole(1024, max)).unwrap();
        j.set_identity(RunIdentity {
            host: "test".into(),
            build: "test".into(),
            config_sha256: [0xC0; 32],
            build_sha256: None,
            build_dirty: None,
            capture_pending: "VENUES",
        });
        assert!(j.record(
            RecordKind::Runtime,
            2,
            switch_payload(true, StrategyVenues::FLAP)
        ));
        let mut s = runtime();
        let (k, p) = block(100);
        assert!(j.record(k, 3, p.clone()));
        s.record(k, &p, 3, Some(&mut j));
        let held = Reheld {
            token: [0xAB; 20],
            creator: [0; 20],
            curve: HeldCurve::Flap {
                terms: profile_terms(100, 100),
                reserve: U256::from(2_000_000_000_000_000_000u128),
                circulating: U256::from(300_000_000_000_000_000_000_000u128),
            },
            tokens: U256::from(1_000_000_000_000_000_000u128),
            spent_wei: U256::from(10_000_000_000_000_000u128),
            filled_ms: 0,
            exits: None,
        };
        s.reheld(&held, 4, &mut j);
        assert!(s.holds(&held.token));
        for n in 101..=185 {
            let (k, p) = block(n);
            assert!(j.record(k, 5, p.clone()));
            s.record(k, &p, 5, Some(&mut j));
        }
        assert!(s.has_pending(), "the held position's timeout exit");
        assert!(j.record(
            RecordKind::Runtime,
            6,
            switch_payload(false, StrategyVenues::FLAP)
        ));
        assert!(j.record(RecordKind::Runtime, 7, EXIT_PAYLOAD.to_vec()));
        j.finish().unwrap();
        let r = replay(&base, "7", max, &config(), &gas(), Some([0xC0; 32]), None).unwrap();
        assert_eq!(r.verdict(), "PASS", "{}", r.summary());
        assert!(
            r.journaled > 0 && r.journaled == r.replayed,
            "{}",
            r.summary()
        );
        assert_eq!(r.counters, s.counters);
        std::fs::remove_dir_all(&base).unwrap();
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
        assert!(j.record(
            RecordKind::Runtime,
            2,
            switch_payload(true, StrategyVenues::FLAP)
        ));
        let mut s = runtime();
        let mut records = launch_records(100);
        records.extend((101..=107).map(block));
        for (k, p) in records {
            assert!(j.record(k, 3, p.clone()));
            s.record(k, &p, 3, Some(&mut j));
        }
        // Phase I: the PASS is one BUY action of the configured input on
        // the token's curve at the decision.
        assert!(s.has_pending());
        let actions = s.take_actions();
        assert!(!s.has_pending());
        assert!(
            matches!(
                actions.as_slice(),
                [StrategyAction::Buy { token, input_wei, .. }]
                    if *token == TOKEN && *input_wei == config().input_wei
            ),
            "{actions:?}"
        );
        // Gate M1: the action at the PASS block's completion (105, fill
        // delay 1: the fill is 106), journaled as the runtime does.
        assert!(j.record(RecordKind::Runtime, 3, action_payload(105)));
        assert!(j.record(
            RecordKind::Runtime,
            4,
            switch_payload(false, StrategyVenues::FLAP)
        ));
        // After it: journaled only.
        assert!(j.record(RecordKind::Canonical, 5, block_payload(108, &HASH, 6_000)));
        assert!(j.record(RecordKind::Runtime, 6, EXIT_PAYLOAD.to_vec()));
        j.finish().unwrap();

        let r = replay(&base, "7", max, &config(), &gas(), Some([0xC0; 32]), None).unwrap();
        assert_eq!(r.verdict(), "PASS", "{}", r.summary());
        assert_eq!((r.starts, r.journaled, r.replayed), (1, 2, 2));
        assert_eq!(r.counters, s.counters);
        // H gate L10: the position entered at 106 was still open at the
        // switch.
        assert_eq!(r.open_at_end, 1);
        // Phase I gate M1: its position is the PASS's, decided on block 105
        // (not the fill's 106) and acted on at 7 ms after the run's start
        // (run 7, the action record at 3 ns): eligible for 106 (4.5 s).
        assert_eq!(
            r.positions,
            [Position {
                paper: false,
                token: TOKEN,
                decided_block: 105,
                decided_unix_us: Some(7_000),
                eligible_block: Some(106),
                lead_us: Some(4_493_000),
                ..Position::default()
            }]
        );
        assert_eq!(r.position_totals(), (1, 0, 0));
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
        assert_eq!(
            rec.get(darkbot_bsc_record::schema::bsc_runtime::strategy_replay::POSITIONS_NEXT_BLOCK),
            Some(&darkbot_bsc_record::Value::U64(1))
        );

        let mut other = config();
        other.input_wei = U256::from(20_000_000_000_000_000u64);
        let r = replay(&base, "7", max, &other, &gas(), Some([0xC0; 32]), None).unwrap();
        assert_eq!((r.verdict(), r.first_difference), ("DIFFERS", Some(1)));
        let r = replay(&base, "7", max, &config(), &gas(), Some([0xC1; 32]), None).unwrap();
        assert_eq!((r.verdict(), r.config_match), ("DIFFERS", Some(false)));
        // H gate MEDIUM 3: the decision's features are part of its record,
        // so a feature computed differently is a difference.
        let mut filtered = config();
        // The creator's share rounded at 2 places instead of 4.
        filtered.ratio_places = 2;
        let r = replay(&base, "7", max, &filtered, &gas(), Some([0xC0; 32]), None).unwrap();
        assert_eq!((r.verdict(), r.first_difference), ("DIFFERS", Some(0)));
        // H gate L6: without the run manifest the configuration is unknown.
        std::fs::remove_file(base.join("7").join(crate::runtime_journal::RUN_MANIFEST)).unwrap();
        let r = replay(&base, "7", max, &config(), &gas(), Some([0xC0; 32]), None).unwrap();
        assert_eq!((r.verdict(), r.config_match), ("INCOMPLETE", None));
        std::fs::remove_dir_all(&base).unwrap();
    }

    /// Frontend package (06.11.): a start after the operator's settings
    /// (`p`) replays with them in place on the run's configuration; the
    /// same outputs without the record differ, and a settings record
    /// without the configuration does not replay.
    #[test]
    fn a_start_replays_with_the_settings_it_took() {
        use crate::runtime_journal::{EXIT_PAYLOAD, JournalSetup, RunIdentity};
        use crate::strategy_settings::{Settings, effective};
        const CONFIG: &str = include_str!("../../../../config/ingress.public.env");
        let file = crate::runtime::RuntimeConfig::parse(CONFIG)
            .unwrap()
            .strategy
            .engine;
        // No entry filter: the launch without BUYs passes (the file's
        // filters reject it).
        let settings = Settings {
            generation: 3,
            request_id: None,
            values: vec![("BSC_STRATEGY_C_FILTERS", String::new())],
        };
        let set = effective(CONFIG, &settings).unwrap().strategy.engine;
        let max = 16_384;
        let journal = |run: u64, with_settings: bool, lose: bool| {
            let base = std::env::temp_dir().join(format!(
                "bsc-strategy-settings-replay-{run}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&base);
            let mut j = RuntimeJournal::open(&base, run, 1, JournalSetup::whole(64, max)).unwrap();
            j.set_identity(RunIdentity {
                host: "test".into(),
                build: "test".into(),
                config_sha256: [0xC0; 32],
                build_sha256: None,
                build_dirty: None,
                capture_pending: "VENUES",
            });
            if with_settings {
                assert!(j.record(RecordKind::Runtime, 1, settings.journal()));
            }
            assert!(j.record(
                RecordKind::Runtime,
                2,
                switch_payload(true, StrategyVenues::FLAP)
            ));
            let mut s = StrategyRuntime::new(set.clone(), gas(), StrategyVenues::FLAP);
            let mut records = launch_records(100);
            records.extend((101..=107).map(block));
            for (k, p) in records {
                assert!(j.record(k, 3, p.clone()));
                s.record(k, &p, 3, Some(&mut j));
            }
            assert!(s.counters.passed > 0);
            if lose {
                assert!(!j.record(RecordKind::Runtime, 5, vec![0; max + 1]));
            }
            assert!(j.record(RecordKind::Runtime, 6, EXIT_PAYLOAD.to_vec()));
            j.finish().unwrap();
            base
        };
        let base = journal(7, true, false);
        let r = replay(
            &base,
            "7",
            max,
            &file,
            &gas(),
            Some([0xC0; 32]),
            Some(CONFIG),
        )
        .unwrap();
        assert_eq!(r.verdict(), "PASS", "{}", r.summary());
        assert!(r.journaled > 0 && r.counters.passed > 0, "{}", r.summary());
        let r = replay(&base, "7", max, &file, &gas(), Some([0xC0; 32]), None).unwrap();
        assert_eq!(r.verdict(), "INCOMPLETE", "{}", r.summary());
        std::fs::remove_dir_all(&base).unwrap();
        let base = journal(8, false, false);
        let r = replay(
            &base,
            "8",
            max,
            &file,
            &gas(),
            Some([0xC0; 32]),
            Some(CONFIG),
        )
        .unwrap();
        assert_eq!(r.verdict(), "DIFFERS", "{}", r.summary());
        std::fs::remove_dir_all(&base).unwrap();
        // bugs_v4 D5: a record the capture lost leaves the replay
        // INCOMPLETE, never PASS (before: PASS).
        let base = journal(9, true, true);
        let r = replay(
            &base,
            "9",
            max,
            &file,
            &gas(),
            Some([0xC0; 32]),
            Some(CONFIG),
        )
        .unwrap();
        assert_eq!((r.verdict(), r.lost), ("INCOMPLETE", 1), "{}", r.summary());
        assert!(
            r.reasons()
                .iter()
                .any(|x| x.contains("1 journal records lost"))
        );
        std::fs::remove_dir_all(&base).unwrap();
    }

    /// SC4: the foreign BUYs an entry is placed against are the canonical
    /// BUYs of either venue, never a SELL.
    #[test]
    fn foreign_buys_are_the_canonical_buys_of_either_venue() {
        let trade = |sell| Trade {
            block_number: 101,
            tx_index: 1,
            log_index: 0,
            sell,
            trader: [0x77; 20],
            amount: U256::ONE,
            eth: U256::ONE,
            fee: U256::ZERO,
        };
        let post = FourPost {
            remaining: U256::ONE,
            raised: U256::ONE,
        };
        for sell in [false, true] {
            let want = (!sell).then_some((TOKEN, 101));
            assert_eq!(
                foreign_buy(&Input::Trade {
                    token: TOKEN,
                    trade: trade(sell)
                }),
                want
            );
            assert_eq!(
                foreign_buy(&Input::FourTrade {
                    token: TOKEN,
                    trade: trade(sell),
                    post
                }),
                want
            );
        }
        assert_eq!(foreign_buy(&Input::FourIncomplete), None);
    }

    /// Phase I (PHASENPLAN §17 J input): an entry's position against the
    /// foreign BUYs of its token. Blocks every 450 ms from 1 s; a decision
    /// on block 100 at 1.5 s is after the header time of 101 (1.45 s), so
    /// its earliest block is 102; a foreign BUY in 101 is ahead of it, one
    /// in 102 competes there; BUYs of another token, in the decided block
    /// or SELLs (not passed in) do not count. A decision at 1.1 s is
    /// eligible for 101. Without the run's start time nothing is placed.
    #[test]
    fn an_entry_is_placed_against_the_foreign_buys() {
        let blocks: std::collections::BTreeMap<u64, u64> =
            (100..=104).map(|n| (n, 1_000 + (n - 100) * 450)).collect();
        let other = [0x77; 20];
        let buys = [
            (TOKEN, 100),
            (TOKEN, 101),
            (other, 101),
            (TOKEN, 102),
            (TOKEN, 104),
        ];
        let entries = [
            (false, TOKEN, 100, 1_500_000_000, true),
            (true, TOKEN, 100, 1_100_000_000, true),
            (false, other, 103, 3_000_000_000, false),
        ];
        let p = positions(&entries, Some(0), &blocks, &buys);
        assert_eq!(
            p[0],
            Position {
                paper: false,
                token: TOKEN,
                decided_block: 100,
                decided_unix_us: Some(1_500_000),
                eligible_block: Some(102),
                lead_us: Some(-50_000),
                buys_before: 1,
                buys_in_eligible: 1,
                first_buy_gap_blocks: Some(1),
                time_lower_bound: false,
            }
        );
        // An older journal's PASS: its record's time, a lower bound.
        assert!(p[2].time_lower_bound);
        assert_eq!(
            (
                p[1].paper,
                p[1].eligible_block,
                p[1].lead_us,
                p[1].buys_before,
                p[1].buys_in_eligible
            ),
            (true, Some(101), Some(350_000), 0, 1)
        );
        // No journaled block after 3 s: no eligible block.
        assert_eq!(
            (p[2].eligible_block, p[2].first_buy_gap_blocks),
            (None, None)
        );
        let r = Replay {
            positions: p,
            ..Replay::default()
        };
        assert_eq!(r.position_totals(), (1, 2, 1));
        let unplaced = positions(&entries[..1], None, &blocks, &buys);
        assert_eq!(
            (
                unplaced[0].decided_unix_us,
                unplaced[0].eligible_block,
                unplaced[0].lead_us
            ),
            (None, None, None)
        );
        assert_eq!(unplaced[0].first_buy_gap_blocks, Some(1));
    }

    /// Phase I gate pass-4 LOW: a PASS whose BUY reached the PAPER sink is
    /// placed once, from its J record (before: also as SHADOW, counted
    /// twice in the totals).
    #[test]
    fn a_paper_buy_replaces_its_shadow_position() {
        use crate::runtime_journal::{EXIT_PAYLOAD, JournalSetup, RunIdentity};
        let base =
            std::env::temp_dir().join(format!("bsc-strategy-paper-pos-{}", std::process::id()));
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
        assert!(j.record(
            RecordKind::Runtime,
            2,
            switch_payload(true, StrategyVenues::FLAP)
        ));
        let mut records = launch_records(100);
        records.extend((101..=107).map(block));
        for (k, p) in records {
            assert!(j.record(k, 3, p));
        }
        // The PASS's BUY at the sink: J | BUY | sequence | hash | token |
        // head block 105 | times | bytes.
        let mut paper = vec![b'J', 1];
        paper.extend([0u8; 40]);
        paper.extend(TOKEN);
        paper.extend(105u64.to_le_bytes());
        paper.extend([0u8; 32]);
        paper.extend([0xAA; 8]);
        assert!(j.record(RecordKind::Runtime, 5_000_000, paper));
        assert!(j.record(
            RecordKind::Runtime,
            6_000_000,
            switch_payload(false, StrategyVenues::FLAP)
        ));
        assert!(j.record(RecordKind::Runtime, 7_000_000, EXIT_PAYLOAD.to_vec()));
        j.finish().unwrap();
        let r = replay(&base, "7", max, &config(), &gas(), Some([0xC0; 32]), None).unwrap();
        assert_eq!(r.positions.len(), 1, "{:?}", r.positions);
        let p = &r.positions[0];
        assert_eq!(
            (
                p.paper,
                p.decided_block,
                p.decided_unix_us,
                p.time_lower_bound
            ),
            (true, 105, Some(12_000), false)
        );
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
            profiles_max: 1,
        };
        let text = std::fs::read_to_string(root.join("bsc/config/ingress.public.env")).unwrap();
        let configured = crate::runtime::RuntimeConfig::parse(&text)
            .unwrap()
            .strategy;
        assert!(configured.gas(&root).is_ok());
        for bad in [
            &["bsc/config/ingress.public.env"][..],
            &["bsc/native/evidence/gas/../gas/flap_gas_20261031.txt"][..],
            // Audit E 1.7: the former test-fixture location is no gas evidence.
            &["bsc/engine/crates/trading/fixtures/bsc/flap_gas_20261031.txt"][..],
            &[][..],
        ] {
            assert!(with(bad).gas(&root).is_err(), "{bad:?}");
        }
        // Audit E 1.7: the frozen gas evidence is evidence, each file one the
        // economics snapshot binds.
        for (path, _) in crate::runtime::StrategyConfig::FROZEN_GAS_EVIDENCE {
            assert!(path.starts_with("bsc/native/evidence/gas/"), "{path}");
            assert!(
                darkbot_bsc_trading::economics::snapshot::BOUND_FILES.contains(&path),
                "{path}"
            );
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

    /// SC4: the Four.meme strategy records.
    mod four {
        use super::*;
        use crate::canonical_join::Venue as LaunchVenue;
        use crate::runtime::exec::tests::{four_launch, four_observation, openfour_launch};
        use alloy_primitives::{Address, B256};
        use darkbot_bsc_geth_ipc::ingress::decode_tx_target;
        use darkbot_bsc_trading::economics::four_classic::LaunchTerms;
        use darkbot_bsc_trading::economics::openfour::launch_curve;
        use darkbot_bsc_trading::venues::fourmeme::deployment::{
            bsc_mainnet_r12b_131568366, bsc_mainnet_r12d_131608421,
        };
        use darkbot_bsc_trading::venues::fourmeme::profile::{
            OPENFOUR_WITNESSED_RAISE, WITNESSED_MAX_OFFERS, WITNESSED_TOTAL_SUPPLY,
            classify_classic_launch,
        };
        use darkbot_bsc_trading::venues::fourmeme::recognize::{Recognized, recognize};

        const MAX: usize = 16_384;

        /// The witnessed launches classify on the validated deployment to
        /// their launch curves (OpenFour: the bonding preset's terms; its
        /// token from its signer), each refusal with its code; the `n`
        /// record reads back to that curve and code.
        #[test]
        fn four_launches_classify_to_their_launch_curves() {
            let d = bsc_mainnet_r12d_131608421();
            let v = d.validate(&four_observation(&d).observed, 1).unwrap();
            // OpenFour.
            let (hash, raw, launcher) = openfour_launch(1);
            let launch = match recognize(
                &d,
                decode_tx_target(&raw).unwrap().to.map(Address::from),
                decode_tx_target(&raw).unwrap().data,
                MAX,
            ) {
                Recognized::OpenFourLaunch(l) => l,
                other => panic!("{other:?}"),
            };
            let token = d
                .openfour_token(
                    launch.preset_id,
                    launch.init.token_salt,
                    launch.request_id,
                    Some(Address::from(launcher)),
                )
                .unwrap()
                .into_array();
            let classify = |validated, venue, raw: Option<&[u8]>, sender, token| {
                classify_four_launch(validated, venue, raw, sender, (token, hash), MAX)
            };
            let curve = classify(
                Some(v),
                LaunchVenue::OpenFour,
                Some(&raw),
                Some(launcher),
                token,
            )
            .unwrap();
            let start = launch_curve(&LaunchTerms {
                total_supply: WITNESSED_TOTAL_SUPPLY,
                max_offers: WITNESSED_MAX_OFFERS,
                max_raising: OPENFOUR_WITNESSED_RAISE,
            })
            .unwrap();
            let FourCurve::OpenFour { start: s, now, fee } = curve else {
                panic!()
            };
            assert_eq!(s, start);
            assert!(
                now.remaining < start.remaining,
                "the creator's BUY is in it"
            );
            assert_eq!((fee.protocol_bps, fee.dev_bps), (50, 50));
            for (validated, venue, raw, sender, t, code) in [
                (
                    None,
                    LaunchVenue::OpenFour,
                    Some(&raw[..]),
                    Some(launcher),
                    token,
                    0,
                ),
                (
                    Some(v),
                    LaunchVenue::OpenFour,
                    None,
                    Some(launcher),
                    token,
                    1,
                ),
                (
                    Some(v),
                    LaunchVenue::OpenFour,
                    Some(&raw[..]),
                    None,
                    token,
                    1,
                ),
                (
                    Some(v),
                    LaunchVenue::Four,
                    Some(&raw[..]),
                    Some(launcher),
                    token,
                    2,
                ),
                (
                    Some(v),
                    LaunchVenue::OpenFour,
                    Some(&raw[..]),
                    Some([0x11; 20]),
                    token,
                    4,
                ),
                (
                    Some(v),
                    LaunchVenue::OpenFour,
                    Some(&raw[..]),
                    Some(launcher),
                    [0x22; 20],
                    4,
                ),
            ] {
                assert_eq!(classify(validated, venue, raw, sender, t), Err(code));
            }
            // A pin without the executed preset: outside the profile.
            let earlier = bsc_mainnet_r12b_131568366();
            let unbonded = earlier
                .validate(&four_observation(&earlier).observed, 1)
                .unwrap();
            assert_eq!(
                classify(
                    Some(unbonded),
                    LaunchVenue::OpenFour,
                    Some(&raw),
                    Some(launcher),
                    token
                ),
                Err(3)
            );
            // The record: the launch curve, read back; a refusal's code.
            let p = four_launch_payload(
                LaunchVenue::OpenFour,
                (9, 4),
                &token,
                &[0xC1; 20],
                Ok(curve),
            );
            assert_eq!(p.len(), FOUR_LAUNCH_LEN);
            assert_eq!(
                four_launch_of(&p),
                Some((
                    LaunchVenue::OpenFour,
                    9,
                    4,
                    token,
                    [0xC1; 20],
                    Ok(FourCurve::openfour(start, fee))
                ))
            );
            for (code, why) in FOUR_LAUNCH_REFUSALS.iter().enumerate() {
                let p =
                    four_launch_payload(LaunchVenue::Four, (9, 4), &token, &[0xC1; 20], Err(code));
                assert_eq!(four_launch_of(&p).map(|r| r.5), Some(Err(*why)));
            }
            let mut flap = p.clone();
            flap[1] = 1;
            let mut code = p.clone();
            code[58] = FOUR_LAUNCH_REFUSALS.len() as u8 + 1;
            let mut product = four_launch_payload(
                LaunchVenue::OpenFour,
                (9, 4),
                &token,
                &[0xC1; 20],
                Ok(curve),
            );
            product[60] ^= 1;
            for bad in [flap, code, product, p[..p.len() - 1].to_vec()] {
                assert_eq!(four_launch_of(&bad), None);
            }
            // Classic: the launch curve its classification leaves, with the
            // creator's BUY taken back.
            let (hash, raw, _) = four_launch(2);
            let target = decode_tx_target(&raw).unwrap();
            let Recognized::ClassicLaunch(l) =
                recognize(&d, target.to.map(Address::from), target.data, MAX)
            else {
                panic!()
            };
            let c = classify_classic_launch(
                &v,
                &l,
                U256::from_be_slice(target.value),
                B256::from(hash),
            )
            .unwrap();
            let curve = classify_four_launch(
                Some(v),
                LaunchVenue::Four,
                Some(&raw),
                None,
                (c.token().into_array(), hash),
                MAX,
            )
            .unwrap();
            let FourCurve::Classic { start, now, fee } = curve else {
                panic!()
            };
            assert_eq!((now, fee), (c.curve(), c.fee()));
            assert!(start.sold.is_zero() && start.t == now.t + now.sold);
            let p = four_launch_payload(LaunchVenue::Four, (9, 4), &[1; 20], &[2; 20], Ok(curve));
            assert_eq!(
                four_launch_of(&p).map(|r| r.5),
                Some(Ok(FourCurve::classic(start, fee).unwrap()))
            );
        }

        /// The Four.meme curve records are the engine's Four inputs (a phase
        /// into trading is none); the strategy follows only the venues it
        /// was switched on with, and the switch record names them (Flap
        /// alone as before).
        #[test]
        fn four_records_reach_only_a_strategy_that_follows_their_venue() {
            use crate::chain_follower::CanonicalFourTrade;
            use crate::runtime_journal::{
                four_phase_payload, four_trade_gap_payload, four_trade_payload,
            };
            let trade = CanonicalFourTrade {
                venue: LaunchVenue::OpenFour,
                block_number: 5,
                block_hash: HASH,
                tx_index: 3,
                tx_hash: [3; 32],
                log_index: 1,
                origin: [0x77; 20],
                sell: true,
                token: TOKEN,
                trader: [0x77; 20],
                amount: U256::from(11u8).to_be_bytes(),
                quote: U256::from(12u8).to_be_bytes(),
                remaining: U256::from(13u8).to_be_bytes(),
                raised: U256::from(14u8).to_be_bytes(),
            };
            assert_eq!(
                input_of_record(RecordKind::Canonical, &four_trade_payload(&trade)),
                Some(Input::FourTrade {
                    token: TOKEN,
                    trade: Trade {
                        block_number: 5,
                        tx_index: 3,
                        log_index: 1,
                        sell: true,
                        trader: [0x77; 20],
                        amount: U256::from(11u8),
                        eth: U256::from(12u8),
                        fee: U256::ZERO,
                    },
                    post: FourPost {
                        remaining: U256::from(13u8),
                        raised: U256::from(14u8),
                    },
                })
            );
            let phase = |to| four_phase_payload(5, &HASH, (3, 2), &TOKEN, (1, to));
            assert_eq!(input_of_record(RecordKind::Canonical, &phase(1)), None);
            assert_eq!(
                input_of_record(RecordKind::Canonical, &phase(2)),
                Some(Input::FourOffCurve { token: TOKEN })
            );
            assert_eq!(
                input_of_record(
                    RecordKind::Canonical,
                    &four_trade_gap_payload(5, &HASH, 3, 4)
                ),
                Some(Input::FourIncomplete)
            );
            // The switch records.
            let four = StrategyVenues {
                flap: false,
                four: true,
                openfour: true,
            };
            assert_eq!(switch_payload(true, StrategyVenues::FLAP), [b'U', 1]);
            assert_eq!(switch_payload(false, four), [b'U', 0]);
            assert_eq!(switched_on(&switch_payload(true, four)), Some(four));
            assert_eq!(switched_on(&[b'U', 1]), Some(StrategyVenues::FLAP));
            for bad in [&[b'U', 0][..], &[b'U', 1, 0], &[b'U', 1, 8], &[b'U', 2, 1]] {
                assert_eq!(switched_on(bad), None, "{bad:?}");
            }
            // Only the followed venues' launches are inputs.
            let launch =
                four_launch_payload(LaunchVenue::OpenFour, (5, 2), &TOKEN, &CREATOR, Err(3));
            let flap = flap_launch_payload(5, 3, &[0xBB; 20], &CREATOR);
            // A Flap-only strategy neither steps nor counts the Four.meme
            // curve records.
            for (venues, counted) in [(StrategyVenues::FLAP, (0, 0, 0)), (four, (1, 1, 1))] {
                let mut s = StrategyRuntime::new(config(), gas(), venues);
                for p in [
                    four_trade_payload(&trade),
                    phase(2),
                    four_trade_gap_payload(5, &HASH, 3, 4),
                ] {
                    s.record(RecordKind::Canonical, &p, 1, None);
                }
                assert_eq!(
                    (
                        s.counters.trades,
                        s.counters.migrations,
                        s.counters.incomplete
                    ),
                    counted,
                    "{venues:?}"
                );
            }
            for (venues, launches) in [
                (StrategyVenues::FLAP, 1),
                (four, 1),
                (StrategyVenues { flap: true, ..four }, 2),
            ] {
                let mut s = StrategyRuntime::new(config(), gas(), venues);
                s.record(
                    RecordKind::Canonical,
                    &block_payload(5, &HASH, 750),
                    1,
                    None,
                );
                s.record(RecordKind::Canonical, &launch, 2, None);
                s.record(RecordKind::Canonical, &flap, 3, None);
                assert_eq!(s.counters.launches, launches, "{venues:?}");
                assert_eq!(s.followed() as u64, launches);
            }
        }

        /// SC4: a PASS on an OpenFour token is a BUY action on its Four.meme
        /// curve as its journaled trades name it at the complete block; once the
        /// position is open its exit action sells the position's tokens on
        /// the shadow (the real curve with them in it).
        #[test]
        fn an_openfour_pass_and_exit_are_four_actions_on_the_exact_curve() {
            use crate::chain_follower::CanonicalFourTrade;
            use crate::runtime_journal::{JournalSetup, four_trade_payload};
            use darkbot_bsc_trading::economics::network::{Action, PathClass, Venue};
            use darkbot_bsc_trading::economics::openfour::OpenFourFee;
            let base =
                std::env::temp_dir().join(format!("bsc-strategy-four-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&base);
            let mut j = RuntimeJournal::open(&base, 7, 1, JournalSetup::whole(64, 16_384)).unwrap();
            let mut gas = gas();
            for (action, used) in [
                (Action::Buy, 1_694_570),
                (Action::Approve, 48_788),
                (Action::Sell, 570_922),
            ] {
                gas.add(
                    PathClass {
                        venue: Venue::OpenFour,
                        action,
                    },
                    used,
                )
                .unwrap();
            }
            let venues = StrategyVenues {
                flap: false,
                four: false,
                openfour: true,
            };
            let mut s = StrategyRuntime::new(config(), gas, venues);
            let start = launch_curve(&LaunchTerms {
                total_supply: WITNESSED_TOTAL_SUPPLY,
                max_offers: WITNESSED_MAX_OFFERS,
                max_raising: OPENFOUR_WITNESSED_RAISE,
            })
            .unwrap();
            let fee = OpenFourFee {
                protocol_bps: 50,
                dev_bps: 50,
            };
            let mut real = FourCurve::openfour(start, fee);
            let mut feed = |s: &mut StrategyRuntime, p: Vec<u8>| {
                s.record(RecordKind::Canonical, &p, 1, Some(&mut j));
            };
            feed(&mut s, block_payload(100, &HASH, 0));
            feed(
                &mut s,
                four_launch_payload(LaunchVenue::OpenFour, (100, 1), &TOKEN, &CREATOR, Ok(real)),
            );
            let buy = |s: &mut StrategyRuntime, real: &mut FourCurve, block, trader, wei: u64| {
                let (tokens, paid) = real.buy(U256::from(wei)).unwrap();
                *real = real.shifted(tokens).unwrap();
                let FourCurve::OpenFour { start, now, .. } = *real else {
                    panic!()
                };
                let p = four_trade_payload(&CanonicalFourTrade {
                    venue: LaunchVenue::OpenFour,
                    block_number: block,
                    block_hash: HASH,
                    tx_index: 2,
                    tx_hash: [2; 32],
                    log_index: 0,
                    origin: trader,
                    sell: false,
                    token: TOKEN,
                    trader,
                    amount: tokens.to_be_bytes(),
                    quote: paid.to_be_bytes(),
                    remaining: now.remaining.to_be_bytes(),
                    raised: (now.quote_reserve - start.quote_reserve).to_be_bytes(),
                });
                s.record(RecordKind::Canonical, &p, 1, None);
            };
            for b in 101..=104u64 {
                feed(&mut s, block_payload(b, &HASH, (b - 100) * 750));
                buy(&mut s, &mut real, b, [b as u8; 20], 20_000_000_000_000_000);
            }
            feed(&mut s, block_payload(105, &HASH, 5 * 750));
            assert_eq!(s.counters.passed, 1);
            let actions = s.take_actions();
            assert_eq!(
                actions,
                [StrategyAction::Buy {
                    token: TOKEN,
                    creator: CREATOR,
                    curve: StrategyCurve::Four(real),
                    input_wei: config().input_wei,
                    decided_ns: 1,
                    // Decision 06.11.: the deciding strategy's exit rules.
                    exits: Some(config().exits()),
                }]
            );
            // Filled at the end of 106: the exit sells the position's
            // tokens on the shadow.
            feed(&mut s, block_payload(106, &HASH, 6 * 750));
            feed(&mut s, block_payload(107, &HASH, 7 * 750));
            let tokens = s.engine.position_tokens(&TOKEN).unwrap();
            assert_eq!(tokens, real.buy(config().input_wei).unwrap().0);
            assert_eq!(
                s.exit_action(TOKEN, 9),
                Some(StrategyAction::Exit {
                    token: TOKEN,
                    tokens,
                    curve: StrategyCurve::Four(real.shifted(tokens).unwrap()),
                    decided_ns: 9,
                })
            );
            let _ = j.finish();
            std::fs::remove_dir_all(&base).unwrap();
        }
    }
}
