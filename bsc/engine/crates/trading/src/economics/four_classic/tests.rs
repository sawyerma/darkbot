//! The Classic rules against the chain witness
//! (`fixtures/fourmeme/classic_parity_20261104.txt`): executed BUYs and
//! SELLs on mainnet state, the launch receipts, executed direct trades and
//! the venue's quote helper, each reproduced to the wei from the original
//! calldata.

use alloy_primitives::{Address, B256, U256};

use super::*;
use crate::venues::fourmeme::abi::{AbiError, encode_buy_token_amap, encode_sell_token_min_funds};
use crate::venues::fourmeme::deployment::{
    ValidatedFourDeployment, bsc_mainnet_g01_129842711, bsc_mainnet_r12b_131568366,
};
use crate::venues::fourmeme::profile::{
    CLASSIC_PROFILES, ClassicTokenRead, UnsupportedClassicHolding, UnsupportedClassicLaunch,
    VenueFee, WITNESSED_MAX_OFFERS, WITNESSED_TOTAL_SUPPLY, WITNESSED_VENUE_FEE,
    classify_classic_holding, classify_classic_launch,
};
use crate::venues::fourmeme::recognize::{
    ClassicLaunch, FourCall, decode_classic_buy_amap, decode_classic_create,
    decode_classic_sell_min_funds,
};

const FIXTURE: &str = include_str!("../../../fixtures/fourmeme/classic_parity_20261104.txt");

fn rows(kind: &'static str) -> impl Iterator<Item = Vec<&'static str>> {
    FIXTURE
        .lines()
        .map(|l| l.split(' ').collect::<Vec<_>>())
        .filter(move |f| f[0] == kind)
}

fn u(s: &str) -> U256 {
    U256::from_str_radix(s, 10).unwrap()
}

fn bytes(s: &str) -> Vec<u8> {
    alloy_primitives::hex::decode(s).unwrap()
}

fn fee(rate: &str, min: &str, tax_bps: u16) -> ClassicFee {
    ClassicFee {
        rate_bps: rate.parse().unwrap(),
        min_fee: u(min),
        tax_bps,
    }
}

/// The terms of a priced word under the witnessed venue fee.
fn priced(tax_bps: u16) -> ClassicFee {
    ClassicFee {
        rate_bps: WITNESSED_VENUE_FEE.rate_bps,
        min_fee: WITNESSED_VENUE_FEE.min_fee,
        tax_bps,
    }
}

/// A token's curve from `_tokenInfos` (every witnessed token offers
/// [`WITNESSED_MAX_OFFERS`]).
fn curve(k: &str, t: &str, offers: &str) -> ClassicCurve {
    let offers = u(offers);
    ClassicCurve {
        k: u(k),
        t: u(t),
        offers,
        sold: WITNESSED_MAX_OFFERS - offers,
    }
}

fn word(hex: &str) -> B256 {
    hex.parse().unwrap()
}

/// A launch row: tx, value, calldata, `K` and quote at the block, the
/// receipt's purchase.
struct Launch {
    block: &'static str,
    tx: B256,
    value: U256,
    calldata: Vec<u8>,
    k: U256,
    quote: Address,
    fee_setting: &'static str,
    purchase: Option<[U256; 5]>,
}

fn launches() -> Vec<(Address, Launch)> {
    rows("LAUNCH")
        .map(|f| {
            let purchase = (f[8] != "-").then(|| [8, 9, 10, 11, 12].map(|i| u(f[i])));
            (
                f[3].parse().unwrap(),
                Launch {
                    block: f[1],
                    tx: f[2].parse().unwrap(),
                    value: u(f[4]),
                    calldata: bytes(f[13]),
                    k: u(f[5]),
                    quote: f[6].parse().unwrap(),
                    fee_setting: f[7],
                    purchase,
                },
            )
        })
        .collect()
}

/// The launch block of a token: its LAUNCH or LAUNCHED row.
fn launch_block(token: &str) -> u64 {
    FIXTURE
        .lines()
        .map(|l| l.split(' ').collect::<Vec<_>>())
        .find_map(|f| match f[0] {
            "LAUNCH" if f[3].eq_ignore_ascii_case(token) => f[1].parse().ok(),
            "LAUNCHED" if f[1].eq_ignore_ascii_case(token) => f[2].parse().ok(),
            _ => None,
        })
        .unwrap()
}

/// The current deployment as its own pins validate it (generation 1).
fn validated() -> ValidatedFourDeployment {
    let d = bsc_mainnet_r12b_131568366();
    d.validate(
        &crate::venues::fourmeme::deployment::testkit::pinned_observation(&d),
        1,
    )
    .unwrap()
}

fn decoded(l: &Launch) -> ClassicLaunch<'_> {
    decode_classic_create(&l.calldata, bsc_mainnet_r12b_131568366().classic_args_len).unwrap()
}

fn start(l: &Launch) -> ClassicCurve {
    let a = decoded(l).launch_args().unwrap();
    launch_curve(&LaunchTerms {
        total_supply: a.total_supply,
        max_offers: a.max_offers,
        max_raising: a.max_raising,
    })
    .unwrap()
}

/// The executed BUY of the probe, its SELL right after and the `minAmount`
/// bound: a priced word's are the quotes with its tax, from its launch
/// block to the witnessed age; the G0.1 implementation's word with the
/// same parameters had the same tax.
#[test]
fn every_executed_buy_and_sell_is_the_quote_with_the_words_tax() {
    // The G0.1-era word with the priced word's parameter bytes (not priced:
    // no launch of it is in use).
    let g01_word = word("0x014ff1bd86c24140c6b1dc4783c2c9ca7cd8877e0000000000640a0a00002400");
    let (mut priced_rows, mut g01_rows, mut other, mut oldest) = (0, 0, 0, 0);
    for f in rows("PROBE") {
        // TokenManager2 accepts the BUY at its received tokens as minAmount
        // and reverts one wei above.
        assert_eq!((f[14], f[15]), ("OK", "REVERT"), "{f:?}");
        let template = word(&format!("0x{}", f[3]));
        let tax_bps = match CLASSIC_PROFILES.iter().find(|p| p.template == template) {
            Some(p) => {
                priced_rows += 1;
                let age = f[1].parse::<u64>().unwrap() - launch_block(f[2]);
                assert!(age <= p.tax_witnessed_blocks, "{f:?}");
                oldest = oldest.max(age);
                p.tax_bps
            }
            None if template == g01_word => {
                g01_rows += 1;
                CLASSIC_PROFILES[0].tax_bps
            }
            None => {
                other += 1;
                continue;
            }
        };
        assert_eq!(f[4], "0", "{f:?}");
        let c = curve(f[5], f[6], f[7]);
        let terms = fee(f[8], f[9], tax_bps);
        let b = quote_buy(&c, &terms, u(f[10])).unwrap();
        assert_eq!((b.amount, b.spent), (u(f[11]), u(f[12])), "{f:?}");
        let s = quote_sell(&b.after, &terms, b.amount).unwrap();
        assert_eq!(s.funds, u(f[13]), "{f:?}");
        // Without the tax the quote is the helper's, not the execution.
        let helper = quote_buy(&c, &fee(f[8], f[9], 0), u(f[10])).unwrap();
        assert_ne!(helper.amount, b.amount);
    }
    // SC4: 9 rows of the Strategy C input (0.01 BNB) more; SC4 c (B2): 16
    // on moved curves.
    assert_eq!((priced_rows, g01_rows, other), (38 + 16, 2, 3));
    assert_eq!(oldest, CLASSIC_PROFILES[0].tax_witnessed_blocks);
}

