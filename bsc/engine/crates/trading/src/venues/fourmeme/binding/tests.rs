//! The Classic BUY's expectation, signature and final binding, from an
//! admission on a real launch of the R12 fixture.

use super::*;
use crate::economics::four_classic::{QualifiedClassicState, admit_classic_buy};
use crate::venues::flap::binding::ExpectedTx;
use crate::venues::fourmeme::abi::AbiError;
use crate::venues::fourmeme::deployment::bsc_mainnet_r12b_131568366;
use crate::venues::fourmeme::profile::classify_classic_launch;
use crate::venues::fourmeme::recognize::decode_classic_create;

const FIXTURE: &str = include_str!("../../../../fixtures/fourmeme/classic_parity_20261104.txt");
const FUNDS: u64 = 1_000_000_000_000_000;

fn validated(generation: u64) -> ValidatedFourDeployment {
    let d = bsc_mainnet_r12b_131568366();
    d.validate(
        &crate::venues::fourmeme::deployment::testkit::pinned_observation(&d),
        generation,
    )
    .unwrap()
}

fn policy(generation: u64, funds: u64, slippage_bps: u16) -> BuyPolicy {
    BuyPolicy::new(
        generation,
        U256::from(funds),
        slippage_bps,
        100_000_000,
        400_000,
    )
    .unwrap()
}

/// The admitted 0.001 BNB BUY on the first priced launch of the fixture.
fn admitted() -> AdmittedClassicBuy {
    let v = validated(1);
    for f in FIXTURE
        .lines()
        .filter(|l| l.starts_with("LAUNCH "))
        .map(|l| l.split(' ').collect::<Vec<_>>())
    {
        let calldata = alloy_primitives::hex::decode(f[13]).unwrap();
        let launch = decode_classic_create(&calldata, v.deployment().classic_args_len).unwrap();
        let tx: B256 = f[2].parse().unwrap();
        let value = U256::from_str_radix(f[4], 10).unwrap();
        let Ok(c) = classify_classic_launch(&v, &launch, value, tx) else {
            continue;
        };
        let snapshot = SnapshotId {
            canonical_block_hash: B256::repeat_byte(9),
            ordered_state_cursor: 1,
            pending_candidate: tx,
            policy_generation: 5,
            codehash_generation: 1,
        };
        let state = QualifiedClassicState::fresh_launch(&c, 0, snapshot).unwrap();
        return admit_classic_buy(&state, &policy(5, FUNDS, 500)).unwrap();
    }
    unreachable!("the fixture has priced launches")
}

fn expectation(signer: &Signer, nonce: u64) -> ExpectedClassicBuy {
    ExpectedClassicBuy::from_admitted(
        &validated(1),
        &admitted(),
        &policy(5, FUNDS, 500),
        signer.address(),
        nonce,
    )
    .unwrap()
}

#[test]
fn the_production_signer_signs_exactly_the_admitted_buy() {
    let signer = Signer::from_secret_bytes(&[0x61; 32]).unwrap();
    let expected = expectation(&signer, 3);
    let a = admitted();
    assert_eq!(
        (
            expected.token(),
            expected.funds(),
            expected.min_amount(),
            expected.token_manager2()
        ),
        (
            a.token(),
            a.funds(),
            a.min_tokens(),
            bsc_mainnet_r12b_131568366().token_manager2
        )
    );
    let (mut scratch, mut raw) = (Vec::new(), Vec::new());
    let hash = expected.sign(&signer, &mut scratch, &mut raw).unwrap();
    assert_eq!(
        verify_final_classic_buy(&raw, &expected, &expected.snapshot(), &mut scratch),
        Ok(hash)
    );
    let signed = decode_signed(&raw).unwrap();
    assert_eq!(
        (signed.tx.to, signed.tx.value),
        (expected.token_manager2(), a.funds())
    );
    assert_eq!(signed.tx.input, a.calldata().unwrap().as_slice());
    let other = Signer::from_secret_bytes(&[0x62; 32]).unwrap();
    assert_eq!(
        expected.sign(&other, &mut scratch, &mut raw),
        Err(BuildError::WrongSigner)
    );
    assert!(raw.is_empty());
}

