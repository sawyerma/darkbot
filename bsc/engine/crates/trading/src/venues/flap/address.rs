//! Token-address derivation: EIP-1167 creation code and EIP-1014 CREATE2.
//!
//! A `TOKEN_TAXED_V3` token is an EIP-1167 minimal proxy of the tax-token
//! implementation, deployed by the Portal with CREATE2 (EIP-1014):
//!
//! ```text
//! init_code = 3d602d80600a3d3981f3363d3d373d3d3d363d73
//!             ‖ implementation (20 bytes)
//!             ‖ 5af43d82803e903d91602b57fd5bf3                    (55 bytes)
//! token     = keccak256(0xff ‖ portal (20) ‖ salt (32)
//!                       ‖ keccak256(init_code))[12..]   (85-byte preimage)
//! ```
//!
//! The deployer enters the preimage as exactly 20 bytes. The historical
//! verifier defect (G0.2 correction) padded it to 32 bytes, producing a
//! 97-byte preimage and a wrong address; the types here make that
//! impossible and a regression test pins the wrong address.

use alloy_primitives::{Address, B256, keccak256};

/// EIP-1167 minimal-proxy creation code before the implementation address.
pub const MINIMAL_PROXY_PREFIX: [u8; 20] = [
    0x3d, 0x60, 0x2d, 0x80, 0x60, 0x0a, 0x3d, 0x39, 0x81, 0xf3, 0x36, 0x3d, 0x3d, 0x37, 0x3d, 0x3d,
    0x3d, 0x36, 0x3d, 0x73,
];

/// EIP-1167 minimal-proxy creation code after the implementation address.
pub const MINIMAL_PROXY_SUFFIX: [u8; 15] = [
    0x5a, 0xf4, 0x3d, 0x82, 0x80, 0x3e, 0x90, 0x3d, 0x91, 0x60, 0x2b, 0x57, 0xfd, 0x5b, 0xf3,
];

/// Size of the minimal-proxy creation code.
pub const MINIMAL_PROXY_INIT_CODE_LEN: usize = 55;

/// Size of the CREATE2 preimage.
pub const CREATE2_PREIMAGE_LEN: usize = 85;

/// Creation code of the EIP-1167 minimal proxy of `implementation`.
pub fn minimal_proxy_init_code(implementation: Address) -> [u8; MINIMAL_PROXY_INIT_CODE_LEN] {
    let mut code = [0u8; MINIMAL_PROXY_INIT_CODE_LEN];
    code[..20].copy_from_slice(&MINIMAL_PROXY_PREFIX);
    code[20..40].copy_from_slice(implementation.as_slice());
    code[40..].copy_from_slice(&MINIMAL_PROXY_SUFFIX);
    code
}

/// The EIP-1014 preimage `0xff ‖ deployer ‖ salt ‖ init_code_hash`.
pub fn create2_preimage(
    deployer: Address,
    salt: B256,
    init_code_hash: B256,
) -> [u8; CREATE2_PREIMAGE_LEN] {
    let mut preimage = [0u8; CREATE2_PREIMAGE_LEN];
    preimage[0] = 0xff;
    preimage[1..21].copy_from_slice(deployer.as_slice());
    preimage[21..53].copy_from_slice(salt.as_slice());
    preimage[53..].copy_from_slice(init_code_hash.as_slice());
    preimage
}