/// The venue's quote helper is the curve without the token's tax.
#[test]
fn every_helper_buy_quote_is_the_untaxed_curve() {
    let mut n = 0;
    let mut capped = 0;
    for f in rows("BUY") {
        let c = curve(f[3], f[4], f[5]);
        let got = quote_buy(&c, &fee(f[6], f[7], 0), u(f[8]));
        // A native-quote BUY pays `funds` as msg.value, nothing by
        // approval: buyTokenAMAP's value is the quoted funds.
        assert_eq!(
            (u(f[12]), u(f[13]), u(f[14])),
            (u(f[8]), U256::ZERO, u(f[8]))
        );
        if u(f[9]) == c.offers {
            // The helper caps the BUY at the remaining offers (graduation).
            assert_eq!(got, Err(ClassicQuoteError::ReachesMaxOffers), "{f:?}");
            capped += 1;
        } else {
            let b = got.unwrap();
            assert_eq!(
                (b.amount, b.cost, b.fee),
                (u(f[9]), u(f[10]), u(f[11])),
                "{f:?}"
            );
            assert_eq!((b.tax, b.spent), (U256::ZERO, b.cost + b.fee));
            assert_eq!(b.after.t, c.t - b.amount);
            assert_eq!(b.after.offers + b.amount, c.offers);
        }
        n += 1;
    }
    assert_eq!((n, capped), (39, 1));
}

#[test]
fn every_helper_sell_quote_is_the_untaxed_curve() {
    let mut n = 0;
    for f in rows("SELL") {
        let c = curve(f[3], f[4], f[5]);
        let s = quote_sell(&c, &fee(f[6], f[7], 0), u(f[8])).unwrap();
        assert_eq!((s.funds, s.fee), (u(f[9]), u(f[10])), "{f:?}");
        assert_eq!(s.after.sold + u(f[8]), c.sold);
        n += 1;
    }
    assert_eq!(n, 23);
}

/// Every launch's `K` (all quotes and `maxRaising` values) and every
/// creator purchase of a priced word (a BUY of its `args` creator value
/// with the word's tax), from the launch calldata alone.
#[test]
fn every_launch_curve_and_priced_creator_purchase_is_reproduced() {
    let (mut curves, mut purchases) = (0, 0);
    for (token, l) in launches() {
        let a = decoded(&l).launch_args().unwrap();
        let s = start(&l);
        assert_eq!(s.k, l.k, "{token}");
        curves += 1;
        let Some(p) = CLASSIC_PROFILES.iter().find(|p| p.template == a.template) else {
            continue;
        };
        assert_eq!((l.quote, l.fee_setting), (Address::ZERO, "0"), "{token}");
        assert_eq!(a.creator_value, l.value, "{token}");
        assert!(p.creator_values.contains(&a.creator_value), "{token}");
        let [amount, cost, fee_wei, offers, funds] = l.purchase.unwrap();
        let b = quote_buy(&s, &priced(p.tax_bps), a.creator_value).unwrap();
        assert_eq!(
            (b.amount, b.cost, b.fee),
            (amount, cost, fee_wei),
            "{token}"
        );
        // The first purchase: the curve's funds after it are its cost.
        assert_eq!((b.after.offers, b.cost), (offers, funds), "{token}");
        purchases += 1;
    }
    // SC4 c (B2): seven launches of moved curves more.
    assert_eq!((curves, purchases), (40 + 7, 31 + 7));
}

/// A priced launch names its token (CREATE2 under its word's pair) and the
/// curve after it; every other word is refused.
#[test]
fn a_priced_launch_names_its_token_and_state_before_execution() {
    let (mut priced_launches, mut refused) = (0, 0);
    for (token, l) in launches() {
        let launch = decoded(&l);
        match classify_classic_launch(&validated(), &launch, l.value, l.tx) {
            Ok(c) => {
                assert_eq!((c.token(), c.candidate()), (token, l.tx));
                let [_, _, _, offers, _] = l.purchase.unwrap();
                assert_eq!(c.curve().offers, offers);
                assert_eq!(c.fee(), priced(CLASSIC_PROFILES[0].tax_bps));
                priced_launches += 1;
            }
            Err(e) => {
                assert_eq!(e, UnsupportedClassicLaunch::Template, "{token}");
                refused += 1;
            }
        }
    }
    assert_eq!((priced_launches, refused), (31 + 7, 9));
}

