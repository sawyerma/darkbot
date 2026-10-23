//! G0.6a DRY records: the imported, hash-bound DRY evidence of the G0.7
//! attempt and the read-only Mainnet preflight (imported baseline and the
//! current preflight G0.7 reads itself, one schema).

use super::{PROVENANCE_OBSERVED_UNIX_MS, PROVENANCE_TOOL};
use crate::{Field, Kind::*, Presence, Schema};

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

    /// The fields of both variants after their provenance (tags 1–4).
    macro_rules! fields {
        ($($head:expr),* $(,)?) => {
            &[
                $($head,)*
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
            ]
        };
    }

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g06a.self-launch-bundle-dry.import.v1",
        record_type: 2,
        schema_version: 1,
        fields: fields!(
            Field::req(1, "_import.legacy_sha256", B256),
            Field::req(2, "_import.legacy_path", Text),
        ),
    };

    /// The same fields as the native output of a requalification DRY run,
    /// with provenance instead of a legacy file.
    pub const NATIVE: Schema = Schema {
        name: "darkbot.g06a.self-launch-bundle-dry.v2",
        record_type: 19,
        schema_version: 1,
        fields: fields!(
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64
            ),
        ),
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
            Field::req(
                PORTAL_IMPLEMENTATION_CODE_BYTES,
                "portal_implementation_code_bytes",
                U64,
            ),
            Field::req(
                PORTAL_IMPLEMENTATION_CODE_SHA256,
                "portal_implementation_code_sha256",
                B256,
            ),
            Field::req(TAX_V3_IMPL, "tax_v3_impl", Address),
            Field::opt(TAX_V3_IMPL_SOURCE, "tax_v3_impl_source", Text),
            Field::req(TAX_V3_IMPL_CODE_BYTES, "tax_v3_impl_code_bytes", U64),
            Field::req(TAX_V3_IMPL_CODE_SHA256, "tax_v3_impl_code_sha256", B256),
            Field::req(GAS_PRICE_WEI, "gas_price_wei", U256),
            Field::req(QUOTE_ENABLED, "native_quote_configuration.enabled", U256),
            Field::req(
                QUOTE_DEFAULT_CURVE_ID,
                "native_quote_configuration.default_curve_id",
                U256,
            ),
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
            Field::req(
                TAX_ON_BONDING_CURVE_ENABLED,
                "tax_on_bonding_curve_enabled",
                Bool,
            ),
            Field::opt(LAUNCHER_ADDRESS, "launcher_address", Address),
            Field::opt(LAUNCHER_PENDING_NONCE, "launcher_pending_nonce", U64),
            Field::opt(LAUNCHER_BALANCE_WEI, "launcher_balance_wei", U256),
            Field::opt(BUYER_ADDRESS, "buyer_address", Address),
            Field::opt(BUYER_PENDING_NONCE, "buyer_pending_nonce", U64),
            Field::opt(BUYER_BALANCE_WEI, "buyer_balance_wei", U256),
            Field::req(
                ADDRESSES_BOUND,
                "wallet_qualification.addresses_bound",
                Bool,
            ),
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
            Field::opt(
                LAUNCHER_SUFFICIENT,
                "wallet_qualification.launcher_sufficient",
                Bool,
            ),
            Field::opt(
                BUYER_SUFFICIENT,
                "wallet_qualification.buyer_sufficient",
                Bool,
            ),
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

    /// The fields of both variants after their provenance (tags 1–4).
    macro_rules! fields {
        ($($head:expr),* $(,)?) => {
            &[
                $($head,)*
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
            ]
        };
    }

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g06a.local-simulation.import.v1",
        record_type: 4,
        schema_version: 1,
        fields: fields!(
            Field::req(1, "_import.legacy_sha256", B256),
            Field::req(2, "_import.legacy_path", Text),
        ),
    };

    /// The same fields as the native output of a requalification DRY run,
    /// with provenance instead of a legacy file.
    pub const NATIVE: Schema = Schema {
        name: "darkbot.g06a.local-simulation.v2",
        record_type: 20,
        schema_version: 1,
        fields: fields!(
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64
            ),
        ),
    };
}

