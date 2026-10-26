//! The frozen Four.meme deployment on BSC mainnet: addresses, runtime
//! fingerprints and the inputs of the token-address derivation.
//!
//! Witness: the G0.1 Four freeze at block 129842711
//! (`bsc/native/evidence/g01/four_freeze_20261026T053659Z/{classic,openfour}.dbrec`),
//! re-verified by the v3 freeze at block 129907738
//! (`four_freeze_20261026T134532Z`: the same implementations, deployers and
//! Classic template; the OpenFour presets in use over 20 000 blocks).
//! Addresses: the published BSC tables of four-meme-community/fourmeme-docs
//! (TokenManager2) and openfour-docs (Registry), the other OpenFour
//! contracts from the Registry's getters at the reference block, the
//! Classic deployer and template from the archived fixture token. Every
//! implementation is an EIP-1967 proxy target; none is source-verified
//! (Sourcify NO_MATCH, BscScan not used), so the verified ABI is UNKNOWN.
//!
//! The derivation (`token = CREATE2(deployer, salt, keccak256(EIP-1167
//! creation code of the template))`, the salt in the pending calldata)
//! reproduced 4/4 Classic and 3/3 OpenFour canonical launches of the first
//! freeze and 4/4 Classic and 6/6 OpenFour of the v3 freeze. It stays
//! valid while the deployer, the template (per OpenFour preset) and the
//! calldata layout are those frozen here; a canonical launch event that
//! names another token is a mode outside the freeze.

use alloy_primitives::{Address, B256, U256, address, b256, keccak256};

use crate::venues::flap::address::{create2_address, minimal_proxy_init_code};

/// The two Four.meme surfaces (no shared protocol assumption between them).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    /// Four Classic: TokenManager2.
    Classic,
    /// OpenFour: Core.
    OpenFour,
}

/// An OpenFour preset whose token implementation the freeze read
/// (`OpenFourRegistry.getPresetTokenImpl(presetId)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenFourPreset {
    pub preset_id: U256,
    /// The implementation every token of the preset is an EIP-1167 clone of.
    pub token_implementation: Address,
}

/// The frozen Four.meme contracts of one chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FourDeployment {
    pub chain_id: u64,
    /// Classic TokenManager2 (proxy): launch and trade calls, its events.
    pub token_manager2: Address,
    pub token_manager2_implementation: Address,
    pub token_manager2_implementation_runtime_keccak256: B256,
    /// The Classic token deployer (CREATE2) and the template its tokens
    /// clone (an immutable of the deployer's runtime).
    pub classic_token_deployer: Address,
    pub classic_token_deployer_runtime_keccak256: B256,
    pub classic_token_template: Address,
    pub classic_token_template_runtime_keccak256: B256,
    /// The one Classic `createToken` argument length the fixtures witness
    /// (bytes of `args`; 8 launches of both freezes); the salt position is
    /// proven for it only. It holds name and symbol of at most 32 bytes
    /// each: a longer name is another length, refused as an unwitnessed
    /// layout and counted, never guessed (Phase E review L4).
    pub classic_args_len: usize,
    pub openfour_registry: Address,
    /// OpenFour Core (proxy): launch and trade calls, its events.
    pub openfour_core: Address,
    pub openfour_core_implementation: Address,
    pub openfour_core_implementation_runtime_keccak256: B256,
    /// The OpenFour create deployer (proxy, CREATE2).
    pub openfour_create_deployer: Address,
    pub openfour_create_deployer_implementation: Address,
    pub openfour_create_deployer_implementation_runtime_keccak256: B256,
    /// The presets with a frozen token implementation.
    pub openfour_presets: &'static [OpenFourPreset],
}

/// Every preset in use over the v3 freeze's whole window (OpenFour launch
/// pass blocks 129886197..=129906196, WINDOW_COMPLETE: 1 + 2 + 2 + 51
/// launches), each with a direct launch whose CREATE2 derivation
/// reproduced against `getPresetTokenImpl` at block 129907738. Any other
/// preset is `PendingToken::Unknown`.
const MAINNET_PRESETS: &[OpenFourPreset] = &[
    OpenFourPreset {
        preset_id: U256::from_limbs([1_778_027_615_723, 0, 0, 0]),
        token_implementation: address!("12fee093ff5a34ec2f82730d8d9e5443bd2d56fc"),
    },
    OpenFourPreset {
        preset_id: U256::from_limbs([1_778_027_615_733, 0, 0, 0]),
        token_implementation: address!("e610cc146a94f1b04156f9857d1eb3c8626bcd18"),
    },
    OpenFourPreset {
        preset_id: U256::from_limbs([1_778_027_615_734, 0, 0, 0]),
        token_implementation: address!("509c46c832bccb0c442a4dc19b3c1bff34219268"),
    },
    OpenFourPreset {
        preset_id: U256::from_limbs([1_778_027_615_735, 0, 0, 0]),
        token_implementation: address!("c4a52a520bb379a37e44c31beb1edf7ffdc4a27d"),
    },
];

