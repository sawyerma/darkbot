//! secp256k1 transaction signing and signer recovery.
//!
//! Consolidates the signing half of the four Gate-0 `sign_legacy` copies, the
//! two `signer_address` copies and the signature recovery of
//! `g06a_self_launch_bundle::recover_sender` and the G0.3 step-4 check.
//!
//! This is the only signer. Replay, PAPER, shadow and LIVE differ in the key
//! material they load, never in the signing code (Runtime Contract §6).
//!
//! # Invariant
//!
//! [`Signer::sign_legacy`] is the only way for another crate to obtain raw
//! signed bytes. It signs and encodes the same [`LegacyTx`] in one call, so
//! the bytes it returns are bound to exactly those fields and to this
//! signer's key. What the invariant does not cover is that the fields are
//! the intended ones: the caller's send path still binds transaction hash,
//! chain and sender to its intent before writing to a transport.
//!
//! # Latency
//!
//! Signatures are deterministic (RFC 6979) and always low-`s`, as produced by
//! `k256`. A [`Signer`] is constructed once before trading. Signing performs
//! no I/O. With `scratch` and `out` reserved once to
//! [`max_signing_payload_len`](crate::tx::legacy::max_signing_payload_len) and
//! [`max_signed_len`](crate::tx::legacy::max_signed_len), this crate does not
//! allocate while signing.
//!
//! # Secrets
//!
//! The key is accepted only as raw bytes from the caller's secret path, is
//! never formatted, and is zeroized on drop by `k256`.

use alloy_primitives::{Address, B256, keccak256};
use k256::ecdsa::{RecoveryId, Signature, SigningKey, VerifyingKey};

use crate::tx::legacy::{LegacyTx, RecoverableSignature, SignedLegacyTx, TxError};

/// Reason a signing or recovery operation failed. Every failure is terminal
/// for the affected intent: callers fail closed and never retry with altered
/// fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignerError {
    /// The 32 secret bytes are not a valid secp256k1 scalar.
    InvalidSecretKey,
    /// The signing primitive reported an error.
    SigningFailed,
    /// The signature's recovery id is not 0 or 1 and therefore cannot be
    /// expressed as an EIP-155 `v` value.
    UnsupportedRecoveryId,
    /// `r`/`s` are not valid signature scalars.
    InvalidSignature,
    /// `s` is in the upper half of the curve order (rejected by EIP-2).
    HighS,
    /// No public key could be recovered.
    RecoveryFailed,
    /// The transaction is signed for a different chain than expected.
    WrongChain,
    /// The transaction could not be encoded.
    Tx(TxError),
}

impl From<TxError> for SignerError {
    fn from(e: TxError) -> Self {
        Self::Tx(e)
    }
}

impl core::fmt::Display for SignerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidSecretKey => f.write_str("signer: invalid secret key"),
            Self::SigningFailed => f.write_str("signer: signing failed"),
            Self::UnsupportedRecoveryId => f.write_str("signer: unsupported recovery id"),
            Self::InvalidSignature => f.write_str("signer: invalid signature scalars"),
            Self::HighS => f.write_str("signer: high-s signature"),
            Self::RecoveryFailed => f.write_str("signer: public key recovery failed"),
            Self::WrongChain => f.write_str("signer: transaction is signed for another chain"),
            Self::Tx(e) => write!(f, "signer: {e}"),
        }
    }
}

impl std::error::Error for SignerError {}

/// Ethereum address of a public key: the last 20 bytes of the Keccak-256 of
/// the uncompressed point without its `0x04` tag.
pub fn address_of(key: &VerifyingKey) -> Address {
    let point = key.to_encoded_point(false);
    let hash = keccak256(&point.as_bytes()[1..]);
    Address::from_slice(&hash.as_slice()[12..])
}

/// A warm signing key bound to its address.
pub struct Signer {
    key: SigningKey,
    address: Address,
}

