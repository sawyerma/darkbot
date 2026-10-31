//! BSC3-069: deterministic adversity on the production admission and
//! quote at real mainnet curve states (parity v3): a sandwiched BUY, a SELL
//! after a crash and after a fast drop. A transaction executes only if its
//! output reaches the minimum it was signed with (the Portal refuses below
//! it: the transaction reverts, its gas is lost). Apart from
//! `native_curve20.rs`, which the frozen economics snapshot binds.

use alloy_primitives::{Address, B256, U256};
use darkbot_bsc_trading::chain_state::SnapshotId;
use darkbot_bsc_trading::economics::flap::native_curve20::{
    CurveState, HoldingCap, NativeBuyInputs, NativeBuyQuote, NativeSellInputs, NativeSellQuote,
    PricedToken, TokenTerms, WITNESSED_MAINNET_TERMS, admit_curve_buy, admit_curve_sell,
    profile_terms, quote_native_buy, quote_native_sell,
};
use darkbot_bsc_trading::economics::policy::{BuyPolicy, ExitPolicy};

const PARITY_V3: &str = include_str!("../fixtures/flap/quote_parity_g02_v3.txt");

fn snap() -> SnapshotId {
    SnapshotId {
        canonical_block_hash: B256::from([7u8; 32]),
        ordered_state_cursor: 1,
        pending_candidate: B256::ZERO,
        policy_generation: 4,
        codehash_generation: 1,
    }
}

/// The v3 samples in the production scope: terms, reserve, circulating
/// supply, the sample's input, and whether it is a BUY.
fn samples() -> Vec<(PricedToken, U256, U256, U256, bool)> {
    PARITY_V3
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f[18] != "SUPPORTED" || f[9] == "REVERT" || f[10] != "0" {
                return None;
            }
            let n = |i: usize| U256::from_str_radix(f[i], 10).unwrap();
            let terms = PricedToken::new(&TokenTerms {
                dex_supply_threshold: n(17),
                ..profile_terms(f[5].parse().unwrap(), f[6].parse().unwrap())
            })
            .unwrap();
            Some((terms, n(3), n(4), n(8), f[7] == "BUY"))
        })
        .collect()
}

fn bought(
    terms: PricedToken,
    reserve: U256,
    circulating: U256,
    input: U256,
) -> Option<NativeBuyQuote> {
    quote_native_buy(&NativeBuyInputs {
        token: terms,
        reserve_before: reserve,
        circulating_before: circulating,
        input_amount: input,
        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
        tax_on_bonding_curve_enabled: WITNESSED_MAINNET_TERMS.tax_on_bonding_curve_enabled,
        caller_fee_exempt: WITNESSED_MAINNET_TERMS.caller_fee_exempt,
        holding_cap: HoldingCap::None,
    })
    .ok()
}

fn sold(
    terms: PricedToken,
    reserve: U256,
    circulating: U256,
    tokens: U256,
) -> Option<NativeSellQuote> {
    quote_native_sell(&NativeSellInputs {
        token: terms,
        reserve_before: reserve,
        circulating_before: circulating,
        input_amount: tokens,
        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_sell_fee_bps,
        tax_on_bonding_curve_enabled: WITNESSED_MAINNET_TERMS.tax_on_bonding_curve_enabled,
        caller_fee_exempt: WITNESSED_MAINNET_TERMS.caller_fee_exempt,
    })
    .ok()
}

fn state(terms: PricedToken, reserve: U256, circulating: U256) -> CurveState {
    CurveState::tracked(
        Address::ZERO,
        terms,
        reserve,
        circulating,
        &WITNESSED_MAINNET_TERMS,
        snap(),
    )
    .unwrap()
}