/// The executed direct BUYs and SELLs (tokens without a tax): the state
/// before each from the event's `offers` and the token's constant
/// `T − offers`, the call's own amounts; a BUY's calldata is the encoder's
/// layout, its `msg.value` the funds with the native quote (another quote
/// asset is paid by approval: the same curve).
#[test]
fn the_executed_direct_trades_are_reproduced_from_their_calls() {
    let (mut buys, mut sells) = (0, 0);
    for f in rows("EXEC") {
        assert_eq!(f[5], "1", "{f:?}");
        let token: Address = f[4].parse().unwrap();
        let [amount, cost, fee_wei, offers, k, t_end, offers_end] =
            [7, 8, 9, 10, 14, 15, 16].map(|i| u(f[i]));
        let native = f[19].parse::<Address>().unwrap() == Address::ZERO;
        let terms = fee(f[17], f[18], 0);
        let offset = t_end - offers_end;
        let before = |o: U256| ClassicCurve {
            k,
            t: o + offset,
            offers: o,
            sold: WITNESSED_MAX_OFFERS - o,
        };
        let input = bytes(f[13]);
        match f[6] {
            "PURCHASE" => {
                let call = decode_classic_buy_amap(&input).unwrap();
                assert_eq!(call.token, token);
                assert_eq!(u(f[12]), if native { call.funds } else { U256::ZERO });
                let b = quote_buy(&before(offers + amount), &terms, call.funds).unwrap();
                assert_eq!((b.amount, b.cost, b.fee), (amount, cost, fee_wei), "{f:?}");
                // Every witnessed call accepts any amount (minAmount 0), which
                // the encoder refuses: its bytes with the minimum word zeroed
                // are the call's.
                assert_eq!(call.min_amount, U256::ZERO);
                assert_eq!(
                    encode_buy_token_amap(token, call.funds, U256::ZERO),
                    Err(AbiError::ZeroMinAmount)
                );
                let mut encoded = encode_buy_token_amap(token, call.funds, U256::ONE).unwrap();
                encoded[4 + 2 * 32..].fill(0);
                assert_eq!(encoded.as_slice(), input.as_slice());
                buys += 1;
            }
            "SALE" => {
                let call = decode_classic_sell_min_funds(&input).unwrap();
                assert_eq!((call.token, call.amount), (token, amount), "{f:?}");
                let s = quote_sell(&before(offers - amount), &terms, amount).unwrap();
                assert_eq!((s.gross, s.fee), (cost, fee_wei), "{f:?}");
                // R12c-3: the SELL encoder's layout is the witnessed call's
                // (a zero minimum, which it refuses, with the word zeroed).
                let mut encoded =
                    encode_sell_token_min_funds(token, call.amount, call.min_funds.max(U256::ONE))
                        .unwrap();
                if call.min_funds.is_zero() {
                    encoded[4 + 2 * 32..].fill(0);
                }
                assert_eq!(encoded.as_slice(), input.as_slice(), "{f:?}");
                sells += 1;
            }
            other => panic!("{other}"),
        }
    }
    assert_eq!((buys, sells), (10, 5));
}

/// R12c-3: the encoder's SELL is the call the production decoder reads
/// back, with the selector of `sellToken(address,uint256,uint256)`; zero
/// amounts are refused.
#[test]
fn the_encoded_sell_decodes_to_its_arguments() {
    let token: Address = "0x1b1955899341dbd9569c8537f639d642c2a9ffff"
        .parse()
        .unwrap();
    let (amount, min) = (u("196160233546478000000000"), u("793783783783783"));
    let input = encode_sell_token_min_funds(token, amount, min).unwrap();
    assert_eq!(input[..4], FourCall::ClassicSellTokenMinFunds.selector());
    let call = decode_classic_sell_min_funds(&input).unwrap();
    assert_eq!(
        (call.token, call.amount, call.min_funds),
        (token, amount, min)
    );
    assert_eq!(
        encode_sell_token_min_funds(token, U256::ZERO, min),
        Err(AbiError::ZeroAmount)
    );
    assert_eq!(
        encode_sell_token_min_funds(token, amount, U256::ZERO),
        Err(AbiError::ZeroMinAmount)
    );
}

/// The encoder's BUY is the call the production decoder reads back, with
/// the selector of `buyTokenAMAP(address,uint256,uint256)`.
#[test]
fn the_encoded_buy_decodes_to_its_arguments() {
    let token: Address = "0x4b033949e81ed2cfc3e36080d407e457a8654444"
        .parse()
        .unwrap();
    let (funds, min) = (u("1000000000000000"), u("212000000000000000000000"));
    let input = encode_buy_token_amap(token, funds, min).unwrap();
    assert_eq!(input[..4], FourCall::ClassicBuyTokenAmap.selector());
    let call = decode_classic_buy_amap(&input).unwrap();
    assert_eq!(
        (call.token, call.funds, call.min_amount),
        (token, funds, min)
    );
}

#[test]
fn what_the_rules_do_not_cover_is_refused() {
    let start = launch_curve(&LaunchTerms {
        total_supply: u("1000000000000000000000000000"),
        max_offers: WITNESSED_MAX_OFFERS,
        max_raising: u("14300000000000000000"),
    })
    .unwrap();
    let ether = u("1000000000000000000");
    let taxed = priced(1_000);
    assert_eq!(
        quote_buy(&start, &taxed, U256::ZERO),
        Err(ClassicQuoteError::ZeroInput)
    );
    let min_fee = ClassicFee {
        min_fee: U256::from(1u8),
        ..taxed
    };
    assert_eq!(
        quote_buy(&start, &min_fee, ether),
        Err(ClassicQuoteError::UnwitnessedMinFee)
    );
    assert_eq!(
        quote_sell(&start, &min_fee, ether),
        Err(ClassicQuoteError::UnwitnessedMinFee)
    );
    // Fee and tax at the whole, or summing past u16.
    for tax_bps in [9_900, u16::MAX] {
        let whole = ClassicFee { tax_bps, ..taxed };
        assert_eq!(
            quote_buy(&start, &whole, ether),
            Err(ClassicQuoteError::InvalidState)
        );
    }
    // 100 BNB takes every offer of a 14.3 BNB curve.
    assert_eq!(
        quote_buy(&start, &taxed, ether * U256::from(100u8)),
        Err(ClassicQuoteError::ReachesMaxOffers)
    );
    // One wei buys no whole gwei-unit.
    assert_eq!(
        quote_buy(&start, &taxed, U256::from(1u8)),
        Err(ClassicQuoteError::ZeroOutput)
    );
    // Nothing sold, nothing to sell back.
    assert_eq!(
        quote_sell(&start, &taxed, ether),
        Err(ClassicQuoteError::ExceedsSold)
    );
    let bad = |s: &str, o: &str| {
        launch_curve(&LaunchTerms {
            total_supply: u(s),
            max_offers: u(o),
            max_raising: ether,
        })
    };
    assert_eq!(bad("1", "2"), Err(ClassicQuoteError::InvalidState));
    assert_eq!(bad("2", "2"), Err(ClassicQuoteError::InvalidState));
    // 98·O <= 100·(S−O): no curve ends below the reserve price.
    assert_eq!(bad("198", "100"), Err(ClassicQuoteError::InvalidState));
    assert_eq!(
        encode_buy_token_amap(Address::ZERO, U256::ZERO, U256::from(1u8)),
        Err(AbiError::ZeroFunds)
    );
    assert_eq!(
        encode_buy_token_amap(Address::ZERO, U256::from(1u8), U256::ZERO),
        Err(AbiError::ZeroMinAmount)
    );
}