/// `economics_crosscheck.json` of the frozen G0.6a DRY.
pub mod economics {
    use super::*;

    pub const STATUS: u16 = 12;

    /// The fields of both variants after their provenance (tags 1–4).
    macro_rules! fields {
        ($($head:expr),* $(,)?) => {
            &[
                $($head,)*
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
            ]
        };
    }

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g06a.economics-evm-crosscheck.import.v1",
        record_type: 5,
        schema_version: 1,
        fields: fields!(
            Field::req(1, "_import.legacy_sha256", B256),
            Field::req(2, "_import.legacy_path", Text),
        ),
    };

    /// The same fields as the native output of a requalification DRY run,
    /// with provenance instead of a legacy file.
    pub const NATIVE: Schema = Schema {
        name: "darkbot.g06a.economics-evm-crosscheck.v2",
        record_type: 21,
        schema_version: 1,
        fields: fields!(
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64
            ),
        ),
    };

    /// SHA-256 over the candidate salt domain and the salt (the commitment
    /// of the candidate record).
    pub const SALT_COMMITMENT: u16 = 60;

    /// The independent EVM cross-check of a secret-salt candidate (SELF_B,
    /// plan v8/v9.1 B2): local EVM only, no salt search and no remote
    /// estimateGas/eth_call; the salt only as its commitment. The other
    /// fields keep the names and tags of the native record.
    pub const PRIVATE: Schema = Schema {
        name: "darkbot.g06a.economics-evm-crosscheck.private.v1",
        record_type: 35,
        schema_version: 1,
        fields: &[
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(11, "preflight_path", Text),
            Field::req(STATUS, "status", Text),
            Field::req(20, "state_source.block_number", U64),
            Field::req(21, "state_source.block_hash", B256),
            Field::req(22, "state_source.rpc_state_reads_only", Bool),
            Field::req(36, "portal", Address),
            Field::req(37, "tax_v3_impl", Address),
            Field::req(38, "launcher", Address),
            Field::req(39, "buyer", Address),
            Field::req(40, "local_only_funded_callers", Bool),
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
            Field::req(SALT_COMMITMENT, "salt_commitment", B256),
        ],
    };
}

/// The builder gas-floor policy: the frozen 2026-10-11 import
/// (`g06a_builder_floor_addendum_2026-10-11.json`) and its native
/// re-determination.
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
    pub const SOURCES: u16 = 60;

    /// The fields of both variants after their provenance (tags 1–4), then
    /// a variant's own fields.
    macro_rules! fields {
        ($($head:expr),* $(,)? $(; $($tail:expr),* $(,)?)?) => {
            &[
                $($head,)*
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
                $($($tail,)*)?
            ]
        };
    }

    /// One official documentation page as the refresh fetched it.
    pub mod source {
        use super::*;

        pub const URL: u16 = 1;
        pub const HTTP_STATUS: u16 = 2;
        pub const FETCHED_UNIX_MS: u16 = 3;
        pub const BODY_SHA256: u16 = 4;
        pub const BODY_BYTES: u16 = 5;
        pub const FINDING: u16 = 6;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g06a.builder-floor-policy.source",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(URL, "url", Text),
                Field::req(HTTP_STATUS, "http_status", U64),
                Field::req(FETCHED_UNIX_MS, "fetched_unix_ms", U64),
                Field::req(BODY_SHA256, "body_sha256", B256),
                Field::req(BODY_BYTES, "body_bytes", U64),
                Field::req(FINDING, "finding", Text),
            ],
        };
    }

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g06a.builder-floor-evidence-addendum.import.v1",
        record_type: 6,
        schema_version: 1,
        fields: fields!(
            Field::req(1, "_import.legacy_sha256", B256),
            Field::req(2, "_import.legacy_path", Text),
        ),
    };

    /// The same fields, re-determined by `g06a_builder_policy_record` from
    /// the official documentation pages it fetched (`sources`: what each
    /// page said, when, and the SHA-256 of the body it read), with
    /// provenance instead of a legacy file. Its freshness is the fetch time
    /// of its oldest source, never a typed-in date.
    pub const NATIVE: Schema = Schema {
        name: "darkbot.g06a.builder-floor-policy.v2",
        record_type: 22,
        schema_version: 2,
        fields: fields!(
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64
            );
            Field::nested(SOURCES, "sources", Presence::Repeated, &source::SCHEMA),
        ),
    };
}

