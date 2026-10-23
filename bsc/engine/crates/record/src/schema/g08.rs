//! G0.8 records: verification result, producer-timing verdict and chain
//! capture payloads.
//!
//! The chain capture journal (`darkbot-bsc-journal` framing) carries one
//! record per payload: a session record first, then per canonical block its
//! header, its transactions in index order and its receipts with their logs.
//! Every value is decoded from the JSON-RPC wire into its type before it is
//! recorded; no RPC body is kept.

use crate::{Field, Kind::*, Presence, Schema};

/// BSC caps a transaction at 128 KiB; the bound leaves room for any
/// canonical calldata while a record stays far below the record limit.
const MAX_CALLDATA: u32 = 256 * 1024;
/// Bound of the data of one log.
const MAX_LOG_DATA: u32 = 512 * 1024;

/// The first payload of the G0.8 chain capture journal.
pub mod chain_session {
    use super::*;

    pub const ATTEMPT_ID: u16 = 1;
    pub const LAUNCH_TX_HASH: u16 = 2;
    pub const BUY_TX_HASH: u16 = 3;
    pub const LAUNCH_BLOCK: u16 = 4;
    pub const LAUNCH_BLOCK_TIMESTAMP_SECONDS: u16 = 5;
    pub const WINDOW_SECONDS: u16 = 6;
    pub const TIMESTAMP_SOURCE: u16 = 7;
    pub const TIMESTAMP_RESOLUTION_NS: u16 = 8;
    pub const PRODUCER_EXECUTION_TIMING_AUTHORITY: u16 = 9;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g08.chain-capture-session.v2",
        record_type: 10,
        schema_version: 2,
        fields: &[
            Field::req(ATTEMPT_ID, "attempt_id", Text),
            Field::req(LAUNCH_TX_HASH, "launch_tx_hash", B256),
            Field::req(BUY_TX_HASH, "buy_tx_hash", B256),
            Field::req(LAUNCH_BLOCK, "launch_block", U64),
            Field::req(LAUNCH_BLOCK_TIMESTAMP_SECONDS, "launch_block_timestamp_seconds", U64),
            Field::req(WINDOW_SECONDS, "window_seconds", U64),
            Field::req(TIMESTAMP_SOURCE, "timestamp_source", Text),
            Field::req(TIMESTAMP_RESOLUTION_NS, "timestamp_resolution_ns", U64),
            Field::req(
                PRODUCER_EXECUTION_TIMING_AUTHORITY,
                "producer_execution_timing_authority",
                Bool,
            ),
        ],
    };
}

/// The header of one canonical block of the capture window.
pub mod chain_block {
    use super::*;

    pub const NUMBER: u16 = 1;
    pub const HASH: u16 = 2;
    pub const PARENT_HASH: u16 = 3;
    pub const TIMESTAMP: u16 = 4;
    pub const MILLI_TIMESTAMP: u16 = 5;
    pub const MIX_HASH: u16 = 6;
    pub const MINER: u16 = 7;
    pub const GAS_LIMIT: u16 = 8;
    pub const GAS_USED: u16 = 9;
    pub const BASE_FEE_PER_GAS: u16 = 10;
    pub const TRANSACTION_COUNT: u16 = 11;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g08.chain-block.v1",
        record_type: 11,
        schema_version: 1,
        fields: &[
            Field::req(NUMBER, "number", U64),
            Field::req(HASH, "hash", B256),
            Field::req(PARENT_HASH, "parent_hash", B256),
            Field::req(TIMESTAMP, "timestamp", U64),
            // BEP-520: reported by newer nodes, else derivable from mixHash.
            Field::opt(MILLI_TIMESTAMP, "milli_timestamp", U64),
            Field::opt(MIX_HASH, "mix_hash", B256),
            Field::req(MINER, "miner", Address),
            Field::req(GAS_LIMIT, "gas_limit", U64),
            Field::req(GAS_USED, "gas_used", U64),
            Field::opt(BASE_FEE_PER_GAS, "base_fee_per_gas", U256),
            // The number of transaction and receipt records that follow.
            Field::req(TRANSACTION_COUNT, "transaction_count", U64),
        ],
    };
}

/// One transaction of a canonical block.
pub mod chain_transaction {
    use super::*;

