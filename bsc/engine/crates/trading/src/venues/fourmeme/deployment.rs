//! The frozen Four.meme deployment on BSC mainnet: addresses, runtime
//! fingerprints and the inputs of the token-address derivation.
//!
//! Current: [`bsc_mainnet_r12d_131608421`] (R12d: the R12b deployment with
//! the witnessed OpenFour bonding preset pinned), on [`bsc_mainnet_r12b_131568366`], the R12b refreeze at block
//! 131568366 (`bsc/native/evidence/g01/four_freeze_20261104T052604Z/`,
//! over the free archive endpoint, receipts of direct calls only):
//! TokenManager2 runs implementation 0xa38d9d48 since the G0.1 freeze; the
//! other contracts' code and both Classic pairs are unchanged, the
//! window's Classic launches reproduce under both pairs (3 and 1); one
//! OpenFour preset (…735) reproduced in the window, the others are carried
//! over from the v3 freeze (a direct-calls scan cannot name every preset).
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
//!
//! Classic has two (deployer, template) pairs (Paket 8 review H1): the
//! frozen one (every frozen launch; tokens ending ffff) and a second one
//! TokenManager2 (same implementation) launches with as well, witnessed by
//! two canonical launches (tokens ending 4444) and read at block 129992928
//! (`fixtures/fourmeme/classic_template2_20261027.txt`). The calldata names
//! neither, so a pending Classic launch has a candidate per pair and no
//! named token.

use alloy_primitives::{Address, B256, U256, address, b256, keccak256};

use super::profile::VenueFee;
use crate::economics::openfour::{OpenFourFee, charged};
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

/// One Classic (deployer, template) pair: the CREATE2 deployer and the
/// implementation its tokens are EIP-1167 clones of (an immutable of the
/// deployer's runtime).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassicTemplate {
    pub deployer: Address,
    pub deployer_runtime_keccak256: B256,
    pub template: Address,
    pub template_runtime_keccak256: B256,
}

/// The Classic pairs TokenManager2 launches with (see the module doc).
pub const CLASSIC_TEMPLATES: usize = 2;

/// The frozen Four.meme contracts of one chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FourDeployment {
    pub chain_id: u64,
    /// Classic TokenManager2 (proxy): launch and trade calls, its events.
    pub token_manager2: Address,
    /// TokenManagerHelper3: its `getTokenInfo` names the venue's fee terms.
    pub helper3: Address,
    /// A token of TokenManager2 whose `getTokenInfo` the probe reads for
    /// the fee terms before a launch's token exists.
    pub fee_reference_token: Address,
    pub token_manager2_implementation: Address,
    pub token_manager2_implementation_runtime_keccak256: B256,
    /// The Classic (deployer, template) pairs: the G0.1 freeze's first,
    /// the second witnessed after it.
    pub classic_templates: [ClassicTemplate; CLASSIC_TEMPLATES],
    /// The one Classic `createToken` argument length the fixtures witness
    /// (bytes of `args`; 8 launches of both freezes); the salt position is
    /// proven for it only. It holds name and symbol of at most 32 bytes
    /// each: a longer name is another length, refused as an unwitnessed
    /// layout and counted, never guessed (Phase E review L4).
    pub classic_args_len: usize,
    pub openfour_registry: Address,
    /// OpenFour Core (proxy): launch and trade calls, its events.
    pub openfour_core: Address,
    /// OpenFour Tools (proxy, the registry's `openFourTool()`): the curve
    /// liquidity snapshot a held token is read with (R12d).
    pub openfour_tools: Address,
    pub openfour_core_implementation: Address,
    pub openfour_core_implementation_runtime_keccak256: B256,
    /// The OpenFour create deployer (proxy, CREATE2).
    pub openfour_create_deployer: Address,
    pub openfour_create_deployer_implementation: Address,
    pub openfour_create_deployer_implementation_runtime_keccak256: B256,
    /// The presets with a frozen token implementation.
    pub openfour_presets: &'static [OpenFourPreset],
    /// Whether the Classic BUY rules were executed on this TokenManager2
    /// implementation (`economics::four_classic`); a deployment without them
    /// decodes but classifies no launch.
    pub classic_economics: ClassicEconomics,
    /// R12d: the OpenFour preset whose bonding economics are executed
    /// (`economics::openfour`) with its modules as pinned; none: OpenFour
    /// decodes but classifies no launch.
    pub openfour_bonding: Option<OpenFourBondingPreset>,
}

/// One OpenFour module of a preset: the beacon its token's module proxies
/// follow (`Registry.resolvePresetImplementations`) and the implementation
/// behind it (`beacon.implementation()`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenFourModule {
    pub beacon: Address,
    pub implementation: Address,
    pub implementation_runtime_keccak256: B256,
}