/// The local Curve-20 quote of a requalification DRY run.
pub mod quote {
    use super::*;

    pub const PREFLIGHT_RECORD_SHA256: u16 = 10;
    pub const CURVE_ID: u16 = 11;
    pub const BUY_VALUE_WEI: u16 = 12;
    pub const PROTOCOL_FEE_BPS: u16 = 13;
    pub const BUY_TAX_BPS: u16 = 14;
    pub const TAX_ON_BONDING_CURVE_ENABLED: u16 = 15;
    pub const PROTOCOL_FEE_WEI: u16 = 16;
    pub const BONDING_CURVE_TAX_WEI: u16 = 17;
    pub const COMBINED_FEE_WEI: u16 = 18;
    pub const NET_RESERVE_INPUT_WEI: u16 = 19;
    pub const RESERVE_AFTER_WEI: u16 = 20;
    pub const CIRCULATING_BEFORE: u16 = 21;
    pub const EXPECTED_OUTPUT: u16 = 22;
    pub const SLIPPAGE_BPS: u16 = 23;
    pub const MIN_OUTPUT: u16 = 24;
    pub const NETWORK_ACTIONS: u16 = 25;
    pub const AUTHORITY: u16 = 26;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g06a.native-bnb-quote.v2",
        record_type: 23,
        schema_version: 1,
        fields: &[
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(PREFLIGHT_RECORD_SHA256, "preflight_record_sha256", B256),
            Field::req(CURVE_ID, "curve_id", U64),
            Field::req(BUY_VALUE_WEI, "buy_value_wei", U256),
            Field::req(PROTOCOL_FEE_BPS, "protocol_fee_bps", U64),
            Field::req(BUY_TAX_BPS, "buy_tax_bps", U64),
            Field::req(
                TAX_ON_BONDING_CURVE_ENABLED,
                "tax_on_bonding_curve_enabled",
                Bool,
            ),
            Field::req(PROTOCOL_FEE_WEI, "protocol_fee_wei", U256),
            Field::req(BONDING_CURVE_TAX_WEI, "bonding_curve_tax_wei", U256),
            Field::req(COMBINED_FEE_WEI, "combined_fee_wei", U256),
            Field::req(NET_RESERVE_INPUT_WEI, "net_reserve_input_wei", U256),
            Field::req(RESERVE_AFTER_WEI, "reserve_after_wei", U256),
            Field::req(CIRCULATING_BEFORE, "circulating_before", U256),
            Field::req(EXPECTED_OUTPUT, "expected_output", U256),
            Field::req(SLIPPAGE_BPS, "slippage_bps", U64),
            Field::req(MIN_OUTPUT, "min_output", U256),
            Field::req(NETWORK_ACTIONS, "network_actions", Text),
            Field::req(AUTHORITY, "authority", Text),
        ],
    };
}

/// Step R0 of a requalification: every `Upgraded(address)` event of the
/// Portal proxy since the frozen DRY head, with complete coverage that needs
/// no `eth_getLogs`: every header of the range is hash-verified
/// (keccak256(rlp(header)) equals its hash) and linked by its parent hash
/// from `first_parent_hash` to `last_block_hash`; its `logsBloom` proves the
/// absence of the event (a bloom has no false negatives), and every block
/// whose bloom could hold it is read in full (`eth_getBlockReceipts`) and its
/// receipts verified against the header's `receiptsRoot`. Each event is
/// further bound by the implementation slot before and after it on an
/// archive endpoint; the record carries the implementation now active and
/// its code hash.
pub mod portal_upgrade {
    use super::*;