impl core::fmt::Debug for Signer {
    /// Prints the address only; the key is never formatted.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Signer")
            .field("address", &self.address)
            .finish_non_exhaustive()
    }
}

impl Signer {
    /// Builds a signer from 32 secret bytes supplied by the caller's secret
    /// path. The bytes are not retained by reference; the caller remains
    /// responsible for wiping its own copy.
    pub fn from_secret_bytes(secret: &[u8; 32]) -> Result<Self, SignerError> {
        let key =
            SigningKey::from_slice(secret.as_slice()).map_err(|_| SignerError::InvalidSecretKey)?;
        let address = address_of(key.verifying_key());
        Ok(Self { key, address })
    }

    /// Address that signatures of this signer recover to.
    #[inline]
    pub fn address(&self) -> Address {
        self.address
    }

    /// Signs a 32-byte prehash. Private: other crates obtain signatures only
    /// as part of [`Signer::sign_legacy`].
    fn sign_prehash(&self, prehash: &B256) -> Result<RecoverableSignature, SignerError> {
        let (signature, recovery_id) = self
            .key
            .sign_prehash_recoverable(prehash.as_slice())
            .map_err(|_| SignerError::SigningFailed)?;
        let y_parity = match recovery_id.to_byte() {
            0 => false,
            1 => true,
            _ => return Err(SignerError::UnsupportedRecoveryId),
        };
        let bytes = signature.to_bytes();
        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        r.copy_from_slice(&bytes[..32]);
        s.copy_from_slice(&bytes[32..]);
        Ok(RecoverableSignature { r, s, y_parity })
    }

    /// Signs `tx` and leaves exactly its raw signed bytes in `out`.
    ///
    /// Returns the transaction hash. Both buffers are cleared first: `scratch`
    /// is reused for the signing payload, and `out` never carries bytes of an
    /// earlier transaction. On error `out` is empty.
    pub fn sign_legacy(
        &self,
        tx: &LegacyTx<'_>,
        scratch: &mut Vec<u8>,
        out: &mut Vec<u8>,
    ) -> Result<B256, SignerError> {
        out.clear();
        let prehash = tx.signing_hash(scratch);
        let signature = self.sign_prehash(&prehash)?;
        tx.encode_signed(&signature, out)?;
        Ok(keccak256(out.as_slice()))
    }
}

/// Recovers the address that produced `signature` over `prehash`.
///
/// High-`s` signatures are rejected: they are invalid for transactions.
pub fn recover_signer(
    prehash: &B256,
    signature: &RecoverableSignature,
) -> Result<Address, SignerError> {
    let mut rs = [0u8; 64];
    rs[..32].copy_from_slice(&signature.r);
    rs[32..].copy_from_slice(&signature.s);
    let parsed = Signature::from_slice(&rs).map_err(|_| SignerError::InvalidSignature)?;
    if parsed.normalize_s().is_some() {
        return Err(SignerError::HighS);
    }
    let recovery_id =
        RecoveryId::from_byte(u8::from(signature.y_parity)).ok_or(SignerError::InvalidSignature)?;
    let key = VerifyingKey::recover_from_prehash(prehash.as_slice(), &parsed, recovery_id)
        .map_err(|_| SignerError::RecoveryFailed)?;
    Ok(address_of(&key))
}

