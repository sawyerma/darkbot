//! G0.6a DRY records: the imported, hash-bound DRY evidence of the G0.7
//! attempt and the read-only Mainnet preflight (imported baseline and the
//! current preflight G0.7 reads itself, one schema).

use crate::{Field, Kind::*, Schema};

/// `dry_result.json` of the frozen G0.6a DRY.
pub mod dry {
    use super::*;

    pub const STATUS: u16 = 10;
    pub const NETWORK_WRITE_CAPABILITY: u16 = 12;
    pub const DRY_1: u16 = 400;
    pub const DRY_2: u16 = 401;
    pub const DRY_3: u16 = 402;
    pub const DRY_4: u16 = 403;
    pub const DRY_ALL: u16 = 404;
    pub const ZERO_EGRESS_STRUCTURAL: u16 = 405;
    pub const BUY_GAS_LIMIT: u16 = 500;
    pub const BUY_TX_HASH: u16 = 501;
    pub const BUY_VALUE_WEI: u16 = 502;
    pub const CHAIN_ID: u16 = 503;
    pub const GAS_PRICE_WEI: u16 = 504;
    pub const LAUNCH_GAS_LIMIT: u16 = 505;
    pub const LAUNCH_TX_HASH: u16 = 506;
    pub const LAUNCH_VALUE_WEI: u16 = 507;
    pub const LOCAL_EXPECTED_OUTPUT: u16 = 508;
    pub const MIN_OUTPUT: u16 = 509;
    pub const PORTAL: u16 = 510;
    pub const TOKEN: u16 = 511;
    pub const BUYER: u16 = 600;
    pub const DISTINCT: u16 = 601;
    pub const LAUNCHER: u16 = 602;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g06a.self-launch-bundle-dry.import.v1",
        record_type: 2,
        schema_version: 1,
        fields: &[
            Field::req(1, "_import.legacy_sha256", B256),
            Field::req(2, "_import.legacy_path", Text),
            Field::req(STATUS, "status", Text),
            Field::req(11, "live_submission", Text),
            Field::req(NETWORK_WRITE_CAPABILITY, "network_write_capability", Text),
            Field::req(13, "qualification_note", Text),
            Field::req(100, "budget.v2_proposal_per_attempt_ceiling_wei", U256),
            Field::req(101, "budget.worst_case_reserved_wei", U256),
            Field::req(200, "bundle_projection.blockrazor_payload_sha256", B256),
            Field::req(201, "bundle_projection.fortyeightclub_noMerge", Bool),
            Field::req(202, "bundle_projection.fortyeightclub_payload_sha256", B256),
            Field::req(203, "bundle_projection.fortyeightclub_positionFirst", Bool),
            Field::req(204, "bundle_projection.identical_txs_across_builders", Bool),
            Field::req(205, "bundle_projection.max_block", U64),
            Field::rep(206, "bundle_projection.order", Text),
            Field::req(207, "bundle_projection.send_head", U64),
            Field::req(300, "contract.approved", Bool),
            Field::req(301, "contract.compiled_live_hash", Text),
            Field::req(302, "contract.execution_ready", Bool),
            Field::req(303, "contract.path", Text),
            Field::req(304, "contract.sha256", B256),
            Field::req(DRY_1, "dry_pass.DRY_1_bytes_and_signers", Bool),
            Field::req(DRY_2, "dry_pass.DRY_2_independent_local_simulation", Bool),
            Field::req(DRY_3, "dry_pass.DRY_3_gas_floor_and_budget", Bool),
            Field::req(DRY_4, "dry_pass.DRY_4_expiry_nonces_zero_egress", Bool),
            Field::req(DRY_ALL, "dry_pass.all", Bool),
            Field::req(ZERO_EGRESS_STRUCTURAL, "dry_pass.zero_egress_structural", Bool),
            Field::req(BUY_GAS_LIMIT, "transactions.buy_gas_limit", U64),
            Field::req(BUY_TX_HASH, "transactions.buy_tx_hash", B256),
            Field::req(BUY_VALUE_WEI, "transactions.buy_value_wei", U256),
            Field::req(CHAIN_ID, "transactions.chain_id", U64),
            Field::req(GAS_PRICE_WEI, "transactions.gas_price_wei", U64),
            Field::req(LAUNCH_GAS_LIMIT, "transactions.launch_gas_limit", U64),
            Field::req(LAUNCH_TX_HASH, "transactions.launch_tx_hash", B256),
            Field::req(LAUNCH_VALUE_WEI, "transactions.launch_value_wei", U256),
            Field::req(LOCAL_EXPECTED_OUTPUT, "transactions.local_expected_output", U256),
            Field::req(MIN_OUTPUT, "transactions.min_output", U256),
            Field::req(PORTAL, "transactions.portal", Address),
            Field::req(TOKEN, "transactions.token", Address),
            Field::req(BUYER, "wallets.buyer", Address),
            Field::req(DISTINCT, "wallets.distinct", Bool),
            Field::req(LAUNCHER, "wallets.launcher", Address),
            Field::req(603, "wallets.same_wallet_test_mode", Bool),
        ],
    };
}

