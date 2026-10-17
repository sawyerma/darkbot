//! G0.7 records: the producer-witness source contract (imported), the
//! binary acceptance contract of attempt 1, the attempt evidence and the
//! builder-probe evidence.

use crate::{Field, Kind::*, Presence, Schema};

/// `g07_producer_witness_source_attempt1.json`.
pub mod witness_source {
    use super::*;

    pub const LEGACY_SCHEMA: u16 = 10;
    pub const ATTEMPT_ID: u16 = 11;
    pub const SOURCE_READY: u16 = 12;
    pub const ORIGINAL_EXECUTION_ONLY: u16 = 13;
    pub const REQUIRED_WITNESS_SCHEMA: u16 = 14;
    pub const DELIVERY_MAY_BE_ASYNCHRONOUS: u16 = 15;
    pub const CANONICAL_IDENTITY_BINDING_REQUIRED: u16 = 16;
    pub const CLOCK_METADATA_REQUIRED: u16 = 17;
    pub const SOURCE_DESCRIPTION: u16 = 18;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.producer-witness-source.import.v1",
        record_type: 7,
        schema_version: 1,
        fields: &[
            Field::req(1, "_import.legacy_sha256", B256),
            Field::req(2, "_import.legacy_path", Text),
            Field::req(LEGACY_SCHEMA, "schema", Text),
            Field::req(ATTEMPT_ID, "attempt_id", Text),
            Field::req(SOURCE_READY, "source_ready", Bool),
            Field::req(
                ORIGINAL_EXECUTION_ONLY,
                "original_successful_producer_execution_only",
                Bool,
            ),
            Field::req(REQUIRED_WITNESS_SCHEMA, "required_witness_schema", Text),
            Field::req(
                DELIVERY_MAY_BE_ASYNCHRONOUS,
                "delivery_may_be_asynchronous",
                Bool,
            ),
            Field::req(
                CANONICAL_IDENTITY_BINDING_REQUIRED,
                "canonical_identity_binding_required",
                Bool,
            ),
            Field::req(
                CLOCK_METADATA_REQUIRED,
                "clock_and_uncertainty_metadata_required",
                Bool,
            ),
            Field::req(SOURCE_DESCRIPTION, "source_description", Text),
            Field::req(19, "operator_reference", Text),
        ],
    };
}

/// The binary acceptance contract of G0.7 attempt 1. It replaces the JSON
/// v2 proposal/draft as the only possible execution authority; the guard
/// binds the SHA-256 of its exact file bytes at compile time.
pub mod contract {
    use super::*;

    pub const STATUS: u16 = 10;
    pub const APPROVED: u16 = 100;
    pub const EXECUTION_READY: u16 = 101;
    pub const LAUNCHER: u16 = 104;
    pub const BUYER: u16 = 105;
    pub const ATTEMPT_ID: u16 = 108;
    pub const DRY_RECORD_SHA256: u16 = 110;
    pub const PREFLIGHT_RECORD_SHA256: u16 = 111;
    pub const BUILDER_POLICY_RECORD_SHA256: u16 = 112;
    pub const WITNESS_SOURCE_RECORD_SHA256: u16 = 113;
    pub const DRY_LEGACY_SHA256: u16 = 114;
    pub const PREFLIGHT_LEGACY_SHA256: u16 = 115;
    pub const BUILDER_POLICY_LEGACY_SHA256: u16 = 116;
    pub const WITNESS_SOURCE_LEGACY_SHA256: u16 = 117;
    pub const DRY_RECORD_PATH: u16 = 120;
    pub const PREFLIGHT_RECORD_PATH: u16 = 121;
    pub const BUILDER_POLICY_RECORD_PATH: u16 = 122;
    pub const WITNESS_SOURCE_RECORD_PATH: u16 = 123;
    pub const CHAIN_ID: u16 = 200;
    pub const PORTAL: u16 = 204;
    pub const MAX_ATTEMPTS: u16 = 400;
    pub const MAX_SLIPPAGE_BPS: u16 = 603;
    pub const MAX_SIGNED_GAS_PRICE_WEI: u16 = 613;
    pub const MAX_DIRECT_BUILDER_TIP_WEI: u16 = 621;
    pub const MAX_BUY_VALUE: u16 = 700;
    pub const MAX_LAUNCH_VALUE: u16 = 701;
    pub const MAX_LAUNCH_GAS_LIMIT: u16 = 702;
    pub const MAX_BUY_GAS_LIMIT: u16 = 703;
    pub const MAX_GAS_PRICE_WEI: u16 = 704;
    pub const MAX_TOTAL_PER_ATTEMPT: u16 = 706;
    pub const ABSOLUTE_BUDGET: u16 = 707;
    pub const CAPTURE_REQUIRED: u16 = 1007;
    pub const WITNESS_CONTRACT_REQUIRED: u16 = 1008;

