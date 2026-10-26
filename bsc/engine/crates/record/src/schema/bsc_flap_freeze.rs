//! G0.1 / Phase D Flap refreeze (order V2, Phase D contract §3): one record
//! of the current Flap Portal deployment, written by the read-only
//! `g01_flap_freeze` qualification binary.
//!
//! Every state read is pinned to one canonical block (EIP-1898 block hash).
//! Selectors, topics, code hashes and CREATE2 addresses are alloy's,
//! cross-checked against the archived official IPortal sources, Sourcify's
//! verified ABI, the implementation dispatcher and canonical fixtures. The
//! witnessed constants of `trading::venues::flap::deployment` are compared
//! field by field. Each property is `PROVEN`, `UNKNOWN` or `UNSUPPORTED` with
//! its evidence. The sub-records shared with the Four freeze (Sourcify
//! lookup, contract, check, parameter, property, archive input) are those of
//! [`super::bsc_four_freeze`].

use super::bsc_four_freeze::four_freeze as shared;
use crate::{Field, Kind::*, Presence, Schema};

pub mod flap_freeze {
    use super::*;

    /// One witnessed constant of `bsc_mainnet_v5_24_0()` against the chain.
    pub mod constant {
        use super::*;

        /// The `FlapDeployment` field name.
        pub const NAME: u16 = 1;
        pub const WITNESSED: u16 = 2;
        pub const OBSERVED: u16 = 3;
        /// Version strings.
        pub const WITNESSED_TEXT: u16 = 4;
        pub const OBSERVED_TEXT: u16 = 5;
        /// `UNCHANGED`, `CHANGED` or `NOT_OBSERVED`.
        pub const STATUS: u16 = 6;
        pub const DETAIL: u16 = 7;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.flap-freeze.constant",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(NAME, "name", Text),
                Field::opt(WITNESSED, "witnessed", Bytes).max(32),
                Field::opt(OBSERVED, "observed", Bytes).max(32),
                Field::opt(WITNESSED_TEXT, "witnessed_text", Text),
                Field::opt(OBSERVED_TEXT, "observed_text", Text),
                Field::req(STATUS, "status", Text),
                Field::req(DETAIL, "detail", Text),
            ],
        };
    }

    /// One function selector or event topic of the Portal.
    pub mod selector {
        use super::*;

        /// `FUNCTION` or `EVENT`.
        pub const KIND: u16 = 1;
        /// `LAUNCH`, `LAUNCH_TWO_STEP`, `BUY`, `SELL`, `STATE`, `EVENT`.
        pub const ROLE: u16 = 2;
        pub const SIGNATURE: u16 = 3;
        /// 4-byte selector or 32-byte topic.
        pub const VALUE: u16 = 4;
        pub const DERIVATION: u16 = 5;
        /// The archived official IPortal: `MATCH` or `NOT_IN_ARCHIVE`.
        pub const ABI_OFFICIAL: u16 = 6;
        /// The archived incjanta/Portal IPortal: `MATCH` or `NOT_IN_ARCHIVE`.
        pub const ABI_INTERFACES: u16 = 7;
        /// Sourcify's verified implementation ABI: `IN_VERIFIED_ABI`,
        /// `NOT_IN_VERIFIED_ABI` or `UNAVAILABLE`.
        pub const VERIFIED_ABI: u16 = 8;
        /// The archived web3_sha3 reference: `MATCH`, `MISMATCH`,
        /// `NOT_IN_REFERENCE`.
        pub const KECCAK_REFERENCE: u16 = 9;
        /// Functions: `PUSH4_PRESENT`, `PUSH<n>_PRESENT` or `ABSENT` in the
        /// implementation dispatcher. Events: where the topic is a PUSH32
        /// (`PortalImplementation`, helper addresses) or `NOT_FOUND`.
        pub const FOUND_IN: u16 = 10;
        /// Functions: top-level Portal calls with this selector in the
        /// fetched receipts; events: Portal logs with this topic there.
        pub const LIVE_COUNT: u16 = 11;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.flap-freeze.selector",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(KIND, "kind", Text),
                Field::req(ROLE, "role", Text),
                Field::req(SIGNATURE, "signature", Text),
                Field::req(VALUE, "value", Bytes).max(32),
                Field::req(DERIVATION, "derivation", Text),
                Field::req(ABI_OFFICIAL, "abi_official", Text),
                Field::req(ABI_INTERFACES, "abi_interfaces", Text),
                Field::req(VERIFIED_ABI, "verified_abi", Text),
                Field::req(KECCAK_REFERENCE, "keccak_reference", Text),
                Field::req(FOUND_IN, "found_in", Text),
                Field::req(LIVE_COUNT, "live_count", U64),
            ],
        };
    }

    /// One Portal log of a canonical fixture transaction.
    pub mod log {
        use super::*;

        pub const LOG_INDEX: u16 = 1;
        pub const SIGNATURE: u16 = 2;
        pub const TOPICS: u16 = 3;
        pub const DATA: u16 = 4;
        /// Decoded fields (derived view of `data`).
        pub const PARAMS: u16 = 5;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.flap-freeze.log",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(LOG_INDEX, "log_index", U64),
                Field::req(SIGNATURE, "signature", Text),
                Field::rep(TOPICS, "topics", B256),
                Field::req(DATA, "data", Bytes).max(64 * 1024),
                Field::nested(PARAMS, "params", Presence::Repeated, &shared::param::SCHEMA),
            ],
        };
    }

    /// A canonical launch, BUY, SELL, DEX migration or reverted Portal call.
    pub mod fixture {
        use super::*;

        /// `LAUNCH`, `BUY`, `SELL`, `LAUNCHED_TO_DEX` or `REVERTED`.
        pub const KIND: u16 = 1;
        /// `RECEIPT_SCAN` or `ARCHIVED_2026_09_27_REVERIFIED`.
        pub const SOURCE: u16 = 2;
        pub const TX_HASH: u16 = 3;
        pub const BLOCK_NUMBER: u16 = 4;
        pub const BLOCK_HASH: u16 = 5;
        pub const TX_INDEX: u16 = 6;
        pub const RECEIPT_STATUS: u16 = 7;
        pub const FROM: u16 = 8;
        pub const TO: u16 = 9;
        pub const VALUE_WEI: u16 = 10;
        pub const INPUT: u16 = 11;
        pub const SELECTOR: u16 = 12;
        /// The Portal function of a direct call, else `ROUTED`.
        pub const CALL: u16 = 13;
        /// The fixture's own log (absent for a reverted call).
        pub const LOG_INDEX: u16 = 14;
        pub const TOKEN: u16 = 15;
        pub const TOKEN_VERSION: u16 = 16;
        /// Every Portal log of the transaction, decoded where frozen.
        pub const LOGS: u16 = 17;
        /// Decoded calldata (direct Portal calls).
        pub const PARAMS: u16 = 18;
        pub const CHECKS: u16 = 19;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.flap-freeze.fixture",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(KIND, "kind", Text),
                Field::req(SOURCE, "source", Text),
                Field::req(TX_HASH, "tx_hash", B256),
                Field::req(BLOCK_NUMBER, "block_number", U64),
                Field::req(BLOCK_HASH, "block_hash", B256),
                Field::req(TX_INDEX, "tx_index", U64),
                Field::req(RECEIPT_STATUS, "receipt_status", U64),
                Field::req(FROM, "from", Address),
                Field::opt(TO, "to", Address),
                Field::req(VALUE_WEI, "value_wei", U256),
                Field::req(INPUT, "input", Bytes).max(128 * 1024),
                Field::req(SELECTOR, "selector", Bytes).max(4),
                Field::req(CALL, "call", Text),
                Field::opt(LOG_INDEX, "log_index", U64),
                Field::opt(TOKEN, "token", Address),
                Field::opt(TOKEN_VERSION, "token_version", U64),
                Field::nested(LOGS, "logs", Presence::Repeated, &log::SCHEMA),
                Field::nested(PARAMS, "params", Presence::Repeated, &shared::param::SCHEMA),
                Field::nested(CHECKS, "checks", Presence::Repeated, &shared::check::SCHEMA),
            ],
        };
    }

    /// One observed launch: path, mode and the token-address reproduction.
    pub mod launch {
        use super::*;

        pub const TX_HASH: u16 = 1;
        pub const BLOCK_NUMBER: u16 = 2;
        /// `RECEIPT_SCAN` or `ARCHIVED_2026_09_27_REVERIFIED`.
        pub const SOURCE: u16 = 3;
        /// The top-level destination and selector.
        pub const TO: u16 = 4;
        pub const SELECTOR: u16 = 5;
        /// The Portal function of a direct call, else `ROUTED`.
        pub const CALL: u16 = 6;
        pub const TOKEN: u16 = 7;
        /// `TokenVersionSet` of the same transaction.
        pub const TOKEN_VERSION: u16 = 8;
        pub const TOKEN_VERSION_LABEL: u16 = 9;
        /// `TokenQuoteSet` of the same transaction (zero: native).
        pub const QUOTE_TOKEN: u16 = 10;
        /// EIP-1167 target of the token runtime at the reference block.
        pub const TEMPLATE: u16 = 11;
        /// Where the salt was found: the decoded Portal struct, a byte
        /// offset of the pending calldata, or `NOT_FOUND`.
        pub const SALT_SOURCE: u16 = 12;
        pub const SALT: u16 = 13;
        pub const INIT_CODE_HASH: u16 = 14;
        pub const PREDICTED: u16 = 15;
        pub const REPRODUCED: u16 = 16;
        /// `FlapDeployment::taxed_v3_token(salt)` of the production code:
        /// `MATCH`, `MISMATCH` or `NOT_APPLICABLE`.
        pub const PRODUCTION_DERIVATION: u16 = 17;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.flap-freeze.launch",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(TX_HASH, "tx_hash", B256),
                Field::req(BLOCK_NUMBER, "block_number", U64),
                Field::req(SOURCE, "source", Text),
                Field::opt(TO, "to", Address),
                Field::req(SELECTOR, "selector", Bytes).max(4),
                Field::req(CALL, "call", Text),
                Field::req(TOKEN, "token", Address),
                Field::opt(TOKEN_VERSION, "token_version", U64),
                Field::req(TOKEN_VERSION_LABEL, "token_version_label", Text),
                Field::opt(QUOTE_TOKEN, "quote_token", Address),
                Field::opt(TEMPLATE, "template", Address),
                Field::req(SALT_SOURCE, "salt_source", Text),
                Field::opt(SALT, "salt", B256),
                Field::opt(INIT_CODE_HASH, "init_code_hash", B256),
                Field::opt(PREDICTED, "predicted", Address),
                Field::req(REPRODUCED, "reproduced", Bool),
                Field::req(PRODUCTION_DERIVATION, "production_derivation", Text),
            ],
        };
    }

    /// One bucket of the live mode census.
    pub mod census_bucket {
        use super::*;

        /// `CALL_PATH`, `TOKEN_VERSION`, `QUOTE_TOKEN` or `TEMPLATE`.
        pub const DIMENSION: u16 = 1;
        pub const VALUE: u16 = 2;
        pub const COUNT: u16 = 3;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.flap-freeze.census-bucket",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(DIMENSION, "dimension", Text),
                Field::req(VALUE, "value", Text),
                Field::req(COUNT, "count", U64),
            ],
        };
    }

    /// The token-address derivation of one token version.
    pub mod derivation_version {
        use super::*;

        pub const TOKEN_VERSION: u16 = 1;
        pub const LABEL: u16 = 2;
        /// `PROVEN` or `UNKNOWN`.
        pub const STATUS: u16 = 3;
        pub const TEMPLATE: u16 = 4;
        pub const INIT_CODE_HASH: u16 = 5;
        pub const LAUNCHES: u16 = 6;
        /// Launches whose salt was located in the pending calldata.
        pub const SALT_LOCATED: u16 = 7;
        pub const REPRODUCED: u16 = 8;
        pub const REASON: u16 = 9;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.flap-freeze.derivation-version",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::opt(TOKEN_VERSION, "token_version", U64),
                Field::req(LABEL, "label", Text),
                Field::req(STATUS, "status", Text),
                Field::opt(TEMPLATE, "template", Address),
                Field::opt(INIT_CODE_HASH, "init_code_hash", B256),
                Field::req(LAUNCHES, "launches", U64),
                Field::req(SALT_LOCATED, "salt_located", U64),
                Field::req(REPRODUCED, "reproduced", U64),
                Field::req(REASON, "reason", Text),
            ],
        };
    }

    /// Portal log topics in the fetched receipts.
    pub mod topic_count {
        use super::*;

        pub const TOPIC: u16 = 1;
        /// The verified-ABI or archived event signature, or `UNLABELLED`.
        pub const LABEL: u16 = 2;
        pub const COUNT: u16 = 3;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.flap-freeze.topic-count",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(TOPIC, "topic", B256),
                Field::req(LABEL, "label", Text),
                Field::req(COUNT, "count", U64),
            ],
        };
    }

    /// The bounded receipt scan (no `eth_getLogs`).
    pub mod scan {
        use super::*;

        pub const METHOD: u16 = 1;
        pub const FROM_BLOCK: u16 = 2;
        pub const SEARCH_TO_BLOCK: u16 = 3;
        /// The headers are parent-linked up to the reference block.
        pub const LINKED_TO_BLOCK: u16 = 4;
        pub const HEADERS: u16 = 5;
        pub const BLOOM_CANDIDATE_BLOCKS: u16 = 6;
        pub const RECEIPT_BLOCKS_FETCHED: u16 = 7;
        pub const RECEIPT_BLOCK_BUDGET: u16 = 8;
        /// Every block whose bloom can hold a `TokenCreated` of the Portal
        /// was read: the launch census of the window is complete.
        pub const LAUNCH_CENSUS_COMPLETE: u16 = 9;
        pub const LAUNCHES: u16 = 10;
        pub const ROUTED_LAUNCHES: u16 = 11;
        pub const REVERTED_PORTAL_CALLS: u16 = 12;
        pub const ERROR: u16 = 13;
        /// Blocks read only to find a reverted Portal call.
        pub const REVERT_SEARCH_BLOCKS_FETCHED: u16 = 14;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.flap-freeze.scan",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(METHOD, "method", Text),
                Field::req(FROM_BLOCK, "from_block", U64),
                Field::req(SEARCH_TO_BLOCK, "search_to_block", U64),
                Field::req(LINKED_TO_BLOCK, "linked_to_block", U64),
                Field::req(HEADERS, "headers", U64),
                Field::req(BLOOM_CANDIDATE_BLOCKS, "bloom_candidate_blocks", U64),
                Field::req(RECEIPT_BLOCKS_FETCHED, "receipt_blocks_fetched", U64),
                Field::req(RECEIPT_BLOCK_BUDGET, "receipt_block_budget", U64),
                Field::req(LAUNCH_CENSUS_COMPLETE, "launch_census_complete", Bool),
                Field::req(LAUNCHES, "launches", U64),
                Field::req(ROUTED_LAUNCHES, "routed_launches", U64),
                Field::req(REVERTED_PORTAL_CALLS, "reverted_portal_calls", U64),
                Field::opt(ERROR, "error", Text),
                Field::req(
                    REVERT_SEARCH_BLOCKS_FETCHED,
                    "revert_search_blocks_fetched",
                    U64,
                ),
            ],
        };
    }

    pub const CHAIN_ID: u16 = 1;
    pub const PORTAL: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    pub const REFERENCE_BLOCK: u16 = 5;
    pub const REFERENCE_BLOCK_HASH: u16 = 6;
    pub const REFERENCE_TIMESTAMP: u16 = 7;
    /// The reference block still had the pinned hash after the state reads.
    pub const PIN_RECHECKED: u16 = 8;
    /// Host only: never a path or query.
    pub const ENDPOINT_HOST: u16 = 9;
    pub const SOURCIFY_HOST: u16 = 10;
    /// SHA-256 of the running freeze binary.
    pub const TOOL_SHA256: u16 = 11;
    pub const GIT_HEAD: u16 = 12;
    pub const ARCHIVE_ROOT: u16 = 13;
    pub const ARCHIVE_INPUTS: u16 = 14;
    /// `version()` of the Portal at the reference block.
    pub const PORTAL_VERSION: u16 = 15;
    pub const CONSTANTS: u16 = 16;
    pub const CONTRACTS: u16 = 17;
    pub const SELECTORS: u16 = 18;
    pub const FIXTURES: u16 = 19;
    pub const LAUNCHES: u16 = 20;
    pub const CENSUS: u16 = 21;
    pub const DERIVATION: u16 = 22;
    pub const TOPICS: u16 = 23;
    pub const SCAN: u16 = 24;
    pub const PROPERTIES: u16 = 25;
    /// `COMPLETE` when every property is `PROVEN`, else `PARTIAL`.
    pub const FREEZE_STATUS: u16 = 26;
    pub const NON_CLAIMS: u16 = 27;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g01.flap-freeze.v1",
        record_type: 52,
        schema_version: 1,
        fields: &[
            Field::req(CHAIN_ID, "chain_id", U64),
            Field::req(PORTAL, "portal", Address),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(REFERENCE_BLOCK, "reference_block", U64),
            Field::req(REFERENCE_BLOCK_HASH, "reference_block_hash", B256),
            Field::req(REFERENCE_TIMESTAMP, "reference_timestamp", U64),
            Field::req(PIN_RECHECKED, "pin_rechecked", Bool),
            Field::req(ENDPOINT_HOST, "endpoint_host", Text),
            Field::opt(SOURCIFY_HOST, "sourcify_host", Text),
            Field::req(TOOL_SHA256, "tool_sha256", B256),
            Field::opt(GIT_HEAD, "git_head", Text),
            Field::req(ARCHIVE_ROOT, "archive_root", Text),
            Field::nested(
                ARCHIVE_INPUTS,
                "archive_inputs",
                Presence::Repeated,
                &shared::archive_input::SCHEMA,
            ),
            Field::opt(PORTAL_VERSION, "portal_version", Text),
            Field::nested(
                CONSTANTS,
                "constants",
                Presence::Repeated,
                &constant::SCHEMA,
            ),
            Field::nested(
                CONTRACTS,
                "contracts",
                Presence::Repeated,
                &shared::contract::SCHEMA,
            )
            .max(8 * 1024 * 1024),
            Field::nested(
                SELECTORS,
                "selectors",
                Presence::Repeated,
                &selector::SCHEMA,
            ),
            Field::nested(FIXTURES, "fixtures", Presence::Repeated, &fixture::SCHEMA),
            Field::nested(LAUNCHES, "launches", Presence::Repeated, &launch::SCHEMA),
            Field::nested(CENSUS, "census", Presence::Repeated, &census_bucket::SCHEMA),
            Field::nested(
                DERIVATION,
                "derivation",
                Presence::Repeated,
                &derivation_version::SCHEMA,
            ),
            Field::nested(TOPICS, "topics", Presence::Repeated, &topic_count::SCHEMA),
            Field::nested(SCAN, "scan", Presence::Required, &scan::SCHEMA),
            Field::nested(
                PROPERTIES,
                "properties",
                Presence::Repeated,
                &shared::property::SCHEMA,
            ),
            Field::req(FREEZE_STATUS, "freeze_status", Text),
            Field::rep(NON_CLAIMS, "non_claims", Text),
        ],
    };
}

