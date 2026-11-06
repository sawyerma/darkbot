//! R13 (FOREIGN_AUTO): the foreign launch profile against the G0.4 campaign's foreign launches as the chain executed
//! them (`fixtures/flap/foreign_launch_parity_20261106.txt`).

use super::*;
use crate::economics::flap::QuoteError;
use crate::economics::flap::native_curve20::{
    HoldingCap, NativeBuyInputs, NativeBuyQuote, NativeSellInputs, PricedToken,
    WITNESSED_MAINNET_TERMS, profile_terms, quote_native_buy, quote_native_sell,
};
use crate::venues::flap::abi::decode_new_token_v6;
use crate::venues::flap::deployment::testkit::validated_mainnet;

const FIXTURE: &str = include_str!("../../../fixtures/flap/foreign_launch_parity_20261106.txt");

fn rows(kind: &'static str) -> impl Iterator<Item = Vec<&'static str>> {
    FIXTURE
        .lines()
        .map(|l| l.split(' ').collect::<Vec<_>>())
        .filter(move |f| f[0] == kind)
}

fn u(s: &str) -> U256 {
    U256::from_str_radix(s, 10).unwrap()
}

fn priced(buy: &str, sell: &str) -> PricedToken {
    PricedToken::new(&profile_terms(buy.parse().unwrap(), sell.parse().unwrap())).unwrap()
}

fn buy(
    token: PricedToken,
    reserve: U256,
    circulating: U256,
    input: U256,
) -> Result<NativeBuyQuote, QuoteError> {
    quote_native_buy(&NativeBuyInputs {
        token,
        reserve_before: reserve,
        circulating_before: circulating,
        input_amount: input,
        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
        tax_on_bonding_curve_enabled: WITNESSED_MAINNET_TERMS.tax_on_bonding_curve_enabled,
        caller_fee_exempt: WITNESSED_MAINNET_TERMS.caller_fee_exempt,
        holding_cap: HoldingCap::None,
    })
}

/// The launch row of `tx`.
fn launch(tx: &str) -> Vec<&'static str> {
    rows("LAUNCH").find(|f| f[2] == tx).unwrap()
}

/// The curve a launch row's creator purchase left (the fresh curve
/// without one).
fn after_launch(f: &[&str]) -> (U256, U256) {
    if u(f[10]).is_zero() {
        return (U256::ZERO, U256::ZERO);
    }
    let q = buy(priced(f[5], f[6]), U256::ZERO, U256::ZERO, u(f[10])).unwrap();
    (q.reserve_after, q.supply_after)
}

/// Every creator purchase of a native launch (its receipt's TokenBought:
/// the transaction's value in, its tokens and protocol fee) is the
/// production quote on the fresh curve under its taxes, whatever its
/// distribution, tax or anti-farmer duration; one that buys the curve out
/// (the DEX threshold, 8e26 tokens) is refused by the quote.
#[test]
fn every_creator_purchase_is_the_quote_on_the_fresh_curve() {
    let threshold = U256::from(800_000_000_000_000_000_000_000_000u128);
    let (mut priced_rows, mut out, mut durations) = (0, 0, std::collections::BTreeSet::new());
    for f in rows("LAUNCH").filter(|f| f[10] != "0") {
        let q = buy(priced(f[5], f[6]), U256::ZERO, U256::ZERO, u(f[10]));
        if u(f[11]) == threshold {
            assert_eq!(q.err(), Some(QuoteError::ReachesDexThreshold), "{f:?}");
            out += 1;
            continue;
        }
        let q = q.unwrap();
        assert_eq!(
            (q.output_amount, q.protocol_fee),
            (u(f[11]), u(f[12])),
            "{f:?}"
        );
        durations.insert((f[7], f[8]));
        priced_rows += 1;
    }
    assert!(priced_rows >= 1_000 && out >= 1, "{priced_rows} {out}");
    assert!(durations.len() > 3, "{durations:?}");
}