    pub const PROXY: u16 = 10;
    pub const SCANNED_FROM_BLOCK: u16 = 11;
    pub const SCANNED_TO_BLOCK: u16 = 12;
    pub const UPGRADES: u16 = 13;
    pub const CURRENT_IMPLEMENTATION: u16 = 14;
    pub const BASELINE_IMPLEMENTATION: u16 = 15;
    pub const COVERAGE_METHOD: u16 = 16;
    pub const CHAIN_SOURCE: u16 = 17;
    pub const ARCHIVE_SOURCE: u16 = 18;
    pub const HEADERS_VERIFIED: u16 = 19;
    pub const BLOOM_CANDIDATE_BLOCKS: u16 = 20;
    pub const CURRENT_IMPLEMENTATION_CODE_SHA256: u16 = 21;
    pub const CURRENT_IMPLEMENTATION_CODE_BYTES: u16 = 22;
    pub const FIRST_PARENT_HASH: u16 = 23;
    pub const LAST_BLOCK_HASH: u16 = 24;

    /// The only coverage method the requalification accepts.
    pub const METHOD: &str = "header-hash-chain+logs-bloom+receipts-root";

    pub mod upgrade {
        use super::*;

        pub const BLOCK: u16 = 1;
        pub const BLOCK_HASH: u16 = 2;
        pub const TRANSACTION_HASH: u16 = 3;
        pub const LOG_INDEX: u16 = 4;
        pub const IMPLEMENTATION: u16 = 5;
        pub const RECEIPT_STATUS: u16 = 6;
        pub const SLOT_BEFORE: u16 = 7;
        pub const SLOT_AFTER: u16 = 8;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g07.portal-upgrade-scan.upgrade",
            record_type: 0,
            schema_version: 2,
            fields: &[
                Field::req(BLOCK, "block", U64),
                Field::req(BLOCK_HASH, "block_hash", B256),
                Field::req(TRANSACTION_HASH, "transaction_hash", B256),
                Field::req(LOG_INDEX, "log_index", U64),
                Field::req(IMPLEMENTATION, "implementation", Address),
                Field::req(RECEIPT_STATUS, "receipt_status", U64),
                Field::req(SLOT_BEFORE, "implementation_slot_before", Address),
                Field::req(SLOT_AFTER, "implementation_slot_after", Address),
            ],
        };
    }

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g07.portal-upgrade-scan.v2",
        record_type: 24,
        schema_version: 2,
        fields: &[
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(PROVENANCE_OBSERVED_UNIX_MS, "_provenance.observed_unix_ms", U64),
            Field::req(PROXY, "proxy", Address),
            Field::req(SCANNED_FROM_BLOCK, "scanned_from_block", U64),
            Field::req(SCANNED_TO_BLOCK, "scanned_to_block", U64),
            Field::nested(UPGRADES, "upgrades", Presence::Repeated, &upgrade::SCHEMA),
            Field::req(CURRENT_IMPLEMENTATION, "current_implementation", Address),
            Field::req(BASELINE_IMPLEMENTATION, "baseline_implementation", Address),
            Field::req(COVERAGE_METHOD, "coverage_method", Text),
            Field::req(CHAIN_SOURCE, "chain_source", Text),
            Field::req(ARCHIVE_SOURCE, "archive_source", Text),
            Field::req(HEADERS_VERIFIED, "headers_verified", U64),
            Field::req(BLOOM_CANDIDATE_BLOCKS, "bloom_candidate_blocks", U64),
            Field::req(
                CURRENT_IMPLEMENTATION_CODE_SHA256,
                "current_implementation_code_sha256",
                B256,
            ),
            Field::req(
                CURRENT_IMPLEMENTATION_CODE_BYTES,
                "current_implementation_code_bytes",
                U64,
            ),
            Field::req(FIRST_PARENT_HASH, "first_parent_hash", B256),
            Field::req(LAST_BLOCK_HASH, "last_block_hash", B256),
        ],
    };
}

/// The typed acceptance of a requalification DRY run against the frozen
/// DRY (`g06a_requal_check`): every check with its outcome, the records it
/// evaluated and the verdict `FLAP_REQUALIFICATION` (PASS or STOP).
pub mod requalification {
    use super::*;

