//! The two G0.7 legacy transactions: RLP encoding, EIP-155 signing, the
//! predicted token address and the launch/BUY calldata (byte-identical to DRY).

use super::*;

pub(crate) fn rlp_bytes(b: &[u8]) -> Vec<u8> {
    if b.len() == 1 && b[0] < 0x80 {
        return b.to_vec();
    }
    if b.len() <= 55 {
        let mut out = Vec::with_capacity(1 + b.len());
        out.push(0x80 + b.len() as u8);
        out.extend_from_slice(b);
        return out;
    }
    let len_bytes = b.len().to_be_bytes();
    let first = len_bytes.iter().position(|x| *x != 0).unwrap();
    let lb = &len_bytes[first..];
    let mut out = Vec::with_capacity(1 + lb.len() + b.len());
    out.push(0xb7 + lb.len() as u8);
    out.extend_from_slice(lb);
    out.extend_from_slice(b);
    out
}

pub(crate) fn rlp_u64(v: u64) -> Vec<u8> {
    if v == 0 {
        return rlp_bytes(&[]);
    }
    let b = v.to_be_bytes();
    let first = b.iter().position(|x| *x != 0).unwrap();
    rlp_bytes(&b[first..])
}

pub(crate) fn rlp_u256(v: U256) -> Vec<u8> {
    if v == U256::ZERO {
        return rlp_bytes(&[]);
    }
    let b = v.to_be_bytes::<32>();
    let first = b.iter().position(|x| *x != 0).unwrap();
    rlp_bytes(&b[first..])
}

pub(crate) fn rlp_list(items: &[Vec<u8>]) -> Vec<u8> {
    let payload_len: usize = items.iter().map(Vec::len).sum();
    let mut payload = Vec::with_capacity(payload_len);
    for item in items {
        payload.extend_from_slice(item);
    }
    if payload.len() <= 55 {
        let mut out = Vec::with_capacity(1 + payload.len());
        out.push(0xc0 + payload.len() as u8);
        out.extend_from_slice(&payload);
        return out;
    }
    let len_bytes = payload.len().to_be_bytes();
    let first = len_bytes.iter().position(|x| *x != 0).unwrap();
    let lb = &len_bytes[first..];
    let mut out = Vec::with_capacity(1 + lb.len() + payload.len());
    out.push(0xf7 + lb.len() as u8);
    out.extend_from_slice(lb);
    out.extend_from_slice(&payload);
    out
}

pub(crate) fn sign_legacy(
    signer: &SigningKey,
    nonce: u64,
    gas_price: u64,
    gas_limit: u64,
    to: Address,
    value: U256,
    calldata: &[u8],
) -> SignedLegacy {
    let unsigned = rlp_list(&[
        rlp_u64(nonce),
        rlp_u64(gas_price),
        rlp_u64(gas_limit),
        rlp_bytes(to.as_slice()),
        rlp_u256(value),
        rlp_bytes(calldata),
        rlp_u64(CHAIN_ID),
        rlp_bytes(&[]),
        rlp_bytes(&[]),
    ]);
    let sighash = keccak256(unsigned);
    let (sig, rid) = signer
        .sign_prehash_recoverable(sighash.as_slice())
        .expect("signing failed");
    let sb = sig.to_bytes();
    let v = 35u64 + 2 * CHAIN_ID + rid.to_byte() as u64;
    let raw = rlp_list(&[
        rlp_u64(nonce),
        rlp_u64(gas_price),
        rlp_u64(gas_limit),
        rlp_bytes(to.as_slice()),
        rlp_u256(value),
        rlp_bytes(calldata),
        rlp_u64(v),
        rlp_u256(U256::from_be_slice(&sb[..32])),
        rlp_u256(U256::from_be_slice(&sb[32..])),
    ]);
    SignedLegacy {
        hash: keccak256(&raw),
        raw,
    }
}

pub(crate) fn predicted_token(portal: Address, impl_addr: Address, salt: B256) -> Address {
    let mut init = hex::decode("3d602d80600a3d3981f3363d3d373d3d3d363d73").unwrap();
    init.extend_from_slice(impl_addr.as_slice());
    init.extend_from_slice(&hex::decode("5af43d82803e903d91602b57fd5bf3").unwrap());
    let mut preimage = Vec::with_capacity(85);
    preimage.push(0xff);
    preimage.extend_from_slice(portal.as_slice());
    preimage.extend_from_slice(salt.as_slice());
    preimage.extend_from_slice(keccak256(init).as_slice());
    let digest = keccak256(preimage);
    Address::from_slice(&digest.as_slice()[12..])
}

pub(crate) fn launch_calldata(launcher: Address, salt: B256) -> Vec<u8> {
    newTokenV6Call {
        params: LaunchParams {
            name: "DarkBot G06A Self Launch".into(),
            symbol: "DBG06A".into(),
            uri: "ipfs://darkbot-g06a-dry".into(),
            dexThresh: 1,
            salt,
            migratorType: 1,
            quoteToken: Address::ZERO,
            quoteAmt: U256::ZERO,
            beneficiary: launcher,
            permit: Bytes::new(),
            extensionId: B256::ZERO,
            extensionData: Bytes::new(),
            dexId: 0,
            lpFeeProfile: 0,
            buyTax: 100,
            sellTax: 100,
            taxDuration: 31_536_000,
            antiFarmerDuration: 86_400,
            mktBps: 10_000,
            deflationBps: 0,
            dividendBps: 0,
            lpBps: 0,
            minimumShareBalance: U256::ZERO,
            dividendToken: Address::ZERO,
            commissionReceiver: Address::ZERO,
            tokenVersion: 6,
        },
    }
    .abi_encode()
}

pub(crate) fn buy_calldata(token: Address, input: U256, minimum: U256) -> Vec<u8> {
    swapExactInputCall {
        params: SwapParams {
            inputToken: Address::ZERO,
            outputToken: token,
            inputAmount: input,
            minOutputAmount: minimum,
            permit: Bytes::new(),
        },
    }
    .abi_encode()
}