/// A BUY that takes exactly the remaining offers graduates the token: it
/// is refused like one that would take more; one gwei-unit more on offer
/// and it is quoted.
#[test]
fn a_buy_of_exactly_the_remaining_offers_is_refused() {
    let launched = launch_curve(&LaunchTerms {
        total_supply: u("1000000000000000000000000000"),
        max_offers: WITNESSED_MAX_OFFERS,
        max_raising: u("14300000000000000000"),
    })
    .unwrap();
    let (taxed, funds) = (priced(1_000), u("1000000000000000000"));
    let amount = quote_buy(&launched, &taxed, funds).unwrap().amount;
    let gwei = U256::from(1_000_000_000u64);
    let offering = |offers: U256| ClassicCurve {
        offers,
        sold: WITNESSED_MAX_OFFERS - offers,
        ..launched
    };
    assert_eq!(
        quote_buy(&offering(amount), &taxed, funds),
        Err(ClassicQuoteError::ReachesMaxOffers)
    );
    assert_eq!(
        quote_buy(&offering(amount + gwei), &taxed, funds)
            .unwrap()
            .amount,
        amount
    );
}

/// Each bound field of a priced launch, changed in its real calldata,
/// refuses it with that field.
#[test]
fn a_priced_launch_with_one_field_changed_is_refused() {
    let d = bsc_mainnet_r12b_131568366();
    let (_, l) = launches()
        .into_iter()
        .find(|(_, l)| l.purchase.is_some() && l.quote == Address::ZERO)
        .unwrap();
    // `args` tuple word k sits at calldata word 4 + k.
    let changed = |word: usize, value: U256| {
        let mut input = l.calldata.clone();
        let at = 4 + 32 * (4 + word);
        input[at..at + 32].copy_from_slice(&value.to_be_bytes::<32>());
        input
    };
    // The fee terms come with the validated observation.
    let classify = |input: &[u8], value: U256, venue: &VenueFee| {
        let launch = decode_classic_create(input, d.classic_args_len).unwrap();
        let mut observed = crate::venues::fourmeme::deployment::testkit::pinned_observation(&d);
        observed.venue_fee = *venue;
        let v = d.validate(&observed, 1).unwrap();
        classify_classic_launch(&v, &launch, value, l.tx).map(|c| c.token())
    };
    assert!(classify(&l.calldata, l.value, &WITNESSED_VENUE_FEE).is_ok());
    // The G0.1 deployment decodes the launch but has no executed economics.
    let g01 = bsc_mainnet_g01_129842711();
    let decode_only = g01
        .validate(
            &crate::venues::fourmeme::deployment::testkit::pinned_observation(&g01),
            1,
        )
        .unwrap();
    let launch = decode_classic_create(&l.calldata, g01.classic_args_len).unwrap();
    assert_eq!(
        classify_classic_launch(&decode_only, &launch, l.value, l.tx),
        Err(UnsupportedClassicLaunch::EconomicsNotWitnessed)
    );
    let one = U256::from(1u8);
    for word in [6, 7, 11, 12, 13, 15] {
        assert_eq!(
            classify(&changed(word, one), l.value, &WITNESSED_VENUE_FEE),
            Err(UnsupportedClassicLaunch::UnknownArgument),
            "{word}"
        );
    }
    let cases = [
        (4, UnsupportedClassicLaunch::TotalSupply),
        (5, UnsupportedClassicLaunch::MaxOffers),
        (8, UnsupportedClassicLaunch::Template),
    ];
    for (word, refusal) in cases {
        assert_eq!(
            classify(&changed(word, one), l.value, &WITNESSED_VENUE_FEE),
            Err(refusal)
        );
    }
    assert_eq!(
        classify(&l.calldata, l.value + one, &WITNESSED_VENUE_FEE),
        Err(UnsupportedClassicLaunch::CreatorValue)
    );
    for other in [
        VenueFee {
            rate_bps: 101,
            ..WITNESSED_VENUE_FEE
        },
        VenueFee {
            min_fee: one,
            ..WITNESSED_VENUE_FEE
        },
    ] {
        assert_eq!(
            classify(&l.calldata, l.value, &other),
            Err(UnsupportedClassicLaunch::FeeTerms)
        );
    }
    // Another creator value, even sent as msg.value, was not witnessed.
    let other_value = l.value + one;
    assert_eq!(
        classify(&changed(10, other_value), other_value, &WITNESSED_VENUE_FEE),
        Err(UnsupportedClassicLaunch::UnwitnessedCreatorValue)
    );
    // The name and symbol where the witnessed layout does not have them.
    for word in [2, 3] {
        assert_eq!(
            classify(
                &changed(word, U256::from(19 * 32)),
                l.value,
                &WITNESSED_VENUE_FEE
            ),
            Err(UnsupportedClassicLaunch::Layout),
            "{word}"
        );
    }
}

/// Every witnessed block names its TokenManager2 implementation; the priced
/// word ran under the current one only, the G0.1-era rows under the frozen
/// one.
#[test]
fn every_witnessed_block_names_its_implementation() {
    let address = |a: Address| a.to_string().to_lowercase();
    let (g01, current) = (
        address(bsc_mainnet_g01_129842711().token_manager2_implementation),
        address(bsc_mainnet_r12b_131568366().token_manager2_implementation),
    );
    let implementation = |block: &str| {
        rows("IMPL")
            .find(|f| f[1] == block)
            .map(|f| f[2])
            .unwrap_or_else(|| panic!("no IMPL row for block {block}"))
    };
    let mut n = 0;
    for kind in ["LAUNCH", "PROBE", "SELLMIN", "BUY", "SELL", "EXEC"] {
        for f in rows(kind) {
            let at = implementation(f[1]);
            assert!(at == g01 || at == current, "{f:?}");
            let template = match kind {
                "LAUNCH" => Some(decoded_template(&f)),
                "PROBE" => Some(word(&format!("0x{}", f[3]))),
                _ => None,
            };
            if template.is_some_and(|t| CLASSIC_PROFILES.iter().any(|p| p.template == t)) {
                assert_eq!(at, current, "{f:?}");
            }
            n += 1;
        }
    }
    // SC4: the 9 PROBE rows of 0.01 BNB.
    // SC4 c (B2): 7 launches, 16 probes and their 16 SELL minimums more.
    assert_eq!(n, 40 + 34 + 29 + 39 + 23 + 15 + 9 + 7 + 16 + 16);
}

fn decoded_template(f: &[&str]) -> B256 {
    let calldata = bytes(f[13]);
    let launch =
        decode_classic_create(&calldata, bsc_mainnet_r12b_131568366().classic_args_len).unwrap();
    launch.launch_args().unwrap().template
}

fn snapshot(candidate: B256) -> crate::chain_state::SnapshotId {
    crate::chain_state::SnapshotId {
        canonical_block_hash: B256::repeat_byte(9),
        ordered_state_cursor: 1,
        pending_candidate: candidate,
        policy_generation: 5,
        codehash_generation: 1,
    }
}