#[test]
fn every_bound_field_is_enforced() {
    let signer = Signer::from_secret_bytes(&[0x63; 32]).unwrap();
    let expected = expectation(&signer, 4);
    let (mut scratch, mut raw) = (Vec::new(), Vec::new());
    expected.sign(&signer, &mut scratch, &mut raw).unwrap();
    let check = |e: &ExpectedClassicBuy, scratch: &mut Vec<u8>| {
        verify_final_classic_buy(&raw, e, &e.snapshot(), scratch)
    };
    let one = U256::from(1u8);
    let cases: [(ExpectedClassicBuy, BindingError); 8] = [
        (
            ExpectedClassicBuy {
                sender: Address::repeat_byte(1),
                ..expected
            },
            BindingError::WrongSender,
        ),
        (
            ExpectedClassicBuy {
                nonce: 5,
                ..expected
            },
            BindingError::WrongNonce,
        ),
        (
            ExpectedClassicBuy {
                token_manager2: Address::repeat_byte(2),
                ..expected
            },
            BindingError::WrongPortal,
        ),
        (
            ExpectedClassicBuy {
                gas_price: expected.gas_price + 1,
                ..expected
            },
            BindingError::WrongGasPrice,
        ),
        (
            ExpectedClassicBuy {
                gas_limit: expected.gas_limit + 1,
                ..expected
            },
            BindingError::WrongGasLimit,
        ),
        (
            ExpectedClassicBuy {
                funds: expected.funds + one,
                ..expected
            },
            BindingError::WrongValue,
        ),
        (
            ExpectedClassicBuy {
                token: Address::repeat_byte(3),
                ..expected
            },
            BindingError::WrongToken,
        ),
        (
            ExpectedClassicBuy {
                min_amount: expected.min_amount + one,
                ..expected
            },
            BindingError::WrongMinOutput,
        ),
    ];
    for (e, error) in cases {
        assert_eq!(check(&e, &mut scratch), Err(error));
    }
    // Another chain: the signature recovers for none of the expectation's.
    let other_chain = ExpectedClassicBuy {
        chain_id: 97,
        ..expected
    };
    assert!(matches!(
        check(&other_chain, &mut scratch),
        Err(BindingError::Signature(_))
    ));
    // A snapshot that moved since the expectation was formed.
    let mut current = expected.snapshot();
    current.ordered_state_cursor += 1;
    assert_eq!(
        verify_final_classic_buy(&raw, &expected, &current, &mut scratch),
        Err(BindingError::StaleSnapshot)
    );
    // Value and calldata funds must agree: a BUY of other funds at the value.
    let calldata =
        encode_buy_token_amap(expected.token, expected.funds + one, expected.min_amount).unwrap();
    let tx = LegacyTx {
        chain_id: expected.chain_id,
        nonce: expected.nonce,
        gas_price: expected.gas_price,
        gas_limit: expected.gas_limit,
        to: expected.token_manager2,
        value: expected.funds,
        input: &calldata,
    };
    let mut forged = Vec::new();
    signer.sign_legacy(&tx, &mut scratch, &mut forged).unwrap();
    assert_eq!(
        verify_final_classic_buy(&forged, &expected, &expected.snapshot(), &mut scratch),
        Err(BindingError::WrongInputAmount)
    );
    // Another call to TokenManager2 is no BUY.
    let tx = LegacyTx {
        input: &[0xf4, 0x64, 0xe7, 0xdb],
        ..tx
    };
    signer.sign_legacy(&tx, &mut scratch, &mut forged).unwrap();
    assert_eq!(
        verify_final_classic_buy(&forged, &expected, &expected.snapshot(), &mut scratch),
        Err(BindingError::FourCalldata(AbiError::WrongSelector))
    );
}

#[test]
fn the_expectation_needs_its_code_and_policy_generation() {
    let a = admitted();
    let sender = Address::repeat_byte(7);
    assert_eq!(
        ExpectedClassicBuy::from_admitted(&validated(2), &a, &policy(5, FUNDS, 500), sender, 1),
        Err(IntentError::GenerationMismatch)
    );
    assert_eq!(
        ExpectedClassicBuy::from_admitted(&validated(1), &a, &policy(6, FUNDS, 500), sender, 1),
        Err(IntentError::PolicyGenerationMismatch)
    );
    for p in [policy(5, FUNDS * 100, 500), policy(5, FUNDS, 400)] {
        assert_eq!(
            ExpectedClassicBuy::from_admitted(&validated(1), &a, &p, sender, 1),
            Err(IntentError::PolicyMismatch)
        );
    }
}

