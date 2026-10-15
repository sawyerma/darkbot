//! EIP-155 legacy (type 0) transaction encoding and decoding.
//!
//! Consolidates the transaction-assembly half of the four Gate-0 `sign_legacy`
//! copies and the `recover_sender` field decoder of the G0.6a bundle harness.
//! Output bytes are identical to the Gate-0 encoders; the test module pins
//! them to the canonical BSC mainnet transaction used as G0.3 evidence.
//!
//! # Scope
//!
//! [`LegacyTx`] is DarkBot's own outgoing BSC contract call. It is not a
//! general Ethereum wire model and not the ingress representation of foreign
//! transactions:
//!
//! - `to` is mandatory (no contract creation);
//! - `nonce`, `gas_price` and `gas_limit` are `u64`, as in the Gate-0
//!   encoders (`gas_price` up to about 18.4 BNB per gas unit);
//! - typed transactions (EIP-2930 / EIP-1559 / EIP-4844 / EIP-7702) are not
//!   represented; [`decode_signed`] rejects them explicitly.
//!
//! Foreign transactions enter the runtime as envelope-agnostic native events
//! that carry their original signed bytes; they are never forced through this
//! type.
//!
//! # Signed bytes
//!
//! Raw signed bytes are produced only by [`crate::signer::Signer::sign_legacy`],
//! which signs and encodes the same transaction in one call. The encoder that
//! accepts caller-supplied signature fields is crate-private, so no code
//! outside this crate can pair a transaction with an arbitrary signature.
//!
//! [`RecoverableSignature`] with public fields is the unvalidated wire
//! representation returned by the decoder. Well-formed RLP says nothing about
//! signature validity, chain scope or sender; those are established by
//! [`crate::signer::recover_legacy_sender`].

use alloy_primitives::{Address, B256, U256, keccak256};
use darkbot_wire::rlp;

/// Signature fields of a transaction as they appear on the wire: `r` and `s`
/// as 32-byte big-endian scalars and the recovery parity.
///
/// Unvalidated. The fields say nothing about whether the scalars are in
/// range, low-`s`, or belong to the transaction they travel with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoverableSignature {
    /// Scalar `r`.
    pub r: [u8; 32],
    /// Scalar `s`.
    pub s: [u8; 32],
    /// Parity of the y coordinate of the ephemeral public key.
    pub y_parity: bool,
}

/// Unsigned EIP-155 legacy transaction that calls a contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyTx<'a> {
    /// EIP-155 chain id (BSC mainnet 56, Chapel 97).
    pub chain_id: u64,
    /// Sender account nonce.
    pub nonce: u64,
    /// Gas price in wei.
    pub gas_price: u64,
    /// Gas limit.
    pub gas_limit: u64,
    /// Called contract.
    pub to: Address,
    /// Native value in wei.
    pub value: U256,
    /// Calldata.
    pub input: &'a [u8],
}

/// A legacy transaction together with its signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignedLegacyTx<'a> {
    /// The signed fields.
    pub tx: LegacyTx<'a>,
    /// The signature over [`LegacyTx::signing_hash`].
    pub signature: RecoverableSignature,
}

/// Reason a transaction cannot be encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxError {
    /// `35 + 2 * chain_id + y_parity` does not fit in a `u64`.
    ChainIdOutOfRange,
}

impl core::fmt::Display for TxError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::ChainIdOutOfRange => "tx: chain id too large for an EIP-155 v value",
        })
    }
}

impl std::error::Error for TxError {}

/// Reason raw bytes are not a canonical signed EIP-155 legacy call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyDecodeError {
    /// The RLP structure is malformed, non-canonical or of another
    /// transaction type (typed transactions do not start with a list).
    Rlp(rlp::DecodeError),
    /// `to` is not a 20-byte address (for example a contract creation).
    BadAddressLength,
    /// `v` is below 35: a pre-EIP-155 signature without chain id.
    NotEip155,
}

impl From<rlp::DecodeError> for LegacyDecodeError {
    fn from(e: rlp::DecodeError) -> Self {
        Self::Rlp(e)
    }
}