fn policy(funds: U256, slippage_bps: u16) -> crate::economics::policy::BuyPolicy {
    crate::economics::policy::BuyPolicy::new(5, funds, slippage_bps, 100_000_000, 400_000).unwrap()
}

/// R12c: from the launch calldata alone, the admitted BUY is the one the
/// probe executed on the state directly after the launch (its tokens and
/// wei to the unit); its minimum is below them, so it executes.
#[test]
fn the_admitted_launch_buy_is_the_executed_one() {
    let mut n = 0;
    for f in rows("PROBE") {
        let token: Address = f[2].parse().unwrap();
        let Some((_, l)) = launches()
            .into_iter()
            .find(|(t, l)| *t == token && l.block == f[1])
        else {
            continue;
        };
        let classified =
            classify_classic_launch(&validated(), &decoded(&l), l.value, l.tx).unwrap();
        let state = QualifiedClassicState::fresh_launch(&classified, 0, snapshot(l.tx)).unwrap();
        let admitted = admit_classic_buy(&state, &policy(u(f[10]), 500)).unwrap();
        assert_eq!((admitted.token(), admitted.funds()), (token, u(f[10])));
        assert_eq!(
            (admitted.expected_tokens(), admitted.spent()),
            (u(f[11]), u(f[12])),
            "{f:?}"
        );
        // The minimum is the slippage floor of the quote, below its tokens.
        assert_eq!(
            admitted.min_tokens(),
            crate::economics::slippage::min_output(u(f[11]), 500).unwrap()
        );
        assert!(admitted.min_tokens() > U256::ZERO && admitted.min_tokens() < u(f[11]));
        assert_eq!(
            admitted.calldata().unwrap(),
            encode_buy_token_amap(token, u(f[10]), admitted.min_tokens()).unwrap()
        );
        n += 1;
    }
    // SC4: 3 launch-block states of 0.01 BNB more.
    assert_eq!(n, 9);
}

#[test]
fn an_admission_outside_the_qualified_state_or_amount_is_refused() {
    let (_, l) = launches()
        .into_iter()
        .find(|(_, l)| l.purchase.is_some() && l.quote == Address::ZERO)
        .unwrap();
    let classified = classify_classic_launch(&validated(), &decoded(&l), l.value, l.tx).unwrap();
    let fresh = |trades: u64, s: crate::chain_state::SnapshotId| {
        QualifiedClassicState::fresh_launch(&classified, trades, s)
    };
    let mut other = snapshot(l.tx);
    other.codehash_generation = 2;
    assert_eq!(
        fresh(0, other),
        Err(ClassicAdmissionError::GenerationMismatch)
    );
    assert_eq!(
        fresh(0, snapshot(B256::repeat_byte(1))),
        Err(ClassicAdmissionError::CandidateMismatch)
    );
    assert_eq!(
        fresh(1, snapshot(l.tx)),
        Err(ClassicAdmissionError::StateNotFresh)
    );
    let state = fresh(0, snapshot(l.tx)).unwrap();
    let milli = U256::from(QUALIFIED_CLASSIC_INPUTS_WEI[0]);
    let stale = crate::economics::policy::BuyPolicy::new(6, milli, 500, 1, 1).unwrap();
    assert_eq!(
        admit_classic_buy(&state, &stale),
        Err(ClassicAdmissionError::PolicyGenerationMismatch)
    );
    // 1 BNB was probed on later states only, never on a fresh launch.
    for funds in [
        milli + U256::from(1u8),
        U256::from(10u8).pow(U256::from(18u8)),
    ] {
        assert_eq!(
            admit_classic_buy(&state, &policy(funds, 500)),
            Err(ClassicAdmissionError::UnqualifiedAmount)
        );
    }
    assert!(admit_classic_buy(&state, &policy(milli, 500)).is_ok());
}

/// R12c-3: the state a held token is read in for its SELL (`_tokenInfos`,
/// `_tokenInfoEx1s`, `Helper3.getTokenInfo` at one block).
fn holding_read(f: &[&str], after: &ClassicCurve) -> ClassicTokenRead {
    ClassicTokenRead {
        token: f[2].parse().unwrap(),
        block_number: f[1].parse().unwrap(),
        block_hash: B256::repeat_byte(0xb1),
        quote: Address::ZERO,
        template: word(&format!("0x{}", f[3])),
        total_supply: WITNESSED_TOTAL_SUPPLY,
        max_offers: WITNESSED_MAX_OFFERS,
        offers: after.offers,
        k: after.k,
        t: after.t,
        status: U256::ZERO,
        fee_setting: u(f[4]),
        extra_fee: U256::ZERO,
        launch_block: launch_block(f[2]),
        venue_fee: VenueFee {
            rate_bps: f[8].parse().unwrap(),
            min_fee: u(f[9]),
        },
        liquidity_added: false,
    }
}

/// The blocks a SELL may land after its read (the shipped
/// `BSC_EXEC_VALIDITY_BLOCKS`).
const VALID: u64 = 2;

fn holding_snapshot(block_hash: B256, codehash_generation: u64) -> SnapshotId {
    SnapshotId {
        canonical_block_hash: block_hash,
        ordered_state_cursor: 3,
        pending_candidate: B256::ZERO,
        policy_generation: 4,
        codehash_generation,
    }
}

fn exit_policy(generation: u64, slippage_bps: u16) -> ExitPolicy {
    ExitPolicy::new(generation, slippage_bps, 50_000_000, 100_000, 1_500_000).unwrap()
}

/// R12c-3: every executed probe SELL of a priced word (the tokens its BUY
/// received, on the state right after that BUY) is the admitted SELL of
/// the held state read there: its expected funds are the wei the
/// execution returned, its minimum the slippage floor of its gross (what
/// TokenManager2 compares `minFunds` with, see the SELLMIN rows), its
/// calldata `sellToken(token, amount, minFunds)`.
#[test]
fn every_executed_probe_sell_is_the_admitted_sell_of_the_held_state() {
    let mut n = 0;
    for f in rows("PROBE") {
        let template = word(&format!("0x{}", f[3]));
        let Some(p) = CLASSIC_PROFILES.iter().find(|p| p.template == template) else {
            continue;
        };
        let b = quote_buy(
            &curve(f[5], f[6], f[7]),
            &fee(f[8], f[9], p.tax_bps),
            u(f[10]),
        )
        .unwrap();
        let read = holding_read(&f, &b.after);
        // The probe sold on the read state itself: it lands 0 blocks later.
        let classified = classify_classic_holding(&validated(), &read, 0).unwrap();
        let state =
            QualifiedClassicHolding::at(&classified, holding_snapshot(read.block_hash, 1)).unwrap();
        let sell = admit_classic_sell(&state, b.amount, &exit_policy(4, 100)).unwrap();
        assert_eq!(sell.expected_funds(), u(f[13]), "{f:?}");
        assert!(sell.expected_gross() > sell.expected_funds());
        assert_eq!(
            sell.min_funds(),
            sell.expected_gross() * U256::from(9_900u64) / U256::from(10_000u64),
            "{f:?}"
        );
        assert_eq!((sell.token(), sell.amount()), (read.token, b.amount));
        let call = decode_classic_sell_min_funds(&sell.calldata().unwrap()).unwrap();
        assert_eq!(
            (call.token, call.amount, call.min_funds),
            (read.token, b.amount, sell.min_funds())
        );
        n += 1;
    }
    // SC4 c (B2): 16 held states on moved curves, beyond the R12 tax age too.
    assert_eq!(n, 38 + 16);
}