/// EIP-1014 CREATE2 address. No allocation.
pub fn create2_address(deployer: Address, salt: B256, init_code_hash: B256) -> Address {
    let digest = keccak256(create2_preimage(deployer, salt, init_code_hash));
    Address::from_slice(&digest.as_slice()[12..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transactions::legacy::testdata::unhex;
    use crate::venues::flap::deployment::bsc_mainnet_v5_24_0;

    /// Salt of the historical launch in block 124415357.
    const HISTORICAL_SALT: &str =
        "0c5ae0f6ffd0a19acfb501ad8eb95542ce1ee50305987db5b6d388537a128e2c";
    /// Token created by that launch (`TokenCreated` event).
    const HISTORICAL_TOKEN: &str = "abe1fe16cf6bc48c9524713054efe6e954b17777";

    /// `deadbeef` repeated eleven times (EIP-1014 example 6).
    const INIT_CODE_44_BYTES: &str = concat!(
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
    );

    fn address(hex: &str) -> Address {
        Address::from_slice(&unhex(hex))
    }

    fn word(hex: &str) -> B256 {
        B256::from_slice(&unhex(hex))
    }

    #[test]
    fn eip1014_published_vectors() {
        // (deployer, salt, init code, expected address) from EIP-1014.
        let vectors: [(&str, &str, &str, &str); 7] = [
            (
                "0000000000000000000000000000000000000000",
                "0000000000000000000000000000000000000000000000000000000000000000",
                "00",
                "4d1a2e2bb4f88f0250f26ffff098b0b30b26bf38",
            ),
            (
                "deadbeef00000000000000000000000000000000",
                "0000000000000000000000000000000000000000000000000000000000000000",
                "00",
                "b928f69bb1d91cd65274e3c79d8986362984fda3",
            ),
            (
                "deadbeef00000000000000000000000000000000",
                "000000000000000000000000feed000000000000000000000000000000000000",
                "00",
                "d04116cdd17bebe565eb2422f2497e06cc1c9833",
            ),
            (
                "0000000000000000000000000000000000000000",
                "0000000000000000000000000000000000000000000000000000000000000000",
                "deadbeef",
                "70f2b2914a2a4b783faefb75f459a580616fcb5e",
            ),
            (
                "00000000000000000000000000000000deadbeef",
                "00000000000000000000000000000000000000000000000000000000cafebabe",
                "deadbeef",
                "60f3f640a8508fc6a86d45df051962668e1e8ac7",
            ),
            (
                "00000000000000000000000000000000deadbeef",
                "00000000000000000000000000000000000000000000000000000000cafebabe",
                INIT_CODE_44_BYTES,
                "1d8bfdc5d46dc4f61d6b6115972536ebe6a8854c",
            ),
            (
                "0000000000000000000000000000000000000000",
                "0000000000000000000000000000000000000000000000000000000000000000",
                "",
                "e33c0c7f7df4809055c3eba6c09cfe4baf1bd9e0",
            ),
        ];
        for (deployer, salt, init_code, expected) in vectors {
            let init_code_hash = keccak256(unhex(init_code));
            assert_eq!(
                create2_address(address(deployer), word(salt), init_code_hash),
                address(expected),
                "deployer {deployer} salt {salt}"
            );
        }
    }

    #[test]
    fn deployer_padded_to_32_bytes_is_the_recorded_wrong_address() {
        // G0.2 correction: the old verifier hashed a 97-byte preimage with
        // the deployer left-padded to 32 bytes.
        let deployment = bsc_mainnet_v5_24_0();
        let correct = create2_preimage(
            deployment.portal,
            word(HISTORICAL_SALT),
            deployment.tax_v3_init_code_hash,
        );
        assert_eq!(correct.len(), 85);
        assert_eq!(
            Address::from_slice(&keccak256(correct).as_slice()[12..]),
            address(HISTORICAL_TOKEN)
        );

        let mut wrong = vec![0xffu8];
        wrong.extend_from_slice(&[0u8; 12]);
        wrong.extend_from_slice(&correct[1..]);
        assert_eq!(wrong.len(), 97);
        let wrong_address = Address::from_slice(&keccak256(wrong).as_slice()[12..]);
        assert_eq!(
            wrong_address,
            address("af5b39fd975d2f1ea9d89ecd8d09676155cb818a")
        );
        assert_ne!(wrong_address, address(HISTORICAL_TOKEN));
    }

    #[test]
    fn wrong_implementation_init_code_or_deployer_yields_another_address() {
        let deployment = bsc_mainnet_v5_24_0();
        let salt = word(HISTORICAL_SALT);
        let token = address(HISTORICAL_TOKEN);

        // Another implementation inside otherwise identical creation code.
        let other_implementation = address("e6ff967a887084c16d0fd71548cf709542cc1557");
        let other_hash = keccak256(minimal_proxy_init_code(other_implementation));
        assert_ne!(other_hash, deployment.tax_v3_init_code_hash);
        assert_ne!(create2_address(deployment.portal, salt, other_hash), token);

        // One flipped bit in the init-code hash.
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(deployment.tax_v3_init_code_hash.as_slice());
        bytes[31] ^= 0x01;
        assert_ne!(
            create2_address(deployment.portal, salt, B256::from_slice(&bytes)),
            token
        );

        // Another deployer.
        assert_ne!(
            create2_address(other_implementation, salt, deployment.tax_v3_init_code_hash),
            token
        );
    }
}