/// The read-only Mainnet preflight: the imported DRY baseline
/// (`preflight.json`, with `_import.*`) and the current preflight G0.7
/// reads before the attempt (without). Legacy-only prose is optional.
pub mod preflight {
    use super::*;

    pub const LEGACY_SCHEMA: u16 = 10;
    pub const CHAIN_ID: u16 = 11;
    pub const LATEST_BLOCK_NUMBER: u16 = 12;
    pub const LATEST_BLOCK_HASH: u16 = 13;
    pub const PORTAL: u16 = 14;
    pub const PORTAL_CODE_BYTES: u16 = 15;
    pub const PORTAL_CODE_SHA256: u16 = 16;
    pub const PORTAL_VERSION: u16 = 17;
    pub const PORTAL_IMPLEMENTATION: u16 = 18;
    pub const PORTAL_IMPLEMENTATION_CODE_BYTES: u16 = 19;
    pub const PORTAL_IMPLEMENTATION_CODE_SHA256: u16 = 20;
    pub const TAX_V3_IMPL: u16 = 21;
    pub const TAX_V3_IMPL_SOURCE: u16 = 22;
    pub const TAX_V3_IMPL_CODE_BYTES: u16 = 23;
    pub const TAX_V3_IMPL_CODE_SHA256: u16 = 24;
    pub const GAS_PRICE_WEI: u16 = 25;
    pub const QUOTE_ENABLED: u16 = 30;
    pub const QUOTE_DEFAULT_CURVE_ID: u16 = 31;
    pub const QUOTE_ALTERNATIVE_CURVE_ID: u16 = 32;
    pub const QUOTE_NATIVE_TO_QUOTE_SWAP_TYPE: u16 = 33;
    pub const QUOTE_DEX_ID: u16 = 34;
    pub const FEE_BUY_BPS: u16 = 40;
    pub const FEE_SELL_BPS: u16 = 41;
    pub const TAX_ON_BONDING_CURVE_ENABLED: u16 = 42;
    pub const LAUNCHER_ADDRESS: u16 = 50;
    pub const LAUNCHER_PENDING_NONCE: u16 = 51;
    pub const LAUNCHER_BALANCE_WEI: u16 = 52;
    pub const BUYER_ADDRESS: u16 = 53;
    pub const BUYER_PENDING_NONCE: u16 = 54;
    pub const BUYER_BALANCE_WEI: u16 = 55;
    pub const ADDRESSES_BOUND: u16 = 60;
    pub const DISTINCT: u16 = 61;
    pub const LAUNCHER_REQUIRED_WEI: u16 = 62;
    pub const BUYER_REQUIRED_WEI: u16 = 63;
    pub const COMBINED_RESERVE_WEI: u16 = 64;
    pub const PROPOSAL_MAX_RESERVE_WEI: u16 = 65;
    pub const PROFILE_GAS_PRICE_WEI: u16 = 66;
    pub const PROFILE_LAUNCH_GAS_LIMIT: u16 = 67;
    pub const PROFILE_BUY_GAS_LIMIT: u16 = 68;
    pub const PROFILE_LAUNCH_VALUE_WEI: u16 = 69;
    pub const PROFILE_BUY_VALUE_WEI: u16 = 70;
    pub const LAUNCHER_SUFFICIENT: u16 = 71;
    pub const BUYER_SUFFICIENT: u16 = 72;
    pub const FUNDING_PASS: u16 = 73;
    pub const NETWORK_ACTIONS: u16 = 80;
    pub const SIGNED_TX_SUBMITTED: u16 = 81;
    pub const BUNDLE_SUBMITTED: u16 = 82;
    pub const DEPENDENCIES: u16 = 83;

