//! The evidence inputs of the G0.1 Four.meme freeze in one table: every
//! contract address the freeze starts from, where it comes from, what it is
//! frozen as, and which archived 2026-09-27 files it is compared with. The
//! freeze verifies each entry on chain at the reference block; an address
//! here is an input to be proven, never a proven fact by itself.

use alloy_primitives::{Address, address};

/// The two protocol families, never conflated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// Four.meme Classic: TokenManager2 (`createToken`, `buyToken*`).
    Classic,
    /// Four.meme OpenFour: Registry, Core and Tools.
    OpenFour,
}

impl Family {
    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "CLASSIC",
            Self::OpenFour => "OPENFOUR",
        }
    }
}

/// What a contract is frozen as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Launch/BUY/quote surface of the family.
    FrozenSurface,
    /// Read only to validate other evidence (never a hot-path dependency).
    ValidationOnly,
    /// Still deployed, not a current launch surface; identity only.
    LegacyNotFrozen,
    /// Needed to derive the token address before execution.
    DerivationInput,
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Self::FrozenSurface => "FROZEN_SURFACE",
            Self::ValidationOnly => "VALIDATION_ONLY",
            Self::LegacyNotFrozen => "LEGACY_NOT_FROZEN",
            Self::DerivationInput => "DERIVATION_INPUT",
        }
    }
}

/// How the freeze knows a contract's address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// A published or previously observed address (provenance in `source`).
    Table(Address),
    /// A zero-argument address getter of another contract of this table,
    /// called at the reference block. `archived` is the 2026-09-27 value.
    Getter {
        on: &'static str,
        signature: &'static str,
        archived: Option<Address>,
    },
}

/// One contract of the table.
#[derive(Clone, Copy, Debug)]
pub struct ContractInput {
    pub family: Family,
    pub role: &'static str,
    pub resolution: Resolution,
    pub scope: Scope,
    /// Read the EIP-1967 implementation slot.
    pub proxy: bool,
    /// The archived published ABI (archive-relative) its dispatcher is
    /// checked against.
    pub abi: Option<&'static str>,
    /// Archived runtime code of the address and of its implementation.
    pub archived_code: Option<&'static str>,
    pub archived_implementation_code: Option<&'static str>,
    /// Where the address comes from.
    pub source: &'static str,
}

pub const TOKEN_MANAGER2: &str = "TokenManager2";
pub const TOKEN_MANAGER_V1: &str = "TokenManagerV1";
pub const HELPER3: &str = "TokenManagerHelper3";
pub const CLASSIC_TOKEN_DEPLOYER: &str = "ClassicTokenDeployer";
pub const CLASSIC_TOKEN_TEMPLATE: &str = "ClassicTokenTemplate";
pub const REGISTRY: &str = "OpenFourRegistry";
pub const CORE: &str = "OpenFourCore";
pub const TOOLS: &str = "OpenFourTools";
pub const CREATE_DEPLOYER: &str = "OpenFourCreateDeployer";

/// four-meme-community/fourmeme-docs, main at 5f7f589b (2026-08-18), as
/// archived on 2026-09-27.
const CLASSIC_DOCS: &str = "four-meme-community/fourmeme-docs@5f7f589b042e4e3b41c2c214d0fe3a81b36725a9 (published BSC addresses)";
/// four-meme-community/openfour-docs, main at 0527d529, as archived.
const OPENFOUR_DOCS: &str = "four-meme-community/openfour-docs@0527d529d59c6a3fbc9927ce391ecc97c66e1b01 (integration guide: BSC Registry)";

