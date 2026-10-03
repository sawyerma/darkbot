//! Final binding of a signed BUY to its admitted economic action.
//!
//! Ports the acceptance of the Gate-0 final-envelope test (256 envelopes,
//! adoption rows SN02/SN06, matrix N2): before a signed BUY may leave the
//! process, its bytes are decoded again and every signature-bound field is
//! compared with the intent: chain, recovered signer, nonce, Portal, gas
//! price, gas limit, value, token and amounts.
//!
//! [`ExpectedBuy`] cannot be assembled from raw numbers. Its constructor
//! takes an [`AdmittedBuy`] — a quote that passed runtime admission — a
//! validated deployment of the same code generation and the operating
//! policy of the same policy generation. An expectation with a zero
//! amount, a zero minimum output, an unqualified token or stale policy
//! values therefore does not exist.
//!
//! # Send boundary
//!
//! [`verify_final_buy`] is the last check before signed bytes may leave
//! the process. Besides the bytes it takes the state snapshot that is
//! current at that moment and refuses the BUY unless the expectation was
//! built for exactly that snapshot. A changed policy generation (slippage,
//! amount, gas, budget), a changed code generation, another pending
//! candidate, a reorg or any later ordered effect yields another snapshot
//! identity, so prepared bytes become unsendable; they are never patched.
//!
//! The direct Portal BUY has no recipient argument; the acquirer is the
//! caller. Binding the recovered signer therefore binds the recipient.
//!
//! This check re-derives the signer from the signature (one public-key
//! recovery). Its cost has not been measured in this library; for prepared
//! payloads it runs before the trigger.

use alloy_primitives::{Address, B256, U256};

use super::abi::{AbiError, decode_swap_exact_input_native};
use super::deployment::ValidatedDeployment;
use crate::chain_state::SnapshotId;
use crate::economics::flap::AdmittedBuy;
use crate::economics::policy::BuyPolicy;
use crate::signer::{SignerError, recover_legacy_sender};
use crate::tx::legacy::{LegacyDecodeError, decode_signed, tx_hash};

/// Reason an expectation cannot be formed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentError {
    /// The deployment was validated under another code generation than the
    /// admitted quote's snapshot.
    GenerationMismatch,
    /// The policy has another generation than the admitted quote's snapshot.
    PolicyGenerationMismatch,
    /// The policy's amount or slippage differs from the admitted quote.
    PolicyMismatch,
}

impl core::fmt::Display for IntentError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::GenerationMismatch => "buy intent: code generation mismatch",
            Self::PolicyGenerationMismatch => "buy intent: policy generation mismatch",
            Self::PolicyMismatch => "buy intent: policy differs from the admitted quote",
        })
    }
}

impl std::error::Error for IntentError {}

/// The economic action a signed BUY must encode, field by field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpectedBuy {
    chain_id: u64,
    portal: Address,
    sender: Address,
    nonce: u64,
    gas_price: u64,
    gas_limit: u64,
    token: Address,
    input_amount: U256,
    min_output_amount: U256,
    snapshot: SnapshotId,
}

impl ExpectedBuy {
    /// Forms the expectation for an admitted BUY under the current policy.
    ///
    /// `sender` is the intended signer and recipient, `nonce` the nonce
    /// reserved for this intent. Gas price and gas limit come from
    /// `policy`, which must be the policy the quote was admitted under.
    pub fn from_admitted(
        deployment: &ValidatedDeployment,
        admitted: &AdmittedBuy,
        policy: &BuyPolicy,
        sender: Address,
        nonce: u64,
    ) -> Result<Self, IntentError> {
        let snapshot = admitted.snapshot();
        if deployment.codehash_generation() != snapshot.codehash_generation {
            return Err(IntentError::GenerationMismatch);
        }
        if policy.generation() != snapshot.policy_generation {
            return Err(IntentError::PolicyGenerationMismatch);
        }
        if policy.input_amount() != admitted.input_amount()
            || policy.slippage_bps() != admitted.slippage_bps()
        {
            return Err(IntentError::PolicyMismatch);
        }
        Ok(Self {
            chain_id: deployment.deployment().chain_id,
            portal: deployment.deployment().portal,
            sender,
            nonce,
            gas_price: policy.gas_price(),
            gas_limit: policy.gas_limit(),
            token: admitted.token(),
            input_amount: admitted.input_amount(),
            min_output_amount: admitted.min_output_amount(),
            snapshot,
        })
    }