    pub const CRITERION: Schema = Schema {
        name: "darkbot.g07.acceptance-contract.dry-criterion",
        record_type: 0,
        schema_version: 1,
        fields: &[Field::req(1, "id", Text), Field::req(2, "pass", Text)],
    };

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g07.acceptance-contract.v3",
        record_type: 1,
        schema_version: 3,
        fields: &[
            Field::req(STATUS, "status", Text),
            Field::req(11, "supersedes", Text),
            Field::req(12, "purpose", Text),
            Field::req(APPROVED, "authorization.approved", Bool),
            Field::req(EXECUTION_READY, "authorization.execution_ready", Bool),
            Field::req(102, "authorization.approval_required_for", Text),
            Field::req(103, "authorization.note", Text),
            Field::req(LAUNCHER, "authorization.launcher_address", Address),
            Field::req(BUYER, "authorization.buyer_address", Address),
            Field::req(106, "authorization.execution_authority", Text),
            Field::req(107, "authorization.approval_scope", Text),
            Field::req(ATTEMPT_ID, "authorization.attempt_id", Text),
            Field::req(
                DRY_RECORD_SHA256,
                "authorization.evidence_bindings.dry_record_sha256",
                B256,
            ),
            Field::req(
                PREFLIGHT_RECORD_SHA256,
                "authorization.evidence_bindings.preflight_record_sha256",
                B256,
            ),
            Field::req(
                BUILDER_POLICY_RECORD_SHA256,
                "authorization.evidence_bindings.builder_policy_record_sha256",
                B256,
            ),
            Field::req(
                WITNESS_SOURCE_RECORD_SHA256,
                "authorization.evidence_bindings.producer_witness_source_record_sha256",
                B256,
            ),
            Field::req(
                DRY_LEGACY_SHA256,
                "authorization.evidence_bindings.dry_legacy_sha256",
                B256,
            ),
            Field::req(
                PREFLIGHT_LEGACY_SHA256,
                "authorization.evidence_bindings.preflight_legacy_sha256",
                B256,
            ),
            Field::req(
                BUILDER_POLICY_LEGACY_SHA256,
                "authorization.evidence_bindings.builder_policy_legacy_sha256",
                B256,
            ),
            Field::req(
                WITNESS_SOURCE_LEGACY_SHA256,
                "authorization.evidence_bindings.producer_witness_source_legacy_sha256",
                B256,
            ),
            Field::req(
                DRY_RECORD_PATH,
                "authorization.evidence_paths.dry_record",
                Text,
            ),
            Field::req(
                PREFLIGHT_RECORD_PATH,
                "authorization.evidence_paths.preflight_record",
                Text,
            ),
            Field::req(
                BUILDER_POLICY_RECORD_PATH,
                "authorization.evidence_paths.builder_policy_record",
                Text,
            ),
            Field::req(
                WITNESS_SOURCE_RECORD_PATH,
                "authorization.evidence_paths.producer_witness_source_record",
                Text,
            ),
            Field::req(
                124,
                "authorization.evidence_paths.builder_requirements_refresh",
                Text,
            ),
            Field::req(
                125,
                "authorization.evidence_paths.builder_requirements_refresh_sha256",
                B256,
            ),
            Field::req(126, "authorization.evidence_paths.note", Text),
            Field::req(CHAIN_ID, "scope.chain_id", U64),
            Field::req(201, "scope.venue", Text),
            Field::req(202, "scope.launch", Text),
            Field::req(203, "scope.tokenVersion", U64),
            Field::req(
                PORTAL,
                "scope.portal_expected_from_current_evidence",
                Address,
            ),
            Field::req(205, "scope.portal_rule", Text),
            Field::req(206, "scope.launch_mode", Text),
            Field::req(207, "scope.quoteToken", Address),
            Field::req(208, "scope.quoteAmt", U256),
            Field::req(209, "scope.buy", Text),
            Field::req(210, "scope.historical_g03_authority", Text),
            Field::req(300, "wallet_separation.launcher", Address),
            Field::req(301, "wallet_separation.buyer", Address),
            Field::req(302, "wallet_separation.must_be_distinct_addresses", Bool),
            Field::req(303, "wallet_separation.reason", Text),
            Field::req(304, "wallet_separation.secret_storage", Text),
            Field::req(MAX_ATTEMPTS, "attempts.maximum", U64),
            Field::req(401, "attempts.economic_attempt_definition", Text),
            Field::rep(402, "attempts.builder_fanout", Text),
            Field::req(403, "attempts.parallel", Bool),
            Field::req(404, "attempts.no_public_launch_broadcast", Bool),
            Field::req(405, "attempts.no_automatic_retry", Bool),
            Field::req(
                406,
                "attempts.no_parallel_fallback_buy_bundle_in_stage_1",
                Bool,
            ),
            Field::req(
                407,
                "attempts.selected_attempts_remain_in_denominator",
                Bool,
            ),
            Field::rep(500, "bundle_bytes.tx_order", Text),
            Field::req(
                501,
                "bundle_bytes.identical_raw_transactions_across_builders",
                Bool,
            ),
            Field::req(502, "bundle_bytes.reverting_transactions_allowed", Bool),
            Field::req(503, "bundle_bytes.max_block_window", Text),
            Field::req(504, "bundle_bytes.expiry_mismatch", Text),
            Field::req(505, "bundle_bytes.fortyeightclub.noMerge", Bool),
            Field::req(506, "bundle_bytes.fortyeightclub.positionFirst", Bool),
            Field::req(507, "bundle_bytes.fortyeightclub.rationale", Text),
            Field::req(508, "bundle_bytes.send_head_rule", Text),
            Field::req(
                600,
                "economics_validation.min_output.must_be_positive",
                Bool,
            ),
            Field::req(601, "economics_validation.min_output.source", Text),
            Field::req(
                602,
                "economics_validation.min_output.independent_cross_check",
                Text,
            ),
            Field::req(
                MAX_SLIPPAGE_BPS,
                "economics_validation.min_output.slippage_max_bps",
                U64,
            ),
            Field::req(
                604,
                "economics_validation.min_output.historical_min_output_reuse_forbidden",
                Bool,
            ),
            Field::req(610, "economics_validation.gas_floor.policy", Text),
            Field::req(
                611,
                "economics_validation.gas_floor.blockrazor_current_documented_reference",
                Text,
            ),
            Field::req(612, "economics_validation.gas_floor.fortyeightclub", Text),
            Field::req(
                MAX_SIGNED_GAS_PRICE_WEI,
                "economics_validation.gas_floor.max_signed_gas_price_wei",
                U256,
            ),
            Field::req(614, "economics_validation.gas_floor.freshness_rule", Text),
            Field::req(
                615,
                "economics_validation.gas_floor.bound_policy_superseded_fields",
                Text,
            ),
            Field::req(620, "economics_validation.bid_policy.stage_1", Text),
            Field::req(
                MAX_DIRECT_BUILDER_TIP_WEI,
                "economics_validation.bid_policy.max_direct_builder_tip_wei",
                U256,
            ),
            Field::req(
                622,
                "economics_validation.bid_policy.policy_change_requires_new_contract_version",
                Bool,
            ),
            Field::req(MAX_BUY_VALUE, "budget_wei.max_buy_value_per_attempt", U256),
            Field::req(
                MAX_LAUNCH_VALUE,
                "budget_wei.max_launch_call_value_per_attempt",
                U256,
            ),
            Field::req(MAX_LAUNCH_GAS_LIMIT, "budget_wei.max_launch_gas_limit", U64),
            Field::req(MAX_BUY_GAS_LIMIT, "budget_wei.max_buy_gas_limit", U64),
            Field::req(MAX_GAS_PRICE_WEI, "budget_wei.max_gas_price_wei", U256),
            Field::req(705, "budget_wei.max_gas_cost_per_attempt", U256),
            Field::req(
                MAX_TOTAL_PER_ATTEMPT,
                "budget_wei.max_total_per_attempt",
                U256,
            ),
            Field::req(
                ABSOLUTE_BUDGET,
                "budget_wei.absolute_max_total_all_attempts",
                U256,
            ),
            Field::req(708, "budget_wei.note", Text),
            Field::rep(800, "pre_arm_checks", Text),
            Field::req(900, "success.acquisition", Text),
            Field::req(901, "success.ordering", Text),
            Field::req(902, "success.builder_evidence", Text),
            Field::req(903, "success.timing", Text),
            Field::req(1000, "retention_observability.record_send_head", Bool),
            Field::req(1001, "retention_observability.record_inclusion_block", Bool),
            Field::req(1002, "retention_observability.record_builder", Bool),
            Field::req(1003, "retention_observability.record_validator", Bool),
            Field::req(1004, "retention_observability.record_bundle_status", Bool),
            Field::rep(
                1005,
                "retention_observability.on_expiry_without_inclusion",
                Text,
            ),
            Field::req(1006, "retention_observability.interpretation", Text),
            Field::req(
                CAPTURE_REQUIRED,
                "retention_observability.pre_arm_bounded_capture_required",
                Bool,
            ),
            Field::req(
                WITNESS_CONTRACT_REQUIRED,
                "retention_observability.producer_witness_contract_required_before_execution",
                Bool,
            ),
            Field::req(
                1009,
                "retention_observability.g07_local_attempt_state",
                Text,
            ),
            Field::req(1100, "thresholds.PASS", Text),
            Field::req(1101, "thresholds.ADAPT", Text),
            Field::req(1102, "thresholds.STOP", Text),
            Field::req(1200, "coverage.foreign_launch_coverage_requirement", Text),
            Field::req(1201, "coverage.note", Text),
            Field::req(1300, "send_guard.fail_closed", Bool),
            Field::rep(1301, "send_guard.required_checks", Text),
            Field::rep(1302, "send_guard.forbidden_authorities", Text),
            Field::req(1400, "dry_pass.status", Text),
            Field::req(1401, "dry_pass.valid_mainnet_bytes_may_leave_machine", Bool),
            Field::nested(1402, "dry_pass.criteria", Presence::Repeated, &CRITERION),
            Field::req(1403, "dry_pass.provider_prequalification", Text),
            Field::req(1404, "dry_pass.all_four_required", Bool),
            Field::req(1405, "dry_pass.evidence_scope_note", Text),
            Field::rep(1500, "pre_arm_check_status", Text),
        ],
    };
}

