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
