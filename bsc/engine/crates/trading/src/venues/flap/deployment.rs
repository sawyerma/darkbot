//! Witnessed Flap deployments and their startup validation.
//!
//! A [`FlapDeployment`] is a set of protocol constants recorded by Gate-0
//! evidence. It is versioned in code and is not operating configuration.
//! It is also not proof that the chain still looks like that: before any
//! launch may be classified, the runner reads the live deployment once,
//! outside the entry path, and [`FlapDeployment::validate`] compares it with
//! the witnessed constants. Only the resulting [`ValidatedDeployment`]
//! unlocks classification and admission, and it carries the code
//! generation under which it was validated.
//!
//! # Fingerprints
//!
//! The `*_runtime_sha256` values are SHA-256 digests of runtime bytecode as
//! recorded by the G0.6a read-only preflight and EVM crosscheck. They are
//! evidence fingerprints. They are **not** the EVM `EXTCODEHASH`, which is
//! Keccak-256. The token init-code hash is a real Keccak-256 because CREATE2
//! consumes it.
//!
//! # Scope
//!
//! The bound code set covers the native-BNB path: Portal proxy, Portal
//! implementation and tax-token implementation. The additional contracts
//! of the DOGE conversion path (router, converter pool, quote token) are
//! not bound here; that profile is disabled at runtime.

use alloy_primitives::{Address, B256, keccak256};

use super::address::{create2_address, minimal_proxy_init_code};

/// SHA-256 of a contract's runtime bytecode. An evidence fingerprint, not
/// the EVM `EXTCODEHASH`.
pub type RuntimeSha256 = [u8; 32];

/// A witnessed Flap deployment for the `TOKEN_TAXED_V3` launch mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlapDeployment {
    /// EIP-155 chain id.
    pub chain_id: u64,
    /// Portal proxy: launch and BUY target, CREATE2 deployer of tokens.
    pub portal: Address,
    /// Version string reported by the Portal.
    pub portal_version: &'static str,
    /// Fingerprint of the Portal proxy's runtime code.
    pub portal_runtime_sha256: RuntimeSha256,
    /// Implementation the Portal proxy delegates to.
    pub portal_implementation: Address,
    /// Fingerprint of the Portal implementation's runtime code.
    pub portal_implementation_runtime_sha256: RuntimeSha256,
    /// Tax-token V3 implementation that new tokens proxy to.
    pub tax_v3_implementation: Address,
    /// Fingerprint of the tax-token implementation's runtime code.
    pub tax_v3_implementation_runtime_sha256: RuntimeSha256,
    /// Keccak-256 of the token creation code (EIP-1167 proxy of the
    /// tax-token implementation).
    pub tax_v3_init_code_hash: B256,
}

/// The live deployment as read by the runner at startup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservedDeployment<'a> {
    /// Chain id reported by the node.
    pub chain_id: u64,
    /// Version string reported by the Portal.
    pub portal_version: &'a str,
    /// SHA-256 of the Portal proxy's runtime code.
    pub portal_runtime_sha256: RuntimeSha256,
    /// Implementation address stored in the proxy.
    pub portal_implementation: Address,
    /// SHA-256 of that implementation's runtime code.
    pub portal_implementation_runtime_sha256: RuntimeSha256,
    /// SHA-256 of the tax-token implementation's runtime code.
    pub tax_v3_implementation_runtime_sha256: RuntimeSha256,
}

/// First difference between the live deployment and the witnessed one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeploymentError {
    /// Another chain.
    ChainId,
    /// Another Portal version.
    PortalVersion,
    /// The Portal proxy code changed.
    PortalCode,
    /// The proxy points to another implementation.
    PortalImplementation,
    /// The Portal implementation code changed.
    PortalImplementationCode,
    /// The tax-token implementation code changed.
    TaxImplementationCode,
}

impl core::fmt::Display for DeploymentError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::ChainId => "flap deployment: unexpected chain id",
            Self::PortalVersion => "flap deployment: unexpected Portal version",
            Self::PortalCode => "flap deployment: Portal proxy code differs",
            Self::PortalImplementation => "flap deployment: Portal implementation differs",
            Self::PortalImplementationCode => "flap deployment: Portal implementation code differs",
            Self::TaxImplementationCode => "flap deployment: tax-token code differs",
        })
    }
}