/// `ExpectedTx` carries the Classic BUY's identity, value, gas and final
/// check (the lane and the plan run it in R12c's runtime path).
#[test]
fn the_intent_kind_carries_the_classic_buy() {
    let signer = Signer::from_secret_bytes(&[0x64; 32]).unwrap();
    let expected = expectation(&signer, 9);
    let tx = ExpectedTx::from(&expected);
    assert_eq!(
        (
            tx.chain_id(),
            tx.sender(),
            tx.nonce(),
            tx.token(),
            tx.portal()
        ),
        (
            56,
            signer.address(),
            9,
            expected.token(),
            expected.token_manager2()
        )
    );
    assert_eq!(
        (tx.snapshot(), tx.value_wei()),
        (expected.snapshot(), expected.funds())
    );
    assert_eq!(
        tx.gas_wei(),
        U256::from(expected.gas_price()) * U256::from(expected.gas_limit())
    );
    let (mut scratch, mut raw) = (Vec::new(), Vec::new());
    let hash = expected.sign(&signer, &mut scratch, &mut raw).unwrap();
    assert_eq!(
        tx.verify_final(&raw, &expected.snapshot(), &mut scratch),
        Ok(hash)
    );
}

/// R12c-3: the admitted SELL of the first priced probe's state (its
/// launch block's curve, the tokens its creator bought).
fn admitted_sell(slippage_bps: u16) -> (AdmittedClassicSell, ExitPolicy) {
    use crate::economics::four_classic::{QualifiedClassicHolding, admit_classic_sell};
    use crate::venues::fourmeme::profile::{
        CLASSIC_PROFILES, ClassicTokenRead, VenueFee, WITNESSED_MAX_OFFERS, WITNESSED_TOTAL_SUPPLY,
        classify_classic_holding,
    };
    let u = |s: &str| U256::from_str_radix(s, 10).unwrap();
    let f: Vec<&str> = FIXTURE
        .lines()
        .map(|l| l.split(' ').collect::<Vec<_>>())
        .find(|f| {
            f[0] == "PROBE"
                && f[3].parse::<B256>().ok() == Some(CLASSIC_PROFILES[0].template)
                && f[4] == "0"
        })
        .unwrap();
    let block: u64 = f[1].parse().unwrap();
    let read = ClassicTokenRead {
        token: f[2].parse().unwrap(),
        block_number: block,
        block_hash: B256::repeat_byte(0xc1),
        quote: Address::ZERO,
        template: CLASSIC_PROFILES[0].template,
        total_supply: WITNESSED_TOTAL_SUPPLY,
        max_offers: WITNESSED_MAX_OFFERS,
        offers: u(f[7]),
        k: u(f[5]),
        t: u(f[6]),
        status: U256::ZERO,
        fee_setting: U256::ZERO,
        extra_fee: U256::ZERO,
        launch_block: block,
        venue_fee: VenueFee {
            rate_bps: f[8].parse().unwrap(),
            min_fee: u(f[9]),
        },
        liquidity_added: false,
    };
    let classified = classify_classic_holding(&validated(1), &read, 2).unwrap();
    let snapshot = SnapshotId {
        canonical_block_hash: read.block_hash,
        ordered_state_cursor: 2,
        pending_candidate: B256::ZERO,
        policy_generation: 5,
        codehash_generation: 1,
    };
    let state = QualifiedClassicHolding::at(&classified, snapshot).unwrap();
    let policy = ExitPolicy::new(5, slippage_bps, 100_000_000, 120_000, 400_000).unwrap();
    let sold = classified.curve().sold;
    (admit_classic_sell(&state, sold, &policy).unwrap(), policy)
}

fn exit_expectation(signer: &Signer, nonce: u64) -> ExpectedClassicExit {
    let (a, policy) = admitted_sell(300);
    ExpectedClassicExit::from_admitted(&validated(1), &a, &policy, signer.address(), nonce).unwrap()
}