/// The modules of a preset in the order the registry names them.
pub const OPENFOUR_MODULES: [&str; 5] = ["token module", "vault", "curve", "trade", "migrate"];

/// R12d: the OpenFour Core's fee router (`Core.feeRouter()`, an EIP-1967
/// proxy) and the implementation behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenFourFeeRouter {
    pub router: Address,
    pub implementation: Address,
    pub implementation_runtime_keccak256: B256,
}

/// R12d: the curve quotes at which the fee router's preset-aware
/// `previewDistribution(preset, quote, [], false, 0)` is probed: a total
/// of 0 at 199 holds only while each fee floors on its own at 50 bps or
/// less, 200 and 10^18 name their sum (the fee rates live in the router's
/// storage, a preset override included, not in pinned code).
pub const OPENFOUR_FEE_PROBE_QUOTES: [U256; 3] = [
    U256::from_limbs([199, 0, 0, 0]),
    U256::from_limbs([200, 0, 0, 0]),
    U256::from_limbs([1_000_000_000_000_000_000, 0, 0, 0]),
];

/// R12d: a preset whose BUY and SELL were executed
/// (`fixtures/fourmeme/openfour_parity_20261104.txt`): its token
/// implementation's code and its modules, no custom data module, the fee
/// router and the fee terms its trades were charged (OFFEE rows).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenFourBondingPreset {
    pub preset_id: U256,
    pub token_implementation_runtime_keccak256: B256,
    pub modules: [OpenFourModule; OPENFOUR_MODULES.len()],
    pub fee_router: OpenFourFeeRouter,
    pub fee: OpenFourFee,
}

/// The live chain's view of a pinned bonding preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservedOpenFourBonding {
    /// `Registry.getPresetTokenImpl(preset)` and its code.
    pub token_implementation: Address,
    pub token_implementation_runtime_keccak256: B256,
    pub modules: [OpenFourModule; OPENFOUR_MODULES.len()],
    pub custom_data_module: Address,
    pub fee_router: OpenFourFeeRouter,
    /// `previewDistribution`'s total fee at each of
    /// [`OPENFOUR_FEE_PROBE_QUOTES`].
    pub fee_charged: [U256; OPENFOUR_FEE_PROBE_QUOTES.len()],
}

/// Whether a deployment's Classic economics are witnessed by execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassicEconomics {
    /// Decode only: no BUY or SELL of a priced word was executed under it.
    DecodeOnly,
    /// The R12 probes executed the priced word's BUYs and SELLs under it.
    Witnessed,
}

/// Every preset in use over the v3 freeze's whole window (OpenFour launch
/// pass blocks 129886197..=129906196, WINDOW_COMPLETE: 1 + 2 + 2 + 51
/// launches), each with a direct launch whose CREATE2 derivation
/// reproduced against `getPresetTokenImpl` at block 129907738. Any other
/// preset is `PendingToken::Unknown`.
/// R12d: the presets frozen through the refreeze at block 131608421: the
/// earlier ones and the preset of the witnessed bonding launch.
const R12D_PRESETS: &[OpenFourPreset] = &[
    MAINNET_PRESETS[0],
    MAINNET_PRESETS[1],
    MAINNET_PRESETS[2],
    MAINNET_PRESETS[3],
    OpenFourPreset {
        preset_id: U256::from_limbs([1_778_027_615_724, 0, 0, 0]),
        token_implementation: address!("f9bf955eb9f2c13b5834e335af795e3857a85e0d"),
    },
];