impl std::error::Error for DeploymentError {}

/// A deployment whose live code matched the witnessed constants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedDeployment {
    deployment: FlapDeployment,
    codehash_generation: u64,
}

impl ValidatedDeployment {
    /// The validated constants.
    #[inline]
    pub fn deployment(&self) -> &FlapDeployment {
        &self.deployment
    }

    /// Code generation under which the validation happened. A later code
    /// observation gets a new generation and invalidates everything bound
    /// to this one.
    #[inline]
    pub fn codehash_generation(&self) -> u64 {
        self.codehash_generation
    }
}

impl FlapDeployment {
    /// Compares the live deployment with the witnessed constants.
    ///
    /// `codehash_generation` identifies this observation; it is assigned by
    /// the owner of the state snapshot.
    pub fn validate(
        &self,
        observed: &ObservedDeployment<'_>,
        codehash_generation: u64,
    ) -> Result<ValidatedDeployment, DeploymentError> {
        if observed.chain_id != self.chain_id {
            return Err(DeploymentError::ChainId);
        }
        if observed.portal_version != self.portal_version {
            return Err(DeploymentError::PortalVersion);
        }
        if observed.portal_runtime_sha256 != self.portal_runtime_sha256 {
            return Err(DeploymentError::PortalCode);
        }
        if observed.portal_implementation != self.portal_implementation {
            return Err(DeploymentError::PortalImplementation);
        }
        if observed.portal_implementation_runtime_sha256
            != self.portal_implementation_runtime_sha256
        {
            return Err(DeploymentError::PortalImplementationCode);
        }
        if observed.tax_v3_implementation_runtime_sha256
            != self.tax_v3_implementation_runtime_sha256
        {
            return Err(DeploymentError::TaxImplementationCode);
        }
        Ok(ValidatedDeployment {
            deployment: *self,
            codehash_generation,
        })
    }

    /// Address of the `TOKEN_TAXED_V3` token deployed with `salt`.
    #[inline]
    pub fn taxed_v3_token(&self, salt: B256) -> Address {
        create2_address(self.portal, salt, self.tax_v3_init_code_hash)
    }
}

/// BSC mainnet (chain 56), Portal v5.24.0.
///
/// Witnesses: G0.6a read-only preflight (block 125351041) for the proxy and
/// tax-token fingerprints, G0.6a EVM crosscheck (block 125373566) for the
/// version, the implementation address and its fingerprint, G0.2/G0.3 for
/// the token derivation.
pub fn bsc_mainnet_v5_24_0() -> FlapDeployment {
    let tax_v3_implementation = Address::from_slice(&[
        0x02, 0x4f, 0x18, 0x29, 0x49, 0x70, 0xb5, 0xc7, 0x6c, 0x06, 0x91, 0xb8, 0x7f, 0x13, 0x8a,
        0x03, 0x17, 0x15, 0x64, 0x22,
    ]);
    FlapDeployment {
        chain_id: 56,
        portal: Address::from_slice(&[
            0xe2, 0xce, 0x6a, 0xb8, 0x08, 0x74, 0xfa, 0x9f, 0xa2, 0xaa, 0xe6, 0x5d, 0x27, 0x7d,
            0xd6, 0xb8, 0xe6, 0x5c, 0x9d, 0xe0,
        ]),
        portal_version: "v5.24.0",
        portal_runtime_sha256: [
            0x1f, 0xc3, 0x65, 0xbf, 0xda, 0x5c, 0x96, 0x94, 0xd3, 0x89, 0x01, 0x22, 0x79, 0x27,
            0x87, 0x2e, 0x61, 0x0f, 0x84, 0xd9, 0xde, 0x9b, 0x4e, 0x36, 0x0f, 0xea, 0x40, 0x57,
            0x8f, 0xb4, 0xbc, 0xc1,
        ],
        portal_implementation: Address::from_slice(&[
            0x27, 0xa0, 0xf9, 0xda, 0x7f, 0x5b, 0xea, 0xd6, 0x16, 0xfe, 0x0b, 0xf0, 0x2b, 0xf0,
            0xc7, 0x93, 0xbe, 0xce, 0x4e, 0x20,
        ]),
        portal_implementation_runtime_sha256: [
            0x2a, 0xc8, 0x93, 0xfc, 0xa8, 0xdf, 0xa7, 0x30, 0xd6, 0x6c, 0xb4, 0xf0, 0xfc, 0x3e,
            0x2c, 0x76, 0x9f, 0x2f, 0xfd, 0xb4, 0x56, 0x5e, 0x55, 0x90, 0x3d, 0x9f, 0xa1, 0x2e,
            0x7c, 0xd7, 0x14, 0x9f,
        ],
        tax_v3_implementation,
        tax_v3_implementation_runtime_sha256: [
            0x34, 0x00, 0x0d, 0xd2, 0x28, 0x05, 0x56, 0x64, 0xfd, 0xa2, 0xde, 0xbd, 0x21, 0xb5,
            0xfe, 0x52, 0x88, 0xb7, 0x50, 0xca, 0x3c, 0xe8, 0xfb, 0x4e, 0x69, 0x33, 0xe5, 0xc5,
            0x29, 0x58, 0x6e, 0xf5,
        ],
        tax_v3_init_code_hash: keccak256(minimal_proxy_init_code(tax_v3_implementation)),
    }
}