    /// Chain the transaction must be signed for.
    #[inline]
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Portal the transaction must call.
    #[inline]
    pub fn portal(&self) -> Address {
        self.portal
    }

    /// Intended signer, which is also the recipient of the tokens.
    #[inline]
    pub fn sender(&self) -> Address {
        self.sender
    }

    /// Nonce reserved for this intent.
    #[inline]
    pub fn nonce(&self) -> u64 {
        self.nonce
    }

    /// Gas price the transaction must carry, in wei.
    #[inline]
    pub fn gas_price(&self) -> u64 {
        self.gas_price
    }

    /// Gas limit the transaction must carry.
    #[inline]
    pub fn gas_limit(&self) -> u64 {
        self.gas_limit
    }

    /// Token to acquire.
    #[inline]
    pub fn token(&self) -> Address {
        self.token
    }

    /// Native amount to spend: transaction value and calldata amount.
    #[inline]
    pub fn input_amount(&self) -> U256 {
        self.input_amount
    }

    /// Minimum output bound into the calldata.
    #[inline]
    pub fn min_output_amount(&self) -> U256 {
        self.min_output_amount
    }

    /// State snapshot the expectation is valid for.
    #[inline]
    pub fn snapshot(&self) -> SnapshotId {
        self.snapshot
    }
}

/// Reason signed bytes are not the intended BUY.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingError {
    /// The expectation was built for another state snapshot than the
    /// current one: policy, code, candidate or chain state changed.
    StaleSnapshot,
    /// The bytes are not a canonical signed legacy call.
    Envelope(LegacyDecodeError),
    /// Wrong chain, invalid signature or failed recovery.
    Signature(SignerError),
    /// Signed by another key: tokens would go to another recipient.
    WrongSender,
    /// Another nonce than the one reserved for this intent.
    WrongNonce,
    /// The call does not target the Portal.
    WrongPortal,
    /// Gas price differs from the current gas policy.
    WrongGasPrice,
    /// Gas limit differs from the current gas policy.
    WrongGasLimit,
    /// Transaction value differs from the intended input amount.
    WrongValue,
    /// The calldata is not a canonical native-input BUY.
    Calldata(AbiError),
    /// The calldata buys another token.
    WrongToken,
    /// Calldata input amount differs from the intended amount.
    WrongInputAmount,
    /// Calldata minimum output differs from the intended minimum.
    WrongMinOutput,
}

impl core::fmt::Display for BindingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::StaleSnapshot => f.write_str("buy binding: expectation is stale"),
            Self::Envelope(e) => write!(f, "buy binding: {e}"),
            Self::Signature(e) => write!(f, "buy binding: {e}"),
            Self::WrongSender => f.write_str("buy binding: signed by another sender"),
            Self::WrongNonce => f.write_str("buy binding: unexpected nonce"),
            Self::WrongPortal => f.write_str("buy binding: call does not target the Portal"),
            Self::WrongGasPrice => f.write_str("buy binding: unexpected gas price"),
            Self::WrongGasLimit => f.write_str("buy binding: unexpected gas limit"),
            Self::WrongValue => f.write_str("buy binding: value differs from the input amount"),
            Self::Calldata(e) => write!(f, "buy binding: {e}"),
            Self::WrongToken => f.write_str("buy binding: calldata buys another token"),
            Self::WrongInputAmount => f.write_str("buy binding: unexpected input amount"),
            Self::WrongMinOutput => f.write_str("buy binding: unexpected minimum output"),
        }
    }
}

impl std::error::Error for BindingError {}