/// R12c-3: the production signer signs exactly the exit's approval (of
/// TokenManager2, on the token, for the tokens sold, at `nonce`) and its
/// SELL (`sellToken(token, amount, minFunds)` to TokenManager2 at
/// `nonce + 1`), no value; each passes its final check and fails the
/// other's.
#[test]
fn the_production_signer_signs_exactly_the_admitted_exit() {
    let signer = Signer::from_secret_bytes(&[0x65; 32]).unwrap();
    let (a, _) = admitted_sell(300);
    let e = exit_expectation(&signer, 6);
    assert_eq!(
        (e.token(), e.amount(), e.min_funds(), e.nonce()),
        (a.token(), a.amount(), a.min_funds(), 6)
    );
    let (mut scratch, mut approve, mut sell) = (Vec::new(), Vec::new(), Vec::new());
    let ha = e.sign_approve(&signer, &mut scratch, &mut approve).unwrap();
    let hs = e.sign_sell(&signer, &mut scratch, &mut sell).unwrap();
    let now = e.snapshot();
    assert_eq!(
        verify_final_classic_approve(&approve, &e, &now, &mut scratch),
        Ok(ha)
    );
    assert_eq!(
        verify_final_classic_sell(&sell, &e, &now, &mut scratch),
        Ok(hs)
    );
    let a_tx = decode_signed(&approve).unwrap();
    assert_eq!(
        (a_tx.tx.to, a_tx.tx.nonce, a_tx.tx.value, a_tx.tx.gas_limit),
        (a.token(), 6, U256::ZERO, 120_000)
    );
    assert_eq!(
        crate::venues::flap::abi::decode_approve(a_tx.tx.input).unwrap(),
        (bsc_mainnet_r12b_131568366().token_manager2, a.amount())
    );
    let s_tx = decode_signed(&sell).unwrap();
    assert_eq!(
        (s_tx.tx.to, s_tx.tx.nonce, s_tx.tx.value, s_tx.tx.gas_limit),
        (e.token_manager2(), 7, U256::ZERO, 400_000)
    );
    assert_eq!(s_tx.tx.input, a.calldata().unwrap().as_slice());
    // Each transaction is refused as the other.
    assert_eq!(
        verify_final_classic_sell(&approve, &e, &now, &mut scratch),
        Err(BindingError::WrongNonce)
    );
    assert_eq!(
        verify_final_classic_approve(&sell, &e, &now, &mut scratch),
        Err(BindingError::WrongNonce)
    );
    let other = Signer::from_secret_bytes(&[0x66; 32]).unwrap();
    assert_eq!(
        e.sign_sell(&other, &mut scratch, &mut sell),
        Err(BuildError::WrongSigner)
    );
    assert!(sell.is_empty());
    assert_eq!(
        e.sign_approve(&other, &mut scratch, &mut approve),
        Err(BuildError::WrongSigner)
    );
}