    pub const BLOCK_NUMBER: u16 = 1;
    pub const BLOCK_HASH: u16 = 2;
    pub const TRANSACTION_INDEX: u16 = 3;
    pub const HASH: u16 = 4;
    pub const FROM: u16 = 5;
    pub const TO: u16 = 6;
    pub const NONCE: u16 = 7;
    pub const VALUE: u16 = 8;
    pub const GAS: u16 = 9;
    pub const GAS_PRICE: u16 = 10;
    pub const TX_TYPE: u16 = 11;
    pub const INPUT: u16 = 12;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g08.chain-transaction.v1",
        record_type: 12,
        schema_version: 1,
        fields: &[
            Field::req(BLOCK_NUMBER, "block_number", U64),
            Field::req(BLOCK_HASH, "block_hash", B256),
            Field::opt(TRANSACTION_INDEX, "transaction_index", U64),
            Field::req(HASH, "hash", B256),
            Field::req(FROM, "from", Address),
            // Absent for a contract creation.
            Field::opt(TO, "to", Address),
            Field::req(NONCE, "nonce", U64),
            Field::req(VALUE, "value", U256),
            Field::req(GAS, "gas", U64),
            Field::opt(GAS_PRICE, "gas_price", U256),
            Field::opt(TX_TYPE, "type", U64),
            Field::req(INPUT, "input", Bytes).max(MAX_CALLDATA),
        ],
    };
}

/// One receipt of a canonical block, with its logs.
pub mod chain_receipt {
    use super::*;

    pub const BLOCK_NUMBER: u16 = 1;
    pub const BLOCK_HASH: u16 = 2;
    pub const TRANSACTION_INDEX: u16 = 3;
    pub const TRANSACTION_HASH: u16 = 4;
    pub const FROM: u16 = 5;
    pub const TO: u16 = 6;
    pub const CONTRACT_ADDRESS: u16 = 7;
    pub const STATUS: u16 = 8;
    pub const GAS_USED: u16 = 9;
    pub const CUMULATIVE_GAS_USED: u16 = 10;
    pub const EFFECTIVE_GAS_PRICE: u16 = 11;
    pub const LOGS: u16 = 12;

    pub mod log {
        use super::*;

        pub const LOG_INDEX: u16 = 1;
        pub const ADDRESS: u16 = 2;
        pub const TOPICS: u16 = 3;
        pub const DATA: u16 = 4;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g08.chain-receipt.log",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(LOG_INDEX, "log_index", U64),
                Field::req(ADDRESS, "address", Address),
                Field::rep(TOPICS, "topics", B256),
                Field::req(DATA, "data", Bytes).max(MAX_LOG_DATA),
            ],
        };
    }

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g08.chain-receipt.v1",
        record_type: 13,
        schema_version: 1,
        fields: &[
            Field::req(BLOCK_NUMBER, "block_number", U64),
            Field::req(BLOCK_HASH, "block_hash", B256),
            Field::req(TRANSACTION_INDEX, "transaction_index", U64),
            Field::req(TRANSACTION_HASH, "transaction_hash", B256),
            Field::req(FROM, "from", Address),
            Field::opt(TO, "to", Address),
            Field::opt(CONTRACT_ADDRESS, "contract_address", Address),
            Field::req(STATUS, "status", U64),
            Field::req(GAS_USED, "gas_used", U64),
            Field::req(CUMULATIVE_GAS_USED, "cumulative_gas_used", U64),
            Field::opt(EFFECTIVE_GAS_PRICE, "effective_gas_price", U256),
            Field::nested(LOGS, "logs", Presence::Repeated, &log::SCHEMA),
        ],
    };
}

/// The G0.8 acquisition, ordering, economics and capture result that
/// `g08_producer_timing` reads.
pub mod result {
    use super::*;