/// What one builder answered, decoded at the transport boundary: no raw
/// JSON-RPC body is kept, only its typed outcome and the SHA-256 and length
/// of the received bytes.
pub mod provider_result {
    use super::*;

    pub const PROVIDER: u16 = 1;
    pub const WRITE_STARTED_NS: u16 = 2;
    pub const WRITE_FLUSHED_NS: u16 = 3;
    pub const BYTES_MAY_HAVE_LEFT: u16 = 4;
    pub const HTTP_STATUS_LINE: u16 = 5;
    pub const HTTP_STATUS_CODE: u16 = 6;
    pub const RESPONSE_SHA256: u16 = 7;
    pub const RESPONSE_LEN: u16 = 8;
    pub const RPC_RESULT_HASH: u16 = 9;
    pub const RPC_RESULT_TEXT: u16 = 10;
    pub const RPC_ERROR_CODE: u16 = 11;
    pub const RPC_ERROR_MESSAGE: u16 = 12;
    pub const RESPONSE_UNDECODABLE: u16 = 13;
    pub const ERROR: u16 = 14;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g07.provider-result",
        record_type: 0,
        schema_version: 1,
        fields: &[
            Field::req(PROVIDER, "provider", Text).max(64),
            Field::opt(WRITE_STARTED_NS, "write_started_ns", U64),
            Field::opt(WRITE_FLUSHED_NS, "write_flushed_ns", U64),
            Field::req(BYTES_MAY_HAVE_LEFT, "bytes_may_have_left", Bool),
            Field::opt(HTTP_STATUS_LINE, "http_status_line", Text).max(256),
            Field::opt(HTTP_STATUS_CODE, "http_status_code", U64),
            Field::opt(RESPONSE_SHA256, "response_sha256", B256),
            Field::opt(RESPONSE_LEN, "response_len", U64),
            Field::opt(RPC_RESULT_HASH, "rpc_result_hash", B256),
            Field::opt(RPC_RESULT_TEXT, "rpc_result_text", Text).max(1024),
            Field::opt(RPC_ERROR_CODE, "rpc_error_code", I64),
            Field::opt(RPC_ERROR_MESSAGE, "rpc_error_message", Text).max(1024),
            Field::opt(RESPONSE_UNDECODABLE, "response_undecodable", Text).max(1024),
            Field::opt(ERROR, "error", Text).max(1024),
        ],
    };
}