/// R12d: the witnessed bonding preset (Registry
/// `resolvePresetImplementations(1778027615724)` and each beacon's
/// implementation at block 131608421).
const R12D_BONDING: OpenFourBondingPreset = OpenFourBondingPreset {
    preset_id: U256::from_limbs([1_778_027_615_724, 0, 0, 0]),
    token_implementation_runtime_keccak256: b256!(
        "7740eab75c52e4c2d18649e8dc71a91a928c8fe4bbd43e4a2bb9504ef7112538"
    ),
    modules: [
        OpenFourModule {
            beacon: address!("746ac66e72753e09945f5beee4f704d076701744"),
            implementation: address!("94b1afe94c7f92ff64cf5a64edc6e3d710981e37"),
            implementation_runtime_keccak256: b256!(
                "a25281359d31a83d6a04576e4b0f4011b030add3edc11737f89ea0ebf79ee855"
            ),
        },
        OpenFourModule {
            beacon: address!("21a4ad8278f98509f3df3ab1bdc85a014f3f87fc"),
            implementation: address!("9daf13acf2fa6c854bed55a7a2cf624a44892f4c"),
            implementation_runtime_keccak256: b256!(
                "c97b5d337513a1c8f7507053f441e3b6e802129d9fc143e7a7d833ecb76f3077"
            ),
        },
        OpenFourModule {
            beacon: address!("c51c864f8b4de51f13f39a0b32e5fbd50314fe88"),
            implementation: address!("eacb4085088a84f7dfc9aebf8f0116a418f2600b"),
            implementation_runtime_keccak256: b256!(
                "04fede0062985aec8b9ed3efe6796c09563cc07fbe71299791b9a0c71b3e970a"
            ),
        },
        OpenFourModule {
            beacon: address!("38926d1e0487795abbd7ab29eb0a6bc698862e01"),
            implementation: address!("020481fcc62530680242cb61f2323396cb8bbc55"),
            implementation_runtime_keccak256: b256!(
                "423708fe9df65a3138ef0bbe3883168a4378987e86819ef7aac4f94bd47714d5"
            ),
        },
        OpenFourModule {
            beacon: address!("7b18eeedbdb27fc8006019ca1489e9e1fa5dba14"),
            implementation: address!("0f1b71c5061ec900b061500b0db9e02320df48ed"),
            implementation_runtime_keccak256: b256!(
                "3654b96214b6c27f0408271f5a15e3fdb03dd7febd9959716621e07644c4b6a3"
            ),
        },
    ],
    fee_router: OpenFourFeeRouter {
        router: address!("7383b244b416db16aa9dd0ab7c296aa60255b9a2"),
        implementation: address!("878f9a5dbfcdf9d61e6d61b71b5e710062d070d4"),
        implementation_runtime_keccak256: b256!(
            "b552167de677fa9000b964e97eeb2c09c8839b433869fb96dff7d308458b8de8"
        ),
    },
    fee: OpenFourFee {
        protocol_bps: 50,
        dev_bps: 50,
    },
};

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
pub const fn bsc_mainnet_g01_129842711() -> FourDeployment {
    FourDeployment {
        chain_id: 56,
        token_manager2: address!("5c952063c7fc8610ffdb798152d69f0b9550762b"),
        helper3: address!("f251f83e40a78868fcfa3fa4599dad6494e46034"),
        fee_reference_token: address!("a89a24209a3ae8ba1ffb3f0567b69ef4765bffff"),
        token_manager2_implementation: address!("f32ee25a7a94f5858692b90815493bda9f992eee"),
        token_manager2_implementation_runtime_keccak256: b256!(
            "07eb30d004f7e1b214a2b5c9704e2c7866648b65d1ffeb0c38fd54b5512f030e"
        ),
        classic_templates: [
            ClassicTemplate {
                deployer: address!("6e0c4ba9ce0c901f1bba6af55cd66da3b0443ef1"),
                deployer_runtime_keccak256: b256!(
                    "666d5ae213c594d3cd4a156fd7cf84c85680db01a01e68eabee892d01ee9ce09"
                ),
                template: address!("7330d8865f4b6800b72bdd73e2007833a5d45c94"),
                template_runtime_keccak256: b256!(
                    "49052d1fff3ba35008538f6694480d71469a0a76c82ce47d0500cab6e8775f6d"
                ),
            },
            ClassicTemplate {
                deployer: address!("e8ce4e9175de23f97f853e19c522ef29cebf13e3"),
                deployer_runtime_keccak256: b256!(
                    "ce2353c2f158bc7f2c7589c9c8b9bede322d749f83efdd8f2471840fe98857b8"
                ),
                template: address!("46862924e2a229170ebd065e24a0da72af58a986"),
                template_runtime_keccak256: b256!(
                    "a5529c201b5fdf90cdcddd546521c4e75374e47fdb1aa356859307a4532da97f"
                ),
            },
        ],
        classic_args_len: 672,
        openfour_registry: address!("912cef0c3ae9ab6eb3ec87cab69371cfb317ab94"),
        openfour_core: address!("ebe7b6c1089d9f72ad07f34e36d898e44e5e27f3"),
        openfour_tools: address!("97da82f56fef1a0b974587a48135fa9da2a4e905"),
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
        classic_economics: ClassicEconomics::DecodeOnly,
        openfour_bonding: None,
    }
}

/// BSC mainnet as re-frozen by R12b at block 131568366: the G0.1 deployment
/// with TokenManager2's new implementation (dispatcher 16/16 launch, BUY and
/// SELL selectors, the window's Classic launches reproduce under both pairs;
/// R12a executed its BUYs and SELLs). See the module doc for the presets.
pub const fn bsc_mainnet_r12b_131568366() -> FourDeployment {
    FourDeployment {
        token_manager2_implementation: address!("a38d9d4845f49188a9fec089e55890ae4b0e2e72"),
        token_manager2_implementation_runtime_keccak256: b256!(
            "8da2a695b0d2e9471ffbf04e1259f306a587857bb2e6f93798b74c3f8b875377"
        ),
        classic_economics: ClassicEconomics::Witnessed,
        fee_reference_token: address!("1b1955899341dbd9569c8537f639d642c2a9ffff"),
        ..bsc_mainnet_g01_129842711()
    }
}