/// Verifies, at the send boundary, that `raw_signed` is exactly the BUY
/// described by `expected` and that `expected` still belongs to the
/// current state snapshot. Returns the transaction hash.
///
/// `scratch` is cleared and reused for the signing payload.
pub fn verify_final_buy(
    raw_signed: &[u8],
    expected: &ExpectedBuy,
    current: &SnapshotId,
    scratch: &mut Vec<u8>,
) -> Result<B256, BindingError> {
    if expected.snapshot != *current {
        return Err(BindingError::StaleSnapshot);
    }
    let signed = decode_signed(raw_signed).map_err(BindingError::Envelope)?;
    let sender = recover_legacy_sender(&signed, expected.chain_id, scratch)
        .map_err(BindingError::Signature)?;
    if sender != expected.sender {
        return Err(BindingError::WrongSender);
    }
    if signed.tx.nonce != expected.nonce {
        return Err(BindingError::WrongNonce);
    }
    if signed.tx.to != expected.portal {
        return Err(BindingError::WrongPortal);
    }
    if signed.tx.gas_price != expected.gas_price {
        return Err(BindingError::WrongGasPrice);
    }
    if signed.tx.gas_limit != expected.gas_limit {
        return Err(BindingError::WrongGasLimit);
    }
    if signed.tx.value != expected.input_amount {
        return Err(BindingError::WrongValue);
    }
    let call = decode_swap_exact_input_native(signed.tx.input).map_err(BindingError::Calldata)?;
    if call.output_token != expected.token {
        return Err(BindingError::WrongToken);
    }
    if call.input_amount != expected.input_amount {
        return Err(BindingError::WrongInputAmount);
    }
    if call.min_output_amount != expected.min_output_amount {
        return Err(BindingError::WrongMinOutput);
    }
    Ok(tx_hash(raw_signed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economics::flap::native_curve20::{
        QUALIFIED_MAINNET_INPUT_WEI, QualifiedNativeState, WITNESSED_MAINNET_TERMS,
        admit_native_buy,
    };
    use crate::signer::Signer;
    use crate::tx::legacy::LegacyTx;
    use crate::venues::flap::abi::encode_swap_exact_input_native;
    use crate::venues::flap::deployment::testkit::validated_mainnet;
    use crate::venues::flap::profile::classify_launch;
    use crate::venues::flap::profile::testkit::{
        candidate_a, candidate_b, witnessed_native_launch,
    };
    use alloy_primitives::keccak256;

    const GAS_PRICE: u64 = 310_000_000;
    const GAS_LIMIT: u64 = 1_500_000;
    const POLICY_GENERATION: u64 = 3;

    fn snapshot(codehash_generation: u64) -> SnapshotId {
        SnapshotId {
            canonical_block_hash: B256::from_slice(&[0xb1u8; 32]),
            ordered_state_cursor: 41,
            pending_candidate: candidate_a(),
            policy_generation: POLICY_GENERATION,
            codehash_generation,
        }
    }

    fn policy(generation: u64, gas_price: u64) -> BuyPolicy {
        BuyPolicy::new(
            generation,
            U256::from(QUALIFIED_MAINNET_INPUT_WEI),
            100,
            gas_price,
            GAS_LIMIT,
        )
        .unwrap()
    }

    /// Test-only expectation with free fields, for the synthetic Gate-0
    /// fixture. Production code has no such constructor.
    fn fixture_expectation(sender: Address, nonce: u64, token: Address) -> ExpectedBuy {
        let deployment = validated_mainnet(7);
        ExpectedBuy {
            chain_id: deployment.deployment().chain_id,
            portal: deployment.deployment().portal,
            sender,
            nonce,
            gas_price: GAS_PRICE,
            gas_limit: GAS_LIMIT,
            token,
            input_amount: U256::from(1000u64),
            min_output_amount: U256::from(1u64),
            snapshot: snapshot(7),
        }
    }

    /// Synthetic token of the Gate-0 G0.6 fixture: keccak(portal ‖ id)[12..].
    fn fixture_token(portal: Address, id: u64) -> Address {
        let mut preimage = [0u8; 28];
        preimage[..20].copy_from_slice(portal.as_slice());
        preimage[20..].copy_from_slice(&id.to_be_bytes());
        Address::from_slice(&keccak256(preimage).as_slice()[12..])
    }

    /// Signs exactly the transaction that `expected` describes.
    fn sign(signer: &Signer, expected: &ExpectedBuy) -> (Vec<u8>, B256) {
        let calldata = encode_swap_exact_input_native(
            expected.token,
            expected.input_amount,
            expected.min_output_amount,
        )
        .unwrap();
        let tx = LegacyTx {
            chain_id: expected.chain_id,
            nonce: expected.nonce,
            gas_price: expected.gas_price,
            gas_limit: expected.gas_limit,
            to: expected.portal,
            value: expected.input_amount,
            input: &calldata,
        };
        let mut scratch = Vec::new();
        let mut raw = Vec::new();
        let hash = signer.sign_legacy(&tx, &mut scratch, &mut raw).unwrap();
        (raw, hash)
    }

    /// The production path up to the expectation: witnessed launch →
    /// classification → qualified state → admission → expectation.
    fn admitted_expectation(signer: &Signer, nonce: u64) -> (ExpectedBuy, [u8; 228]) {
        let deployment = validated_mainnet(7);
        let classified =
            classify_launch(&deployment, candidate_a(), &witnessed_native_launch()).unwrap();
        let terms = WITNESSED_MAINNET_TERMS;
        let state =
            QualifiedNativeState::fresh_launch(&classified, &terms, 0, snapshot(7)).unwrap();
        let current = policy(POLICY_GENERATION, GAS_PRICE);
        let admitted = admit_native_buy(&state, &current).unwrap();
        let expected =
            ExpectedBuy::from_admitted(&deployment, &admitted, &current, signer.address(), nonce)
                .unwrap();
        (expected, admitted.calldata().unwrap())
    }

    /// Port of Gate-0 `parallel_final_envelopes_bind_ticket_token_and_implicit_recipient`
    /// without the nonce lane, which belongs to Unit 4: eight senders,
    /// thirty-two tokens each, nonces from 7.
    #[test]
    fn two_hundred_fifty_six_final_envelopes_bind_sender_nonce_portal_and_token() {
        let portal = validated_mainnet(7).deployment().portal;
        let current = snapshot(7);
        let mut scratch = Vec::new();
        let mut checked = 0usize;
        for worker in 0..8u8 {
            let signer = Signer::from_secret_bytes(&[0x40 + worker; 32]).unwrap();
            for id in 0..32u64 {
                let token = fixture_token(portal, u64::from(worker) * 32 + id);
                let expected = fixture_expectation(signer.address(), 7 + id, token);
                let (raw, hash) = sign(&signer, &expected);
                assert_eq!(
                    verify_final_buy(&raw, &expected, &current, &mut scratch),
                    Ok(hash),
                    "worker {worker} id {id}"
                );

                // The same bytes never satisfy another sender's intent.
                let other = fixture_expectation(portal, 7 + id, token);
                assert_eq!(
                    verify_final_buy(&raw, &other, &current, &mut scratch),
                    Err(BindingError::WrongSender)
                );
                checked += 1;
            }
        }
        assert_eq!(checked, 256);
    }

    #[test]
    fn every_bound_field_is_enforced() {
        let portal = validated_mainnet(7).deployment().portal;
        let signer = Signer::from_secret_bytes(&[0x40u8; 32]).unwrap();
        let token = fixture_token(portal, 0);
        let good = fixture_expectation(signer.address(), 7, token);
        let (raw, hash) = sign(&signer, &good);
        let current = snapshot(7);
        let mut scratch = Vec::new();
        let mut check =
            |expected: &ExpectedBuy| verify_final_buy(&raw, expected, &current, &mut scratch);

        let mut expected = good;
        expected.chain_id = 97;
        assert_eq!(
            check(&expected),
            Err(BindingError::Signature(SignerError::WrongChain))
        );

        let mut expected = good;
        expected.nonce = 8;
        assert_eq!(check(&expected), Err(BindingError::WrongNonce));

        let mut expected = good;
        expected.portal = token;
        assert_eq!(check(&expected), Err(BindingError::WrongPortal));

        let mut expected = good;
        expected.gas_price = GAS_PRICE + 1;
        assert_eq!(check(&expected), Err(BindingError::WrongGasPrice));

        let mut expected = good;
        expected.gas_limit = GAS_LIMIT - 1;
        assert_eq!(check(&expected), Err(BindingError::WrongGasLimit));

        let mut expected = good;
        expected.input_amount = U256::from(999u64);
        assert_eq!(check(&expected), Err(BindingError::WrongValue));

        let mut expected = good;
        expected.token = fixture_token(portal, 1);
        assert_eq!(check(&expected), Err(BindingError::WrongToken));

        let mut expected = good;
        expected.min_output_amount = U256::from(2u64);
        assert_eq!(check(&expected), Err(BindingError::WrongMinOutput));

        assert_eq!(check(&good), Ok(hash));
    }

    #[test]
    fn value_and_calldata_amount_must_agree() {
        let portal = validated_mainnet(7).deployment().portal;
        let signer = Signer::from_secret_bytes(&[0x41u8; 32]).unwrap();
        let token = fixture_token(portal, 0);
        let expected = fixture_expectation(signer.address(), 7, token);
        // Calldata spends 999 while the transaction carries 1000.
        let calldata =
            encode_swap_exact_input_native(token, U256::from(999u64), U256::from(1u64)).unwrap();
        let tx = LegacyTx {
            chain_id: 56,
            nonce: 7,
            gas_price: GAS_PRICE,
            gas_limit: GAS_LIMIT,
            to: portal,
            value: U256::from(1000u64),
            input: &calldata,
        };
        let mut scratch = Vec::new();
        let mut raw = Vec::new();
        signer.sign_legacy(&tx, &mut scratch, &mut raw).unwrap();
        assert_eq!(
            verify_final_buy(&raw, &expected, &snapshot(7), &mut scratch),
            Err(BindingError::WrongInputAmount)
        );
    }

    #[test]
    fn foreign_calldata_and_malformed_bytes_are_rejected() {
        let portal = validated_mainnet(7).deployment().portal;
        let signer = Signer::from_secret_bytes(&[0x42u8; 32]).unwrap();
        let token = fixture_token(portal, 0);
        let expected = fixture_expectation(signer.address(), 7, token);
        let (good_raw, _) = sign(&signer, &expected);
        let current = snapshot(7);
        let mut scratch = Vec::new();

        // A validly signed call to the Portal that is not a BUY.
        let tx = LegacyTx {
            chain_id: 56,
            nonce: 7,
            gas_price: GAS_PRICE,
            gas_limit: GAS_LIMIT,
            to: portal,
            value: U256::from(1000u64),
            input: &[0xde, 0xad, 0xbe, 0xef],
        };
        let mut raw = Vec::new();
        signer.sign_legacy(&tx, &mut scratch, &mut raw).unwrap();
        assert_eq!(
            verify_final_buy(&raw, &expected, &current, &mut scratch),
            Err(BindingError::Calldata(AbiError::WrongSelector))
        );

        // Truncated envelope.
        let truncated = &good_raw[..good_raw.len() - 1];
        assert!(matches!(
            verify_final_buy(truncated, &expected, &current, &mut scratch),
            Err(BindingError::Envelope(_))
        ));

        // A flipped calldata bit invalidates the signature binding: the
        // bytes recover to another sender or fail to recover at all.
        let mut tampered = good_raw.clone();
        let position = tampered.len() - 80;
        tampered[position] ^= 0x01;
        assert!(verify_final_buy(&tampered, &expected, &current, &mut scratch).is_err());
    }

    #[test]
    fn admitted_buy_signs_and_binds_end_to_end() {
        let signer = Signer::from_secret_bytes(&[0x43u8; 32]).unwrap();
        let (expected, calldata) = admitted_expectation(&signer, 12);
        let deployment = validated_mainnet(7);
        assert_eq!(expected.chain_id(), 56);
        assert_eq!(expected.portal(), deployment.deployment().portal);
        assert_eq!(expected.sender(), signer.address());
        assert_eq!(expected.nonce(), 12);
        assert_eq!(expected.gas_price(), GAS_PRICE);
        assert_eq!(expected.gas_limit(), GAS_LIMIT);
        assert_eq!(
            expected.input_amount(),
            U256::from(QUALIFIED_MAINNET_INPUT_WEI)
        );
        assert_eq!(
            expected.min_output_amount(),
            U256::from(174_898_315_381_323_502_112_040u128)
        );
        assert_eq!(expected.snapshot(), snapshot(7));

        let tx = LegacyTx {
            chain_id: expected.chain_id(),
            nonce: expected.nonce(),
            gas_price: expected.gas_price(),
            gas_limit: expected.gas_limit(),
            to: expected.portal(),
            value: expected.input_amount(),
            input: &calldata,
        };
        let mut scratch = Vec::new();
        let mut raw = Vec::new();
        let hash = signer.sign_legacy(&tx, &mut scratch, &mut raw).unwrap();
        assert_eq!(
            verify_final_buy(&raw, &expected, &snapshot(7), &mut scratch),
            Ok(hash)
        );
    }

    /// Prepared, correctly signed bytes become unsendable as soon as any
    /// part of the snapshot identity changes.
    #[test]
    fn prepared_buy_is_refused_after_any_snapshot_change() {
        let signer = Signer::from_secret_bytes(&[0x44u8; 32]).unwrap();
        let (expected, _) = admitted_expectation(&signer, 12);
        let (raw, hash) = sign(&signer, &expected);
        let mut scratch = Vec::new();
        let built_for = snapshot(7);
        assert_eq!(
            verify_final_buy(&raw, &expected, &built_for, &mut scratch),
            Ok(hash)
        );

        // Slippage, amount, gas or budget policy changed.
        let mut current = built_for;
        current.policy_generation += 1;
        assert_eq!(
            verify_final_buy(&raw, &expected, &current, &mut scratch),
            Err(BindingError::StaleSnapshot)
        );

        // Deployment code was observed again.
        let mut current = built_for;
        current.codehash_generation += 1;
        assert_eq!(
            verify_final_buy(&raw, &expected, &current, &mut scratch),
            Err(BindingError::StaleSnapshot)
        );

        // Another launch transaction became the pending candidate.
        let mut current = built_for;
        current.pending_candidate = candidate_b();
        assert_eq!(
            verify_final_buy(&raw, &expected, &current, &mut scratch),
            Err(BindingError::StaleSnapshot)
        );

        // A later ordered effect, or another canonical block.
        let mut current = built_for;
        current.ordered_state_cursor += 1;
        assert_eq!(
            verify_final_buy(&raw, &expected, &current, &mut scratch),
            Err(BindingError::StaleSnapshot)
        );
        let mut current = built_for;
        current.canonical_block_hash = B256::ZERO;
        assert_eq!(
            verify_final_buy(&raw, &expected, &current, &mut scratch),
            Err(BindingError::StaleSnapshot)
        );
    }

    #[test]
    fn expectation_requires_matching_code_and_policy_generation() {
        let deployment = validated_mainnet(7);
        let classified =
            classify_launch(&deployment, candidate_a(), &witnessed_native_launch()).unwrap();
        let terms = WITNESSED_MAINNET_TERMS;
        let state =
            QualifiedNativeState::fresh_launch(&classified, &terms, 0, snapshot(7)).unwrap();
        let current = policy(POLICY_GENERATION, GAS_PRICE);
        let admitted = admit_native_buy(&state, &current).unwrap();
        let sender = Address::from_slice(&[0x11u8; 20]);

        // The deployment was re-validated under a newer code generation.
        assert_eq!(
            ExpectedBuy::from_admitted(&validated_mainnet(8), &admitted, &current, sender, 1),
            Err(IntentError::GenerationMismatch)
        );

        // The policy changed after admission: new generation.
        let repriced = policy(POLICY_GENERATION + 1, GAS_PRICE + 1);
        assert_eq!(
            ExpectedBuy::from_admitted(&deployment, &admitted, &repriced, sender, 1),
            Err(IntentError::PolicyGenerationMismatch)
        );

        // Same generation number but other values: refused as well.
        let other_slippage = BuyPolicy::new(
            POLICY_GENERATION,
            U256::from(QUALIFIED_MAINNET_INPUT_WEI),
            50,
            GAS_PRICE,
            GAS_LIMIT,
        )
        .unwrap();
        assert_eq!(
            ExpectedBuy::from_admitted(&deployment, &admitted, &other_slippage, sender, 1),
            Err(IntentError::PolicyMismatch)
        );
    }
}