/// The attempt evidence G0.7 writes atomically at the release, after the
/// send threads and at the end, and G0.8 reads.
pub mod evidence {
    use super::*;

    pub const GATE: u16 = 1;
    pub const ATTEMPT_ID: u16 = 2;
    pub const CLASSIFICATION: u16 = 3;
    pub const CHAIN_ID: u16 = 4;
    pub const PORTAL: u16 = 5;
    pub const NEXT_REQUIRED_GATE: u16 = 6;
    pub const CRITICAL_RULE: u16 = 7;
    pub const CONTRACT_PATH: u16 = 10;
    pub const CONTRACT_SHA256: u16 = 11;
    pub const DRY_PATH: u16 = 20;
    pub const DRY_SHA256: u16 = 21;
    pub const DRY_LEGACY_SHA256: u16 = 22;
    pub const PREFLIGHT_PATH: u16 = 23;
    pub const PREFLIGHT_SHA256: u16 = 24;
    pub const PREFLIGHT_LEGACY_SHA256: u16 = 25;
    pub const BUILDER_POLICY_PATH: u16 = 26;
    pub const BUILDER_POLICY_SHA256: u16 = 27;
    pub const BUILDER_POLICY_LEGACY_SHA256: u16 = 28;
    pub const WITNESS_SOURCE_PATH: u16 = 29;
    pub const WITNESS_SOURCE_SHA256: u16 = 30;
    pub const WITNESS_SOURCE_LEGACY_SHA256: u16 = 31;
    pub const STATE_DIR: u16 = 40;
    pub const SINGLE_USE_LATCH: u16 = 41;
    pub const ATTEMPT_NUMBER: u16 = 42;
    pub const SPENT_BEFORE_WEI: u16 = 43;
    pub const RESERVED_WORST_CASE_WEI: u16 = 44;
    pub const LAUNCHER: u16 = 50;
    pub const BUYER: u16 = 51;
    pub const DISTINCT: u16 = 52;
    pub const LAUNCHER_NONCE: u16 = 53;
    pub const BUYER_NONCE: u16 = 54;
    pub const PREFLIGHT: u16 = 60;
    pub const LAUNCH_TX_HASH: u16 = 70;
    pub const BUY_TX_HASH: u16 = 71;
    pub const TOKEN: u16 = 72;
    pub const TOKEN_CODE_BEFORE_EMPTY: u16 = 73;
    pub const SALT: u16 = 74;
    pub const LAUNCH_VALUE_WEI: u16 = 75;
    pub const BUY_VALUE_WEI: u16 = 76;
    pub const GAS_PRICE_WEI: u16 = 77;
    pub const LAUNCH_GAS_ESTIMATE: u16 = 78;
    pub const LAUNCH_GAS_LIMIT: u16 = 79;
    pub const BUY_GAS_LIMIT: u16 = 80;
    pub const LOCAL_EXPECTED_OUTPUT: u16 = 81;
    pub const MIN_OUTPUT: u16 = 82;
    pub const SLIPPAGE_BPS: u16 = 83;
    pub const CAPTURE_COMPLETE: u16 = 90;
    pub const CAPTURE_STATUS: u16 = 91;
    pub const JOURNAL_PATH: u16 = 92;
    pub const JOURNAL: u16 = 93;
    pub const CAPTURE_ERROR: u16 = 94;
    pub const BOTH_RECEIPTS_OBSERVED_NS: u16 = 110;
    pub const COVERAGE_END_NS: u16 = 111;
    pub const CANONICAL_WINDOW_RECONCILED_IN_G08: u16 = 112;
    pub const CANONICAL_WINDOW_RECONCILIATION: u16 = 113;
    pub const LAUNCH_RECEIPT_OBSERVED: u16 = 114;
    pub const BUY_RECEIPT_OBSERVED: u16 = 115;
    pub const POST_WINDOW_HEAD: u16 = 116;
    pub const T0_TO_T_BUY: u16 = 120;
    pub const WITNESS_SOURCE_DESCRIPTION: u16 = 121;
    pub const WITNESS_SOURCE_READY: u16 = 122;
    pub const TIMING_RULE: u16 = 123;
    pub const BUNDLE: u16 = 130;
    pub const SEND: u16 = 131;