/// BSC mainnet as re-frozen by R12d at block 131608421: the R12b
/// deployment with the preset of the one OpenFour launch of 20 000 blocks
/// (its token derived from the launch calldata) and that preset's bonding
/// modules pinned (`fixtures/fourmeme/openfour_parity_20261104.txt`: its
/// BUY and SELL executed); the contracts' code as R12b froze it.
pub const fn bsc_mainnet_r12d_131608421() -> FourDeployment {
    FourDeployment {
        openfour_presets: R12D_PRESETS,
        openfour_bonding: Some(R12D_BONDING),
        ..bsc_mainnet_r12b_131568366()
    }
}

/// What the live chain shows of the pinned contracts at one block: the
/// runtime's read-only probe (`venue_probe::probe_four`) reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservedFour {
    pub chain_id: u64,
    pub token_manager2_implementation: Address,
    pub token_manager2_implementation_runtime_keccak256: B256,
    pub openfour_core_implementation: Address,
    pub openfour_core_implementation_runtime_keccak256: B256,
    pub openfour_create_deployer_implementation: Address,
    pub openfour_create_deployer_implementation_runtime_keccak256: B256,
    /// Each Classic pair's deployer and template runtime keccak.
    pub classic_runtime_keccak256: [(B256, B256); CLASSIC_TEMPLATES],
    /// `Helper3.getTokenInfo(fee_reference_token)`: `tradingFeeRate`,
    /// `minTradingFee` (whether a rate is fixed per token at its creation is
    /// unwitnessed; the BUY's minAmount bounds any difference).
    pub venue_fee: VenueFee,
    /// R12d: the pinned bonding preset as the registry resolves it (read
    /// only for a deployment that pins one).
    pub openfour_bonding: Option<ObservedOpenFourBonding>,
}

/// The first pin the live chain differs from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FourDrift {
    ChainId,
    TokenManager2Implementation,
    TokenManager2Code,
    OpenFourCoreImplementation,
    OpenFourCoreCode,
    CreateDeployerImplementation,
    CreateDeployerCode,
    /// A Classic pair's deployer or template code (its index).
    ClassicPair(usize),
    /// R12d: the pinned bonding preset's token implementation or code, a
    /// module (the index of [`OPENFOUR_MODULES`]: beacon, implementation or
    /// code) or a custom data module; or the preset is not read.
    OpenFourPresetToken,
    OpenFourPresetModule(usize),
    OpenFourPresetCustomData,
    /// R12d: the Core's fee router, its implementation or code.
    OpenFourFeeRouter,
    /// R12d: the fee router charges the preset other than its pinned fee
    /// terms at a probe quote.
    OpenFourFees,
}

impl core::fmt::Display for FourDrift {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ChainId => f.write_str("four deployment: another chain"),
            Self::TokenManager2Implementation => {
                f.write_str("four deployment: TokenManager2 implementation")
            }
            Self::TokenManager2Code => {
                f.write_str("four deployment: TokenManager2 implementation code")
            }
            Self::OpenFourCoreImplementation => {
                f.write_str("four deployment: OpenFour Core implementation")
            }
            Self::OpenFourCoreCode => {
                f.write_str("four deployment: OpenFour Core implementation code")
            }
            Self::CreateDeployerImplementation => {
                f.write_str("four deployment: OpenFour create deployer implementation")
            }
            Self::CreateDeployerCode => {
                f.write_str("four deployment: OpenFour create deployer implementation code")
            }
            Self::ClassicPair(i) => write!(f, "four deployment: Classic pair {} code", i + 1),
            Self::OpenFourPresetToken => {
                f.write_str("four deployment: OpenFour preset token implementation")
            }
            Self::OpenFourPresetModule(i) => write!(
                f,
                "four deployment: OpenFour preset {}",
                OPENFOUR_MODULES.get(*i).copied().unwrap_or("module")
            ),
            Self::OpenFourPresetCustomData => {
                f.write_str("four deployment: OpenFour preset custom data module")
            }
            Self::OpenFourFeeRouter => f.write_str("four deployment: OpenFour fee router"),
            Self::OpenFourFees => f.write_str("four deployment: OpenFour preset fee terms"),
        }
    }
}

impl std::error::Error for FourDrift {}