pub const CONTRACTS: &[ContractInput] = &[
    ContractInput {
        family: Family::Classic,
        role: TOKEN_MANAGER2,
        resolution: Resolution::Table(address!("0x5c952063c7fc8610FFDB798152D69F0B9550762b")),
        scope: Scope::FrozenSurface,
        proxy: true,
        abi: Some(CLASSIC_ABI),
        archived_code: Some("fresh_primary/four.code.hex"),
        archived_implementation_code: Some("fresh_primary/four.implementation.code.hex"),
        source: CLASSIC_DOCS,
    },
    ContractInput {
        family: Family::Classic,
        role: TOKEN_MANAGER_V1,
        resolution: Resolution::Table(address!("0xEC4549caDcE5DA21Df6E6422d448034B5233bFbC")),
        scope: Scope::LegacyNotFrozen,
        proxy: true,
        abi: None,
        archived_code: None,
        archived_implementation_code: None,
        source: CLASSIC_DOCS,
    },
    ContractInput {
        family: Family::Classic,
        role: HELPER3,
        resolution: Resolution::Table(address!("0xF251F83e40a78868FcfA3FA4599Dad6494E46034")),
        scope: Scope::ValidationOnly,
        proxy: true,
        abi: None,
        archived_code: Some("four_helper_latest.code.hex"),
        archived_implementation_code: None,
        source: CLASSIC_DOCS,
    },
    // Observed in this freeze from the archived canonical launch (tx
    // 0x8cc24c0c..., block 124415333): the token's first owner and minter
    // before TokenManager2. The freeze verifies that its runtime contains a
    // CREATE2 instruction and the template below as an immutable, and that
    // every Classic launch fixture reproduces through it.
    ContractInput {
        family: Family::Classic,
        role: CLASSIC_TOKEN_DEPLOYER,
        resolution: Resolution::Table(address!("0x6e0c4ba9ce0c901f1bba6af55cd66da3b0443ef1")),
        scope: Scope::DerivationInput,
        proxy: false,
        abi: None,
        archived_code: None,
        archived_implementation_code: None,
        source: "observed 2026-10-26: OwnershipTransferred(0 -> deployer) of the archived fixture token, log 0x123",
    },
    // The EIP-1167 target of the archived fixture token's runtime.
    ContractInput {
        family: Family::Classic,
        role: CLASSIC_TOKEN_TEMPLATE,
        resolution: Resolution::Table(address!("0x7330d8865f4b6800b72bdd73e2007833a5d45c94")),
        scope: Scope::DerivationInput,
        proxy: false,
        abi: None,
        archived_code: None,
        archived_implementation_code: None,
        source: "observed 2026-10-26: EIP-1167 target of token 0xa89a24209a3ae8ba1ffb3f0567b69ef4765bffff",
    },
    ContractInput {
        family: Family::OpenFour,
        role: REGISTRY,
        resolution: Resolution::Table(address!("0x912CEf0C3aE9Ab6eB3Ec87cab69371cFb317Ab94")),
        scope: Scope::FrozenSurface,
        proxy: true,
        abi: Some(OPENFOUR_REGISTRY_ABI),
        archived_code: Some("openfour_registry_latest.code.hex"),
        archived_implementation_code: Some("openfour/registry.implementation.code.hex"),
        source: OPENFOUR_DOCS,
    },
    ContractInput {
        family: Family::OpenFour,
        role: CORE,
        resolution: Resolution::Getter {
            on: REGISTRY,
            signature: "openFourCore()",
            archived: Some(address!("0xebe7b6c1089d9f72ad07f34e36d898e44e5e27f3")),
        },
        scope: Scope::FrozenSurface,
        proxy: true,
        abi: Some(OPENFOUR_CORE_ABI),
        archived_code: Some("openfour_core_latest.code.hex"),
        archived_implementation_code: Some("openfour/core.implementation.code.hex"),
        source: "OpenFourRegistry.openFourCore() at the reference block",
    },
    ContractInput {
        family: Family::OpenFour,
        role: TOOLS,
        resolution: Resolution::Getter {
            on: REGISTRY,
            signature: "openFourTool()",
            archived: Some(address!("0x97da82f56fef1a0b974587a48135fa9da2a4e905")),
        },
        scope: Scope::FrozenSurface,
        proxy: true,
        abi: Some(OPENFOUR_TOOLS_ABI),
        archived_code: Some("openfour_tools_latest.code.hex"),
        archived_implementation_code: Some("openfour/tools.implementation.code.hex"),
        source: "OpenFourRegistry.openFourTool() at the reference block",
    },
    ContractInput {
        family: Family::OpenFour,
        role: CREATE_DEPLOYER,
        resolution: Resolution::Getter {
            on: REGISTRY,
            signature: "createDeployer()",
            archived: None,
        },
        scope: Scope::DerivationInput,
        proxy: true,
        abi: None,
        archived_code: None,
        archived_implementation_code: None,
        source: "OpenFourRegistry.createDeployer() at the reference block",
    },
];

/// The archived 2026-09-27 G0.1 evidence (relative to the repository root).
pub const ARCHIVE_DEFAULT: &str =
    "prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/2026-09-27-g01";
/// Published ABIs (archive-relative).
pub const CLASSIC_ABI: &str = "sources/TokenManager2.lite.json";
pub const OPENFOUR_REGISTRY_ABI: &str = "openfour/OpenFourRegistry.json";
pub const OPENFOUR_CORE_ABI: &str = "openfour/OpenFourCore.json";
pub const OPENFOUR_TOOLS_ABI: &str = "openfour/OpenFourTools.json";
/// The independent `web3_sha3` keccak reference of the earlier run.
pub const KECCAK_REFERENCE: &str = "keccak_comparison.json";
/// The canonical Classic launch fixture.
pub const CLASSIC_FIXTURE_TX: &str = "canonical_fixture_scan/four_create_transaction.json";
pub const CLASSIC_FIXTURE_RECEIPT: &str = "canonical_fixture_scan/four_create_receipt.json";
/// The archived implementation observations.
pub const CLASSIC_IMPLEMENTATION_SUMMARY: &str = "fresh_primary/summary.json";
pub const OPENFOUR_IMPLEMENTATION_SLOT_REQUEST: &str = "openfour/impl_slot_rpc/001.request.json";
pub const OPENFOUR_IMPLEMENTATION_SLOT_RESPONSE: &str = "openfour/impl_slot_rpc/001.response.json";
/// The earlier run's refused `eth_getLogs` (why the freeze scans receipts).
pub const GETLOGS_REFUSAL: &str = "four_launch_logs_attempt2/001.response.json";

/// The archived Classic fixture the freeze re-verifies.
pub const CLASSIC_FIXTURE_TX_HASH: &str =
    "0x8cc24c0cb9f40ee92240959fbafece1a2fc22f2e367b42366050a33cba827e5b";

/// The table entry of a role.
pub fn input(role: &str) -> &'static ContractInput {
    CONTRACTS
        .iter()
        .find(|c| c.role == role)
        .unwrap_or_else(|| panic!("no table entry {role}"))
}

/// The table address of a role resolved from the table itself.
pub fn table_address(role: &str) -> Address {
    match input(role).resolution {
        Resolution::Table(a) => a,
        Resolution::Getter { .. } => panic!("{role} is resolved through a getter"),
    }
}