/// R12c-3: every bound field of the exit is enforced at the send boundary.
#[test]
fn every_bound_exit_field_is_enforced() {
    let signer = Signer::from_secret_bytes(&[0x67; 32]).unwrap();
    let e = exit_expectation(&signer, 2);
    let (mut scratch, mut approve, mut sell) = (Vec::new(), Vec::new(), Vec::new());
    e.sign_approve(&signer, &mut scratch, &mut approve).unwrap();
    e.sign_sell(&signer, &mut scratch, &mut sell).unwrap();
    let one = U256::from(1u8);
    let both: [(ExpectedClassicExit, BindingError); 5] = [
        (
            ExpectedClassicExit {
                sender: Address::repeat_byte(1),
                ..e
            },
            BindingError::WrongSender,
        ),
        (
            ExpectedClassicExit { nonce: 3, ..e },
            BindingError::WrongNonce,
        ),
        (
            ExpectedClassicExit {
                gas_price: e.gas_price + 1,
                ..e
            },
            BindingError::WrongGasPrice,
        ),
        (
            ExpectedClassicExit {
                amount: e.amount + one,
                ..e
            },
            BindingError::WrongInputAmount,
        ),
        (
            ExpectedClassicExit {
                token: Address::repeat_byte(3),
                ..e
            },
            BindingError::WrongToken,
        ),
    ];
    for (x, error) in both {
        let now = x.snapshot();
        assert_eq!(
            verify_final_classic_approve(&approve, &x, &now, &mut scratch),
            Err(error)
        );
        assert_eq!(
            verify_final_classic_sell(&sell, &x, &now, &mut scratch),
            Err(error)
        );
    }
    let tm2 = ExpectedClassicExit {
        token_manager2: Address::repeat_byte(4),
        ..e
    };
    let now = e.snapshot();
    assert_eq!(
        verify_final_classic_approve(&approve, &tm2, &now, &mut scratch),
        Err(BindingError::WrongSpender)
    );
    assert_eq!(
        verify_final_classic_sell(&sell, &tm2, &now, &mut scratch),
        Err(BindingError::WrongPortal)
    );
    let gas = ExpectedClassicExit {
        approve_gas_limit: e.approve_gas_limit + 1,
        sell_gas_limit: e.sell_gas_limit + 1,
        ..e
    };
    for raw in [&approve, &sell] {
        let check = if raw == &approve {
            verify_final_classic_approve(raw, &gas, &now, &mut scratch)
        } else {
            verify_final_classic_sell(raw, &gas, &now, &mut scratch)
        };
        assert_eq!(check, Err(BindingError::WrongGasLimit));
    }
    let min = ExpectedClassicExit {
        min_funds: e.min_funds + one,
        ..e
    };
    assert_eq!(
        verify_final_classic_sell(&sell, &min, &now, &mut scratch),
        Err(BindingError::WrongMinOutput)
    );
    let mut moved = now;
    moved.ordered_state_cursor += 1;
    assert_eq!(
        verify_final_classic_sell(&sell, &e, &moved, &mut scratch),
        Err(BindingError::StaleSnapshot)
    );
    assert_eq!(
        verify_final_classic_approve(&approve, &e, &moved, &mut scratch),
        Err(BindingError::StaleSnapshot)
    );
    // A SELL that carries value is no exit's.
    let input =
        crate::venues::fourmeme::abi::encode_sell_token_min_funds(e.token, e.amount, e.min_funds)
            .unwrap();
    let mut forged = Vec::new();
    signer
        .sign_legacy(
            &LegacyTx {
                chain_id: e.chain_id,
                nonce: e.nonce + 1,
                gas_price: e.gas_price,
                gas_limit: e.sell_gas_limit,
                to: e.token_manager2,
                value: one,
                input: &input,
            },
            &mut scratch,
            &mut forged,
        )
        .unwrap();
    assert_eq!(
        verify_final_classic_sell(&forged, &e, &now, &mut scratch),
        Err(BindingError::WrongValue)
    );
}

/// R12c-3: the exit's expectation needs its code and policy generation and
/// the slippage it was admitted with; `ExpectedTx` carries both of its
/// transactions (the SELL at the approval's nonce + 1, no value, TokenManager2).
#[test]
fn the_exit_expectation_and_its_intent_kinds() {
    let (a, policy) = admitted_sell(300);
    let sender = Address::repeat_byte(8);
    assert_eq!(
        ExpectedClassicExit::from_admitted(&validated(2), &a, &policy, sender, 1),
        Err(IntentError::GenerationMismatch)
    );
    let other_gen = ExitPolicy::new(6, 300, 100_000_000, 120_000, 400_000).unwrap();
    assert_eq!(
        ExpectedClassicExit::from_admitted(&validated(1), &a, &other_gen, sender, 1),
        Err(IntentError::PolicyGenerationMismatch)
    );
    let other_slippage = ExitPolicy::new(5, 200, 100_000_000, 120_000, 400_000).unwrap();
    assert_eq!(
        ExpectedClassicExit::from_admitted(&validated(1), &a, &other_slippage, sender, 1),
        Err(IntentError::PolicyMismatch)
    );
    assert_eq!(
        ExpectedClassicExit::from_admitted(&validated(1), &a, &policy, sender, u64::MAX),
        Err(IntentError::PolicyMismatch)
    );
    let signer = Signer::from_secret_bytes(&[0x68; 32]).unwrap();
    let e = exit_expectation(&signer, 11);
    let (approve, sell) = (ExpectedTx::ClassicApprove(&e), ExpectedTx::ClassicSell(&e));
    assert_eq!((approve.nonce(), sell.nonce()), (11, 12));
    for tx in [approve, sell] {
        assert_eq!(
            (
                tx.chain_id(),
                tx.sender(),
                tx.token(),
                tx.portal(),
                tx.value_wei()
            ),
            (
                56,
                signer.address(),
                e.token(),
                e.token_manager2(),
                U256::ZERO
            )
        );
        assert_eq!(tx.snapshot(), e.snapshot());
    }
    assert_eq!(
        (approve.gas_wei(), sell.gas_wei()),
        (
            U256::from(100_000_000u64 * 120_000),
            U256::from(100_000_000u64 * 400_000)
        )
    );
    let (mut scratch, mut raw) = (Vec::new(), Vec::new());
    let h = e.sign_sell(&signer, &mut scratch, &mut raw).unwrap();
    assert_eq!(sell.verify_final(&raw, &e.snapshot(), &mut scratch), Ok(h));
    assert!(
        approve
            .verify_final(&raw, &e.snapshot(), &mut scratch)
            .is_err()
    );
    let h = e.sign_approve(&signer, &mut scratch, &mut raw).unwrap();
    assert_eq!(
        approve.verify_final(&raw, &e.snapshot(), &mut scratch),
        Ok(h)
    );
}