    /// The deployment and economics fields that must equal the frozen DRY
    /// baseline before the attempt.
    pub const DRIFT_CHECKED: [u16; 16] = [
        CHAIN_ID,
        PORTAL,
        PORTAL_CODE_SHA256,
        PORTAL_VERSION,
        PORTAL_IMPLEMENTATION,
        PORTAL_IMPLEMENTATION_CODE_SHA256,
        TAX_V3_IMPL,
        TAX_V3_IMPL_CODE_SHA256,
        QUOTE_ENABLED,
        QUOTE_DEFAULT_CURVE_ID,
        QUOTE_ALTERNATIVE_CURVE_ID,
        QUOTE_NATIVE_TO_QUOTE_SWAP_TYPE,
        QUOTE_DEX_ID,
        FEE_BUY_BPS,
        FEE_SELL_BPS,
        TAX_ON_BONDING_CURVE_ENABLED,
    ];

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g06a.preflight-readonly.v3",
        record_type: 3,
        schema_version: 1,
        fields: &[
            Field::opt(1, "_import.legacy_sha256", B256),
            Field::opt(2, "_import.legacy_path", Text),
            Field::opt(LEGACY_SCHEMA, "schema", Text),
            Field::req(CHAIN_ID, "chain_id", U64),
            Field::req(LATEST_BLOCK_NUMBER, "latest_block_number", U64),
            Field::req(LATEST_BLOCK_HASH, "latest_block_hash", B256),
            Field::req(PORTAL, "portal", Address),
            Field::req(PORTAL_CODE_BYTES, "portal_code_bytes", U64),
            Field::req(PORTAL_CODE_SHA256, "portal_code_sha256", B256),
            Field::req(PORTAL_VERSION, "portal_version", Text),
            Field::req(PORTAL_IMPLEMENTATION, "portal_implementation", Address),
            Field::req(PORTAL_IMPLEMENTATION_CODE_BYTES, "portal_implementation_code_bytes", U64),
            Field::req(PORTAL_IMPLEMENTATION_CODE_SHA256, "portal_implementation_code_sha256", B256),
            Field::req(TAX_V3_IMPL, "tax_v3_impl", Address),
            Field::opt(TAX_V3_IMPL_SOURCE, "tax_v3_impl_source", Text),
            Field::req(TAX_V3_IMPL_CODE_BYTES, "tax_v3_impl_code_bytes", U64),
            Field::req(TAX_V3_IMPL_CODE_SHA256, "tax_v3_impl_code_sha256", B256),
            Field::req(GAS_PRICE_WEI, "gas_price_wei", U256),
            Field::req(QUOTE_ENABLED, "native_quote_configuration.enabled", U256),
            Field::req(QUOTE_DEFAULT_CURVE_ID, "native_quote_configuration.default_curve_id", U256),
            Field::req(
                QUOTE_ALTERNATIVE_CURVE_ID,
                "native_quote_configuration.alternative_curve_id",
                U256,
            ),
            Field::req(
                QUOTE_NATIVE_TO_QUOTE_SWAP_TYPE,
                "native_quote_configuration.native_to_quote_swap_type",
                U256,
            ),
            Field::req(QUOTE_DEX_ID, "native_quote_configuration.dex_id", U256),
            Field::req(FEE_BUY_BPS, "protocol_fee_bps.buy", U256),
            Field::req(FEE_SELL_BPS, "protocol_fee_bps.sell", U256),
            Field::req(TAX_ON_BONDING_CURVE_ENABLED, "tax_on_bonding_curve_enabled", Bool),
            Field::opt(LAUNCHER_ADDRESS, "launcher_address", Address),
            Field::opt(LAUNCHER_PENDING_NONCE, "launcher_pending_nonce", U64),
            Field::opt(LAUNCHER_BALANCE_WEI, "launcher_balance_wei", U256),
            Field::opt(BUYER_ADDRESS, "buyer_address", Address),
            Field::opt(BUYER_PENDING_NONCE, "buyer_pending_nonce", U64),
            Field::opt(BUYER_BALANCE_WEI, "buyer_balance_wei", U256),
            Field::req(ADDRESSES_BOUND, "wallet_qualification.addresses_bound", Bool),
            Field::opt(DISTINCT, "wallet_qualification.distinct", Bool),
            Field::req(
                LAUNCHER_REQUIRED_WEI,
                "wallet_qualification.launcher_required_worst_case_wei",
                U256,
            ),
            Field::req(
                BUYER_REQUIRED_WEI,
                "wallet_qualification.buyer_required_worst_case_wei",
                U256,
            ),
            Field::req(
                COMBINED_RESERVE_WEI,
                "wallet_qualification.combined_attempt_reserve_wei",
                U256,
            ),
            Field::req(
                PROPOSAL_MAX_RESERVE_WEI,
                "wallet_qualification.proposal_max_attempt_reserve_wei",
                U256,
            ),
            Field::req(
                PROFILE_GAS_PRICE_WEI,
                "wallet_qualification.funding_profile.gas_price_wei",
                U256,
            ),
            Field::req(
                PROFILE_LAUNCH_GAS_LIMIT,
                "wallet_qualification.funding_profile.launch_gas_limit",
                U64,
            ),
            Field::req(
                PROFILE_BUY_GAS_LIMIT,
                "wallet_qualification.funding_profile.buy_gas_limit",
                U64,
            ),
            Field::req(
                PROFILE_LAUNCH_VALUE_WEI,
                "wallet_qualification.funding_profile.launch_value_wei",
                U256,
            ),
            Field::req(
                PROFILE_BUY_VALUE_WEI,
                "wallet_qualification.funding_profile.buy_value_wei",
                U256,
            ),
            Field::opt(LAUNCHER_SUFFICIENT, "wallet_qualification.launcher_sufficient", Bool),
            Field::opt(BUYER_SUFFICIENT, "wallet_qualification.buyer_sufficient", Bool),
            Field::opt(FUNDING_PASS, "wallet_qualification.funding_pass", Bool),
            Field::req(NETWORK_ACTIONS, "network_actions", Text),
            Field::req(SIGNED_TX_SUBMITTED, "signed_tx_submitted", Bool),
            Field::req(BUNDLE_SUBMITTED, "bundle_submitted", Bool),
            Field::opt(DEPENDENCIES, "dependencies", Text),
        ],
    };
}