impl core::fmt::Display for LegacyDecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Rlp(e) => write!(f, "legacy tx: {e}"),
            Self::BadAddressLength => f.write_str("legacy tx: `to` is not a 20-byte address"),
            Self::NotEip155 => f.write_str("legacy tx: signature carries no EIP-155 chain id"),
        }
    }
}

impl std::error::Error for LegacyDecodeError {}

/// Upper bound of the encoded size of everything except the calldata bytes
/// in a signing payload: list prefix (9) + nonce (9) + gas price (9) +
/// gas limit (9) + `to` (21) + value (33) + calldata prefix (9) +
/// chain id (9) + two empty items (2).
const SIGNING_PAYLOAD_OVERHEAD_MAX: usize = 110;

/// Upper bound of the encoded size of everything except the calldata bytes
/// in a signed transaction: list prefix (9) + nonce (9) + gas price (9) +
/// gas limit (9) + `to` (21) + value (33) + calldata prefix (9) + v (9) +
/// r (33) + s (33).
const SIGNED_OVERHEAD_MAX: usize = 174;

/// Upper bound of the signing-payload size for `input_len` bytes of
/// calldata. Reserving this once keeps [`LegacyTx::signing_hash`]
/// allocation-free.
#[inline]
pub const fn max_signing_payload_len(input_len: usize) -> usize {
    input_len.saturating_add(SIGNING_PAYLOAD_OVERHEAD_MAX)
}

/// Upper bound of the raw signed size for `input_len` bytes of calldata.
/// Reserving this once keeps signing allocation-free.
#[inline]
pub const fn max_signed_len(input_len: usize) -> usize {
    input_len.saturating_add(SIGNED_OVERHEAD_MAX)
}

#[inline]
fn eip155_v(chain_id: u64, y_parity: bool) -> Result<u64, TxError> {
    chain_id
        .checked_mul(2)
        .and_then(|x| x.checked_add(35 + u64::from(y_parity)))
        .ok_or(TxError::ChainIdOutOfRange)
}

#[inline]
fn left_pad_32(be: &[u8]) -> [u8; 32] {
    // Callers pass at most 32 bytes (checked by `rlp::canonical_uint`).
    let mut out = [0u8; 32];
    let start = 32usize.saturating_sub(be.len());
    let take = be.len().min(32);
    out[start..].copy_from_slice(&be[be.len() - take..]);
    out
}

impl LegacyTx<'_> {
    /// Encoded length of the six fields shared by the signing payload and the
    /// signed transaction.
    #[inline]
    fn common_len(&self) -> usize {
        let value = self.value.to_be_bytes::<32>();
        rlp::u64_encoded_len(self.nonce)
            + rlp::u64_encoded_len(self.gas_price)
            + rlp::u64_encoded_len(self.gas_limit)
            + rlp::bytes_encoded_len(self.to.as_slice())
            + rlp::uint_be_encoded_len(&value)
            + rlp::bytes_encoded_len(self.input)
    }

    #[inline]
    fn put_common(&self, out: &mut Vec<u8>) {
        let value = self.value.to_be_bytes::<32>();
        rlp::put_u64(out, self.nonce);
        rlp::put_u64(out, self.gas_price);
        rlp::put_u64(out, self.gas_limit);
        rlp::put_bytes(out, self.to.as_slice());
        rlp::put_uint_be(out, &value);
        rlp::put_bytes(out, self.input);
    }

    /// Appends the EIP-155 signing payload
    /// `RLP([nonce, gasPrice, gasLimit, to, value, data, chainId, 0, 0])`.
    pub fn encode_signing_payload(&self, out: &mut Vec<u8>) {
        // The two trailing zeros encode as one byte each (0x80).
        let payload_len = self.common_len() + rlp::u64_encoded_len(self.chain_id) + 2;
        out.reserve(rlp::list_encoded_len(payload_len));
        rlp::put_list_header(out, payload_len);
        self.put_common(out);
        rlp::put_u64(out, self.chain_id);
        rlp::put_u64(out, 0);
        rlp::put_u64(out, 0);
    }

    /// Keccak-256 of the signing payload. `scratch` is cleared and reused so
    /// that a warmed buffer makes this call allocation-free.
    pub fn signing_hash(&self, scratch: &mut Vec<u8>) -> B256 {
        scratch.clear();
        self.encode_signing_payload(scratch);
        keccak256(scratch.as_slice())
    }

    /// Appends the signed transaction
    /// `RLP([nonce, gasPrice, gasLimit, to, value, data, v, r, s])`
    /// with `v = 35 + 2 * chainId + yParity`.
    ///
    /// Crate-private: the signature fields are taken as given. The only
    /// producer of signed bytes for other crates is
    /// [`crate::signer::Signer::sign_legacy`]. On error nothing is appended.
    pub(crate) fn encode_signed(
        &self,
        signature: &RecoverableSignature,
        out: &mut Vec<u8>,
    ) -> Result<(), TxError> {
        let v = eip155_v(self.chain_id, signature.y_parity)?;
        let payload_len = self.common_len()
            + rlp::u64_encoded_len(v)
            + rlp::uint_be_encoded_len(&signature.r)
            + rlp::uint_be_encoded_len(&signature.s);
        out.reserve(rlp::list_encoded_len(payload_len));
        rlp::put_list_header(out, payload_len);
        self.put_common(out);
        rlp::put_u64(out, v);
        rlp::put_uint_be(out, &signature.r);
        rlp::put_uint_be(out, &signature.s);
        Ok(())
    }
}