    pub const GATE: u16 = 1;
    pub const ATTEMPT_ID: u16 = 2;
    pub const VERDICT: u16 = 3;
    pub const G07_EVIDENCE_SHA256: u16 = 4;
    pub const LAUNCH_TX: u16 = 10;
    pub const BUY_TX: u16 = 11;
    pub const LAUNCH_STATUS: u16 = 12;
    pub const BUY_STATUS: u16 = 13;
    pub const LAUNCH_SCOPE_OK: u16 = 14;
    pub const BUY_SCOPE_OK: u16 = 15;
    pub const BUY_CALLDATA_OK: u16 = 16;
    pub const TOKEN: u16 = 20;
    pub const BUYER: u16 = 21;
    pub const ACQUISITION_EVIDENCE_COMPLETE: u16 = 22;
    pub const TOKEN_ABSENT_BEFORE_LAUNCH_BLOCK: u16 = 23;
    pub const TOKEN_EXISTS_AT_BUY_BLOCK: u16 = 24;
    pub const BUYER_TOKEN_BALANCE_AT_BUY_BLOCK: u16 = 25;
    pub const POSITIVE_ATTRIBUTABLE_ACQUISITION: u16 = 26;
    pub const BUY_RECEIPT_TOKEN_NET: u16 = 27;
    pub const BUY_EVENT_FEE_WEI: u16 = 28;
    pub const BLOCK_HASH: u16 = 30;
    pub const LAUNCH_BLOCK_HASH: u16 = 31;
    pub const BUY_BLOCK_HASH: u16 = 32;
    pub const LAUNCH_BLOCK: u16 = 33;
    pub const LAUNCH_INDEX: u16 = 34;
    pub const BUY_BLOCK: u16 = 35;
    pub const BUY_INDEX: u16 = 36;
    pub const ORDERED: u16 = 37;
    pub const SAME_BLOCK: u16 = 38;
    pub const ADJACENT: u16 = 39;
    pub const CANONICAL: u16 = 40;
    pub const LAUNCH_HASH_AT_INDEX: u16 = 41;
    pub const BUY_HASH_AT_INDEX: u16 = 42;
    pub const EXTERNAL_BUYS_BETWEEN_COUNT: u16 = 43;
    pub const EXTERNAL_BUYS_BETWEEN_HASHES: u16 = 44;
    pub const LAUNCH_GAS_COST_WEI: u16 = 50;
    pub const BUY_GAS_COST_WEI: u16 = 51;
    pub const BUY_VALUE_WEI: u16 = 52;
    pub const BUYER_TOTAL_DEBIT_WEI: u16 = 53;
    pub const EXPERIMENT_TOTAL_COST_WEI: u16 = 54;
    pub const DIRECT_BUILDER_TIP_WEI: u16 = 55;
    pub const G07_LOCAL_JOURNAL_COMPLETE: u16 = 60;
    pub const CHAIN_JOURNAL: u16 = 61;
    pub const CHAIN_JOURNAL_ERROR: u16 = 62;
    pub const CHAIN_WINDOW_COMPLETE: u16 = 63;
    pub const MARKET_WINDOW_SECONDS: u16 = 64;
    pub const CHAIN_RECORD_TIMESTAMP_SOURCE: u16 = 65;
    pub const CHAIN_RECORD_TIMESTAMP_RESOLUTION_NS: u16 = 66;
    pub const CHAIN_RECORD_TIMESTAMP_IS_PRODUCER_TIME: u16 = 67;
    pub const COMPETITION_SCAN: u16 = 68;
    pub const CURRENT_HEAD: u16 = 70;
    pub const CONFIRMATIONS_AFTER_LAUNCH_BLOCK: u16 = 71;
    pub const T0_TO_BUY: u16 = 80;
    pub const CHAIN_DATA_IS_NOT_EXECUTION_TIMESTAMP: u16 = 81;
    pub const TIMING_NEXT: u16 = 82;
    /// The validator (`miner`) of the BUY block: the observable end of the
    /// inclusion path; which builder delivered the bundle is not on chain.
    pub const BUY_BLOCK_MINER: u16 = 45;
    /// The earliest successful external BUY of the token in the captured
    /// canonical window (not ours, not the launch), for the entry-price
    /// comparison with our BUY value / net token.
    pub const FIRST_EXTERNAL_BUY_SCAN: u16 = 90;
    pub const FIRST_EXTERNAL_BUY_TX: u16 = 91;
    pub const FIRST_EXTERNAL_BUY_BLOCK: u16 = 92;
    pub const FIRST_EXTERNAL_BUY_INDEX: u16 = 93;
    pub const FIRST_EXTERNAL_BUY_BUYER: u16 = 94;
    pub const FIRST_EXTERNAL_BUY_VALUE_WEI: u16 = 95;
    pub const FIRST_EXTERNAL_BUY_TOKEN_AMOUNT: u16 = 96;

    /// The chain capture journal after it was closed; absent, with
    /// `capture.chain_journal_error` instead, when the capture failed.
    pub mod chain_journal {
        use super::*;