/// The frozen deployment as matched by a live observation, under the code
/// generation the owner of the state snapshot assigned to it. A later
/// observation gets a new generation and invalidates everything bound to
/// this one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedFourDeployment {
    deployment: FourDeployment,
    codehash_generation: u64,
    venue_fee: VenueFee,
}

impl ValidatedFourDeployment {
    #[inline]
    pub fn deployment(&self) -> &FourDeployment {
        &self.deployment
    }

    #[inline]
    pub fn codehash_generation(&self) -> u64 {
        self.codehash_generation
    }

    /// The venue's fee terms the matching observation read.
    #[inline]
    pub fn venue_fee(&self) -> VenueFee {
        self.venue_fee
    }

    /// Whether its Classic economics are witnessed by execution.
    #[inline]
    pub fn classic_economics_witnessed(&self) -> bool {
        self.deployment.classic_economics == ClassicEconomics::Witnessed
    }

    /// R12d: the bonding preset whose OpenFour economics are witnessed by
    /// execution, its modules matched by the observation.
    #[inline]
    pub fn openfour_bonding(&self) -> Option<&OpenFourBondingPreset> {
        self.deployment.openfour_bonding.as_ref()
    }
}

impl FourDeployment {
    /// Every pin `observed` differs from, in the order of [`FourDrift`].
    pub fn drifts(&self, observed: &ObservedFour) -> Vec<FourDrift> {
        let mut out: Vec<FourDrift> = [
            (observed.chain_id == self.chain_id, FourDrift::ChainId),
            (
                observed.token_manager2_implementation == self.token_manager2_implementation,
                FourDrift::TokenManager2Implementation,
            ),
            (
                observed.token_manager2_implementation_runtime_keccak256
                    == self.token_manager2_implementation_runtime_keccak256,
                FourDrift::TokenManager2Code,
            ),
            (
                observed.openfour_core_implementation == self.openfour_core_implementation,
                FourDrift::OpenFourCoreImplementation,
            ),
            (
                observed.openfour_core_implementation_runtime_keccak256
                    == self.openfour_core_implementation_runtime_keccak256,
                FourDrift::OpenFourCoreCode,
            ),
            (
                observed.openfour_create_deployer_implementation
                    == self.openfour_create_deployer_implementation,
                FourDrift::CreateDeployerImplementation,
            ),
            (
                observed.openfour_create_deployer_implementation_runtime_keccak256
                    == self.openfour_create_deployer_implementation_runtime_keccak256,
                FourDrift::CreateDeployerCode,
            ),
        ]
        .into_iter()
        .filter(|(ok, _)| !ok)
        .map(|(_, drift)| drift)
        .collect();
        for (i, (t, (deployer, template))) in self
            .classic_templates
            .iter()
            .zip(observed.classic_runtime_keccak256)
            .enumerate()
        {
            if t.deployer_runtime_keccak256 != deployer || t.template_runtime_keccak256 != template
            {
                out.push(FourDrift::ClassicPair(i));
            }
        }
        if let Some(pinned) = &self.openfour_bonding {
            let Some(seen) = &observed.openfour_bonding else {
                out.push(FourDrift::OpenFourPresetToken);
                return out;
            };
            let token = self
                .openfour_template(pinned.preset_id)
                .is_some_and(|t| t == seen.token_implementation);
            if !token
                || seen.token_implementation_runtime_keccak256
                    != pinned.token_implementation_runtime_keccak256
            {
                out.push(FourDrift::OpenFourPresetToken);
            }
            for (i, (pin, live)) in pinned.modules.iter().zip(&seen.modules).enumerate() {
                if pin != live {
                    out.push(FourDrift::OpenFourPresetModule(i));
                }
            }
            if seen.custom_data_module != Address::ZERO {
                out.push(FourDrift::OpenFourPresetCustomData);
            }
            if seen.fee_router != pinned.fee_router {
                out.push(FourDrift::OpenFourFeeRouter);
            }
            if OPENFOUR_FEE_PROBE_QUOTES
                .iter()
                .zip(&seen.fee_charged)
                .any(|(q, total)| charged(&pinned.fee, *q).ok() != Some(*total))
            {
                out.push(FourDrift::OpenFourFees);
            }
        }
        out
    }

    /// The deployment as `observed` matches it under `codehash_generation`
    /// (a name of the observation); else the first drift.
    pub fn validate(
        &self,
        observed: &ObservedFour,
        codehash_generation: u64,
    ) -> Result<ValidatedFourDeployment, FourDrift> {
        if let Some(drift) = self.drifts(observed).first() {
            return Err(*drift);
        }
        Ok(ValidatedFourDeployment {
            deployment: *self,
            codehash_generation,
            venue_fee: observed.venue_fee,
        })
    }

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