/// Transaction hash of a raw signed legacy transaction.
#[inline]
pub fn tx_hash(raw_signed: &[u8]) -> B256 {
    keccak256(raw_signed)
}

/// Decodes a raw signed EIP-155 legacy call without copying calldata.
///
/// Strict about form: any non-canonical encoding, any trailing byte and any
/// other transaction form is an error. A field wider than this type's scope
/// (see the module documentation) is reported as an RLP length overflow.
///
/// This validates form only. It does not verify the signature, the chain or
/// the sender; [`crate::signer::recover_legacy_sender`] does, against an
/// expected chain id.
pub fn decode_signed(raw: &[u8]) -> Result<SignedLegacyTx<'_>, LegacyDecodeError> {
    let payload = rlp::list_payload_exact(raw)?;
    let (nonce, rest) = rlp::take_bytes(payload)?;
    let (gas_price, rest) = rlp::take_bytes(rest)?;
    let (gas_limit, rest) = rlp::take_bytes(rest)?;
    let (to, rest) = rlp::take_bytes(rest)?;
    let (value, rest) = rlp::take_bytes(rest)?;
    let (input, rest) = rlp::take_bytes(rest)?;
    let (v, rest) = rlp::take_bytes(rest)?;
    let (r, rest) = rlp::take_bytes(rest)?;
    let (s, rest) = rlp::take_bytes(rest)?;
    if !rest.is_empty() {
        return Err(LegacyDecodeError::Rlp(rlp::DecodeError::TrailingBytes));
    }

    let nonce = rlp::uint_u64(nonce)?;
    let gas_price = rlp::uint_u64(gas_price)?;
    let gas_limit = rlp::uint_u64(gas_limit)?;
    if to.len() != 20 {
        return Err(LegacyDecodeError::BadAddressLength);
    }
    let value = rlp::canonical_uint(value, 32)?;
    let v = rlp::uint_u64(v)?;
    if v < 35 {
        return Err(LegacyDecodeError::NotEip155);
    }
    let r = rlp::canonical_uint(r, 32)?;
    let s = rlp::canonical_uint(s, 32)?;

    Ok(SignedLegacyTx {
        tx: LegacyTx {
            chain_id: (v - 35) / 2,
            nonce,
            gas_price,
            gas_limit,
            to: Address::from_slice(to),
            value: if value.is_empty() {
                U256::ZERO
            } else {
                U256::from_be_slice(value)
            },
            input,
        },
        signature: RecoverableSignature {
            r: left_pad_32(r),
            s: left_pad_32(s),
            y_parity: (v - 35) % 2 == 1,
        },
    })
}