    /// The local capture journal summary after it was closed.
    pub mod journal {
        use super::*;

        pub const RECORDS: u16 = 1;
        pub const FIRST_SEQUENCE: u16 = 2;
        pub const LAST_SEQUENCE: u16 = 3;
        pub const BYTES: u16 = 4;
        pub const SHA256: u16 = 5;
        pub const NEXT_SEQUENCE: u16 = 6;
        pub const ENQUEUED: u16 = 7;
        pub const DROPPED_FULL: u16 = 8;
        pub const REJECTED_OVERSIZE: u16 = 9;
        pub const DISCONNECTED: u16 = 10;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g07.evidence.journal",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(RECORDS, "records", U64),
                Field::opt(FIRST_SEQUENCE, "first_sequence", U64),
                Field::opt(LAST_SEQUENCE, "last_sequence", U64),
                Field::req(BYTES, "bytes", U64),
                Field::req(SHA256, "sha256", B256),
                Field::req(NEXT_SEQUENCE, "next_sequence", U64),
                Field::req(ENQUEUED, "enqueued", U64),
                Field::req(DROPPED_FULL, "dropped_full", U64),
                Field::req(REJECTED_OVERSIZE, "rejected_oversize", U64),
                Field::req(DISCONNECTED, "disconnected", U64),
            ],
        };
    }

    /// The bundle as released, set at the release.
    pub mod bundle {
        use super::*;

        pub const TX_ORDER: u16 = 1;
        pub const IDENTICAL_RAW_TXS: u16 = 2;
        pub const SEND_HEAD: u16 = 3;
        pub const MAX_BLOCK: u16 = 4;
        pub const FORTYEIGHTCLUB_MAX_TIMESTAMP: u16 = 5;
        pub const EXPIRY_RULE: u16 = 6;
        pub const SEND_HEAD_READ: u16 = 7;
        pub const SEND_HEAD_HASH: u16 = 8;
        pub const SEND_HEAD_MILLI_TIMESTAMP: u16 = 9;
        pub const LOCAL_UNIX_MS_AFTER_READ: u16 = 10;
        pub const HEAD_AGE_MS: u16 = 11;
        pub const EXCESS_STALENESS_MS: u16 = 12;
        pub const READ_ROUND_TRIP_NS: u16 = 13;
        pub const MAX_HEAD_AGE_MS: u16 = 14;
        pub const MAX_LOCAL_CLOCK_LAG_MS: u16 = 15;
        pub const PREFLIGHT_HEAD: u16 = 16;
        pub const PROBE_HEAD: u16 = 17;
        pub const PROBE_HEAD_AGE_MS: u16 = 18;
        pub const PROBE_EXCESS_STALENESS_MS: u16 = 19;
        pub const PROBE_READ_ROUND_TRIP_NS: u16 = 20;
        pub const PUBLIC_MEMPOOL_LAUNCH: u16 = 21;
        pub const AUTOMATIC_RETRY: u16 = 22;
        pub const FORTYEIGHTCLUB_NO_MERGE: u16 = 23;
        pub const FORTYEIGHTCLUB_POSITION_FIRST: u16 = 24;
        pub const BLOCKRAZOR_NO_MERGE: u16 = 25;
        pub const BLOCKRAZOR_POSITION_FIRST: u16 = 26;
        pub const FORTYEIGHTCLUB_BODY_SHA256: u16 = 27;
        pub const FORTYEIGHTCLUB_BODY_LEN: u16 = 28;
        pub const BLOCKRAZOR_BODY_SHA256: u16 = 29;
        pub const BLOCKRAZOR_BODY_LEN: u16 = 30;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g07.evidence.bundle",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::rep(TX_ORDER, "tx_order", Text),
                Field::req(
                    IDENTICAL_RAW_TXS,
                    "identical_raw_transactions_across_builders",
                    Bool,
                ),
                Field::req(SEND_HEAD, "send_head", U64),
                Field::req(MAX_BLOCK, "max_block", U64),
                Field::req(
                    FORTYEIGHTCLUB_MAX_TIMESTAMP,
                    "fortyeightclub_maxTimestamp",
                    U64,
                ),
                Field::req(EXPIRY_RULE, "expiry_rule", Text),
                Field::req(SEND_HEAD_READ, "send_head_source.read", Text),
                Field::req(SEND_HEAD_HASH, "send_head_source.hash", B256),
                Field::req(
                    SEND_HEAD_MILLI_TIMESTAMP,
                    "send_head_source.milli_timestamp",
                    U64,
                ),
                Field::req(
                    LOCAL_UNIX_MS_AFTER_READ,
                    "send_head_source.local_unix_ms_after_read",
                    U64,
                ),
                Field::req(HEAD_AGE_MS, "send_head_source.head_age_ms", I64),
                Field::req(
                    EXCESS_STALENESS_MS,
                    "send_head_source.excess_staleness_ms",
                    I64,
                ),
                Field::req(
                    READ_ROUND_TRIP_NS,
                    "send_head_source.read_round_trip_ns",
                    U64,
                ),
                Field::req(MAX_HEAD_AGE_MS, "send_head_source.max_head_age_ms", I64),
                Field::req(
                    MAX_LOCAL_CLOCK_LAG_MS,
                    "send_head_source.max_local_clock_lag_ms",
                    I64,
                ),
                Field::req(PREFLIGHT_HEAD, "send_head_source.preflight_head", U64),
                Field::req(PROBE_HEAD, "send_head_source.probe.head", U64),
                Field::req(PROBE_HEAD_AGE_MS, "send_head_source.probe.head_age_ms", I64),
                Field::req(
                    PROBE_EXCESS_STALENESS_MS,
                    "send_head_source.probe.excess_staleness_ms",
                    I64,
                ),
                Field::req(
                    PROBE_READ_ROUND_TRIP_NS,
                    "send_head_source.probe.read_round_trip_ns",
                    U64,
                ),
                Field::req(PUBLIC_MEMPOOL_LAUNCH, "public_mempool_launch", Bool),
                Field::req(AUTOMATIC_RETRY, "automatic_retry", Bool),
                Field::req(FORTYEIGHTCLUB_NO_MERGE, "fortyeightclub_noMerge", Bool),
                Field::req(
                    FORTYEIGHTCLUB_POSITION_FIRST,
                    "fortyeightclub_positionFirst",
                    Bool,
                ),
                Field::req(BLOCKRAZOR_NO_MERGE, "blockrazor_noMerge", Bool),
                Field::req(BLOCKRAZOR_POSITION_FIRST, "blockrazor_positionFirst", Bool),
                Field::req(
                    FORTYEIGHTCLUB_BODY_SHA256,
                    "fortyeightclub_body_sha256",
                    B256,
                ),
                Field::req(FORTYEIGHTCLUB_BODY_LEN, "fortyeightclub_body_len", U64),
                Field::req(BLOCKRAZOR_BODY_SHA256, "blockrazor_body_sha256", B256),
                Field::req(BLOCKRAZOR_BODY_LEN, "blockrazor_body_len", U64),
            ],
        };
    }

    /// The local send boundaries of the release, set at the release and
    /// completed with the provider results after the send threads.
    pub mod send {
        use super::*;

        pub const CLOCK_ID: u16 = 1;
        pub const BOOT_ID: u16 = 2;
        pub const RELEASE_BEFORE_NS: u16 = 3;
        pub const RELEASE_AFTER_NS: u16 = 4;
        pub const RELEASE_MARKER: u16 = 5;
        pub const PROVIDER_RESULTS_PENDING: u16 = 6;
        pub const FORTYEIGHTCLUB: u16 = 7;
        pub const BLOCKRAZOR: u16 = 8;
        pub const BOTH_FLUSH_COMPLETED: u16 = 9;
        pub const FLUSH_SPREAD_NS: u16 = 10;
        pub const ANY_BYTES_MAY_HAVE_LEFT: u16 = 11;
        pub const NOT_T0_TO_BUY: u16 = 12;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g07.evidence.local-send-boundaries",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(CLOCK_ID, "clock_id", Text),
                Field::req(BOOT_ID, "boot_id", Text),
                Field::req(RELEASE_BEFORE_NS, "release_before_ns", U64),
                Field::req(RELEASE_AFTER_NS, "release_after_ns", U64),
                Field::req(RELEASE_MARKER, "release_marker", Text),
                Field::req(PROVIDER_RESULTS_PENDING, "provider_results_pending", Bool),
                Field::nested(
                    FORTYEIGHTCLUB,
                    "fortyeightclub",
                    Presence::Optional,
                    &provider_result::SCHEMA,
                ),
                Field::nested(
                    BLOCKRAZOR,
                    "blockrazor",
                    Presence::Optional,
                    &provider_result::SCHEMA,
                ),
                Field::opt(BOTH_FLUSH_COMPLETED, "both_flush_completed", Bool),
                Field::opt(FLUSH_SPREAD_NS, "write_flush_boundary_spread_ns", U64),
                Field::req(
                    ANY_BYTES_MAY_HAVE_LEFT,
                    "any_valid_bytes_may_have_left",
                    Bool,
                ),
                Field::req(NOT_T0_TO_BUY, "not_t0_to_buy", Bool),
            ],
        };
    }

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g07.mainnet-self-launch-buy.v3",
        record_type: 8,
        schema_version: 3,
        fields: &[
            Field::req(GATE, "gate", Text),
            Field::req(ATTEMPT_ID, "attempt_id", Text),
            Field::req(CLASSIFICATION, "classification", Text),
            Field::req(CHAIN_ID, "chain_id", U64),
            Field::req(PORTAL, "portal", Address),
            Field::req(NEXT_REQUIRED_GATE, "next_required_gate", Text),
            Field::req(CRITICAL_RULE, "critical_rule", Text),
            Field::req(CONTRACT_PATH, "acceptance_contract.path", Text),
            Field::req(CONTRACT_SHA256, "acceptance_contract.sha256", B256),
            Field::req(DRY_PATH, "evidence_inputs.dry_path", Text),
            Field::req(DRY_SHA256, "evidence_inputs.dry_sha256", B256),
            Field::req(DRY_LEGACY_SHA256, "evidence_inputs.dry_legacy_sha256", B256),
            Field::req(PREFLIGHT_PATH, "evidence_inputs.preflight_path", Text),
            Field::req(PREFLIGHT_SHA256, "evidence_inputs.preflight_sha256", B256),
            Field::req(
                PREFLIGHT_LEGACY_SHA256,
                "evidence_inputs.preflight_legacy_sha256",
                B256,
            ),
            Field::req(
                BUILDER_POLICY_PATH,
                "evidence_inputs.builder_policy_path",
                Text,
            ),
            Field::req(
                BUILDER_POLICY_SHA256,
                "evidence_inputs.builder_policy_sha256",
                B256,
            ),
            Field::req(
                BUILDER_POLICY_LEGACY_SHA256,
                "evidence_inputs.builder_policy_legacy_sha256",
                B256,
            ),
            Field::req(
                WITNESS_SOURCE_PATH,
                "evidence_inputs.producer_witness_source_path",
                Text,
            ),
            Field::req(
                WITNESS_SOURCE_SHA256,
                "evidence_inputs.producer_witness_source_sha256",
                B256,
            ),
            Field::req(
                WITNESS_SOURCE_LEGACY_SHA256,
                "evidence_inputs.producer_witness_source_legacy_sha256",
                B256,
            ),
            Field::req(STATE_DIR, "attempt.state_dir", Text),
            Field::req(SINGLE_USE_LATCH, "attempt.single_use_latch", Text),
            Field::req(ATTEMPT_NUMBER, "attempt.number", U64),
            Field::req(SPENT_BEFORE_WEI, "attempt.spent_before_wei", U256),
            Field::req(
                RESERVED_WORST_CASE_WEI,
                "attempt.reserved_worst_case_wei",
                U256,
            ),
            Field::req(LAUNCHER, "wallets.launcher", Address),
            Field::req(BUYER, "wallets.buyer", Address),
            Field::req(DISTINCT, "wallets.distinct", Bool),
            Field::req(LAUNCHER_NONCE, "wallets.launcher_nonce", U64),
            Field::req(BUYER_NONCE, "wallets.buyer_nonce", U64),
            Field::nested(
                PREFLIGHT,
                "preflight",
                Presence::Required,
                &super::super::g06a::preflight::SCHEMA,
            ),
            Field::req(LAUNCH_TX_HASH, "transactions.launch_tx_hash", B256),
            Field::req(BUY_TX_HASH, "transactions.buy_tx_hash", B256),
            Field::req(TOKEN, "transactions.token", Address),
            Field::req(
                TOKEN_CODE_BEFORE_EMPTY,
                "transactions.token_code_before_empty",
                Bool,
            ),
            Field::req(SALT, "transactions.salt", B256),
            Field::req(LAUNCH_VALUE_WEI, "transactions.launch_value_wei", U256),
            Field::req(BUY_VALUE_WEI, "transactions.buy_value_wei", U256),
            Field::req(GAS_PRICE_WEI, "transactions.gas_price_wei", U64),
            Field::req(LAUNCH_GAS_ESTIMATE, "transactions.launch_gas_estimate", U64),
            Field::req(LAUNCH_GAS_LIMIT, "transactions.launch_gas_limit", U64),
            Field::req(BUY_GAS_LIMIT, "transactions.buy_gas_limit", U64),
            Field::req(
                LOCAL_EXPECTED_OUTPUT,
                "transactions.local_expected_output",
                U256,
            ),
            Field::req(MIN_OUTPUT, "transactions.min_output", U256),
            Field::req(SLIPPAGE_BPS, "transactions.slippage_bps", U64),
            Field::req(CAPTURE_COMPLETE, "capture.complete_local_journal", Bool),
            Field::req(CAPTURE_STATUS, "capture.status", Text),
            Field::req(JOURNAL_PATH, "capture.journal_path", Text),
            Field::nested(
                JOURNAL,
                "capture.journal",
                Presence::Optional,
                &journal::SCHEMA,
            ),
            Field::opt(CAPTURE_ERROR, "capture.error", Text),
            Field::opt(
                BOTH_RECEIPTS_OBSERVED_NS,
                "capture_window.both_receipts_observed_ns",
                U64,
            ),
            Field::opt(COVERAGE_END_NS, "capture_window.coverage_end_ns", U64),
            Field::opt(
                CANONICAL_WINDOW_RECONCILED_IN_G08,
                "capture_window.canonical_t0_plus_15s_reconciled_in_g08",
                Bool,
            ),
            Field::opt(
                CANONICAL_WINDOW_RECONCILIATION,
                "capture_window.canonical_t0_plus_15s_reconciliation",
                Text,
            ),
            Field::opt(
                LAUNCH_RECEIPT_OBSERVED,
                "capture_window.launch_receipt_observed",
                Bool,
            ),
            Field::opt(
                BUY_RECEIPT_OBSERVED,
                "capture_window.buy_receipt_observed",
                Bool,
            ),
            Field::opt(POST_WINDOW_HEAD, "capture_window.post_window_head", U64),
            Field::req(T0_TO_T_BUY, "timing.T0_to_T_buy", Text),
            Field::req(
                WITNESS_SOURCE_DESCRIPTION,
                "timing.producer_witness_source",
                Text,
            ),
            Field::req(
                WITNESS_SOURCE_READY,
                "timing.producer_witness_source_ready",
                Bool,
            ),
            Field::req(TIMING_RULE, "timing.rule", Text),
            Field::nested(BUNDLE, "bundle", Presence::Optional, &bundle::SCHEMA),
            Field::nested(
                SEND,
                "local_send_boundaries",
                Presence::Optional,
                &send::SCHEMA,
            ),
        ],
    };
}