/// `local_simulation.json` of the frozen G0.6a DRY.
pub mod local_simulation {
    use super::*;

    pub const SCHEMA_NAME: u16 = 10;
    pub const LAUNCH_SUCCESS: u16 = 37;
    pub const BUY_SUCCESS: u16 = 38;
    pub const OUTPUT_DIFFERENCE: u16 = 43;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g06a.local-simulation.import.v1",
        record_type: 4,
        schema_version: 1,
        fields: &[
            Field::req(1, "_import.legacy_sha256", B256),
            Field::req(2, "_import.legacy_path", Text),
            Field::req(SCHEMA_NAME, "schema", Text),
            Field::req(11, "backend", Text),
            Field::req(12, "bsc_execution_profile", Text),
            Field::req(20, "state_source.block_number", U64),
            Field::req(21, "state_source.block_hash", B256),
            Field::req(22, "state_source.rpc_state_reads_only", Bool),
            Field::req(23, "state_source.valid_signed_tx_bytes_submitted", Bool),
            Field::req(30, "launch_tx_hash", B256),
            Field::req(31, "buy_tx_hash", B256),
            Field::req(32, "token", Address),
            Field::req(33, "launcher", Address),
            Field::req(34, "buyer", Address),
            Field::req(35, "launcher_pending_nonce", U64),
            Field::req(36, "buyer_pending_nonce", U64),
            Field::req(LAUNCH_SUCCESS, "launch_success", Bool),
            Field::req(BUY_SUCCESS, "buy_success", Bool),
            Field::req(39, "simulated_received_amount", U256),
            Field::req(40, "simulated_buyer_balance_after", U256),
            Field::req(41, "local_expected_output_input", U256),
            Field::req(42, "min_output", U256),
            // A signed difference in the legacy file: kept as its exact text.
            Field::req(OUTPUT_DIFFERENCE, "output_difference", Text),
            Field::req(44, "launch_execution_gas", U64),
            Field::req(45, "buy_execution_gas", U64),
            Field::req(46, "launch_intrinsic_gas", U64),
            Field::req(47, "buy_intrinsic_gas", U64),
            Field::req(48, "launch_gas_used", U64),
            Field::req(49, "buy_gas_used", U64),
            Field::req(50, "gas_accounting", Text),
            Field::req(51, "strictest_verified_builder_floor_wei", U256),
            Field::req(52, "builder_floor_evidence_sha256", B256),
            Field::req(53, "raw_signed_transactions_persisted", Bool),
            Field::req(54, "raw_bundle_persisted", Bool),
            Field::rep(55, "limitations", Text),
        ],
    };
}