/// A BUY another one front-runs (a sandwich's first leg) executes on the
/// moved curve only while its output reaches its signed minimum: there
/// is exactly one largest front-run it survives, its loss there within
/// the slippage, one wei more and it reverts; a wider slippage survives
/// a larger front-run (why the grant bounds the BUY slippage).
#[test]
fn a_sandwiched_buy_executes_within_its_slippage_or_reverts() {
    let (mut checked, mut grew) = (0, 0);
    for (terms, reserve, circulating, input, _) in samples().into_iter().filter(|s| s.4).take(40) {
        let at = state(terms, reserve, circulating);
        let mut survived = U256::ZERO;
        for slippage in [50u16, 100, 500, 2_000] {
            let policy = BuyPolicy::new(4, input, slippage, 1, 1).unwrap();
            let Ok(admitted) = admit_curve_buy(&at, &policy) else {
                continue;
            };
            let ours_after = |front: U256| -> Option<U256> {
                if front.is_zero() {
                    return bought(terms, reserve, circulating, input).map(|q| q.output_amount);
                }
                let moved = bought(terms, reserve, circulating, front)?;
                bought(terms, moved.reserve_after, moved.supply_after, input)
                    .map(|q| q.output_amount)
            };
            let executes = |front: U256| {
                ours_after(front).is_some_and(|out| out >= admitted.min_output_amount())
            };
            assert_eq!(ours_after(U256::ZERO), Some(admitted.expected_output()));
            assert!(executes(U256::ZERO));
            let (mut lo, mut hi) = (
                U256::ZERO,
                U256::from(1_000u64) * U256::from(10u64).pow(U256::from(18u8)),
            );
            if executes(hi) {
                continue;
            }
            while hi - lo > U256::from(1u8) {
                let mid = lo + (hi - lo) / U256::from(2u8);
                if executes(mid) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let out = ours_after(lo).unwrap();
            assert!(out >= admitted.min_output_amount());
            // At the largest front-run survived the loss is the slippage
            // (within one basis point): the minimum is exactly it.
            let bps = U256::from(10_000u16);
            assert!(out * bps >= admitted.expected_output() * U256::from(10_000 - slippage) - bps);
            assert!(out * bps <= admitted.expected_output() * U256::from(10_000 - slippage + 1));
            assert!(
                !executes(lo + U256::from(1u8)),
                "the minimum holds one wei beyond"
            );
            assert!(
                lo >= survived,
                "a wider slippage survives at least as large a front-run"
            );
            grew += usize::from(lo > survived);
            survived = lo;
            checked += 1;
        }
    }
    assert!(checked > 40, "{checked}");
    assert!(
        grew > checked / 2,
        "{grew} of {checked}: a wider slippage survives a larger front-run"
    );
}

/// Other holders dump before our SELL lands: others pumped the curve,
/// we bought at the top, they sell part or all of theirs. A SELL
/// admitted at the top executes only within its exit slippage (a fast
/// drop inside it: proceeds at least the minimum); beyond it its stale
/// minimum reverts; admitted again on the crashed curve (the exit's
/// continuation, BSC3-030) it executes there, at a lower minimum.
#[test]
fn a_sell_after_a_crash_reverts_its_stale_minimum_and_sells_requoted() {
    let wei = |milli_bnb: u64| U256::from(milli_bnb) * U256::from(1_000_000_000_000_000u64);
    let (mut within, mut beyond, mut bounded) = (0, 0, 0);
    for (terms, reserve, circulating, _, _) in samples().into_iter().take(40) {
        for pump in [wei(500), wei(2_000), wei(10_000)] {
            let Some(up) = bought(terms, reserve, circulating, pump) else {
                continue;
            };
            let Some(ours_buy) = bought(terms, up.reserve_after, up.supply_after, wei(10)) else {
                continue;
            };
            let (top_reserve, top_supply, tokens) = (
                ours_buy.reserve_after,
                ours_buy.supply_after,
                ours_buy.output_amount,
            );
            let top = state(terms, top_reserve, top_supply);
            for slippage in [100u16, 500, 2_000] {
                let policy = ExitPolicy::new(4, slippage, 1, 1, 1).unwrap();
                let stale = admit_curve_sell(&top, tokens, &policy).unwrap();
                // The largest dump the SELL survives: there its loss is
                // the exit slippage (within one basis point), one token
                // more and its minimum reverts.
                let ours_after = |dump: U256| -> Option<U256> {
                    let crash = sold(terms, top_reserve, top_supply, dump)?;
                    sold(terms, crash.reserve_after, crash.supply_after, tokens)
                        .map(|q| q.output_amount)
                };
                let survives = |dump: U256| {
                    ours_after(dump).is_some_and(|out| out >= stale.min_output_amount())
                };
                let (mut lo, mut hi) = (U256::ZERO, up.output_amount);
                if !survives(hi) {
                    while hi - lo > U256::from(1u8) {
                        let mid = lo + (hi - lo) / U256::from(2u8);
                        if survives(mid) {
                            lo = mid;
                        } else {
                            hi = mid;
                        }
                    }
                    let out = ours_after(lo).unwrap();
                    let bps = U256::from(10_000u16);
                    assert!(
                        out * bps >= stale.expected_output() * U256::from(10_000 - slippage) - bps
                    );
                    assert!(
                        out * bps <= stale.expected_output() * U256::from(10_000 - slippage + 1)
                    );
                    assert!(!survives(lo + U256::from(1u8)));
                    bounded += 1;
                }
                for per_mille in [1u64, 10, 100, 500, 1_000] {
                    let dump = up.output_amount * U256::from(per_mille) / U256::from(1_000u64);
                    let Some(crash) = sold(terms, top_reserve, top_supply, dump) else {
                        continue;
                    };
                    let Some(ours) = sold(terms, crash.reserve_after, crash.supply_after, tokens)
                    else {
                        continue;
                    };
                    // A dump never raises the proceeds (the curve's
                    // integer arithmetic may round one wei either way).
                    assert!(ours.output_amount <= stale.expected_output() + U256::from(1u8));
                    if ours.output_amount >= stale.min_output_amount() {
                        within += 1;
                    } else {
                        beyond += 1;
                        let crashed = state(terms, crash.reserve_after, crash.supply_after);
                        let fresh = admit_curve_sell(&crashed, tokens, &policy).unwrap();
                        assert_eq!(fresh.expected_output(), ours.output_amount);
                        assert!(ours.output_amount >= fresh.min_output_amount());
                        assert!(fresh.min_output_amount() < stale.min_output_amount());
                    }
                }
            }
        }
    }
    assert!(within > 0 && beyond > 0, "{within} within, {beyond} beyond");
    assert!(bounded > 20, "{bounded} boundaries");
}