/// Native probe evidence; raw wire bodies are never stored.
pub mod probe {
    use super::*;
    pub const PLAN_SHA256: u16 = 1;
    pub const STATUS: u16 = 2;
    pub const ACCOUNTS: u16 = 3;
    pub const CASES: u16 = 4;
    pub const SCOPE: u16 = 5;
    pub mod case {
        use super::*;
        pub const CASE: u16 = 1;
        pub const OUTCOME: u16 = 2;
        pub const REASON: u16 = 3;
        pub const BUILDER: u16 = 4;
        pub const QUESTION: u16 = 5;
        pub const GAS_PRICE: u16 = 6;
        pub const TX_HASH: u16 = 7;
        pub const SEND_HEAD: u16 = 8;
        pub const SEND_HEAD_HASH: u16 = 9;
        pub const SEND_HEAD_MS: u16 = 10;
        pub const HEAD_READ_MS: u16 = 11;
        pub const HEAD_AGE_MS: u16 = 12;
        pub const REFUSED: u16 = 13;
        pub const MAX_BLOCK: u16 = 14;
        pub const MAX_TIMESTAMP: u16 = 15;
        pub const BODY_SHA256: u16 = 16;
        pub const BODY_LEN: u16 = 17;
        pub const BEFORE_CONNECT_MS: u16 = 18;
        pub const CONNECTED_MS: u16 = 19;
        pub const WRITTEN_MS: u16 = 20;
        pub const ANSWER_MS: u16 = 21;
        pub const PROVIDER_RESULT: u16 = 22;
        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g07.probe-case",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(CASE, "case", Text),
                Field::req(OUTCOME, "outcome", Text),
                Field::opt(REASON, "reason", Text),
                Field::opt(BUILDER, "builder", Text),
                Field::opt(QUESTION, "question", Text),
                Field::opt(GAS_PRICE, "gas_price", U64),
                Field::rep(TX_HASH, "tx_hash", B256),
                Field::opt(SEND_HEAD, "send_head", U64),
                Field::opt(SEND_HEAD_HASH, "send_head_hash", Text),
                Field::opt(SEND_HEAD_MS, "send_head_ms", U64),
                Field::opt(HEAD_READ_MS, "head_read_ms", U64),
                Field::opt(HEAD_AGE_MS, "head_age_ms", I64),
                Field::rep(REFUSED, "refused", Text),
                Field::opt(MAX_BLOCK, "max_block", U64),
                Field::opt(MAX_TIMESTAMP, "max_timestamp", U64),
                Field::opt(BODY_SHA256, "body_sha256", B256),
                Field::opt(BODY_LEN, "body_len", U64),
                Field::opt(BEFORE_CONNECT_MS, "before_connect_ms", U64),
                Field::opt(CONNECTED_MS, "connected_ms", U64),
                Field::opt(WRITTEN_MS, "written_ms", U64),
                Field::opt(ANSWER_MS, "answer_ms", U64),
                Field::nested(
                    PROVIDER_RESULT,
                    "provider_result",
                    Presence::Optional,
                    &provider_result::SCHEMA,
                ),
            ],
        };
    }
    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g07.builder-probe",
        record_type: 9,
        schema_version: 1,
        fields: &[
            Field::req(PLAN_SHA256, "plan_sha256", B256),
            Field::req(STATUS, "status", Text),
            Field::rep(ACCOUNTS, "throwaway_accounts", Address),
            Field::nested(CASES, "cases", Presence::Repeated, &case::SCHEMA),
            Field::req(SCOPE, "scope", Text),
        ],
    };
}