        pub const PATH: u16 = 1;
        pub const RECORDS: u16 = 2;
        pub const BYTES: u16 = 3;
        pub const SHA256: u16 = 4;
        pub const FIRST_BLOCK: u16 = 5;
        pub const LAST_BLOCK: u16 = 6;
        pub const LEADING_BOUNDARY_INCLUDED: u16 = 7;
        pub const LAUNCH_TIMESTAMP: u16 = 8;
        pub const HORIZON_TIMESTAMP: u16 = 9;
        pub const BOUNDARY_BLOCK_INCLUDED: u16 = 10;
        pub const WINDOW_SECONDS: u16 = 11;
        pub const COMPLETE: u16 = 12;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g08.result.chain-journal",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(PATH, "path", Text),
                Field::req(RECORDS, "records", U64),
                Field::req(BYTES, "bytes", U64),
                Field::req(SHA256, "sha256", B256),
                Field::req(FIRST_BLOCK, "first_block", U64),
                Field::req(LAST_BLOCK, "last_block", U64),
                Field::req(LEADING_BOUNDARY_INCLUDED, "leading_boundary_included", Bool),
                Field::req(LAUNCH_TIMESTAMP, "launch_timestamp", U64),
                Field::req(HORIZON_TIMESTAMP, "horizon_timestamp", U64),
                Field::req(BOUNDARY_BLOCK_INCLUDED, "boundary_block_included", Bool),
                Field::req(WINDOW_SECONDS, "window_seconds", U64),
                Field::req(COMPLETE, "complete", Bool),
            ],
        };
    }

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g08.mainnet-acquisition-ordering.v5",
        record_type: 14,
        schema_version: 5,
        fields: &[
            Field::req(GATE, "gate", Text),
            Field::req(ATTEMPT_ID, "attempt_id", Text),
            Field::req(VERDICT, "verdict", Text),
            Field::req(G07_EVIDENCE_SHA256, "g07_evidence_sha256", B256),
            Field::req(LAUNCH_TX, "transactions.launch_tx", B256),
            Field::req(BUY_TX, "transactions.buy_tx", B256),
            Field::req(LAUNCH_STATUS, "transactions.launch_status", U64),
            Field::req(BUY_STATUS, "transactions.buy_status", U64),
            Field::req(LAUNCH_SCOPE_OK, "transactions.launch_scope_ok", Bool),
            Field::req(BUY_SCOPE_OK, "transactions.buy_scope_ok", Bool),
            Field::req(BUY_CALLDATA_OK, "transactions.buy_calldata_ok", Bool),
            Field::req(TOKEN, "acquisition.token", Address),
            Field::req(BUYER, "acquisition.buyer", Address),
            Field::req(ACQUISITION_EVIDENCE_COMPLETE, "acquisition.evidence_complete", Bool),
            // Absent where the read failed (unknown), as null was in JSON.
            Field::opt(
                TOKEN_ABSENT_BEFORE_LAUNCH_BLOCK,
                "acquisition.token_absent_before_launch_block",
                Bool,
            ),
            Field::opt(TOKEN_EXISTS_AT_BUY_BLOCK, "acquisition.token_exists_at_buy_block", Bool),
            Field::opt(
                BUYER_TOKEN_BALANCE_AT_BUY_BLOCK,
                "acquisition.buyer_token_balance_at_buy_block",
                U256,
            ),
            Field::req(
                POSITIVE_ATTRIBUTABLE_ACQUISITION,
                "acquisition.positive_attributable_acquisition",
                Bool,
            ),
            Field::opt(BUY_RECEIPT_TOKEN_NET, "acquisition.buy_receipt_token_net", U256),
            Field::opt(BUY_EVENT_FEE_WEI, "costs.buy_event_fee_included_in_value", U256),
            Field::req(BLOCK_HASH, "ordering.block_hash", B256),
            Field::req(LAUNCH_BLOCK_HASH, "ordering.launch_block_hash", B256),
            Field::req(BUY_BLOCK_HASH, "ordering.buy_block_hash", B256),
            Field::req(LAUNCH_BLOCK, "ordering.launch_block", U64),
            Field::req(LAUNCH_INDEX, "ordering.launch_index", U64),
            Field::req(BUY_BLOCK, "ordering.buy_block", U64),
            Field::req(BUY_INDEX, "ordering.buy_index", U64),
            Field::req(ORDERED, "ordering.ordered", Bool),
            Field::req(SAME_BLOCK, "ordering.same_block", Bool),
            Field::req(ADJACENT, "ordering.adjacent", Bool),
            Field::req(CANONICAL, "ordering.canonical", Bool),
            Field::req(LAUNCH_HASH_AT_INDEX, "ordering.launch_hash_at_index", Bool),
            Field::req(BUY_HASH_AT_INDEX, "ordering.buy_hash_at_index", Bool),
            // Absent when the scan could not be completed (unknown); the
            // hashes are then absent too.
            Field::opt(
                EXTERNAL_BUYS_BETWEEN_COUNT,
                "ordering.successful_external_buys_between_count",
                U64,
            ),
            Field::rep(
                EXTERNAL_BUYS_BETWEEN_HASHES,
                "ordering.successful_external_buys_between_hashes",
                B256,
            ),
            Field::opt(BUY_BLOCK_MINER, "ordering.buy_block_miner", Address),
            Field::req(LAUNCH_GAS_COST_WEI, "costs.launch_gas_cost_wei", U256),
            Field::req(BUY_GAS_COST_WEI, "costs.buy_gas_cost_wei", U256),
            Field::req(BUY_VALUE_WEI, "costs.buy_value_wei", U256),
            Field::req(BUYER_TOTAL_DEBIT_WEI, "costs.buyer_total_debit_wei", U256),
            Field::req(EXPERIMENT_TOTAL_COST_WEI, "costs.experiment_total_cost_wei", U256),
            Field::req(DIRECT_BUILDER_TIP_WEI, "costs.direct_builder_tip_wei", U256),
            Field::req(G07_LOCAL_JOURNAL_COMPLETE, "capture.g07_local_journal_complete", Bool),
            Field::nested(
                CHAIN_JOURNAL,
                "capture.chain_journal",
                Presence::Optional,
                &chain_journal::SCHEMA,
            ),
            Field::opt(CHAIN_JOURNAL_ERROR, "capture.chain_journal_error", Text),
            Field::req(CHAIN_WINDOW_COMPLETE, "capture.chain_window_complete", Bool),
            Field::req(MARKET_WINDOW_SECONDS, "capture.market_window_seconds", U64),
            Field::req(
                CHAIN_RECORD_TIMESTAMP_SOURCE,
                "capture.chain_record_timestamp_source",
                Text,
            ),
            Field::req(
                CHAIN_RECORD_TIMESTAMP_RESOLUTION_NS,
                "capture.chain_record_timestamp_resolution_ns",
                U64,
            ),
            Field::req(
                CHAIN_RECORD_TIMESTAMP_IS_PRODUCER_TIME,
                "capture.chain_record_timestamp_is_producer_execution_time",
                Bool,
            ),
            Field::req(COMPETITION_SCAN, "capture.competition_scan", Text),
            Field::req(CURRENT_HEAD, "confirmation.current_head", U64),
            Field::req(
                CONFIRMATIONS_AFTER_LAUNCH_BLOCK,
                "confirmation.confirmations_after_launch_block",
                U64,
            ),
            Field::req(T0_TO_BUY, "timing.T0_TO_BUY", Text),
            Field::req(
                CHAIN_DATA_IS_NOT_EXECUTION_TIMESTAMP,
                "timing.standard_chain_data_is_not_execution_timestamp",
                Bool,
            ),
            Field::req(TIMING_NEXT, "timing.next", Text),
            Field::req(
                FIRST_EXTERNAL_BUY_SCAN,
                "pricing.first_external_buy_scan",
                Text,
            ),
            Field::opt(FIRST_EXTERNAL_BUY_TX, "pricing.first_external_buy_tx", B256),
            Field::opt(
                FIRST_EXTERNAL_BUY_BLOCK,
                "pricing.first_external_buy_block",
                U64,
            ),
            Field::opt(
                FIRST_EXTERNAL_BUY_INDEX,
                "pricing.first_external_buy_index",
                U64,
            ),
            Field::opt(
                FIRST_EXTERNAL_BUY_BUYER,
                "pricing.first_external_buy_buyer",
                Address,
            ),
            Field::opt(
                FIRST_EXTERNAL_BUY_VALUE_WEI,
                "pricing.first_external_buy_value_wei",
                U256,
            ),
            Field::opt(
                FIRST_EXTERNAL_BUY_TOKEN_AMOUNT,
                "pricing.first_external_buy_token_amount",
                U256,
            ),
        ],
    };
}