mod openfour {
    //! R12d: the OpenFour BUY of an exact amount and its exit, from the
    //! admission on the witnessed launch.
    use super::*;
    use crate::economics::openfour::{
        QualifiedOpenFourState, admit_openfour_buy, quote_buy_amount,
    };
    use crate::venues::fourmeme::abi::encode_openfour_buy;
    use crate::venues::fourmeme::deployment::bsc_mainnet_r12d_131608421;
    use crate::venues::fourmeme::profile::classify_openfour_launch;
    use crate::venues::fourmeme::recognize::{decode_openfour_buy, decode_openfour_create};

    const OPENFOUR: &str =
        include_str!("../../../../fixtures/fourmeme/openfour_parity_20261104.txt");

    fn validated_r12d(generation: u64) -> ValidatedFourDeployment {
        let d = bsc_mainnet_r12d_131608421();
        d.validate(
            &crate::venues::fourmeme::deployment::testkit::pinned_observation(&d),
            generation,
        )
        .unwrap()
    }

    fn openfour_policy() -> BuyPolicy {
        BuyPolicy::new(5, U256::from(FUNDS), 100, 100_000_000, 900_000).unwrap()
    }

    fn admitted_buy() -> AdmittedOpenFourBuy {
        let f: Vec<&str> = OPENFOUR
            .lines()
            .find(|l| l.starts_with("OFLAUNCH "))
            .unwrap()
            .split(' ')
            .collect();
        let calldata = alloy_primitives::hex::decode(f[31]).unwrap();
        let launch = decode_openfour_create(&calldata).unwrap();
        let tx: B256 = f[2].parse().unwrap();
        let creator: Address = f[6].parse().unwrap();
        let value = U256::from_str_radix(f[7], 10).unwrap();
        let c = classify_openfour_launch(&validated_r12d(1), &launch, creator, value, tx).unwrap();
        let snapshot = SnapshotId {
            canonical_block_hash: B256::repeat_byte(9),
            ordered_state_cursor: 1,
            pending_candidate: tx,
            policy_generation: 5,
            codehash_generation: 1,
        };
        let state = QualifiedOpenFourState::fresh_launch(&c, 0, snapshot).unwrap();
        admit_openfour_buy(&state, &openfour_policy()).unwrap()
    }