    pub const VERDICT: u16 = 10;
    pub const REQUAL_DIR: u16 = 11;
    pub const FROZEN_DIR: u16 = 12;
    pub const CHECKS: u16 = 13;
    pub const RECORDS: u16 = 14;
    pub const UPGRADE_BLOCK: u16 = 15;
    pub const UPGRADE_TRANSACTION: u16 = 16;
    pub const OLD_IMPLEMENTATION: u16 = 17;
    pub const NEW_IMPLEMENTATION: u16 = 18;
    pub const NEW_IMPLEMENTATION_CODE_SHA256: u16 = 19;
    pub const PORTAL_VERSION: u16 = 20;
    pub const LAUNCH_TX_HASH: u16 = 21;
    pub const BUY_TX_HASH: u16 = 22;

    pub mod check {
        use super::*;

        pub const NAME: u16 = 1;
        pub const STATUS: u16 = 2;
        pub const DETAIL: u16 = 3;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g06a.requalification.check",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(NAME, "name", Text),
                Field::req(STATUS, "status", Text),
                Field::req(DETAIL, "detail", Text),
            ],
        };
    }

    pub mod bound {
        use super::*;

        pub const NAME: u16 = 1;
        pub const SHA256: u16 = 2;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g06a.requalification.record",
            record_type: 0,
            schema_version: 1,
            fields: &[Field::req(NAME, "name", Text), Field::req(SHA256, "sha256", B256)],
        };
    }

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g06a.requalification-status.v1",
        record_type: 26,
        schema_version: 1,
        fields: &[
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(PROVENANCE_OBSERVED_UNIX_MS, "_provenance.observed_unix_ms", U64),
            Field::req(VERDICT, "flap_requalification", Text),
            Field::req(REQUAL_DIR, "requal_dir", Text),
            Field::req(FROZEN_DIR, "frozen_dir", Text),
            Field::nested(CHECKS, "checks", Presence::Repeated, &check::SCHEMA),
            Field::nested(RECORDS, "records", Presence::Repeated, &bound::SCHEMA),
            Field::opt(UPGRADE_BLOCK, "upgrade.block", U64),
            Field::opt(UPGRADE_TRANSACTION, "upgrade.transaction_hash", B256),
            Field::opt(OLD_IMPLEMENTATION, "upgrade.old_implementation", Address),
            Field::opt(NEW_IMPLEMENTATION, "upgrade.new_implementation", Address),
            Field::opt(
                NEW_IMPLEMENTATION_CODE_SHA256,
                "upgrade.new_implementation_code_sha256",
                B256,
            ),
            Field::opt(PORTAL_VERSION, "portal_version", Text),
            Field::opt(LAUNCH_TX_HASH, "transactions.launch_tx_hash", B256),
            Field::opt(BUY_TX_HASH, "transactions.buy_tx_hash", B256),
        ],
    };
}

/// The launch-path code identity of a requalification (v9.1 C1/R1): every
/// contract that had code at the pinned block and executed in the local EVM
/// run of the candidate's launch and first BUY, with its code SHA-256, and
/// every storage word of such a contract that selects an executed code
/// address (proxy implementation slots, storage-selected clone targets).
/// Re-verified at the current pinned block before any release.
pub mod launch_path {
    use super::*;