    /// The Classic token of a launch with `salt` under each frozen pair:
    /// the launch's token is one of them (its salt was mined for one), but
    /// the calldata names neither.
    pub fn classic_tokens(&self, salt: B256) -> [Address; CLASSIC_TEMPLATES] {
        self.classic_templates.map(|t| {
            create2_address(
                t.deployer,
                salt,
                keccak256(minimal_proxy_init_code(t.template)),
            )
        })
    }

    /// The frozen token implementation of an OpenFour preset.
    pub fn openfour_template(&self, preset_id: U256) -> Option<Address> {
        self.openfour_presets
            .iter()
            .find(|p| p.preset_id == preset_id)
            .map(|p| p.token_implementation)
    }

    /// The OpenFour token of a launch of `preset_id` with `token_salt` and
    /// `request_id`, sent by `creator`: CREATE2 by the create deployer of a
    /// clone of the preset's frozen template, salted with `token_salt` (the
    /// v3 freeze's launches, 6/6) or, for a zero salt, with
    /// `keccak256(abi.encode(request_id, preset_id, creator))` (R12d: the
    /// witnessed bonding launch; both under the frozen Core implementation).
    /// No frozen template, or a zero salt of an unknown creator: `None`.
    pub fn openfour_token(
        &self,
        preset_id: U256,
        token_salt: B256,
        request_id: U256,
        creator: Option<Address>,
    ) -> Option<Address> {
        let template = self.openfour_template(preset_id)?;
        let salt = if token_salt == B256::ZERO {
            let mut words = [0u8; 3 * 32];
            words[..32].copy_from_slice(&request_id.to_be_bytes::<32>());
            words[32..64].copy_from_slice(&preset_id.to_be_bytes::<32>());
            words[64 + 12..].copy_from_slice(creator?.as_slice());
            keccak256(words)
        } else {
            token_salt
        };
        Some(create2_address(
            self.openfour_create_deployer,
            salt,
            keccak256(minimal_proxy_init_code(template)),
        ))
    }
}

#[cfg(test)]
pub(crate) mod testkit {
    //! Observations for tests of dependent modules.

    use super::*;