/// The first trade of a launch's token after the launch transaction in its
/// block (where a C3 BUY lands) is the production quote on the curve the
/// launch left: a BUY's tokens and fee from its native amount; one that
/// reaches the DEX threshold is refused by the quote.
#[test]
fn every_first_trade_after_a_launch_is_the_quote_on_the_curve_it_left() {
    let threshold = U256::from(800_000_000_000_000_000_000_000_000u128);
    let (mut matched, mut out, mut after_creator) = (0, 0, 0);
    for t in rows("FIRST") {
        let f = launch(t[1]);
        let (reserve, circulating) = after_launch(&f);
        let token = priced(f[5], f[6]);
        let result = match t[2] {
            "BUY" => {
                buy(token, reserve, circulating, u(t[4])).map(|q| (q.output_amount, q.protocol_fee))
            }
            _ => quote_native_sell(&NativeSellInputs {
                token,
                reserve_before: reserve,
                circulating_before: circulating,
                input_amount: u(t[3]),
                protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_sell_fee_bps,
                tax_on_bonding_curve_enabled: WITNESSED_MAINNET_TERMS.tax_on_bonding_curve_enabled,
                caller_fee_exempt: WITNESSED_MAINNET_TERMS.caller_fee_exempt,
            })
            .map(|q| (q.output_amount, q.protocol_fee)),
        };
        match result {
            // Review L6: refused only where the chain's trade bought the
            // curve to the DEX threshold.
            Err(QuoteError::ReachesDexThreshold) => {
                assert_eq!(circulating + u(t[3]), threshold, "{t:?}");
                out += 1;
            }
            r => {
                let expected = match t[2] {
                    "BUY" => (u(t[3]), u(t[5])),
                    _ => (u(t[4]), u(t[5])),
                };
                assert_eq!(r, Ok(expected), "{t:?}");
                matched += 1;
                after_creator += usize::from(f[10] != "0");
            }
        }
    }
    assert!(
        matched >= 150 && after_creator >= 150 && out <= 4,
        "{matched} {after_creator} {out}"
    );
}

/// The sampled launch transactions classify as their receipts and the
/// census say: the foreign profile admits each native launch without
/// permit or extension whatever its identity, its token the receipt's
/// (the CREATE2 of its salt), its creator purchase the transaction's
/// value.
#[test]
fn the_sampled_launches_classify_as_their_receipts_say() {
    let deployment = validated_mainnet(1);
    let mut kinds = std::collections::BTreeMap::new();
    for c in rows("CALLDATA") {
        let input: Vec<u8> = (0..c[4].len() / 2)
            .map(|i| u8::from_str_radix(&c[4][2 * i..2 * i + 2], 16).unwrap())
            .collect();
        let decoded = decode_new_token_v6(&input, input.len()).unwrap();
        let candidate = c[1].parse::<B256>().unwrap();
        let classified = classify_foreign_launch(&deployment, candidate, &decoded, u(c[3]));
        *kinds.entry(c[2]).or_insert(0) += 1;
        match c[2] {
            "OK" => {
                let k = classified.unwrap();
                let f = launch(c[1]);
                assert_eq!(k.token(), f[4].parse::<Address>().unwrap(), "{c:?}");
                assert_eq!(
                    (
                        k.creator_input(),
                        k.buy_tax_bps().to_string(),
                        k.sell_tax_bps().to_string()
                    ),
                    (u(c[3]), f[5].to_string(), f[6].to_string())
                );
                assert_eq!(u(f[10]), k.creator_input());
            }
            why => assert_eq!(
                classified.map(|_| ()).map_err(|e| format!("{e:?}")),
                Err(why.to_string())
            ),
        }
    }
    assert!(kinds["OK"] >= 10 && kinds.len() >= 4, "{kinds:?}");
}

/// Review L3/L4: the foreign profile's tax bounds are the census' extremes
/// (the highest tax, the shortest tax duration of every launch row).
#[test]
fn the_foreign_bounds_are_the_censuss_extremes() {
    let launches: Vec<_> = rows("LAUNCH").collect();
    let max_tax = launches
        .iter()
        .flat_map(|f| [f[5], f[6]])
        .map(|t| t.parse::<u16>().unwrap())
        .max();
    let min_duration = launches.iter().map(|f| f[7].parse::<u64>().unwrap()).min();
    assert_eq!(max_tax, Some(FOREIGN_WITNESSED_MAX_TAX_BPS));
    assert_eq!(min_duration, Some(FOREIGN_WITNESSED_MIN_TAX_DURATION));
}