#[cfg(test)]
pub(crate) mod testdata {
    //! Shared golden vectors. No secret material: the mainnet vector is a
    //! public canonical transaction; the EIP-155 key is the published
    //! specification example.

    pub fn unhex(s: &str) -> Vec<u8> {
        assert!(s.len().is_multiple_of(2), "odd hex length");
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Flap Portal (BSC mainnet), `to` of the golden transaction.
    pub const PORTAL: &str = "e2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0";

    /// Calldata of the canonical BSC mainnet Flap BUY used as G0.3 evidence:
    /// `swapExactInput((0x0, 0xabe1…7777, 0.15 BNB, 6_000_000e18, ""))`.
    pub const GOLDEN_CALLDATA: &str = concat!(
        "ef7ec2e7",
        "0000000000000000000000000000000000000000000000000000000000000020",
        "0000000000000000000000000000000000000000000000000000000000000000",
        "000000000000000000000000abe1fe16cf6bc48c9524713054efe6e954b17777",
        "0000000000000000000000000000000000000000000000000214e8348c4f0000",
        "00000000000000000000000000000000000000000004f68ca6d8cd91c6000000",
        "00000000000000000000000000000000000000000000000000000000000000a0",
        "0000000000000000000000000000000000000000000000000000000000000000",
    );
    pub const GOLDEN_NONCE: u64 = 0x032e;
    pub const GOLDEN_GAS_PRICE: u64 = 0x127a_3980; // 310_000_000 wei
    pub const GOLDEN_GAS_LIMIT: u64 = 0x16_e360; // 1_500_000
    pub const GOLDEN_VALUE_WEI: u64 = 0x0214_e834_8c4f_0000; // 0.15 BNB
    pub const GOLDEN_R: &str = "66e5778143debc8df45dd1f47d2fb5cea41d4a8c49abd312e0847a30ef4bcda0";
    pub const GOLDEN_S: &str = "6a90536f139884940cacd213f0aebf1873569889f800cbfc802056329de86771";
    pub const GOLDEN_SIGNING_HASH: &str =
        "71b74d04f2ee82b95e4f4ec6707e28c706456dd7ab066fa89e109d8d4dcce0e7";
    /// Canonical transaction hash on BSC mainnet (ledger, G0.3 step 4).
    pub const GOLDEN_TX_HASH: &str =
        "8aff738f6fff1302e057609661acf39eb2ebbf8ac13f1bd69dbc0835bb570f6b";
    /// Sender of the golden transaction.
    pub const GOLDEN_SENDER: &str = "b6864c017e9d9d89012d4f90ee382bf1461e4c90";

    /// Raw signed bytes of the golden transaction (343 bytes).
    pub fn golden_raw() -> Vec<u8> {
        let mut hex = String::new();
        hex.push_str("f9015482032e84127a39808316e36094");
        hex.push_str(PORTAL);
        hex.push_str("880214e8348c4f0000b8e4");
        hex.push_str(GOLDEN_CALLDATA);
        hex.push_str("8193a0");
        hex.push_str(GOLDEN_R);
        hex.push_str("a0");
        hex.push_str(GOLDEN_S);
        unhex(&hex)
    }

    pub fn word32(hex: &str) -> [u8; 32] {
        let bytes = unhex(hex);
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::testdata::*;
    use super::*;

    fn golden_tx(calldata: &[u8]) -> LegacyTx<'_> {
        LegacyTx {
            chain_id: 56,
            nonce: GOLDEN_NONCE,
            gas_price: GOLDEN_GAS_PRICE,
            gas_limit: GOLDEN_GAS_LIMIT,
            to: Address::from_slice(&unhex(PORTAL)),
            value: U256::from(GOLDEN_VALUE_WEI),
            input: calldata,
        }
    }

    fn golden_signature() -> RecoverableSignature {
        RecoverableSignature {
            r: word32(GOLDEN_R),
            s: word32(GOLDEN_S),
            y_parity: false,
        }
    }

    #[test]
    fn golden_mainnet_buy_signing_hash_raw_bytes_and_tx_hash() {
        let calldata = unhex(GOLDEN_CALLDATA);
        assert_eq!(calldata.len(), 228);
        let tx = golden_tx(&calldata);

        let mut scratch = Vec::new();
        let signing_hash = tx.signing_hash(&mut scratch);
        assert_eq!(
            signing_hash.as_slice(),
            unhex(GOLDEN_SIGNING_HASH).as_slice()
        );

        let mut raw = Vec::new();
        tx.encode_signed(&golden_signature(), &mut raw).unwrap();
        assert_eq!(raw.len(), 343);
        assert_eq!(raw, golden_raw());
        assert_eq!(tx_hash(&raw).as_slice(), unhex(GOLDEN_TX_HASH).as_slice());
    }

    #[test]
    fn length_bounds_cover_the_golden_transaction_and_the_widest_fields() {
        let calldata = unhex(GOLDEN_CALLDATA);
        let tx = golden_tx(&calldata);

        let mut payload = Vec::new();
        tx.encode_signing_payload(&mut payload);
        assert!(payload.len() <= max_signing_payload_len(calldata.len()));
        assert!(golden_raw().len() <= max_signed_len(calldata.len()));

        // Every field at its maximum width. 70_000 bytes of calldata
        // exercise the three-byte length form.
        let big = vec![0xffu8; 70_000];
        let widest = LegacyTx {
            chain_id: (u64::MAX - 36) / 2,
            nonce: u64::MAX,
            gas_price: u64::MAX,
            gas_limit: u64::MAX,
            to: Address::from_slice(&[0xffu8; 20]),
            value: U256::MAX,
            input: &big,
        };
        let signature = RecoverableSignature {
            r: [0xffu8; 32],
            s: [0xffu8; 32],
            y_parity: true,
        };
        let mut payload = Vec::new();
        widest.encode_signing_payload(&mut payload);
        assert!(payload.len() <= max_signing_payload_len(big.len()));
        let mut raw = Vec::new();
        widest.encode_signed(&signature, &mut raw).unwrap();
        assert!(raw.len() <= max_signed_len(big.len()));
        assert_eq!(decode_signed(&raw).unwrap().tx, widest);
    }

    #[test]
    fn golden_mainnet_buy_decodes_to_the_same_fields() {
        let raw = golden_raw();
        let calldata = unhex(GOLDEN_CALLDATA);
        let decoded = decode_signed(&raw).unwrap();
        assert_eq!(decoded.tx, golden_tx(&calldata));
        assert_eq!(decoded.signature, golden_signature());
    }

    #[test]
    fn eip155_specification_example_signing_payload() {
        // EIP-155, "Example": chain id 1, nonce 9, 20 gwei, 21000 gas,
        // to 0x3535…35, 1 ether, empty data.
        let tx = LegacyTx {
            chain_id: 1,
            nonce: 9,
            gas_price: 20_000_000_000,
            gas_limit: 21_000,
            to: Address::from_slice(&[0x35u8; 20]),
            value: U256::from(1_000_000_000_000_000_000u64),
            input: &[],
        };
        let mut payload: Vec<u8> = Vec::new();
        tx.encode_signing_payload(&mut payload);
        assert_eq!(
            payload,
            unhex(concat!(
                "ec098504a817c80082520894",
                "3535353535353535353535353535353535353535",
                "880de0b6b3a764000080018080"
            ))
        );
        let mut scratch = Vec::new();
        assert_eq!(
            tx.signing_hash(&mut scratch).as_slice(),
            unhex("daf5a779ae972f972197303d7b574746c7ef83eadac0f2791ad23db92e4c8e53").as_slice()
        );
    }

    #[test]
    fn encode_signed_appends_and_reports_chain_id_overflow_without_writing() {
        let calldata = unhex(GOLDEN_CALLDATA);
        let mut tx = golden_tx(&calldata);

        let mut out = vec![0xee, 0xee];
        tx.encode_signed(&golden_signature(), &mut out).unwrap();
        assert_eq!(&out[..2], [0xeeu8, 0xee].as_slice());
        assert_eq!(&out[2..], golden_raw().as_slice());

        tx.chain_id = u64::MAX;
        let mut untouched = vec![0xee];
        assert_eq!(
            tx.encode_signed(&golden_signature(), &mut untouched),
            Err(TxError::ChainIdOutOfRange)
        );
        assert_eq!(untouched, [0xeeu8]);
    }

    #[test]
    fn short_signature_scalars_are_encoded_without_leading_zeros() {
        let calldata = unhex(GOLDEN_CALLDATA);
        let tx = golden_tx(&calldata);
        let mut signature = golden_signature();
        signature.r[0] = 0;
        signature.s[0] = 0;
        signature.s[1] = 0;

        let mut raw = Vec::new();
        tx.encode_signed(&signature, &mut raw).unwrap();
        // 1 + 1 leading zero bytes dropped from r, 2 from s.
        assert_eq!(raw.len(), 343 - 3);
        let decoded = decode_signed(&raw).unwrap();
        assert_eq!(decoded.signature, signature);
    }

    #[test]
    fn decoder_rejects_other_transaction_forms() {
        // EIP-1559 typed transaction envelope: first byte 0x02, not a list.
        let typed: &[u8] = &[0x02, 0xc0];
        assert_eq!(
            decode_signed(typed),
            Err(LegacyDecodeError::Rlp(rlp::DecodeError::ExpectedList))
        );

        // Trailing byte after the list.
        let mut raw = golden_raw();
        raw.push(0x00);
        assert_eq!(
            decode_signed(&raw),
            Err(LegacyDecodeError::Rlp(rlp::DecodeError::TrailingBytes))
        );

        // Truncated.
        let raw = golden_raw();
        let truncated = &raw[..raw.len() - 1];
        assert_eq!(
            decode_signed(truncated),
            Err(LegacyDecodeError::Rlp(rlp::DecodeError::UnexpectedEnd))
        );

        // Pre-EIP-155 signature (v = 27) and contract creation (empty `to`).
        fn fields(to: &[u8], v: u64) -> Vec<u8> {
            let empty: &[u8] = &[];
            rlp::encode_list(&[
                rlp::encode_u64(1),
                rlp::encode_u64(1),
                rlp::encode_u64(21_000),
                rlp::encode_bytes(to),
                rlp::encode_u64(0),
                rlp::encode_bytes(empty),
                rlp::encode_u64(v),
                rlp::encode_u64(1),
                rlp::encode_u64(1),
            ])
        }
        let to = [0x35u8; 20];
        let no_to: &[u8] = &[];

        let pre_eip155 = fields(to.as_slice(), 27);
        assert_eq!(
            decode_signed(&pre_eip155),
            Err(LegacyDecodeError::NotEip155)
        );

        let creation = fields(no_to, 147);
        assert_eq!(
            decode_signed(&creation),
            Err(LegacyDecodeError::BadAddressLength)
        );

        // A gas price wider than 64 bits is outside this type's scope.
        let wide_gas_price = rlp::encode_list(&[
            rlp::encode_u64(1),
            rlp::encode_uint_be(&[0x01u8; 9]),
            rlp::encode_u64(21_000),
            rlp::encode_bytes(to.as_slice()),
            rlp::encode_u64(0),
            rlp::encode_bytes(no_to),
            rlp::encode_u64(147),
            rlp::encode_u64(1),
            rlp::encode_u64(1),
        ]);
        assert_eq!(
            decode_signed(&wide_gas_price),
            Err(LegacyDecodeError::Rlp(rlp::DecodeError::LengthOverflow))
        );

        let odd_parity = fields(to.as_slice(), 148);
        let ok = decode_signed(&odd_parity).unwrap();
        assert_eq!(ok.tx.chain_id, 56);
        assert!(ok.signature.y_parity);
        assert_eq!(ok.tx.value, U256::ZERO);
        assert!(ok.tx.input.is_empty());
    }
}