    /// What an observation of exactly these pins shows.
    pub fn pinned_observation(d: &FourDeployment) -> ObservedFour {
        ObservedFour {
            chain_id: d.chain_id,
            token_manager2_implementation: d.token_manager2_implementation,
            token_manager2_implementation_runtime_keccak256: d
                .token_manager2_implementation_runtime_keccak256,
            openfour_core_implementation: d.openfour_core_implementation,
            openfour_core_implementation_runtime_keccak256: d
                .openfour_core_implementation_runtime_keccak256,
            openfour_create_deployer_implementation: d.openfour_create_deployer_implementation,
            openfour_create_deployer_implementation_runtime_keccak256: d
                .openfour_create_deployer_implementation_runtime_keccak256,
            classic_runtime_keccak256: d
                .classic_templates
                .map(|t| (t.deployer_runtime_keccak256, t.template_runtime_keccak256)),
            venue_fee: crate::venues::fourmeme::profile::WITNESSED_VENUE_FEE,
            openfour_bonding: d.openfour_bonding.map(|b| ObservedOpenFourBonding {
                token_implementation: d.openfour_template(b.preset_id).unwrap_or(Address::ZERO),
                token_implementation_runtime_keccak256: b.token_implementation_runtime_keccak256,
                modules: b.modules,
                custom_data_module: Address::ZERO,
                fee_router: b.fee_router,
                fee_charged: OPENFOUR_FEE_PROBE_QUOTES
                    .map(|q| charged(&b.fee, q).unwrap_or(U256::MAX)),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R12d: the bonding pin is the fixture's witness: the preset token
    /// implementation and its code (OFMODULE token), each module's beacon,
    /// implementation and code in the registry's order (OFBEACON), the fee
    /// router (OFFEEROUTER) and the fee terms whose charges the router's
    /// preset-aware preview returned (OFFEE, every row, the probe quotes
    /// among them); another split of the same 1 % is charged at 199.
    #[test]
    fn the_bonding_pin_is_the_fixtures_witness() {
        const FIXTURE: &str =
            include_str!("../../../fixtures/fourmeme/openfour_parity_20261104.txt");
        let rows = |kind: &'static str| {
            FIXTURE
                .lines()
                .map(|l| l.split(' ').collect::<Vec<_>>())
                .filter(move |f| f[0] == kind)
        };
        let u = |s: &str| U256::from_str_radix(s, 10).unwrap();
        let d = bsc_mainnet_r12d_131608421();
        let b = d.openfour_bonding.unwrap();
        let token = rows("OFMODULE").find(|f| f[2] == "token").unwrap();
        assert_eq!(
            (
                d.openfour_template(b.preset_id),
                b.token_implementation_runtime_keccak256
            ),
            (
                Some(token[5].parse::<Address>().unwrap()),
                token[6].parse::<B256>().unwrap()
            )
        );
        for (m, role) in b
            .modules
            .iter()
            .zip(["tokenModule", "vault", "curve", "trade", "migrate"])
        {
            let f = rows("OFBEACON").find(|f| f[2] == role).unwrap();
            assert_eq!(
                (
                    m.beacon,
                    m.implementation,
                    m.implementation_runtime_keccak256
                ),
                (
                    f[5].parse::<Address>().unwrap(),
                    f[6].parse::<Address>().unwrap(),
                    f[7].parse::<B256>().unwrap()
                ),
                "{role}"
            );
        }
        assert_eq!(rows("OFFEEROUTER").count(), 2);
        for f in rows("OFFEEROUTER") {
            assert_eq!(
                (
                    b.fee_router,
                    f[6].parse::<u16>().unwrap(),
                    f[7].parse::<u16>().unwrap()
                ),
                (
                    OpenFourFeeRouter {
                        router: f[2].parse().unwrap(),
                        implementation: f[3].parse().unwrap(),
                        implementation_runtime_keccak256: f[4].parse().unwrap(),
                    },
                    b.fee.protocol_bps,
                    b.fee.dev_bps
                )
            );
        }
        let mut quotes = Vec::new();
        for f in rows("OFFEE") {
            let (q, total) = (u(f[3]), u(f[4]));
            assert_eq!(u(f[2]), b.preset_id);
            assert_eq!(charged(&b.fee, q).unwrap(), total, "{f:?}");
            assert_eq!((u(f[5]), u(f[6])), (q - total, U256::ZERO), "{f:?}");
            quotes.push(q);
        }
        assert!(OPENFOUR_FEE_PROBE_QUOTES.iter().all(|q| quotes.contains(q)));
        let other = OpenFourFee {
            protocol_bps: 60,
            dev_bps: 40,
        };
        assert_eq!(
            charged(&other, OPENFOUR_FEE_PROBE_QUOTES[0]).unwrap(),
            U256::ONE
        );
    }

    /// The init-code hashes and tokens of the freeze's derivation attempts.
    #[test]
    fn the_frozen_derivation_reproduces_the_freeze() {
        let d = bsc_mainnet_g01_129842711();
        assert_eq!(
            keccak256(minimal_proxy_init_code(d.classic_templates[0].template)),
            b256!("701a364cab6ae8867f16bae0a17fa0b01fe43e0770fa3655f31a03a54262e134")
        );
        assert_eq!(
            d.classic_tokens(b256!(
                "b3b9bc8c554de542fe9181f3df7867fd957debabfe4d7bf3326149b5ada7e48e"
            ))[0],
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
                b256!("3ec074a27944090054e5c3a504cc382fbe9c716e6e9b09ec4fe3a8e1737a01e5"),
                U256::ZERO,
                None,
            ),
            Some(address!("f76ad2f3d84ee09ee10ee1ef8c4d6dbb264effff"))
        );
        // A preset outside the frozen table (…732, none in use over the v3
        // window) has no template, so no token.
        assert_eq!(
            d.openfour_token(
                preset - U256::from(1u8),
                B256::repeat_byte(1),
                U256::ZERO,
                None
            ),
            None
        );
        // R12d: a zero salt is the request's (the witnessed bonding launch:
        // request 0x628b928 of creator 0x5044…c06d named its token), and
        // without its creator no token is named.
        let r12d = bsc_mainnet_r12d_131608421();
        let bonding = U256::from(1_778_027_615_724u64);
        let creator = address!("504497437956dd580a9739e5a18ef080a9a3c06d");
        let request = U256::from(0x0628_b928u64);
        assert_eq!(
            r12d.openfour_token(bonding, B256::ZERO, request, Some(creator)),
            Some(address!("6a081154adc920c37669e7c92e9213ece898813c"))
        );
        assert_eq!(
            r12d.openfour_token(bonding, B256::ZERO, request, None),
            None
        );
        assert_eq!(
            d.openfour_token(bonding, B256::ZERO, request, Some(creator)),
            None
        );
        assert_eq!(d.family_of(d.token_manager2), Some(Family::Classic));
        assert_eq!(d.family_of(d.openfour_core), Some(Family::OpenFour));
        assert_eq!(d.family_of(d.openfour_registry), None);
    }

    /// R12b: the refreeze changes TokenManager2's implementation pin (and
    /// R12c its executed economics and a fee reference token of its own era)
    /// only
    /// (the freeze at block 131568366 read every other pinned contract's
    /// code unchanged; presets other than …735 are carried over from v3).
    #[test]
    fn the_refreeze_moves_only_the_token_manager2_implementation() {
        let (g01, now) = (bsc_mainnet_g01_129842711(), bsc_mainnet_r12b_131568366());
        assert_ne!(
            now.token_manager2_implementation,
            g01.token_manager2_implementation
        );
        assert_ne!(
            now.token_manager2_implementation_runtime_keccak256,
            g01.token_manager2_implementation_runtime_keccak256
        );
        assert_eq!(
            FourDeployment {
                token_manager2_implementation: g01.token_manager2_implementation,
                token_manager2_implementation_runtime_keccak256: g01
                    .token_manager2_implementation_runtime_keccak256,
                classic_economics: ClassicEconomics::DecodeOnly,
                fee_reference_token: g01.fee_reference_token,
                ..now
            },
            g01
        );
        assert_eq!(now.classic_economics, ClassicEconomics::Witnessed);
    }

    /// R12c: an observation validates only when every pin matches; each
    /// changed field is named, the first is the validation's refusal.
    #[test]
    fn a_deployment_validates_only_against_its_own_pins() {
        use testkit::pinned_observation;
        let d = bsc_mainnet_r12b_131568366();
        let pinned = pinned_observation(&d);
        let v = d.validate(&pinned, 7).unwrap();
        assert_eq!((v.codehash_generation(), *v.deployment()), (7, d));
        assert!(v.classic_economics_witnessed());
        // The fee terms are an observation the validation carries, no pin.
        let mut fee = pinned;
        fee.venue_fee.rate_bps = 250;
        assert_eq!(d.validate(&fee, 7).unwrap().venue_fee().rate_bps, 250);
        let a = Address::repeat_byte(0xee);
        let k = B256::repeat_byte(0xee);
        type Change = fn(&mut ObservedFour, Address, B256);
        let changes: [(Change, FourDrift); 10] = [
            (|o, _, _| o.chain_id = 97, FourDrift::ChainId),
            (
                |o, a, _| o.token_manager2_implementation = a,
                FourDrift::TokenManager2Implementation,
            ),
            (
                |o, _, k| o.token_manager2_implementation_runtime_keccak256 = k,
                FourDrift::TokenManager2Code,
            ),
            (
                |o, a, _| o.openfour_core_implementation = a,
                FourDrift::OpenFourCoreImplementation,
            ),
            (
                |o, _, k| o.openfour_core_implementation_runtime_keccak256 = k,
                FourDrift::OpenFourCoreCode,
            ),
            (
                |o, a, _| o.openfour_create_deployer_implementation = a,
                FourDrift::CreateDeployerImplementation,
            ),
            (
                |o, _, k| o.openfour_create_deployer_implementation_runtime_keccak256 = k,
                FourDrift::CreateDeployerCode,
            ),
            (
                |o, _, k| o.classic_runtime_keccak256[0].0 = k,
                FourDrift::ClassicPair(0),
            ),
            (
                |o, _, k| o.classic_runtime_keccak256[0].1 = k,
                FourDrift::ClassicPair(0),
            ),
            (
                |o, _, k| o.classic_runtime_keccak256[1].0 = k,
                FourDrift::ClassicPair(1),
            ),
        ];
        for (change, drift) in changes {
            let mut o = pinned;
            change(&mut o, a, k);
            assert_eq!((d.drifts(&o), d.validate(&o, 1)), (vec![drift], Err(drift)));
        }
        // The G0.1 pins against today's chain: the TokenManager2 drift; and
        // a deployment without executed economics validates but is no
        // witness.
        let g01 = bsc_mainnet_g01_129842711();
        assert_eq!(
            g01.drifts(&pinned),
            [
                FourDrift::TokenManager2Implementation,
                FourDrift::TokenManager2Code
            ]
        );
        assert!(
            !g01.validate(&pinned_observation(&g01), 1)
                .unwrap()
                .classic_economics_witnessed()
        );
    }

    /// Paket 8 review H1: both witnessed launches of the second Classic
    /// pair reproduce under it, and not under the frozen first one (before:
    /// the one candidate contradicted the canonical token).
    #[test]
    fn the_second_classic_pair_reproduces_its_launches() {
        let d = bsc_mainnet_g01_129842711();
        let text = include_str!("../../../fixtures/fourmeme/classic_template2_20261027.txt");
        let mut n = 0;
        for line in text
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
        {
            let f: Vec<&str> = line.split(' ').collect();
            let salt: B256 = f[2].parse().unwrap();
            let token: Address = f[3].parse().unwrap();
            let [first, second] = d.classic_tokens(salt);
            assert_eq!(second, token, "{}", f[0]);
            assert_ne!(first, token);
            n += 1;
        }
        assert_eq!(n, 2);
    }
}