/// What G0.8 prints when a sent transaction has no receipt within the
/// polling bound. Persisted durably before exit; also used before RPC.
pub mod unresolved {
    use super::*;

    pub const GATE: u16 = 1;
    pub const VERDICT: u16 = 2;
    pub const REASON: u16 = 3;
    pub const ATTEMPT_ID: u16 = 4;
    pub const LAUNCH_TX: u16 = 5;
    pub const BUY_TX: u16 = 6;
    pub const LAUNCH_RECEIPT_RESOLVED: u16 = 7;
    pub const BUY_RECEIPT_RESOLVED: u16 = 8;
    pub const AUTOMATIC_RETRY: u16 = 9;
    pub const T0_TO_BUY: u16 = 10;
    /// E2-1: a non-conclusive expiry observation, recorded only when a sent
    /// transaction stayed without receipt. One RPC's snapshot: it proves
    /// neither non-execution nor that the attempt could be reused.
    pub const EXPIRY_OBSERVATION_SCOPE: u16 = 20;
    pub const OBSERVED_HEAD: u16 = 21;
    pub const LAUNCH_TX_KNOWN: u16 = 22;
    pub const BUY_TX_KNOWN: u16 = 23;
    pub const LAUNCHER_NONCE_LATEST: u16 = 24;
    pub const LAUNCHER_NONCE_PENDING: u16 = 25;
    pub const BUYER_NONCE_LATEST: u16 = 26;
    pub const BUYER_NONCE_PENDING: u16 = 27;
    pub const OBSERVATION_ERROR: u16 = 28;

    /// The only value of [`EXPIRY_OBSERVATION_SCOPE`].
    pub const NON_CONCLUSIVE_SCOPE: &str =
        "SINGLE_RPC_SNAPSHOT_NOT_CONCLUSIVE; NO_NON_EXECUTION_OR_REUSE_CLAIM";

