//! R12d: the OpenFour rules against the chain witness
//! (`fixtures/fourmeme/openfour_parity_20261104.txt`): the launch and its
//! creator BUY, executed exact BUYs and SELLs and the admission's BUYs, each
//! reproduced to the wei; `buyByBudget` documented, not priced.

use super::*;

const FIXTURE: &str = include_str!("../../../fixtures/fourmeme/openfour_parity_20261104.txt");

/// The fee terms the witnessed preset charged (protocol and developer
/// 0.5 % each; no tax, no anti-sniper fee).
const FEE: OpenFourFee = OpenFourFee {
    protocol_bps: 50,
    dev_bps: 50,
};

fn rows(kind: &'static str) -> impl Iterator<Item = Vec<&'static str>> {
    FIXTURE
        .lines()
        .map(|l| l.split(' ').collect::<Vec<_>>())
        .filter(move |f| f[0] == kind)
}

fn u(s: &str) -> U256 {
    U256::from_str_radix(s, 10).unwrap()
}

/// The curve of an OFSNAP row (`getCurveLiquiditySnapshot`).
fn snapshot_curve(block: &str) -> OpenFourCurve {
    let f = rows("OFSNAP").find(|f| f[1] == block).unwrap();
    OpenFourCurve {
        k: u(f[3]) * u(f[4]),
        token_reserve: u(f[6]),
        quote_reserve: u(f[5]),
        remaining: u(f[7]),
    }
}

/// The launch's curve from its terms is the snapshot's initial liquidity;
/// its creator BUY (the launch's value) is the receipt's purchase and
/// leaves the snapshot's current state; the last price is the reserves'.
#[test]
fn the_launch_and_its_creator_buy_are_reproduced() {
    let f = rows("OFLAUNCH").next().unwrap();
    let terms = LaunchTerms {
        total_supply: u(f[8]),
        max_offers: u(f[9]),
        max_raising: u(f[10]),
    };
    let start = launch_curve(&terms).unwrap();
    let s = rows("OFSNAP").find(|s| s[1] == f[1]).unwrap();
    assert_eq!(
        (start.quote_reserve, start.token_reserve, start.remaining),
        (u(s[3]), u(s[4]), terms.max_offers)
    );
    assert_eq!(start.k, u(s[3]) * u(s[4]));
    let b = creator_buy(&start, &FEE, u(f[7])).unwrap();
    // The tokens it bought are the tokens the creator requested.
    assert_eq!((u(f[19]), b.amount), (u(f[20]), u(f[20])));
    assert_eq!(
        (b.cost, b.pay, b.protocol_fee, b.dev_fee),
        (u(f[21]), u(f[22]), u(f[25]), u(f[27]))
    );
    assert_eq!((f[26], f[28]), ("0", "0"), "no tax, no anti-sniper fee");
    assert_eq!(b.after, snapshot_curve(f[1]));
    assert_eq!((b.after.remaining, b.cost), (u(f[30]), u(f[29])));
    let last =
        b.after.quote_reserve * U256::from(1_000_000_000_000_000_000u64) / b.after.token_reserve;
    assert_eq!(last, u(f[23]));
    // The value is the slippage limit; the rest of it refunded.
    assert!(b.pay <= u(f[24]));
}

/// Every executed exact BUY: `Core.buy` took exactly the quote's payment
/// for exactly the amount (and refused one wei less).
#[test]
fn every_executed_exact_buy_is_the_quote() {
    let mut n = 0;
    for f in rows("OFEXACT") {
        let q = quote_buy_amount(&snapshot_curve(f[1]), &FEE, u(f[3])).unwrap();
        assert_eq!((q.cost, q.pay), (u(f[4]), u(f[5])), "{f:?}");
        assert_eq!((f[6], f[7]), ("OK", "REVERT"), "{f:?}");
        n += 1;
    }
    assert_eq!(n, 8);
}

/// Every executed SELL after an exact BUY: `Core.sell` paid exactly the
/// quote's proceeds (and refused a minimum one wei above).
#[test]
fn every_executed_sell_is_the_quote() {
    let mut n = 0;
    for f in rows("OFSELL") {
        let b = quote_buy_amount(&snapshot_curve(f[1]), &FEE, u(f[3])).unwrap();
        assert_eq!(b.pay, u(f[4]), "{f:?}");
        let s = quote_sell(&b.after, &FEE, u(f[3])).unwrap();
        assert_eq!((s.gross, s.received), (u(f[5]), u(f[6])), "{f:?}");
        assert_eq!((f[7], f[8]), ("OK", "REVERT"), "{f:?}");
        // Selling what was bought restores the reserves up to rounding.
        assert_eq!(s.after.remaining, snapshot_curve(f[1]).remaining);
        n += 1;
    }
    assert_eq!(n, 4);
}

/// The admission's BUY: the largest amount whose payment fits the limit,
/// executed with the budget as `maxPay` (the rest refunded).
#[test]
fn the_largest_amount_within_a_limit_is_the_executed_one() {
    let mut n = 0;
    for f in rows("OFADMIT") {
        let curve = snapshot_curve(f[1]);
        let budget = u(f[3]);
        let slippage: u16 = f[4].parse().unwrap();
        let limit = budget * U256::from(10_000u64) / U256::from(10_000u64 + u64::from(slippage));
        assert_eq!(limit, u(f[5]), "{f:?}");
        let q = max_buy_amount(&curve, &FEE, limit).unwrap();
        assert_eq!((q.amount, q.pay), (u(f[6]), u(f[7])), "{f:?}");
        assert_eq!(f[8], "OK", "{f:?}");
        let more = quote_buy_amount(&curve, &FEE, q.amount + U256::ONE).unwrap();
        assert!(more.pay > limit, "{f:?}");
        n += 1;
    }
    assert_eq!(n, 8);
}

/// `Core.buyByBudget` is documented, not priced: it bought fewer tokens
/// than the exact inverse (within 2e-7 relative in every probe) and paid
/// their exact price.
#[test]
fn buy_by_budget_buys_less_than_the_exact_inverse() {
    let mut n = 0;
    for f in rows("OFBUDGET") {
        let curve = snapshot_curve(f[1]);
        let (budget, tokens, paid) = (u(f[3]), u(f[4]), u(f[5]));
        assert_eq!(
            quote_buy_amount(&curve, &FEE, tokens).unwrap().pay,
            paid,
            "{f:?}"
        );
        let exact = max_buy_amount(&curve, &FEE, budget).unwrap().amount;
        assert!(tokens < exact, "{f:?}");
        assert!((exact - tokens) * U256::from(5_000_000u64) < exact, "{f:?}");
        n += 1;
    }
    assert_eq!(n, 6);
}

/// Inputs outside the rule are refused, never quoted.
#[test]
fn inputs_outside_the_rule_are_refused() {
    let curve = snapshot_curve("131608421");
    assert_eq!(
        quote_buy_amount(&curve, &FEE, U256::ZERO),
        Err(OpenFourQuoteError::ZeroInput)
    );
    assert_eq!(
        quote_buy_amount(&curve, &FEE, curve.remaining + U256::ONE),
        Err(OpenFourQuoteError::ExceedsRemaining)
    );
    assert_eq!(
        quote_sell(&curve, &FEE, U256::ZERO),
        Err(OpenFourQuoteError::ZeroInput)
    );
    assert_eq!(
        max_buy_amount(&curve, &FEE, U256::ZERO),
        Err(OpenFourQuoteError::ZeroInput)
    );
    let whole = OpenFourFee {
        protocol_bps: 5_000,
        dev_bps: 5_000,
    };
    assert_eq!(
        quote_buy_amount(&curve, &whole, U256::from(1_000u64)),
        Err(OpenFourQuoteError::InvalidState)
    );
    let empty = OpenFourCurve {
        remaining: U256::ZERO,
        ..curve
    };
    assert_eq!(
        quote_buy_amount(&empty, &FEE, U256::ONE),
        Err(OpenFourQuoteError::InvalidState)
    );
}

pub(crate) mod admission {
    //! R12d: the witnessed launch classified from its own calldata, its
    //! token derived before execution, and the admitted BUYs and SELLs
    //! against the executed ones.
    use super::*;
    use crate::venues::fourmeme::deployment::{
        OPENFOUR_MODULES, ValidatedFourDeployment, bsc_mainnet_r12b_131568366,
        bsc_mainnet_r12d_131608421, testkit::pinned_observation,
    };
    use crate::venues::fourmeme::profile::{
        OpenFourTokenRead, UnsupportedOpenFourHolding, UnsupportedOpenFourLaunch,
        classify_openfour_holding, classify_openfour_launch,
    };
    use crate::venues::fourmeme::recognize::{
        OpenFourLaunch, PendingToken, Recognized, decode_openfour_buy, decode_openfour_create,
        decode_openfour_sell, pending_token, recognize,
    };
    use alloy_primitives::{Address, B256};

    fn validated(generation: u64) -> ValidatedFourDeployment {
        let d = bsc_mainnet_r12d_131608421();
        d.validate(&pinned_observation(&d), generation).unwrap()
    }

    fn launch_row() -> Vec<&'static str> {
        rows("OFLAUNCH").next().unwrap()
    }

    fn calldata() -> Vec<u8> {
        alloy_primitives::hex::decode(launch_row()[31]).unwrap()
    }

    fn snapshot(candidate: B256, generation: u64) -> SnapshotId {
        SnapshotId {
            canonical_block_hash: B256::repeat_byte(0xa1),
            ordered_state_cursor: 5,
            pending_candidate: candidate,
            policy_generation: 3,
            codehash_generation: generation,
        }
    }

    fn policy(input: u64, slippage_bps: u16) -> BuyPolicy {
        BuyPolicy::new(3, U256::from(input), slippage_bps, 50_000_000, 1_000_000).unwrap()
    }

    /// The pending launch names its token before execution (the frozen
    /// preset's CREATE2 derivation) and classifies to the state after it:
    /// the snapshot of its block.
    #[test]
    fn the_witnessed_launch_is_classified_from_its_calldata() {
        let f = launch_row();
        let input = calldata();
        let d = bsc_mainnet_r12d_131608421();
        let r = recognize(&d, Some(d.openfour_core), &input, input.len());
        let Recognized::OpenFourLaunch(launch) = r else {
            panic!("{r:?}")
        };
        let token: Address = f[4].parse().unwrap();
        let creator: Address = f[6].parse().unwrap();
        assert_eq!(
            pending_token(&d, &r, Some(creator)),
            PendingToken::Derived(token)
        );
        // Its salt is zero: without the creator no token is named.
        assert_eq!(pending_token(&d, &r, None), PendingToken::Unknown);
        // Not under the earlier freeze: its preset was not frozen.
        let r12b = bsc_mainnet_r12b_131568366();
        assert_eq!(
            pending_token(&r12b, &r, Some(creator)),
            PendingToken::Unknown
        );
        let tx: B256 = f[2].parse().unwrap();
        let c = classify_openfour_launch(&validated(1), &launch, creator, u(f[7]), tx).unwrap();
        assert_eq!(
            (c.token(), c.candidate(), c.codehash_generation()),
            (token, tx, 1)
        );
        assert_eq!(c.curve(), snapshot_curve(f[1]));
        assert_eq!(c.fee(), FEE);
    }

    /// Every deviation from the witness is refused by name.
    #[test]
    fn a_launch_outside_the_witness_is_refused() {
        let f = launch_row();
        let input = calldata();
        let base = decode_openfour_create(&input).unwrap();
        let value = u(f[7]);
        let creator: Address = f[6].parse().unwrap();
        let classify = |l: &OpenFourLaunch<'_>, v: U256| {
            classify_openfour_launch(&validated(1), l, creator, v, B256::ZERO)
        };
        classify(&base, value).unwrap();
        let other = |change: &dyn Fn(&mut OpenFourLaunch<'_>)| {
            let mut l = base;
            change(&mut l);
            classify(&l, value)
        };
        assert_eq!(
            other(&|l| l.preset_id = U256::from(1_778_027_615_735u64)),
            Err(UnsupportedOpenFourLaunch::Preset)
        );
        assert_eq!(
            other(&|l| l.init.quote_asset = Address::repeat_byte(1)),
            Err(UnsupportedOpenFourLaunch::Quote)
        );
        assert_eq!(
            other(&|l| l.init.raise_amount += U256::ONE),
            Err(UnsupportedOpenFourLaunch::Terms)
        );
        assert_eq!(
            other(&|l| l.init.trade_params = &[1]),
            Err(UnsupportedOpenFourLaunch::ModuleParams)
        );
        assert_eq!(
            other(&|l| l.unnamed[3] = B256::repeat_byte(1)),
            Err(UnsupportedOpenFourLaunch::UnknownArgument)
        );
        assert_eq!(
            classify(&base, value + U256::ONE),
            Err(UnsupportedOpenFourLaunch::CreatorValue)
        );
        let mut half = base;
        half.unnamed[2] = B256::from(value / U256::from(2u8));
        assert_eq!(
            classify(&half, value / U256::from(2u8)),
            Err(UnsupportedOpenFourLaunch::UnwitnessedCreatorValue)
        );
        // A deployment that pins no executed bonding preset prices nothing.
        let r12b = bsc_mainnet_r12b_131568366();
        let decode_only = r12b.validate(&pinned_observation(&r12b), 1).unwrap();
        assert_eq!(
            classify_openfour_launch(&decode_only, &base, creator, value, B256::ZERO),
            Err(UnsupportedOpenFourLaunch::EconomicsNotWitnessed)
        );
    }

    /// The admitted BUY on the fresh state is the executed one (OFADMIT at
    /// the launch block): the amount, its pay, the budget as maxPay.
    #[test]
    fn the_admitted_buy_is_the_executed_one() {
        let f = launch_row();
        let input = calldata();
        let launch = decode_openfour_create(&input).unwrap();
        let tx: B256 = f[2].parse().unwrap();
        let creator: Address = f[6].parse().unwrap();
        let c = classify_openfour_launch(&validated(1), &launch, creator, u(f[7]), tx).unwrap();
        let state = QualifiedOpenFourState::fresh_launch(&c, 0, snapshot(tx, 1)).unwrap();
        let mut n = 0;
        for a in rows("OFADMIT").filter(|a| a[1] == f[1]) {
            let (budget, slippage) = (u(a[3]).to::<u64>(), a[4].parse::<u16>().unwrap());
            let b = admit_openfour_buy(&state, &policy(budget, slippage)).unwrap();
            assert_eq!(
                (b.amount(), b.pay(), b.max_pay(), b.token()),
                (u(a[6]), u(a[7]), u(a[3]), c.token()),
                "{a:?}"
            );
            assert_eq!(a[8], "OK");
            let bytes = b.calldata().unwrap();
            let call = decode_openfour_buy(&bytes).unwrap();
            assert_eq!(
                (call.token, call.amount, call.max_pay_amount, call.options),
                (c.token(), b.amount(), b.max_pay(), U256::ZERO)
            );
            assert!(call.proof.is_empty());
            n += 1;
        }
        assert_eq!(n, 4);
        assert_eq!(
            admit_openfour_buy(&state, &policy(2_000_000_000_000_000, 100)),
            Err(OpenFourAdmissionError::UnqualifiedAmount)
        );
        assert_eq!(
            admit_openfour_buy(
                &state,
                &BuyPolicy::new(4, U256::from(QUALIFIED_OPENFOUR_INPUTS_WEI[0]), 100, 1, 1)
                    .unwrap()
            ),
            Err(OpenFourAdmissionError::PolicyGenerationMismatch)
        );
        assert_eq!(
            QualifiedOpenFourState::fresh_launch(&c, 1, snapshot(tx, 1)),
            Err(OpenFourAdmissionError::StateNotFresh)
        );
        assert_eq!(
            QualifiedOpenFourState::fresh_launch(&c, 0, snapshot(B256::ZERO, 1)),
            Err(OpenFourAdmissionError::CandidateMismatch)
        );
        assert_eq!(
            QualifiedOpenFourState::fresh_launch(&c, 0, snapshot(tx, 2)),
            Err(OpenFourAdmissionError::GenerationMismatch)
        );
    }

    /// The held state of the launch block's end, as the probe reads it.
    pub(crate) fn holding_read(block: &str) -> OpenFourTokenRead {
        let f = launch_row();
        let s = rows("OFSNAP").find(|s| s[1] == block).unwrap();
        let bonding = bsc_mainnet_r12d_131608421().openfour_bonding.unwrap();
        OpenFourTokenRead {
            token: f[4].parse().unwrap(),
            block_number: block.parse().unwrap(),
            block_hash: B256::repeat_byte(0xa1),
            preset_id: u(f[5]),
            max_supply: u(f[8]),
            sale_amount: u(f[9]),
            raise_amount: u(f[10]),
            quote_asset: f[12].parse().unwrap(),
            exists: true,
            paused: false,
            anti_sniper_enabled: false,
            custom_data: Address::ZERO,
            phase: 1,
            module_beacons: bonding.modules.map(|m| m.beacon),
            initial_quote: u(s[3]),
            initial_token: u(s[4]),
            current_quote: u(s[5]),
            current_token: u(s[6]),
            remaining: u(s[7]),
        }
    }

    /// The admitted SELL of a held token: its expected proceeds the executed
    /// ones (OFSELL), its minimum the slippage floor of the NET; every field
    /// outside the witness refused.
    #[test]
    fn the_admitted_sell_is_the_executed_one_and_the_holding_is_strict() {
        let exit = |slippage| ExitPolicy::new(3, slippage, 50_000_000, 100_000, 1_500_000).unwrap();
        let mut n = 0;
        for f in rows("OFSELL") {
            let mut read = holding_read(f[1]);
            let b = quote_buy_amount(&snapshot_curve(f[1]), &FEE, u(f[3])).unwrap();
            read.current_quote = b.after.quote_reserve;
            read.current_token = b.after.token_reserve;
            read.remaining = b.after.remaining;
            let held = classify_openfour_holding(&validated(1), &read).unwrap();
            let state = QualifiedOpenFourHolding::at(
                &held,
                SnapshotId {
                    canonical_block_hash: read.block_hash,
                    ..snapshot(B256::ZERO, 1)
                },
            )
            .unwrap();
            let exact = admit_openfour_sell(&state, u(f[3]), &exit(0)).unwrap();
            assert_eq!(
                (exact.expected_received(), exact.min_receive()),
                (u(f[6]), u(f[6])),
                "{f:?}"
            );
            let floor = admit_openfour_sell(&state, u(f[3]), &exit(100)).unwrap();
            assert_eq!(
                floor.min_receive(),
                u(f[6]) * U256::from(9_900u64) / U256::from(10_000u64)
            );
            let bytes = floor.calldata().unwrap();
            let call = decode_openfour_sell(&bytes).unwrap();
            assert_eq!(
                (call.token, call.amount, call.min_receive_amount),
                (read.token, u(f[3]), floor.min_receive())
            );
            n += 1;
        }
        assert_eq!(n, 4);
        let ok = holding_read("131608421");
        type Change = fn(&mut OpenFourTokenRead);
        let cases: [(Change, UnsupportedOpenFourHolding); 10] = [
            (
                |r| r.preset_id += U256::ONE,
                UnsupportedOpenFourHolding::Preset,
            ),
            (
                |r| r.quote_asset = Address::ZERO,
                UnsupportedOpenFourHolding::Quote,
            ),
            (
                |r| r.sale_amount += U256::ONE,
                UnsupportedOpenFourHolding::Terms,
            ),
            (|r| r.exists = false, UnsupportedOpenFourHolding::Mode),
            (|r| r.paused = true, UnsupportedOpenFourHolding::Mode),
            (
                |r| r.anti_sniper_enabled = true,
                UnsupportedOpenFourHolding::Mode,
            ),
            (
                |r| r.custom_data = Address::repeat_byte(2),
                UnsupportedOpenFourHolding::Mode,
            ),
            (|r| r.phase = 2, UnsupportedOpenFourHolding::OffCurve),
            (
                |r| r.module_beacons[2] = Address::repeat_byte(3),
                UnsupportedOpenFourHolding::Modules,
            ),
            (
                |r| r.initial_quote += U256::ONE,
                UnsupportedOpenFourHolding::Curve,
            ),
        ];
        for (change, want) in cases {
            let mut read = ok;
            change(&mut read);
            assert_eq!(classify_openfour_holding(&validated(1), &read), Err(want));
        }
        assert_eq!(OPENFOUR_MODULES.len(), ok.module_beacons.len());
        let held = classify_openfour_holding(&validated(1), &ok).unwrap();
        assert_eq!(
            QualifiedOpenFourHolding::at(
                &held,
                SnapshotId {
                    canonical_block_hash: B256::repeat_byte(0xa2),
                    ..snapshot(B256::ZERO, 1)
                }
            ),
            Err(OpenFourAdmissionError::StaleRead)
        );
        assert_eq!(
            QualifiedOpenFourHolding::at(
                &held,
                SnapshotId {
                    canonical_block_hash: ok.block_hash,
                    ..snapshot(B256::ZERO, 2)
                }
            ),
            Err(OpenFourAdmissionError::GenerationMismatch)
        );
    }
}