/// Derived native readiness report; never an execution authority.
pub mod readiness {
    use super::*;
    pub const STATUS: u16 = 1;
    pub const MODE: u16 = 2;
    pub const OBSERVED_UNIX_MS: u16 = 3;
    pub const LIVE_AUTHORIZED: u16 = 4;
    pub const CONTRACT_SHA256: u16 = 5;
    pub const CHECKS: u16 = 6;
    pub mod check {
        use super::*;
        pub const NAME: u16 = 1; pub const STATUS: u16 = 2; pub const DETAIL: u16 = 3;
        pub const SCHEMA: Schema = Schema { name: "darkbot.g07.readiness-check", record_type: 0, schema_version: 1,
            fields: &[Field::req(NAME,"name",Text),Field::req(STATUS,"status",Text),Field::req(DETAIL,"detail",Text)] };
    }
    pub const SCHEMA: Schema = Schema { name: "darkbot.g07.readiness", record_type: 18, schema_version: 1,
        fields: &[Field::req(STATUS,"status",Text),Field::req(MODE,"mode",Text),
            Field::req(OBSERVED_UNIX_MS,"observed_unix_ms",U64),Field::req(LIVE_AUTHORIZED,"live_authorized",Bool),
            Field::opt(CONTRACT_SHA256,"contract_sha256",Text),Field::nested(CHECKS,"checks",Presence::Repeated,&check::SCHEMA)] };
}

pub const ALL: &[&Schema] = &[
    &readiness::SCHEMA,
    &probe::SCHEMA,
    &witness_source::SCHEMA,
    &contract::SCHEMA,
    &evidence::SCHEMA,
];