    /// Version 2 adds the optional expiry observation (E2-1); a v1 record
    /// is not read as v2. The verdict stays UNRESOLVED.
    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g08.mainnet-acquisition-unresolved.v2",
        record_type: 15,
        schema_version: 2,
        fields: &[
            Field::req(GATE, "gate", Text),
            Field::req(VERDICT, "verdict", Text),
            Field::req(REASON, "reason", Text),
            Field::req(ATTEMPT_ID, "attempt_id", Text),
            Field::req(LAUNCH_TX, "launch_tx", B256),
            Field::req(BUY_TX, "buy_tx", B256),
            Field::req(LAUNCH_RECEIPT_RESOLVED, "launch_receipt_resolved", Bool),
            Field::req(BUY_RECEIPT_RESOLVED, "buy_receipt_resolved", Bool),
            Field::req(AUTOMATIC_RETRY, "automatic_retry", Bool),
            Field::req(T0_TO_BUY, "T0_TO_BUY", Text),
            Field::opt(EXPIRY_OBSERVATION_SCOPE, "expiry_observation.scope", Text),
            Field::opt(OBSERVED_HEAD, "expiry_observation.head", U64),
            Field::opt(LAUNCH_TX_KNOWN, "expiry_observation.launch_tx_known", Bool),
            Field::opt(BUY_TX_KNOWN, "expiry_observation.buy_tx_known", Bool),
            Field::opt(LAUNCHER_NONCE_LATEST, "expiry_observation.launcher_nonce_latest", U64),
            Field::opt(LAUNCHER_NONCE_PENDING, "expiry_observation.launcher_nonce_pending", U64),
            Field::opt(BUYER_NONCE_LATEST, "expiry_observation.buyer_nonce_latest", U64),
            Field::opt(BUYER_NONCE_PENDING, "expiry_observation.buyer_nonce_pending", U64),
            Field::opt(OBSERVATION_ERROR, "expiry_observation.error", Text),
        ],
    };
}

/// The witness of the original successful producing execution attempt,
/// field for field the legacy `darkbot.producer-execution-witness.v1` JSON
/// contract (`bsc/config/producer_execution_witness_v1.schema.json`): its
/// `schema`/`schema_version` are the record type and version, hashes are
/// 32-byte words and nanosecond counts are integers.
pub mod producer_witness {
    use super::*;