    /// The production signer signs exactly the admitted BUY: Core.buy of
    /// the exact amount with the input as maxPay and value; every bound
    /// field is enforced at the send boundary.
    #[test]
    fn the_openfour_buy_is_signed_and_bound() {
        let signer = Signer::from_secret_bytes(&[0x71; 32]).unwrap();
        let a = admitted_buy();
        let e = ExpectedOpenFourBuy::from_admitted(
            &validated_r12d(1),
            &a,
            &openfour_policy(),
            signer.address(),
            4,
        )
        .unwrap();
        let (mut scratch, mut raw) = (Vec::new(), Vec::new());
        let h = e.sign(&signer, &mut scratch, &mut raw).unwrap();
        assert_eq!(
            verify_final_openfour_buy(&raw, &e, &e.snapshot(), &mut scratch),
            Ok(h)
        );
        let signed = decode_signed(&raw).unwrap();
        assert_eq!(
            (
                signed.tx.to,
                signed.tx.value,
                signed.tx.nonce,
                signed.tx.gas_limit
            ),
            (
                bsc_mainnet_r12d_131608421().openfour_core,
                a.max_pay(),
                4,
                900_000
            )
        );
        assert_eq!(signed.tx.input, a.calldata().unwrap().as_slice());
        let call = decode_openfour_buy(signed.tx.input).unwrap();
        assert_eq!(
            (call.amount, call.max_pay_amount),
            (a.amount(), a.max_pay())
        );
        let one = U256::from(1u8);
        let cases: [(ExpectedOpenFourBuy, BindingError); 8] = [
            (
                ExpectedOpenFourBuy {
                    sender: Address::repeat_byte(1),
                    ..e
                },
                BindingError::WrongSender,
            ),
            (
                ExpectedOpenFourBuy { nonce: 5, ..e },
                BindingError::WrongNonce,
            ),
            (
                ExpectedOpenFourBuy {
                    core: Address::repeat_byte(2),
                    ..e
                },
                BindingError::WrongPortal,
            ),
            (
                ExpectedOpenFourBuy {
                    gas_price: e.gas_price + 1,
                    ..e
                },
                BindingError::WrongGasPrice,
            ),
            (
                ExpectedOpenFourBuy {
                    gas_limit: e.gas_limit + 1,
                    ..e
                },
                BindingError::WrongGasLimit,
            ),
            (
                ExpectedOpenFourBuy {
                    max_pay: e.max_pay + one,
                    ..e
                },
                BindingError::WrongValue,
            ),
            (
                ExpectedOpenFourBuy {
                    token: Address::repeat_byte(3),
                    ..e
                },
                BindingError::WrongToken,
            ),
            (
                ExpectedOpenFourBuy {
                    amount: e.amount + one,
                    ..e
                },
                BindingError::WrongInputAmount,
            ),
        ];
        for (x, error) in cases {
            assert_eq!(
                verify_final_openfour_buy(&raw, &x, &x.snapshot(), &mut scratch),
                Err(error)
            );
        }
        // A maxPay in the calldata other than the value.
        let input = encode_openfour_buy(e.token, e.amount, e.max_pay - one).unwrap();
        let mut forged = Vec::new();
        signer
            .sign_legacy(
                &LegacyTx {
                    chain_id: e.chain_id,
                    nonce: e.nonce,
                    gas_price: e.gas_price,
                    gas_limit: e.gas_limit,
                    to: e.core,
                    value: e.max_pay,
                    input: &input,
                },
                &mut scratch,
                &mut forged,
            )
            .unwrap();
        assert_eq!(
            verify_final_openfour_buy(&forged, &e, &e.snapshot(), &mut scratch),
            Err(BindingError::WrongMinOutput)
        );
        let mut moved = e.snapshot();
        moved.ordered_state_cursor += 1;
        assert_eq!(
            verify_final_openfour_buy(&raw, &e, &moved, &mut scratch),
            Err(BindingError::StaleSnapshot)
        );
        // The expectation needs the admission's policy.
        let other = BuyPolicy::new(5, U256::from(FUNDS), 200, 100_000_000, 900_000).unwrap();
        assert_eq!(
            ExpectedOpenFourBuy::from_admitted(&validated_r12d(1), &a, &other, signer.address(), 4),
            Err(IntentError::PolicyMismatch)
        );
        assert_eq!(
            ExpectedOpenFourBuy::from_admitted(
                &validated_r12d(2),
                &a,
                &openfour_policy(),
                signer.address(),
                4
            ),
            Err(IntentError::GenerationMismatch)
        );
        let tx = ExpectedTx::OpenFourBuy(&e);
        assert_eq!(
            (tx.nonce(), tx.value_wei(), tx.portal(), tx.token()),
            (4, a.max_pay(), e.core(), a.token())
        );
        assert_eq!(tx.verify_final(&raw, &e.snapshot(), &mut scratch), Ok(h));
    }