/// `economics_crosscheck.json` of the frozen G0.6a DRY.
pub mod economics {
    use super::*;

    pub const STATUS: u16 = 12;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g06a.economics-evm-crosscheck.import.v1",
        record_type: 5,
        schema_version: 1,
        fields: &[
            Field::req(1, "_import.legacy_sha256", B256),
            Field::req(2, "_import.legacy_path", Text),
            Field::req(10, "schema", Text),
            Field::req(11, "preflight_path", Text),
            Field::req(STATUS, "status", Text),
            Field::req(20, "state_source.block_number", U64),
            Field::req(21, "state_source.block_hash", B256),
            Field::req(22, "state_source.rpc_state_reads_only", Bool),
            Field::req(30, "mainnet_estimate_gas_launch_success", Bool),
            Field::req(31, "mainnet_launch_gas_estimate", U64),
            Field::req(32, "mainnet_launch_gas_limit_120pct", U64),
            Field::req(33, "mainnet_eth_call_launch_success", Bool),
            Field::req(34, "mainnet_eth_call_launch_return", B256),
            Field::req(35, "mainnet_eth_call_predicted_token_match", Bool),
            Field::req(36, "portal", Address),
            Field::req(37, "tax_v3_impl", Address),
            Field::req(38, "launcher", Address),
            Field::req(39, "buyer", Address),
            Field::req(40, "local_only_funded_callers", Bool),
            Field::req(41, "salt", B256),
            Field::req(42, "salt_counter", U64),
            Field::req(43, "token", Address),
            Field::req(44, "launch_value_wei", U256),
            Field::req(45, "buy_value_wei", U256),
            Field::req(46, "buy_min_output_used_for_execution", U256),
            Field::req(47, "launch_success", Bool),
            Field::req(48, "buy_success", Bool),
            Field::req(49, "simulated_received_amount", U256),
            Field::req(50, "simulated_buyer_balance_after", U256),
            Field::req(51, "launch_execution_gas", U64),
            Field::req(52, "buy_execution_gas", U64),
            Field::req(53, "rust_expected_output_was_execution_input", Bool),
            Field::req(54, "signed_transactions_created", Bool),
            Field::req(55, "signed_transactions_submitted", Bool),
            Field::req(56, "bundle_submitted", Bool),
            Field::req(57, "network_actions", Text),
            Field::rep(58, "limitations", Text),
        ],
    };
}