/// BSC mainnet (chain 56) as frozen at block 129842711, its presets from
/// the v3 freeze at block 129907738.
pub fn bsc_mainnet_g01_129842711() -> FourDeployment {
    FourDeployment {
        chain_id: 56,
        token_manager2: address!("5c952063c7fc8610ffdb798152d69f0b9550762b"),
        token_manager2_implementation: address!("f32ee25a7a94f5858692b90815493bda9f992eee"),
        token_manager2_implementation_runtime_keccak256: b256!(
            "07eb30d004f7e1b214a2b5c9704e2c7866648b65d1ffeb0c38fd54b5512f030e"
        ),
        classic_token_deployer: address!("6e0c4ba9ce0c901f1bba6af55cd66da3b0443ef1"),
        classic_token_deployer_runtime_keccak256: b256!(
            "666d5ae213c594d3cd4a156fd7cf84c85680db01a01e68eabee892d01ee9ce09"
        ),
        classic_token_template: address!("7330d8865f4b6800b72bdd73e2007833a5d45c94"),
        classic_token_template_runtime_keccak256: b256!(
            "49052d1fff3ba35008538f6694480d71469a0a76c82ce47d0500cab6e8775f6d"
        ),
        classic_args_len: 672,
        openfour_registry: address!("912cef0c3ae9ab6eb3ec87cab69371cfb317ab94"),
        openfour_core: address!("ebe7b6c1089d9f72ad07f34e36d898e44e5e27f3"),
        openfour_core_implementation: address!("02d714cb43d1544abe1ffc2405ec00b7235b7f26"),
        openfour_core_implementation_runtime_keccak256: b256!(
            "2e3109401d230c6d24ab3b1b28dda6fa9992afa4aed2f815724e6da65684c451"
        ),
        openfour_create_deployer: address!("041b707371469f098d11a8770782cc7bfb83254d"),
        openfour_create_deployer_implementation: address!(
            "313075646b4f989fe497ab1c0d704f7bdbac6fbb"
        ),
        openfour_create_deployer_implementation_runtime_keccak256: b256!(
            "f74aebf96dab002196896168ad3dcf2e64a3f7aa028cc27f047f647eeb6e66bd"
        ),
        openfour_presets: MAINNET_PRESETS,
    }
}

impl FourDeployment {
    /// The contract of `family` that receives its calls and emits its events.
    pub const fn contract(&self, family: Family) -> Address {
        match family {
            Family::Classic => self.token_manager2,
            Family::OpenFour => self.openfour_core,
        }
    }

    /// The family whose contract `address` is.
    pub fn family_of(&self, address: Address) -> Option<Family> {
        if address == self.token_manager2 {
            Some(Family::Classic)
        } else if address == self.openfour_core {
            Some(Family::OpenFour)
        } else {
            None
        }
    }

    /// The Classic token of a launch with `salt`, if it is a clone of the
    /// frozen template by the frozen deployer.
    pub fn classic_token(&self, salt: B256) -> Address {
        create2_address(
            self.classic_token_deployer,
            salt,
            keccak256(minimal_proxy_init_code(self.classic_token_template)),
        )
    }

    /// The frozen token implementation of an OpenFour preset.
    pub fn openfour_template(&self, preset_id: U256) -> Option<Address> {
        self.openfour_presets
            .iter()
            .find(|p| p.preset_id == preset_id)
            .map(|p| p.token_implementation)
    }

    /// The OpenFour token of a launch of `preset_id` with `token_salt`, for
    /// a preset with a frozen template; any other preset: `None`.
    pub fn openfour_token(&self, preset_id: U256, token_salt: B256) -> Option<Address> {
        self.openfour_template(preset_id).map(|template| {
            create2_address(
                self.openfour_create_deployer,
                token_salt,
                keccak256(minimal_proxy_init_code(template)),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The init-code hashes and tokens of the freeze's derivation attempts.
    #[test]
    fn the_frozen_derivation_reproduces_the_freeze() {
        let d = bsc_mainnet_g01_129842711();
        assert_eq!(
            keccak256(minimal_proxy_init_code(d.classic_token_template)),
            b256!("701a364cab6ae8867f16bae0a17fa0b01fe43e0770fa3655f31a03a54262e134")
        );
        assert_eq!(
            d.classic_token(b256!(
                "b3b9bc8c554de542fe9181f3df7867fd957debabfe4d7bf3326149b5ada7e48e"
            )),
            address!("a89a24209a3ae8ba1ffb3f0567b69ef4765bffff")
        );
        let preset = U256::from(1_778_027_615_733u64);
        assert_eq!(
            keccak256(minimal_proxy_init_code(
                d.openfour_template(preset).unwrap()
            )),
            b256!("5c575832c27ec270cc6106fc8f08fd12281d452f2254690ddc8d98f82b0e2090")
        );
        assert_eq!(
            d.openfour_token(
                preset,
                b256!("3ec074a27944090054e5c3a504cc382fbe9c716e6e9b09ec4fe3a8e1737a01e5")
            ),
            Some(address!("f76ad2f3d84ee09ee10ee1ef8c4d6dbb264effff"))
        );
        // A preset outside the frozen table (…732, none in use over the v3
        // window) has no template, so no token.
        assert_eq!(d.openfour_token(preset - U256::from(1u8), B256::ZERO), None);
        assert_eq!(d.family_of(d.token_manager2), Some(Family::Classic));
        assert_eq!(d.family_of(d.openfour_core), Some(Family::OpenFour));
        assert_eq!(d.family_of(d.openfour_registry), None);
    }
}