/// R12c-3: a held token is priced only as the launches are and only while
/// it trades on its curve: each deviating field is refused by name.
#[test]
fn a_held_token_is_priced_only_as_witnessed() {
    let f = rows("PROBE")
        .find(|f| CLASSIC_PROFILES[0].template == word(&format!("0x{}", f[3])))
        .unwrap();
    let ok = holding_read(&f, &curve(f[5], f[6], f[7]));
    classify_classic_holding(&validated(), &ok, VALID).unwrap();
    // The last read whose SELL lands within the witnessed tax age.
    let max_age = ok.launch_block + CLASSIC_PROFILES[0].tax_witnessed_blocks - VALID;
    type Change = fn(&mut ClassicTokenRead, u64);
    let cases: [(Change, UnsupportedClassicHolding); 13] = [
        (
            |r, _| r.quote = Address::repeat_byte(1),
            UnsupportedClassicHolding::Quote,
        ),
        (
            |r, _| r.template = B256::repeat_byte(2),
            UnsupportedClassicHolding::Template,
        ),
        (
            |r, _| r.total_supply += U256::ONE,
            UnsupportedClassicHolding::TotalSupply,
        ),
        (
            |r, _| r.max_offers += U256::ONE,
            UnsupportedClassicHolding::MaxOffers,
        ),
        (
            |r, _| r.fee_setting = U256::ONE,
            UnsupportedClassicHolding::FeeSetting,
        ),
        (
            |r, _| r.extra_fee = U256::ONE,
            UnsupportedClassicHolding::ExtraFee,
        ),
        (
            |r, _| r.status = U256::ONE,
            UnsupportedClassicHolding::OffCurve,
        ),
        (
            |r, _| r.liquidity_added = true,
            UnsupportedClassicHolding::OffCurve,
        ),
        (
            |r, max| r.block_number = max + 1,
            UnsupportedClassicHolding::TaxUnwitnessed,
        ),
        (
            |r, _| r.block_number = r.launch_block - 1,
            UnsupportedClassicHolding::TaxUnwitnessed,
        ),
        (
            |r, _| r.venue_fee.rate_bps += 1,
            UnsupportedClassicHolding::FeeTerms,
        ),
        (
            |r, _| r.venue_fee.min_fee = U256::ONE,
            UnsupportedClassicHolding::FeeTerms,
        ),
        (
            |r, _| r.offers = WITNESSED_MAX_OFFERS + U256::ONE,
            UnsupportedClassicHolding::Curve,
        ),
    ];
    for (change, want) in cases {
        let mut read = ok;
        change(&mut read, max_age);
        assert_eq!(
            classify_classic_holding(&validated(), &read, VALID),
            Err(want)
        );
    }
    // At the oldest witnessed age it is still priced.
    let mut oldest = ok;
    oldest.block_number = max_age;
    classify_classic_holding(&validated(), &oldest, VALID).unwrap();
    // A read at that age whose SELL may land a block later is refused.
    assert_eq!(
        classify_classic_holding(&validated(), &oldest, VALID + 1),
        Err(UnsupportedClassicHolding::TaxUnwitnessed)
    );
    // Not on a deployment validated with other fee terms, nor on a
    // decode-only one.
    let d = bsc_mainnet_r12b_131568366();
    let mut drifted = crate::venues::fourmeme::deployment::testkit::pinned_observation(&d);
    drifted.venue_fee.rate_bps += 1;
    assert_eq!(
        classify_classic_holding(&d.validate(&drifted, 1).unwrap(), &ok, VALID),
        Err(UnsupportedClassicHolding::FeeTerms)
    );
    let g01 = bsc_mainnet_g01_129842711();
    let decode_only = g01
        .validate(
            &crate::venues::fourmeme::deployment::testkit::pinned_observation(&g01),
            1,
        )
        .unwrap();
    assert_eq!(
        classify_classic_holding(&decode_only, &ok, VALID),
        Err(UnsupportedClassicHolding::EconomicsNotWitnessed)
    );
}

/// R12c-3: the admitted SELL belongs to the block its state was read at,
/// the code generation it was classified under and the exit policy's
/// generation; it sells no more than the curve sold and something.
#[test]
fn a_held_state_binds_its_block_generation_and_policy() {
    let f = rows("PROBE")
        .find(|f| CLASSIC_PROFILES[0].template == word(&format!("0x{}", f[3])))
        .unwrap();
    let read = holding_read(&f, &curve(f[5], f[6], f[7]));
    let classified = classify_classic_holding(&validated(), &read, VALID).unwrap();
    assert_eq!(
        QualifiedClassicHolding::at(&classified, holding_snapshot(B256::repeat_byte(0xb2), 1)),
        Err(ClassicAdmissionError::StaleRead)
    );
    assert_eq!(
        QualifiedClassicHolding::at(&classified, holding_snapshot(read.block_hash, 2)),
        Err(ClassicAdmissionError::GenerationMismatch)
    );
    let state =
        QualifiedClassicHolding::at(&classified, holding_snapshot(read.block_hash, 1)).unwrap();
    let sold = classified.curve().sold;
    assert_eq!(
        admit_classic_sell(&state, sold, &exit_policy(5, 100)),
        Err(ClassicAdmissionError::PolicyGenerationMismatch)
    );
    assert_eq!(
        admit_classic_sell(&state, sold + U256::from(GWEI), &exit_policy(4, 100)),
        Err(ClassicAdmissionError::Quote(ClassicQuoteError::ExceedsSold))
    );
    assert_eq!(
        admit_classic_sell(&state, U256::ZERO, &exit_policy(4, 100)),
        Err(ClassicAdmissionError::Quote(ClassicQuoteError::ZeroInput))
    );
    let all = admit_classic_sell(&state, sold, &exit_policy(4, 0)).unwrap();
    assert_eq!(all.min_funds(), all.expected_gross());
    // Whole gwei units only, as every witnessed trade.
    assert_eq!(
        admit_classic_sell(&state, sold - U256::ONE, &exit_policy(4, 100)),
        Err(ClassicAdmissionError::UnalignedAmount)
    );
}