/// The frozen 2026-10-11 builder gas-floor policy
/// (`g06a_builder_floor_addendum_2026-10-11.json`).
pub mod builder_policy {
    use super::*;

    pub const LEGACY_SCHEMA: u16 = 10;
    pub const OBSERVED_DATE: u16 = 11;
    pub const STATUS: u16 = 13;
    pub const BLOCKRAZOR_VERIFIED: u16 = 20;
    pub const FORTYEIGHTCLUB_VERIFIED: u16 = 30;
    pub const STRICTEST_FLOOR_WEI: u16 = 40;
    pub const STAGE1_GAS_PRICE_WEI: u16 = 42;
    pub const DRY3_READY: u16 = 44;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g06a.builder-floor-evidence-addendum.import.v1",
        record_type: 6,
        schema_version: 1,
        fields: &[
            Field::req(1, "_import.legacy_sha256", B256),
            Field::req(2, "_import.legacy_path", Text),
            Field::req(LEGACY_SCHEMA, "schema", Text),
            Field::req(OBSERVED_DATE, "observed_date", Text),
            Field::req(12, "scope", Text),
            Field::req(STATUS, "status", Text),
            Field::req(BLOCKRAZOR_VERIFIED, "blockrazor.verified", Bool),
            Field::req(21, "blockrazor.explicit_minimum_bundle_gas_price_wei", U256),
            Field::req(22, "blockrazor.rule", Text),
            Field::req(23, "blockrazor.source", Text),
            Field::req(24, "blockrazor.checked", Text),
            Field::req(FORTYEIGHTCLUB_VERIFIED, "fortyeightclub.verified", Bool),
            Field::req(31, "fortyeightclub.explicit_minimum_documented", Bool),
            Field::opt(32, "fortyeightclub.explicit_minimum_bundle_gas_price_wei", U256),
            Field::req(33, "fortyeightclub.ranking_rule", Text),
            Field::rep(34, "fortyeightclub.official_sources", Text),
            Field::req(35, "fortyeightclub.checked", Text),
            Field::req(36, "fortyeightclub.policy", Text),
            Field::req(STRICTEST_FLOOR_WEI, "strictest_explicit_verified_floor_wei", U256),
            Field::req(41, "strictest_explicit_verified_floor_source", Text),
            Field::req(STAGE1_GAS_PRICE_WEI, "stage1_signed_gas_price_policy_wei", U256),
            Field::req(43, "stage1_policy_note", Text),
            Field::req(DRY3_READY, "dry3_ready", Bool),
            Field::rep(45, "non_claims", Text),
            Field::req(50, "minimum_spend_probe.date", Text),
            Field::req(51, "minimum_spend_probe.current_rpc_gas_price_wei", U256),
            Field::req(52, "minimum_spend_probe.selected_signed_gas_price_wei", U256),
            Field::req(53, "minimum_spend_probe.selected_buy_value_wei", U256),
            Field::req(54, "minimum_spend_probe.rationale", Text),
        ],
    };
}

pub const ALL: &[&Schema] = &[
    &dry::SCHEMA,
    &preflight::SCHEMA,
    &local_simulation::SCHEMA,
    &economics::SCHEMA,
    &builder_policy::SCHEMA,
];
