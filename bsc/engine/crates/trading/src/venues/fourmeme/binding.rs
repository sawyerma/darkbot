//! The signed Classic BUY bound to its admission (R12c), as
//! `venues::flap::binding` binds the Flap BUY: the expectation of the
//! transaction (chain, sender, nonce, TokenManager2, gas, value, calldata),
//! its build by the production signer, and the final check of the signed
//! bytes at the send boundary. The lane and the plan take it as
//! [`ExpectedTx::ClassicBuy`](crate::venues::flap::binding::ExpectedTx).

use alloy_primitives::{Address, B256, U256};

use super::abi::encode_buy_token_amap;
use super::deployment::ValidatedFourDeployment;
use super::recognize::decode_classic_buy_amap;
use crate::chain_state::SnapshotId;
use crate::economics::four_classic::AdmittedClassicBuy;
use crate::economics::policy::BuyPolicy;
use crate::signer::{Signer, recover_legacy_sender};
use crate::transactions::legacy::{LegacyTx, decode_signed, tx_hash};
use crate::venues::flap::binding::{BindingError, BuildError, IntentError};

/// The Classic BUY a signed transaction must encode, field by field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpectedClassicBuy {
    chain_id: u64,
    token_manager2: Address,
    sender: Address,
    nonce: u64,
    gas_price: u64,
    gas_limit: u64,
    token: Address,
    funds: U256,
    min_amount: U256,
    snapshot: SnapshotId,
}

impl ExpectedClassicBuy {
    /// The expectation of an admitted BUY under the policy it was admitted
    /// with; `sender` signs and receives, `nonce` is reserved for it.
    pub fn from_admitted(
        validated: &ValidatedFourDeployment,
        admitted: &AdmittedClassicBuy,
        policy: &BuyPolicy,
        sender: Address,
        nonce: u64,
    ) -> Result<Self, IntentError> {
        let snapshot = admitted.snapshot();
        if validated.codehash_generation() != snapshot.codehash_generation {
            return Err(IntentError::GenerationMismatch);
        }
        if policy.generation() != snapshot.policy_generation {
            return Err(IntentError::PolicyGenerationMismatch);
        }
        if policy.input_amount() != admitted.funds()
            || policy.slippage_bps() != admitted.slippage_bps()
        {
            return Err(IntentError::PolicyMismatch);
        }
        Ok(Self {
            chain_id: validated.deployment().chain_id,
            token_manager2: validated.deployment().token_manager2,
            sender,
            nonce,
            gas_price: policy.gas_price(),
            gas_limit: policy.gas_limit(),
            token: admitted.token(),
            funds: admitted.funds(),
            min_amount: admitted.min_tokens(),
            snapshot,
        })
    }

    #[inline]
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// The contract the BUY calls.
    #[inline]
    pub fn token_manager2(&self) -> Address {
        self.token_manager2
    }

    #[inline]
    pub fn sender(&self) -> Address {
        self.sender
    }

    #[inline]
    pub fn nonce(&self) -> u64 {
        self.nonce
    }

    #[inline]
    pub fn gas_price(&self) -> u64 {
        self.gas_price
    }

    #[inline]
    pub fn gas_limit(&self) -> u64 {
        self.gas_limit
    }

    #[inline]
    pub fn token(&self) -> Address {
        self.token
    }

    /// The `funds` argument and the transaction's value, in wei.
    #[inline]
    pub fn funds(&self) -> U256 {
        self.funds
    }

    #[inline]
    pub fn min_amount(&self) -> U256 {
        self.min_amount
    }

    #[inline]
    pub fn snapshot(&self) -> SnapshotId {
        self.snapshot
    }

    /// Signs the BUY into `out` (cleared first); returns its hash.
    pub fn sign(
        &self,
        signer: &Signer,
        scratch: &mut Vec<u8>,
        out: &mut Vec<u8>,
    ) -> Result<B256, BuildError> {
        out.clear();
        if signer.address() != self.sender {
            return Err(BuildError::WrongSigner);
        }
        let calldata = encode_buy_token_amap(self.token, self.funds, self.min_amount)
            .map_err(BuildError::FourCalldata)?;
        let tx = LegacyTx {
            chain_id: self.chain_id,
            nonce: self.nonce,
            gas_price: self.gas_price,
            gas_limit: self.gas_limit,
            to: self.token_manager2,
            value: self.funds,
            input: &calldata,
        };
        signer
            .sign_legacy(&tx, scratch, out)
            .map_err(BuildError::Sign)
    }
}

/// Verifies, at the send boundary, that `raw_signed` is exactly the BUY of
/// `expected` and that `expected` still belongs to the `current` snapshot.
/// Returns the transaction hash.
pub fn verify_final_classic_buy(
    raw_signed: &[u8],
    expected: &ExpectedClassicBuy,
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
    if signed.tx.to != expected.token_manager2 {
        return Err(BindingError::WrongPortal);
    }
    if signed.tx.gas_price != expected.gas_price {
        return Err(BindingError::WrongGasPrice);
    }
    if signed.tx.gas_limit != expected.gas_limit {
        return Err(BindingError::WrongGasLimit);
    }
    if signed.tx.value != expected.funds {
        return Err(BindingError::WrongValue);
    }
    let call = decode_classic_buy_amap(signed.tx.input).map_err(BindingError::FourCalldata)?;
    if call.token != expected.token {
        return Err(BindingError::WrongToken);
    }
    if call.funds != expected.funds {
        return Err(BindingError::WrongInputAmount);
    }
    if call.min_amount != expected.min_amount {
        return Err(BindingError::WrongMinOutput);
    }
    Ok(tx_hash(raw_signed))
}

#[cfg(test)]
mod tests;