/// Recovers the sender of a decoded signed legacy transaction and checks
/// that it is signed for `expected_chain_id`.
///
/// `scratch` is cleared and reused for the signing payload.
pub fn recover_legacy_sender(
    signed: &SignedLegacyTx<'_>,
    expected_chain_id: u64,
    scratch: &mut Vec<u8>,
) -> Result<Address, SignerError> {
    if signed.tx.chain_id != expected_chain_id {
        return Err(SignerError::WrongChain);
    }
    let prehash = signed.tx.signing_hash(scratch);
    recover_signer(&prehash, &signed.signature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::legacy::testdata::*;
    use crate::tx::legacy::{decode_signed, max_signed_len, max_signing_payload_len, tx_hash};
    use alloy_primitives::U256;

    /// Order of the secp256k1 group.
    const CURVE_ORDER: &str = "fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141";

    /// `swapExactInput((0x0, 0xabe1…7777, 1000, 1, ""))`, the calldata of the
    /// G0.6 regression fixture (`g06_concurrency::fixture`).
    const FIXTURE_CALLDATA: &str = concat!(
        "ef7ec2e7",
        "0000000000000000000000000000000000000000000000000000000000000020",
        "0000000000000000000000000000000000000000000000000000000000000000",
        "000000000000000000000000abe1fe16cf6bc48c9524713054efe6e954b17777",
        "00000000000000000000000000000000000000000000000000000000000003e8",
        "0000000000000000000000000000000000000000000000000000000000000001",
        "00000000000000000000000000000000000000000000000000000000000000a0",
        "0000000000000000000000000000000000000000000000000000000000000000",
    );

    fn fixture_tx(calldata: &[u8], nonce: u64) -> LegacyTx<'_> {
        LegacyTx {
            chain_id: 56,
            nonce,
            gas_price: 310_000_000,
            gas_limit: 1_500_000,
            to: Address::from_slice(&unhex(PORTAL)),
            value: U256::from(1000u64),
            input: calldata,
        }
    }

    #[test]
    fn golden_mainnet_buy_recovers_its_canonical_sender() {
        let raw = golden_raw();
        let signed = decode_signed(&raw).unwrap();
        let mut scratch = Vec::new();
        let sender = recover_legacy_sender(&signed, 56, &mut scratch).unwrap();
        assert_eq!(sender.as_slice(), unhex(GOLDEN_SENDER).as_slice());

        // Same bytes, other expected network: rejected before any recovery.
        assert_eq!(
            recover_legacy_sender(&signed, 97, &mut scratch),
            Err(SignerError::WrongChain)
        );
    }

    #[test]
    fn eip155_specification_example_signs_byte_exact() {
        // Public example key from the EIP-155 text (not a secret).
        let signer = Signer::from_secret_bytes(&[0x46u8; 32]).unwrap();
        assert_eq!(
            signer.address().as_slice(),
            unhex("9d8a62f656a8d1615c1294fd71e9cfb3e4855a4f").as_slice()
        );
        let tx = LegacyTx {
            chain_id: 1,
            nonce: 9,
            gas_price: 20_000_000_000,
            gas_limit: 21_000,
            to: Address::from_slice(&[0x35u8; 20]),
            value: U256::from(1_000_000_000_000_000_000u64),
            input: &[],
        };
        let mut scratch = Vec::new();
        let mut raw = Vec::new();
        let hash = signer.sign_legacy(&tx, &mut scratch, &mut raw).unwrap();
        assert_eq!(
            raw,
            unhex(concat!(
                "f86c098504a817c80082520894",
                "3535353535353535353535353535353535353535",
                "880de0b6b3a764000080",
                "25",
                "a028ef61340bd939bc2195fe537567866003e1a15d3c71ff63e1590620aa636276",
                "a067cbe9d8997f761aecb703304b3800ccf555c9f3dc64214b297fb1966a3b6d83",
            ))
        );
        assert_eq!(hash, tx_hash(&raw));
    }

    #[test]
    fn g06_fixture_first_envelope_is_byte_exact() {
        // Disposable test key 0x22…22, nonce 0. Expected bytes were computed
        // with an independent RFC 6979 implementation.
        let calldata = unhex(FIXTURE_CALLDATA);
        let signer = Signer::from_secret_bytes(&[0x22u8; 32]).unwrap();
        assert_eq!(
            signer.address().as_slice(),
            unhex("1563915e194d8cfba1943570603f7606a3115508").as_slice()
        );
        let tx = fixture_tx(&calldata, 0);

        let mut scratch = Vec::new();
        assert_eq!(
            tx.signing_hash(&mut scratch).as_slice(),
            unhex("c329f22b504c188c4bda1a70ad99d6a4fbb57bdc794f415d64ad7d183f5d623e").as_slice()
        );

        let mut raw = Vec::new();
        let hash = signer.sign_legacy(&tx, &mut scratch, &mut raw).unwrap();

        let mut expected = String::new();
        expected.push_str("f9014c8084127a39808316e36094");
        expected.push_str(PORTAL);
        expected.push_str("8203e8b8e4");
        expected.push_str(FIXTURE_CALLDATA);
        expected.push_str("8194");
        expected.push_str("a0aa3ea2c151a717048b96d4ea80ec705c694720fcce7ccce8500c2cfd2059b1c9");
        expected.push_str("a050d27f6930db951a03cf2efb9d9ae0741b38ab83372fd07d1a2d23cc42c751a1");
        assert_eq!(raw, unhex(&expected));
        assert_eq!(
            hash.as_slice(),
            unhex("c749da70be48006f4c8967cbc848a5d8e7e10e0d7073ff3d486f6e17d909ca78").as_slice()
        );
    }

    /// Port of `g06_concurrency::signed_envelopes_are_canonical_and_recover_the_correct_sender`.
    /// Canonical integer encoding is enforced by the strict decoder.
    #[test]
    fn signed_envelopes_are_canonical_low_s_and_recover_the_signer() {
        let calldata = unhex(FIXTURE_CALLDATA);
        let mut scratch = Vec::new();
        let mut raw = Vec::new();
        let mut short_scalars = 0usize;

        for (key_byte, address) in [
            (0x22u8, "1563915e194d8cfba1943570603f7606a3115508"),
            (0x33u8, "5cbdd86a2fa8dc4bddd8a8f69dba48572eec07fb"),
        ] {
            let signer = Signer::from_secret_bytes(&[key_byte; 32]).unwrap();
            assert_eq!(signer.address().as_slice(), unhex(address).as_slice());

            for nonce in 0..512u64 {
                let tx = fixture_tx(&calldata, nonce);
                let hash = signer.sign_legacy(&tx, &mut scratch, &mut raw).unwrap();
                assert_eq!(hash, tx_hash(&raw));

                let signed = decode_signed(&raw).unwrap();
                assert_eq!(signed.tx, tx, "key={key_byte:#x} nonce={nonce}");

                // `recover_signer` rejects high-s, so success implies low-s.
                let recovered = recover_legacy_sender(&signed, 56, &mut scratch).unwrap();
                assert_eq!(
                    recovered,
                    signer.address(),
                    "key={key_byte:#x} nonce={nonce}"
                );

                short_scalars += usize::from(signed.signature.r[0] == 0)
                    + usize::from(signed.signature.s[0] == 0);
            }
        }

        // The regression vectors must exercise signature scalars shorter than
        // 32 bytes; the independent reference computation yields exactly 10.
        assert_eq!(short_scalars, 10);
    }

    #[test]
    fn high_s_signature_is_rejected() {
        let raw = golden_raw();
        let signed = decode_signed(&raw).unwrap();
        let mut scratch = Vec::new();
        let prehash = signed.tx.signing_hash(&mut scratch);

        let order = U256::from_be_slice(&unhex(CURVE_ORDER));
        let s = U256::from_be_slice(&signed.signature.s);
        let mut malleated = signed.signature;
        malleated.s = (order - s).to_be_bytes::<32>();
        malleated.y_parity = !malleated.y_parity;

        assert_eq!(
            recover_signer(&prehash, &malleated),
            Err(SignerError::HighS)
        );
    }

    #[test]
    fn wrong_parity_or_tampered_field_does_not_recover_the_sender() {
        let raw = golden_raw();
        let signed = decode_signed(&raw).unwrap();
        let mut scratch = Vec::new();
        let golden_sender = unhex(GOLDEN_SENDER);

        let mut flipped = signed;
        flipped.signature.y_parity = !flipped.signature.y_parity;
        match recover_legacy_sender(&flipped, 56, &mut scratch) {
            Ok(address) => assert_ne!(address.as_slice(), golden_sender.as_slice()),
            Err(e) => assert_eq!(e, SignerError::RecoveryFailed),
        }

        let mut tampered = signed;
        tampered.tx.nonce += 1;
        match recover_legacy_sender(&tampered, 56, &mut scratch) {
            Ok(address) => assert_ne!(address.as_slice(), golden_sender.as_slice()),
            Err(e) => assert_eq!(e, SignerError::RecoveryFailed),
        }
    }

    #[test]
    fn invalid_secret_and_zero_scalars_are_errors() {
        assert_eq!(
            Signer::from_secret_bytes(&[0u8; 32]).err(),
            Some(SignerError::InvalidSecretKey)
        );
        assert_eq!(
            Signer::from_secret_bytes(&[0xffu8; 32]).err(),
            Some(SignerError::InvalidSecretKey)
        );

        let zero = RecoverableSignature {
            r: [0u8; 32],
            s: [0u8; 32],
            y_parity: false,
        };
        assert_eq!(
            recover_signer(&B256::ZERO, &zero),
            Err(SignerError::InvalidSignature)
        );
    }

    #[test]
    fn debug_output_never_contains_key_material() {
        let signer = Signer::from_secret_bytes(&[0x22u8; 32]).unwrap();
        let printed = format!("{signer:?}");
        assert!(printed.contains("address"));
        assert!(!printed.contains("2222222222"));
        assert!(!printed.to_lowercase().contains("key"));
    }

    #[test]
    fn sign_legacy_replaces_the_buffer_and_leaves_it_empty_on_error() {
        let calldata = unhex(FIXTURE_CALLDATA);
        let signer = Signer::from_secret_bytes(&[0x22u8; 32]).unwrap();
        let mut scratch = Vec::new();

        // Stale bytes of an earlier transaction never survive.
        let mut out = vec![0xeeu8; 500];
        signer
            .sign_legacy(&fixture_tx(&calldata, 7), &mut scratch, &mut out)
            .unwrap();
        let signed = decode_signed(&out).unwrap();
        assert_eq!(signed.tx.nonce, 7);

        signer
            .sign_legacy(&fixture_tx(&calldata, 8), &mut scratch, &mut out)
            .unwrap();
        let signed = decode_signed(&out).unwrap();
        assert_eq!(signed.tx.nonce, 8);

        let mut tx = fixture_tx(&calldata, 9);
        tx.chain_id = u64::MAX;
        assert_eq!(
            signer.sign_legacy(&tx, &mut scratch, &mut out),
            Err(SignerError::Tx(TxError::ChainIdOutOfRange))
        );
        assert!(out.is_empty());
    }

    #[test]
    fn reserved_buffers_never_grow_while_signing() {
        let calldata = unhex(FIXTURE_CALLDATA);
        let signer = Signer::from_secret_bytes(&[0x33u8; 32]).unwrap();

        let mut scratch = Vec::with_capacity(max_signing_payload_len(calldata.len()));
        let mut out = Vec::with_capacity(max_signed_len(calldata.len()));
        let scratch_capacity = scratch.capacity();
        let out_capacity = out.capacity();
        let scratch_ptr = scratch.as_ptr();
        let out_ptr = out.as_ptr();

        for nonce in 0..1024u64 {
            let tx = fixture_tx(&calldata, nonce);
            let hash = signer.sign_legacy(&tx, &mut scratch, &mut out).unwrap();
            assert_eq!(hash, tx_hash(&out));
            assert_eq!(scratch.capacity(), scratch_capacity);
            assert_eq!(out.capacity(), out_capacity);
            assert_eq!(scratch.as_ptr(), scratch_ptr);
            assert_eq!(out.as_ptr(), out_ptr);
        }
    }
}