/// R12c-3 review M1: the production SELL `sellToken(token, amount,
/// minFunds)` executed after each priced probe's BUY (the SELLMIN rows):
/// TokenManager2 accepts `minFunds` = the SELL's gross and reverts one wei
/// above it, so it compares the minimum with the curve's wei before the
/// fee and the tax; the admission's expected gross is that gross to the
/// wei, its minimum the slippage floor of it.
#[test]
fn the_sells_minimum_is_bound_to_its_gross_as_executed() {
    let mut n = 0;
    for s in rows("SELLMIN") {
        assert_eq!((s[7], s[8]), ("OK", "REVERT"), "{s:?}");
        let f = rows("PROBE")
            .find(|f| (f[1], f[2], f[10]) == (s[1], s[2], s[3]))
            .unwrap();
        let p = CLASSIC_PROFILES
            .iter()
            .find(|p| p.template == word(&format!("0x{}", f[3])))
            .unwrap();
        let b = quote_buy(
            &curve(f[5], f[6], f[7]),
            &fee(f[8], f[9], p.tax_bps),
            u(f[10]),
        )
        .unwrap();
        assert_eq!(b.amount, u(s[4]), "{s:?}");
        let read = holding_read(&f, &b.after);
        let classified = classify_classic_holding(&validated(), &read, 0).unwrap();
        let state =
            QualifiedClassicHolding::at(&classified, holding_snapshot(read.block_hash, 1)).unwrap();
        let exact = admit_classic_sell(&state, b.amount, &exit_policy(4, 0)).unwrap();
        assert_eq!(
            (
                exact.expected_gross(),
                exact.min_funds(),
                exact.expected_funds()
            ),
            (u(s[6]), u(s[6]), u(s[5])),
            "{s:?}"
        );
        n += 1;
    }
    assert_eq!(n, 29 + 16);
}

/// SC4: Strategy C's BUY on a tracked state. On every state a probe
/// executed the Strategy C input (0.01 BNB) on, the admission of the curve
/// the sold tokens name (the offers read there) yields exactly the tokens
/// and wei the probe's BUY took; another code generation, other venue fee
/// terms or an unqualified input are refused.
#[test]
fn a_tracked_state_admits_the_executed_strategy_buy() {
    let v = validated();
    let input = U256::from(10_000_000_000_000_000u64);
    let mut n = 0;
    for f in rows("PROBE").filter(|f| f[10] == "10000000000000000") {
        let offers = u(f[7]);
        let curve = ClassicCurve {
            k: u(f[5]),
            t: u(f[6]),
            offers,
            sold: WITNESSED_MAX_OFFERS - offers,
        };
        let fee = ClassicFee {
            rate_bps: f[8].parse().unwrap(),
            min_fee: u(f[9]),
            tax_bps: 1_000,
        };
        let token: Address = f[2].parse().unwrap();
        let state =
            QualifiedClassicState::tracked(&v, token, curve, fee, snapshot(B256::ZERO)).unwrap();
        let a = admit_classic_buy(&state, &policy(input, 100)).unwrap();
        assert_eq!(
            (a.expected_tokens(), a.spent(), a.token()),
            (u(f[11]), u(f[12]), token),
            "{f:?}"
        );
        assert!(a.min_tokens() < a.expected_tokens());
        n += 1;
    }
    // Six fresh profile states and three launch-block states (R12), sixteen
    // states third-party trades moved (B2).
    assert_eq!(n, 9 + 16);
    // B2: the bound is the deepest executed state; one gwei of tokens deeper
    // is refused.
    let deepest = rows("PROBE")
        .filter(|f| CLASSIC_PROFILES[0].template == word(&format!("0x{}", f[3])))
        .map(|f| u(f[7]))
        .min()
        .unwrap();
    assert_eq!(deepest, QUALIFIED_CLASSIC_MIN_OFFERS);
    let f = rows("PROBE")
        .find(|f| u(f[7]) == deepest && f[10] == "10000000000000000")
        .unwrap();
    let deeper = |offers: U256| ClassicCurve {
        k: u(f[5]),
        t: u(f[6]) - (deepest - offers),
        offers,
        sold: WITNESSED_MAX_OFFERS - offers,
    };
    let fee = ClassicFee {
        rate_bps: 100,
        min_fee: u(f[9]),
        tax_bps: 1_000,
    };
    assert!(
        QualifiedClassicState::tracked(
            &v,
            Address::ZERO,
            deeper(deepest),
            fee,
            snapshot(B256::ZERO)
        )
        .is_ok()
    );
    let gwei = U256::from(1_000_000_000u64);
    assert_eq!(
        QualifiedClassicState::tracked(
            &v,
            Address::ZERO,
            deeper(deepest - gwei),
            fee,
            snapshot(B256::ZERO)
        ),
        Err(ClassicAdmissionError::Unwitnessed)
    );
    // An exit is not bounded by it (its minFunds refuses an unnamed SELL).
    assert!(
        QualifiedClassicHolding::tracked(
            &v,
            Address::ZERO,
            deeper(deepest - gwei),
            fee,
            snapshot(B256::ZERO)
        )
        .is_ok()
    );
    let f = rows("PROBE")
        .find(|f| f[10] == "10000000000000000")
        .unwrap();
    let curve = ClassicCurve {
        k: u(f[5]),
        t: u(f[6]),
        offers: u(f[7]),
        sold: WITNESSED_MAX_OFFERS - u(f[7]),
    };
    let fee = ClassicFee {
        rate_bps: 100,
        min_fee: u(f[9]),
        tax_bps: 1_000,
    };
    let other_generation = crate::chain_state::SnapshotId {
        codehash_generation: 2,
        ..snapshot(B256::ZERO)
    };
    assert_eq!(
        QualifiedClassicState::tracked(&v, Address::ZERO, curve, fee, other_generation),
        Err(ClassicAdmissionError::GenerationMismatch)
    );
    let other_fee = ClassicFee {
        rate_bps: 101,
        ..fee
    };
    assert_eq!(
        QualifiedClassicState::tracked(&v, Address::ZERO, curve, other_fee, snapshot(B256::ZERO)),
        Err(ClassicAdmissionError::TermsMismatch)
    );
    assert_eq!(
        QualifiedClassicHolding::tracked(&v, Address::ZERO, curve, other_fee, snapshot(B256::ZERO)),
        Err(ClassicAdmissionError::TermsMismatch)
    );
    let state = QualifiedClassicState::tracked(&v, Address::ZERO, curve, fee, snapshot(B256::ZERO))
        .unwrap();
    assert_eq!(
        admit_classic_buy(&state, &policy(U256::from(20_000_000_000_000_000u64), 100)),
        Err(ClassicAdmissionError::UnqualifiedAmount)
    );
    // Review M1: a tracked state admits only the inputs executed on moved
    // curves (0.01 BNB); 0.001 and 0.1 BNB were executed on launch states.
    let moved: std::collections::BTreeSet<Address> =
        rows("MOVE").map(|f| f[2].parse().unwrap()).collect();
    let tracked_inputs: std::collections::BTreeSet<u64> = rows("PROBE")
        .filter(|f| moved.contains(&f[2].parse::<Address>().unwrap()))
        .map(|f| f[10].parse().unwrap())
        .collect();
    assert_eq!(
        tracked_inputs,
        QUALIFIED_CLASSIC_TRACKED_INPUTS_WEI.into_iter().collect()
    );
    for input in [1_000_000_000_000_000u64, 100_000_000_000_000_000] {
        assert!(QUALIFIED_CLASSIC_INPUTS_WEI.contains(&input));
        assert_eq!(
            admit_classic_buy(&state, &policy(U256::from(input), 100)),
            Err(ClassicAdmissionError::UnqualifiedAmount)
        );
    }
    // Review L3: a tax other than a priced word's is refused.
    let untaxed = ClassicFee { tax_bps: 0, ..fee };
    assert_eq!(
        QualifiedClassicState::tracked(&v, Address::ZERO, curve, untaxed, snapshot(B256::ZERO)),
        Err(ClassicAdmissionError::TermsMismatch)
    );
    assert_eq!(
        QualifiedClassicHolding::tracked(&v, Address::ZERO, curve, untaxed, snapshot(B256::ZERO)),
        Err(ClassicAdmissionError::TermsMismatch)
    );
}