    pub const ATTEMPT_ID: u16 = 1;
    pub const NODE_ID: u16 = 10;
    pub const PRODUCER_BOOT_ID: u16 = 11;
    pub const CLIENT_VERSION: u16 = 12;
    pub const CLIENT_COMMIT: u16 = 13;
    pub const EXECUTION_ATTEMPT_ID: u16 = 14;
    pub const CLOCK_ID: u16 = 20;
    pub const CLOCK_SOURCE: u16 = 21;
    pub const CLOCK_RAW_UNIT: u16 = 22;
    pub const CLOCK_RAW_RESOLUTION: u16 = 23;
    pub const NORMALIZATION_TO_NS_METHOD: u16 = 24;
    pub const CLOCK_RESOLUTION_NS: u16 = 25;
    pub const HOOK_READ_OVERHEAD_NS: u16 = 26;
    pub const NORMALIZATION_ERROR_NS: u16 = 27;
    pub const REMAINING_UNCERTAINTY_NS: u16 = 28;
    pub const REMAINING_EXCLUDES_BRACKETED: u16 = 29;
    pub const CLOCK_RESOLUTION_ACCOUNTED: u16 = 30;
    pub const HOOK_OVERHEAD_ACCOUNTED: u16 = 31;
    pub const NORMALIZATION_ERROR_ACCOUNTED: u16 = 32;
    pub const PARENT_HASH: u16 = 40;
    pub const BLOCK_HASH: u16 = 41;
    pub const LAUNCH_TX_HASH: u16 = 42;
    pub const LAUNCH_TX_INDEX: u16 = 43;
    pub const BUY_TX_HASH: u16 = 44;
    pub const BUY_TX_INDEX: u16 = 45;
    pub const LAUNCH_STATUS: u16 = 46;
    pub const BUY_STATUS: u16 = 47;
    pub const CANONICAL_STATUS: u16 = 48;
    pub const REORG_STATUS: u16 = 49;
    pub const ACQUISITION_EVIDENCE_REFS: u16 = 50;
    pub const T0_HOOK: u16 = 60;
    pub const T_BUY_HOOK: u16 = 61;
    pub const RAW_T0_BEFORE: u16 = 70;
    pub const RAW_T0_AFTER: u16 = 71;
    pub const RAW_T_BUY_BEFORE: u16 = 72;
    pub const RAW_T_BUY_AFTER: u16 = 73;
    pub const T0_BEFORE_NS: u16 = 80;
    pub const T0_AFTER_NS: u16 = 81;
    pub const T_BUY_BEFORE_NS: u16 = 82;
    pub const T_BUY_AFTER_NS: u16 = 83;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.producer-execution-witness.v2",
        record_type: 16,
        schema_version: 2,
        fields: &[
            Field::req(ATTEMPT_ID, "attempt_id", Text),
            Field::req(NODE_ID, "node_id", Text),
            Field::req(PRODUCER_BOOT_ID, "producer_boot_id", Text),
            Field::req(CLIENT_VERSION, "client_version", Text),
            Field::req(CLIENT_COMMIT, "client_commit", Text),
            Field::req(EXECUTION_ATTEMPT_ID, "execution_attempt_id", Text),
            Field::req(CLOCK_ID, "clock_id", Text),
            Field::req(CLOCK_SOURCE, "clock_source", Text),
            Field::req(CLOCK_RAW_UNIT, "clock_raw_unit", Text),
            Field::req(CLOCK_RAW_RESOLUTION, "clock_raw_resolution", Text),
            Field::req(NORMALIZATION_TO_NS_METHOD, "normalization_to_ns_method", Text),
            Field::req(CLOCK_RESOLUTION_NS, "clock_resolution_ns", U64),
            Field::req(HOOK_READ_OVERHEAD_NS, "hook_read_overhead_ns", U64),
            Field::req(NORMALIZATION_ERROR_NS, "normalization_error_ns", U64),
            Field::req(
                REMAINING_UNCERTAINTY_NS,
                "remaining_non_overlapping_uncertainty_ns",
                U64,
            ),
            Field::req(
                REMAINING_EXCLUDES_BRACKETED,
                "remaining_uncertainty_excludes_bracketed_uncertainty",
                Bool,
            ),
            Field::req(
                CLOCK_RESOLUTION_ACCOUNTED,
                "clock_resolution_accounted_in_brackets_or_remaining_uncertainty",
                Bool,
            ),
            Field::req(
                HOOK_OVERHEAD_ACCOUNTED,
                "hook_overhead_accounted_in_brackets_or_remaining_uncertainty",
                Bool,
            ),
            Field::req(
                NORMALIZATION_ERROR_ACCOUNTED,
                "normalization_error_accounted_in_remaining_uncertainty",
                Bool,
            ),
            Field::req(PARENT_HASH, "parent_hash", B256),
            Field::req(BLOCK_HASH, "block_hash", B256),
            Field::req(LAUNCH_TX_HASH, "launch_tx_hash", B256),
            Field::req(LAUNCH_TX_INDEX, "launch_tx_index", U64),
            Field::req(BUY_TX_HASH, "buy_tx_hash", B256),
            Field::req(BUY_TX_INDEX, "buy_tx_index", U64),
            Field::req(LAUNCH_STATUS, "launch_status", U64),
            Field::req(BUY_STATUS, "buy_status", U64),
            Field::req(CANONICAL_STATUS, "canonical_status", Text),
            Field::req(REORG_STATUS, "reorg_status", Text),
            Field::rep(ACQUISITION_EVIDENCE_REFS, "acquisition_evidence_refs", Text),
            Field::req(T0_HOOK, "semantic_hook_ids.T0", Text),
            Field::req(T_BUY_HOOK, "semantic_hook_ids.T_buy", Text),
            Field::req(RAW_T0_BEFORE, "original_raw_timestamps.T0_before", Text),
            Field::req(RAW_T0_AFTER, "original_raw_timestamps.T0_after", Text),
            Field::req(RAW_T_BUY_BEFORE, "original_raw_timestamps.T_buy_before", Text),
            Field::req(RAW_T_BUY_AFTER, "original_raw_timestamps.T_buy_after", Text),
            Field::req(T0_BEFORE_NS, "normalized_timestamps_ns.T0_before_ns", U64),
            Field::req(T0_AFTER_NS, "normalized_timestamps_ns.T0_after_ns", U64),
            Field::req(T_BUY_BEFORE_NS, "normalized_timestamps_ns.T_buy_before_ns", U64),
            Field::req(T_BUY_AFTER_NS, "normalized_timestamps_ns.T_buy_after_ns", U64),
        ],
    };
}

/// The strict T0 -> T_buy verdict of `g08_producer_timing`.
pub mod timing_verdict {
    use super::*;