    pub const BLOCK_NUMBER: u16 = 10;
    pub const BLOCK_HASH: u16 = 11;
    pub const PORTAL: u16 = 12;
    pub const METHOD: u16 = 13;
    pub const TOKEN: u16 = 14;
    pub const LAUNCH_SUCCESS: u16 = 15;
    pub const BUY_SUCCESS: u16 = 16;
    pub const LAUNCH_RETURN_TOKEN: u16 = 17;
    /// One code entry per index of these four equally long lists.
    pub const CODE_ADDRESS: u16 = 20;
    pub const CODE_SHA256: u16 = 21;
    pub const CODE_BYTES: u16 = 22;
    /// How it executed, e.g. "launch:DELEGATECALL buy:CALL".
    pub const CODE_ROLES: u16 = 23;
    /// One storage selector per index of these four equally long lists.
    pub const STORAGE_ADDRESS: u16 = 30;
    pub const STORAGE_SLOT: u16 = 31;
    pub const STORAGE_VALUE: u16 = 32;
    /// The executed code address the word selects.
    pub const STORAGE_SELECTS: u16 = 33;
    /// The trace method of `tools/g06a_launch_path_trace.cjs`.
    pub const TRACE_METHOD: &str = "local-evm-message+sload-trace:newTokenV6+swapExactInput";

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g06a.launch-path",
        record_type: 33,
        schema_version: 1,
        fields: &[
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(BLOCK_NUMBER, "block_number", U64),
            Field::req(BLOCK_HASH, "block_hash", B256),
            Field::req(PORTAL, "portal", Address),
            Field::req(METHOD, "method", Text),
            Field::req(TOKEN, "token", Address),
            Field::req(LAUNCH_SUCCESS, "launch_success", Bool),
            Field::req(BUY_SUCCESS, "buy_success", Bool),
            Field::req(LAUNCH_RETURN_TOKEN, "launch_return_token", Address),
            Field::rep(CODE_ADDRESS, "code.address", Address),
            Field::rep(CODE_SHA256, "code.code_sha256", B256),
            Field::rep(CODE_BYTES, "code.code_bytes", U64),
            Field::rep(CODE_ROLES, "code.roles", Text),
            Field::rep(STORAGE_ADDRESS, "storage.address", Address),
            Field::rep(STORAGE_SLOT, "storage.slot", B256),
            Field::rep(STORAGE_VALUE, "storage.value", B256),
            Field::rep(STORAGE_SELECTS, "storage.selects", Address),
        ],
    };
}

/// The PREPARE → SIGN handoff of a DRY run (plan v9.1 D3): what a key-free
/// PREPARE at H1 passes to the later SIGN at H2 — only the validated R0
/// coverage record (by SHA-256 and its stated chain) and the unchanged
/// candidate reference, never a salt and no H1 economics.
pub mod prepare_handoff {
    use super::*;

    pub const MODE: u16 = 10;
    pub const R0_RECORD: u16 = 11;
    pub const R0_RECORD_SHA256: u16 = 12;
    pub const R0_FROM_BLOCK: u16 = 13;
    pub const R0_TO_BLOCK: u16 = 14;
    pub const R0_FIRST_PARENT_HASH: u16 = 15;
    pub const R0_LAST_BLOCK_HASH: u16 = 16;
    pub const CANDIDATE_PROFILE: u16 = 20;
    pub const CANDIDATE_ID: u16 = 21;
    pub const CANDIDATE_GENERATION: u16 = 22;
    /// Diagnostic only: SIGN computes its own preflight at H2.
    pub const H1_PREFLIGHT_SHA256: u16 = 30;

    pub const MODE_TEXT: &str = "PREPARE_ONLY / NOT_REQUALIFIED";

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g06a.prepare-handoff",
        record_type: 36,
        schema_version: 1,
        fields: &[
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(MODE, "mode", Text),
            Field::req(R0_RECORD, "r0.record", Text),
            Field::req(R0_RECORD_SHA256, "r0.record_sha256", B256),
            Field::req(R0_FROM_BLOCK, "r0.scanned_from_block", U64),
            Field::req(R0_TO_BLOCK, "r0.scanned_to_block", U64),
            Field::req(R0_FIRST_PARENT_HASH, "r0.first_parent_hash", B256),
            Field::req(R0_LAST_BLOCK_HASH, "r0.last_block_hash", B256),
            Field::req(CANDIDATE_PROFILE, "candidate.profile", Text),
            Field::req(CANDIDATE_ID, "candidate.candidate_id", B256),
            Field::req(CANDIDATE_GENERATION, "candidate.selection_generation", U64),
            Field::req(H1_PREFLIGHT_SHA256, "h1.preflight_record_sha256", B256),
        ],
    };
}

pub const ALL: &[&Schema] = &[
    &dry::SCHEMA,
    &preflight::SCHEMA,
    &local_simulation::SCHEMA,
    &economics::SCHEMA,
    &builder_policy::SCHEMA,
    &dry::NATIVE,
    &local_simulation::NATIVE,
    &economics::NATIVE,
    &economics::PRIVATE,
    &builder_policy::NATIVE,
    &quote::SCHEMA,
    &portal_upgrade::SCHEMA,
    &requalification::SCHEMA,
    &launch_path::SCHEMA,
    &prepare_handoff::SCHEMA,
];