/// SC4 c (B2): every state third-party trades moved a priced-word curve to
/// (MOVE rows: _tokenInfos at the block's end) is the token's launch curve
/// (its LAUNCH row, classified) after the reported sale: the same `T`, and
/// the raised funds checked; on it the executed Strategy C input (PROBE
/// rows at or after the move, before the next) is the quote to the unit,
/// its SELL the quote's funds, inside and beyond the R12 tax age.
#[test]
fn every_moved_state_is_the_launch_curve_after_its_sale() {
    use crate::strategy::book::{FourCurve, FourPost};
    let v = validated();
    let launches = launches();
    let mut moves: Vec<(Address, u64, ClassicCurve)> = Vec::new();
    let (mut buys, mut sells) = (0, 0);
    for f in rows("MOVE") {
        let token: Address = f[2].parse().unwrap();
        let (_, l) = launches.iter().find(|(t, _)| *t == token).unwrap();
        let c = classify_classic_launch(&v, &decoded(l), l.value, l.tx).unwrap();
        let launch = FourCurve::classic_at(c.curve(), c.fee()).unwrap();
        let post = FourPost {
            remaining: u(f[5]),
            raised: u(f[6]),
        };
        let Some(FourCurve::Classic { now, .. }) = launch.after(post) else {
            panic!("{f:?}")
        };
        assert_eq!((now.t, now.offers), (u(f[7]), u(f[5])), "{f:?}");
        match f[3] {
            "BUY" => buys += 1,
            _ => sells += 1,
        }
        moves.push((token, f[1].parse().unwrap(), now));
    }
    assert_eq!((buys, sells), (10, 1));
    let mut probed = 0;
    for f in rows("PROBE") {
        let token: Address = f[2].parse().unwrap();
        let block: u64 = f[1].parse().unwrap();
        let Some((_, _, now)) = moves
            .iter()
            .filter(|(t, b, _)| *t == token && *b <= block)
            .max_by_key(|(_, b, _)| *b)
        else {
            continue;
        };
        assert_eq!(
            (now.k, now.t, now.offers),
            (u(f[5]), u(f[6]), u(f[7])),
            "{f:?}"
        );
        let terms = fee(f[8], f[9], CLASSIC_PROFILES[0].tax_bps);
        let b = quote_buy(now, &terms, u(f[10])).unwrap();
        assert_eq!((b.amount, b.spent), (u(f[11]), u(f[12])), "{f:?}");
        let sold = quote_sell(&b.after, &terms, b.amount).unwrap();
        assert_eq!(sold.funds, u(f[13]), "{f:?}");
        probed += 1;
    }
    assert_eq!(probed, 16);
}

/// SC4 d: a BUY's spend is its curve cost (a TokenPurchase's `cost`) with
/// the trading fee and the token's tax: the wei each executed probe of the
/// priced word lost from its cost; each launch receipt's trading fee from
/// its cost (the tax is its template's; a launch row names none).
#[test]
fn a_purchases_spend_is_its_cost_with_fee_and_tax() {
    let tax_bps = CLASSIC_PROFILES[0].tax_bps;
    let mut n = 0;
    for f in rows("PROBE") {
        let template = word(&format!("0x{}", f[3]));
        if !CLASSIC_PROFILES.iter().any(|p| p.template == template) {
            continue;
        }
        let terms = fee(f[8], f[9], tax_bps);
        let b = quote_buy(&curve(f[5], f[6], f[7]), &terms, u(f[10])).unwrap();
        assert_eq!(purchase_spent(&terms, b.cost).unwrap(), u(f[12]), "{f:?}");
        n += 1;
    }
    assert!(n > 0);
    let mut launches = 0;
    for f in rows("LAUNCH").filter(|f| f[8] != "-") {
        let (fee_wei, _, _) = split_purchase(&fee("100", "0", tax_bps), u(f[9])).unwrap();
        assert_eq!(fee_wei, u(f[10]), "{f:?}");
        launches += 1;
    }
    assert!(launches > 0);
}

/// SC4 d: a LIVE SELL's funds from its TokenSale's gross (`cost`): every
/// executed SELL of the probes (SELLMIN rows: the gross and the funds the
/// seller received) is `sale_funds` of its gross under the priced terms.
#[test]
fn a_sales_funds_are_its_gross_less_fee_and_tax() {
    let mut n = 0;
    for f in rows("SELLMIN") {
        let probe = rows("PROBE")
            .find(|p| p[1] == f[1] && p[2] == f[2] && p[10] == f[3])
            .unwrap();
        let terms = fee(probe[8], probe[9], CLASSIC_PROFILES[0].tax_bps);
        assert_eq!(sale_funds(&terms, u(f[6])).unwrap(), u(f[5]), "{f:?}");
        n += 1;
    }
    assert_eq!(n, 29 + 16);
}