    /// The OpenFour exit: approve(Core, amount) on the token at `n`, then
    /// Core.sell at `n + 1`, both bound field by field.
    #[test]
    fn the_openfour_exit_is_signed_and_bound() {
        use crate::economics::openfour::{
            OpenFourCurve, QualifiedOpenFourHolding, admit_openfour_sell,
        };
        use crate::economics::policy::ExitPolicy;
        let signer = Signer::from_secret_bytes(&[0x72; 32]).unwrap();
        let b = admitted_buy();
        // The held state after the admitted BUY (its curve read at a block).
        let mut read = crate::economics::openfour::tests::admission::holding_read("131608421");
        let fresh =
            crate::venues::fourmeme::profile::classify_openfour_holding(&validated_r12d(1), &read)
                .unwrap();
        let after = quote_buy_amount(&fresh.curve(), &fresh.fee(), b.amount())
            .unwrap()
            .after;
        read.current_quote = after.quote_reserve;
        read.current_token = after.token_reserve;
        read.remaining = after.remaining;
        let held =
            crate::venues::fourmeme::profile::classify_openfour_holding(&validated_r12d(1), &read)
                .unwrap();
        let snapshot = SnapshotId {
            canonical_block_hash: read.block_hash,
            ordered_state_cursor: 2,
            pending_candidate: B256::ZERO,
            policy_generation: 5,
            codehash_generation: 1,
        };
        let state = QualifiedOpenFourHolding::at(&held, snapshot).unwrap();
        let policy = ExitPolicy::new(5, 300, 100_000_000, 120_000, 400_000).unwrap();
        let s = admit_openfour_sell(&state, b.amount(), &policy).unwrap();
        let _: OpenFourCurve = held.curve();
        let e = ExpectedOpenFourExit::from_admitted(
            &validated_r12d(1),
            &s,
            &policy,
            signer.address(),
            6,
        )
        .unwrap();
        let (mut scratch, mut approve, mut sell) = (Vec::new(), Vec::new(), Vec::new());
        let ha = e.sign_approve(&signer, &mut scratch, &mut approve).unwrap();
        let hs = e.sign_sell(&signer, &mut scratch, &mut sell).unwrap();
        let now = e.snapshot();
        assert_eq!(
            verify_final_openfour_approve(&approve, &e, &now, &mut scratch),
            Ok(ha)
        );
        assert_eq!(
            verify_final_openfour_sell(&sell, &e, &now, &mut scratch),
            Ok(hs)
        );
        let a_tx = decode_signed(&approve).unwrap();
        assert_eq!(
            (a_tx.tx.to, a_tx.tx.nonce, a_tx.tx.value),
            (s.token(), 6, U256::ZERO)
        );
        assert_eq!(
            crate::venues::flap::abi::decode_approve(a_tx.tx.input).unwrap(),
            (e.core(), s.amount())
        );
        let s_tx = decode_signed(&sell).unwrap();
        assert_eq!(
            (s_tx.tx.to, s_tx.tx.nonce, s_tx.tx.value),
            (e.core(), 7, U256::ZERO)
        );
        assert_eq!(s_tx.tx.input, s.calldata().unwrap().as_slice());
        let one = U256::from(1u8);
        for (x, error) in [
            (
                ExpectedOpenFourExit {
                    amount: e.amount + one,
                    ..e
                },
                BindingError::WrongInputAmount,
            ),
            (
                ExpectedOpenFourExit {
                    token: Address::repeat_byte(3),
                    ..e
                },
                BindingError::WrongToken,
            ),
            (
                ExpectedOpenFourExit { nonce: 7, ..e },
                BindingError::WrongNonce,
            ),
        ] {
            assert_eq!(
                verify_final_openfour_approve(&approve, &x, &now, &mut scratch),
                Err(error)
            );
            assert_eq!(
                verify_final_openfour_sell(&sell, &x, &now, &mut scratch),
                Err(error)
            );
        }
        let core = ExpectedOpenFourExit {
            core: Address::repeat_byte(4),
            ..e
        };
        assert_eq!(
            verify_final_openfour_approve(&approve, &core, &now, &mut scratch),
            Err(BindingError::WrongSpender)
        );
        assert_eq!(
            verify_final_openfour_sell(&sell, &core, &now, &mut scratch),
            Err(BindingError::WrongPortal)
        );
        let min = ExpectedOpenFourExit {
            min_receive: e.min_receive + one,
            ..e
        };
        assert_eq!(
            verify_final_openfour_sell(&sell, &min, &now, &mut scratch),
            Err(BindingError::WrongMinOutput)
        );
        let (ta, ts) = (
            ExpectedTx::OpenFourApprove(&e),
            ExpectedTx::OpenFourSell(&e),
        );
        assert_eq!((ta.nonce(), ts.nonce()), (6, 7));
        assert_eq!((ta.value_wei(), ts.value_wei()), (U256::ZERO, U256::ZERO));
        assert_eq!(ts.verify_final(&sell, &now, &mut scratch), Ok(hs));
        assert!(ta.verify_final(&sell, &now, &mut scratch).is_err());
        // The quote the minimum is the floor of.
        let q = quote_buy_amount(&held.curve(), &held.fee(), U256::from(1_000u64)).unwrap();
        assert!(q.pay > U256::ZERO);
    }
}