#[cfg(test)]
pub(crate) mod testkit {
    //! Validated mainnet deployment for tests of dependent modules.

    use super::*;

    /// The observation that matches the witnessed mainnet deployment.
    pub fn matching_observation() -> ObservedDeployment<'static> {
        let deployment = bsc_mainnet_v5_24_0();
        ObservedDeployment {
            chain_id: deployment.chain_id,
            portal_version: deployment.portal_version,
            portal_runtime_sha256: deployment.portal_runtime_sha256,
            portal_implementation: deployment.portal_implementation,
            portal_implementation_runtime_sha256: deployment.portal_implementation_runtime_sha256,
            tax_v3_implementation_runtime_sha256: deployment.tax_v3_implementation_runtime_sha256,
        }
    }

    /// Mainnet deployment validated under `codehash_generation`.
    pub fn validated_mainnet(codehash_generation: u64) -> ValidatedDeployment {
        bsc_mainnet_v5_24_0()
            .validate(&matching_observation(), codehash_generation)
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;
    use crate::transactions::legacy::testdata::unhex;

    fn address(hex: &str) -> Address {
        Address::from_slice(&unhex(hex))
    }

    fn word(hex: &str) -> B256 {
        B256::from_slice(&unhex(hex))
    }

    #[test]
    fn mainnet_constants_match_the_recorded_evidence() {
        let deployment = bsc_mainnet_v5_24_0();
        assert_eq!(deployment.chain_id, 56);
        assert_eq!(
            deployment.portal,
            address("e2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0")
        );
        assert_eq!(deployment.portal_version, "v5.24.0");
        // g06a_preflight_readonly.json `portal_code_sha256`.
        assert_eq!(
            deployment.portal_runtime_sha256.as_slice(),
            unhex("1fc365bfda5c9694d38901227927872e610f84d9de9b4e360fea40578fb4bcc1").as_slice()
        );
        // g06a_economics_evm_crosscheck `portal.implementation` and
        // `portal.implementation_code_sha256`.
        assert_eq!(
            deployment.portal_implementation,
            address("27a0f9da7f5bead616fe0bf02bf0c793bece4e20")
        );
        assert_eq!(
            deployment.portal_implementation_runtime_sha256.as_slice(),
            unhex("2ac893fca8dfa730d66cb4f0fc3e2c769f2ffdb4565e55903d9fa12e7cd7149f").as_slice()
        );
        // g06a_preflight_readonly.json `tax_v3_impl` and
        // `tax_v3_impl_code_sha256`.
        assert_eq!(
            deployment.tax_v3_implementation,
            address("024f18294970b5c76c0691b87f138a0317156422")
        );
        assert_eq!(
            deployment.tax_v3_implementation_runtime_sha256.as_slice(),
            unhex("34000dd228055664fda2debd21b5fe5288b750ca3ce8fb4e6933e5c529586ef5").as_slice()
        );
    }

    #[test]
    fn token_creation_code_and_hash_match_the_recorded_evidence() {
        let deployment = bsc_mainnet_v5_24_0();
        // Creation code literal of Gate-0 `g03`.
        assert_eq!(
            minimal_proxy_init_code(deployment.tax_v3_implementation).as_slice(),
            unhex(concat!(
                "3d602d80600a3d3981f3363d3d373d3d3d363d73",
                "024f18294970b5c76c0691b87f138a0317156422",
                "5af43d82803e903d91602b57fd5bf3",
            ))
            .as_slice()
        );
        // `init_code_hash` of flap_g02_verified_correction.json.
        assert_eq!(
            deployment.tax_v3_init_code_hash,
            word("2f7f413fcc6c3812c665c15bd4a012e663f567d626112a81d401066fd5a771b4")
        );
    }

    #[test]
    fn historical_and_simulated_mainnet_tokens_are_reproduced() {
        let deployment = bsc_mainnet_v5_24_0();
        // Launch in block 124415357 (`TokenCreated` event).
        assert_eq!(
            deployment.taxed_v3_token(word(
                "0c5ae0f6ffd0a19acfb501ad8eb95542ce1ee50305987db5b6d388537a128e2c"
            )),
            address("abe1fe16cf6bc48c9524713054efe6e954b17777")
        );
        // G0.6a EVM crosscheck: mainnet `eth_call` of the launch returned
        // this token for this salt.
        assert_eq!(
            deployment.taxed_v3_token(word(
                "09c4b20c7956530d4bf91b3569ed173abfce47d16d412d94dde5e6f4034c6eeb"
            )),
            address("e51c4deb1038afcd95ef12d4a649cdbcf6fa7777")
        );
    }

    #[test]
    fn captured_pending_launches_predict_the_tax_token_suffix() {
        // Salts of the five V6 launches in the G0.4 pending captures. Their
        // created addresses are not part of the captures, so this pins only
        // the venue's vanity suffix 0x7777 (chance 2^-16 each), not a
        // witnessed address.
        let deployment = bsc_mainnet_v5_24_0();
        for salt in [
            "7c5f4edf381d7a86610613b0385f86a73664a0443c110a865e28813d2c7aafaa",
            "50c1d10299f0b55a9ba364634b25a71a7ca6b62bfad6031d37356fd850c98d04",
            "4f831125de154f5eff09d256fc572b8700075b42e7e653d8c9efc4ca7f8f92a0",
            "ad21b54c7c13d09f87eecfb5d2c6666830e3c7979d51fae67652b12b1b79f8c2",
            "377cc8d2a320bd5284fdc98daa86918096aa622ce44f37caded43a79ee116f91",
        ] {
            let token = deployment.taxed_v3_token(word(salt));
            assert_eq!(
                &token.as_slice()[18..],
                [0x77u8, 0x77].as_slice(),
                "salt {salt}"
            );
        }
    }

    #[test]
    fn matching_live_deployment_validates_and_carries_its_generation() {
        let validated = bsc_mainnet_v5_24_0()
            .validate(&matching_observation(), 7)
            .unwrap();
        assert_eq!(validated.codehash_generation(), 7);
        assert_eq!(*validated.deployment(), bsc_mainnet_v5_24_0());
    }

    #[test]
    fn every_deviation_of_the_live_deployment_is_rejected() {
        let deployment = bsc_mainnet_v5_24_0();
        let good = matching_observation();

        let mut observed = good;
        observed.chain_id = 97;
        assert_eq!(
            deployment.validate(&observed, 1),
            Err(DeploymentError::ChainId)
        );

        let mut observed = good;
        observed.portal_version = "v5.24.1";
        assert_eq!(
            deployment.validate(&observed, 1),
            Err(DeploymentError::PortalVersion)
        );

        let mut observed = good;
        observed.portal_runtime_sha256[0] ^= 0x01;
        assert_eq!(
            deployment.validate(&observed, 1),
            Err(DeploymentError::PortalCode)
        );

        let mut observed = good;
        observed.portal_implementation = deployment.portal;
        assert_eq!(
            deployment.validate(&observed, 1),
            Err(DeploymentError::PortalImplementation)
        );

        let mut observed = good;
        observed.portal_implementation_runtime_sha256[31] ^= 0x01;
        assert_eq!(
            deployment.validate(&observed, 1),
            Err(DeploymentError::PortalImplementationCode)
        );

        let mut observed = good;
        observed.tax_v3_implementation_runtime_sha256[15] ^= 0x01;
        assert_eq!(
            deployment.validate(&observed, 1),
            Err(DeploymentError::TaxImplementationCode)
        );
    }
}