pub const ALL: &[&Schema] = &[&flap_freeze::SCHEMA];

#[cfg(test)]
mod tests {
    use super::flap_freeze as f;
    use super::shared;
    use crate::{Draft, Value, decode};

    /// A record with a launch, a derivation and a property round-trips and
    /// fails closed on a flipped byte.
    #[test]
    fn a_flap_record_round_trips() {
        let mut scan = Draft::new(&f::scan::SCHEMA);
        scan.set(
            f::scan::METHOD,
            "HEADER_LOGSBLOOM THEN eth_getBlockReceipts",
        )
        .set(f::scan::FROM_BLOCK, 1u64)
        .set(f::scan::SEARCH_TO_BLOCK, 2u64)
        .set(f::scan::LINKED_TO_BLOCK, 3u64)
        .set(f::scan::HEADERS, 3u64)
        .set(f::scan::BLOOM_CANDIDATE_BLOCKS, 1u64)
        .set(f::scan::RECEIPT_BLOCKS_FETCHED, 1u64)
        .set(f::scan::RECEIPT_BLOCK_BUDGET, 10u64)
        .set(f::scan::LAUNCH_CENSUS_COMPLETE, true)
        .set(f::scan::LAUNCHES, 1u64)
        .set(f::scan::ROUTED_LAUNCHES, 0u64)
        .set(f::scan::REVERTED_PORTAL_CALLS, 0u64)
        .set(f::scan::REVERT_SEARCH_BLOCKS_FETCHED, 0u64);
        let mut launch = Draft::new(&f::launch::SCHEMA);
        launch
            .set(f::launch::TX_HASH, Value::B256([1; 32]))
            .set(f::launch::BLOCK_NUMBER, 2u64)
            .set(f::launch::SOURCE, "RECEIPT_SCAN")
            .set(
                f::launch::SELECTOR,
                Value::Bytes(vec![0x8c, 0xb5, 0x77, 0x2c]),
            )
            .set(f::launch::CALL, "newTokenV6")
            .set(f::launch::TOKEN, Value::Address([7; 20]))
            .set(f::launch::TOKEN_VERSION, 6u64)
            .set(f::launch::TOKEN_VERSION_LABEL, "TOKEN_TAXED_V3")
            .set(f::launch::SALT_SOURCE, "DECODED newTokenV6.salt")
            .set(f::launch::REPRODUCED, true)
            .set(f::launch::PRODUCTION_DERIVATION, "MATCH");
        let mut property = Draft::new(&shared::property::SCHEMA);
        property
            .set(shared::property::NAME, "token_address_derivation")
            .set(shared::property::STATUS, "PROVEN")
            .set(shared::property::EVIDENCE, "1/1");
        let mut d = Draft::new(&f::SCHEMA);
        d.set(f::CHAIN_ID, 56u64)
            .set(f::PORTAL, Value::Address([0xe2; 20]))
            .set(f::PROVENANCE_TOOL, "g01_flap_freeze")
            .set(f::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(f::REFERENCE_BLOCK, 3u64)
            .set(f::REFERENCE_BLOCK_HASH, Value::B256([3; 32]))
            .set(f::REFERENCE_TIMESTAMP, 9u64)
            .set(f::PIN_RECHECKED, true)
            .set(f::ENDPOINT_HOST, "bsc-dataseed.bnbchain.org")
            .set(f::TOOL_SHA256, Value::B256([1; 32]))
            .set(f::ARCHIVE_ROOT, "prestudy/archive")
            .push(f::LAUNCHES, launch.into_nested())
            .set(f::SCAN, scan.into_nested())
            .push(f::PROPERTIES, property.into_nested())
            .set(f::FREEZE_STATUS, "PARTIAL")
            .push(f::NON_CLAIMS, "READ_ONLY");
        let bytes = d.encode().unwrap();
        let record = decode(&f::SCHEMA, &bytes).unwrap();
        assert_eq!(
            record.nested_all(f::LAUNCHES).unwrap()[0]
                .u64(f::launch::TOKEN_VERSION)
                .unwrap(),
            6
        );
        assert_eq!(
            crate::schema::decode_any(&bytes).unwrap().fields().len(),
            record.fields().len()
        );
        let mut flipped = bytes.clone();
        flipped[30] ^= 1;
        assert!(decode(&f::SCHEMA, &flipped).is_err());
    }
}