    pub const ATTEMPT_ID: u16 = 1;
    pub const VERDICT: u16 = 2;
    pub const STRICT_LIMIT_NS: u16 = 3;
    pub const AUTHORITY: u16 = 4;
    pub const EXPLICITLY_NOT_AUTHORITY: u16 = 5;
    pub const NODE_ID: u16 = 10;
    pub const PRODUCER_BOOT_ID: u16 = 11;
    pub const CLIENT_VERSION: u16 = 12;
    pub const CLIENT_COMMIT: u16 = 13;
    pub const EXECUTION_ATTEMPT_ID: u16 = 14;
    pub const CLOCK_ID: u16 = 20;
    pub const CLOCK_SOURCE: u16 = 21;
    pub const RAW_UNIT: u16 = 22;
    pub const RAW_RESOLUTION: u16 = 23;
    pub const NORMALIZATION_TO_NS_METHOD: u16 = 24;
    pub const NORMALIZATION_ERROR_NS: u16 = 25;
    pub const CLOCK_RESOLUTION_NS: u16 = 26;
    pub const HOOK_READ_OVERHEAD_NS: u16 = 27;
    pub const REMAINING_UNCERTAINTY_NS: u16 = 28;
    pub const T0_HOOK: u16 = 30;
    pub const T_BUY_HOOK: u16 = 31;
    pub const RAW_T0_BEFORE: u16 = 40;
    pub const RAW_T0_AFTER: u16 = 41;
    pub const RAW_T_BUY_BEFORE: u16 = 42;
    pub const RAW_T_BUY_AFTER: u16 = 43;
    pub const T0_BEFORE_NS: u16 = 50;
    pub const T0_AFTER_NS: u16 = 51;
    pub const T_BUY_BEFORE_NS: u16 = 52;
    pub const T_BUY_AFTER_NS: u16 = 53;
    pub const LOWER_BOUND_NS: u16 = 60;
    pub const BRACKET_UPPER_NS: u16 = 61;
    pub const CONSERVATIVE_UPPER_BOUND_NS: u16 = 62;
    pub const FORMULA: u16 = 63;
    pub const PASS_RULE: u16 = 64;
    pub const G08_SHA256: u16 = 70;
    pub const PRODUCER_WITNESS_SHA256: u16 = 71;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g08.producer-timing-verdict.v2",
        record_type: 17,
        schema_version: 2,
        fields: &[
            Field::req(ATTEMPT_ID, "attempt_id", Text),
            Field::req(VERDICT, "verdict", Text),
            Field::req(STRICT_LIMIT_NS, "strict_limit_ns", U64),
            Field::req(AUTHORITY, "authority", Text),
            Field::rep(EXPLICITLY_NOT_AUTHORITY, "explicitly_not_authority", Text),
            Field::req(NODE_ID, "producer_identity.node_id", Text),
            Field::req(PRODUCER_BOOT_ID, "producer_identity.producer_boot_id", Text),
            Field::req(CLIENT_VERSION, "producer_identity.client_version", Text),
            Field::req(CLIENT_COMMIT, "producer_identity.client_commit", Text),
            Field::req(EXECUTION_ATTEMPT_ID, "producer_identity.execution_attempt_id", Text),
            Field::req(CLOCK_ID, "clock.clock_id", Text),
            Field::req(CLOCK_SOURCE, "clock.clock_source", Text),
            Field::req(RAW_UNIT, "clock.raw_unit", Text),
            Field::req(RAW_RESOLUTION, "clock.raw_resolution", Text),
            Field::req(NORMALIZATION_TO_NS_METHOD, "clock.normalization_to_ns_method", Text),
            Field::req(NORMALIZATION_ERROR_NS, "clock.normalization_error_ns", U64),
            Field::req(CLOCK_RESOLUTION_NS, "clock.clock_resolution_ns", U64),
            Field::req(HOOK_READ_OVERHEAD_NS, "clock.hook_read_overhead_ns", U64),
            Field::req(
                REMAINING_UNCERTAINTY_NS,
                "clock.remaining_non_overlapping_uncertainty_ns",
                U64,
            ),
            Field::req(T0_HOOK, "hooks.T0", Text),
            Field::req(T_BUY_HOOK, "hooks.T_buy", Text),
            Field::req(RAW_T0_BEFORE, "original_raw_timestamps.T0_before", Text),
            Field::req(RAW_T0_AFTER, "original_raw_timestamps.T0_after", Text),
            Field::req(RAW_T_BUY_BEFORE, "original_raw_timestamps.T_buy_before", Text),
            Field::req(RAW_T_BUY_AFTER, "original_raw_timestamps.T_buy_after", Text),
            Field::req(T0_BEFORE_NS, "normalized_brackets_ns.T0_before_ns", U64),
            Field::req(T0_AFTER_NS, "normalized_brackets_ns.T0_after_ns", U64),
            Field::req(T_BUY_BEFORE_NS, "normalized_brackets_ns.T_buy_before_ns", U64),
            Field::req(T_BUY_AFTER_NS, "normalized_brackets_ns.T_buy_after_ns", U64),
            Field::req(LOWER_BOUND_NS, "derived.lower_bound_ns", U64),
            Field::req(BRACKET_UPPER_NS, "derived.bracket_upper_ns", U64),
            Field::req(CONSERVATIVE_UPPER_BOUND_NS, "derived.conservative_upper_bound_ns", U64),
            Field::req(FORMULA, "derived.formula", Text),
            Field::req(PASS_RULE, "derived.pass_rule", Text),
            Field::req(G08_SHA256, "evidence_hashes.g08_sha256", B256),
            Field::req(PRODUCER_WITNESS_SHA256, "evidence_hashes.producer_witness_sha256", B256),
        ],
    };
}

pub const ALL: &[&Schema] = &[
    &chain_session::SCHEMA,
    &chain_block::SCHEMA,
    &chain_transaction::SCHEMA,
    &chain_receipt::SCHEMA,
    &result::SCHEMA,
    &unresolved::SCHEMA,
    &producer_witness::SCHEMA,
    &timing_verdict::SCHEMA,
];
