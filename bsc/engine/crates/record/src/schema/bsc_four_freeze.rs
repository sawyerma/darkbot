//! G0.1 Four.meme contract freeze (order V2 Phase E / §15.3): one record per
//! protocol family — `CLASSIC` (TokenManager2) and `OPENFOUR` (the modular
//! Registry/Core/Tools) — written by the read-only `g01_four_freeze`
//! qualification binary.
//!
//! Every state read of a record is pinned to one canonical block (EIP-1898
//! block hash). Selectors, topics, CREATE2 addresses and code fingerprints
//! come from alloy (`keccak256`, `sol!`), cross-checked against the archived
//! 2026-09-27 G0.1 inputs, the implementation dispatcher and canonical
//! fixtures. Each property is `PROVEN`, `UNKNOWN` or `UNSUPPORTED` with its
//! evidence; nothing unproven is stated as protocol semantics. A Sourcify
//! response is kept as opaque evidence bytes next to its SHA-256 and the
//! facts extracted from it; it is never read back as an input.

use crate::{Field, Kind::*, Presence, Schema};

pub mod four_freeze {
    use super::*;

    /// One archived G0.1 input (2026-09-27 legacy evidence) the run read.
    pub mod archive_input {
        use super::*;

        /// Relative to the archive root.
        pub const PATH: u16 = 1;
        pub const ROLE: u16 = 2;
        pub const SHA256: u16 = 3;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.four-freeze.archive-input",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(PATH, "path", Text),
                Field::req(ROLE, "role", Text),
                Field::req(SHA256, "sha256", B256),
            ],
        };
    }

    /// One Sourcify v2 lookup of a contract on chain 56.
    pub mod sourcify {
        use super::*;

        pub const ADDRESS: u16 = 1;
        /// `EXACT_MATCH`, `MATCH`, `NO_MATCH` or `UNAVAILABLE`.
        pub const STATUS: u16 = 2;
        pub const CREATION_MATCH: u16 = 3;
        pub const RUNTIME_MATCH: u16 = 4;
        pub const CONTRACT_NAME: u16 = 5;
        pub const COMPILER_VERSION: u16 = 6;
        /// Implementations Sourcify's proxy resolution names.
        pub const PROXY_IMPLEMENTATIONS: u16 = 7;
        pub const DEPLOYMENT_TX: u16 = 8;
        pub const DEPLOYMENT_BLOCK: u16 = 9;
        pub const DEPLOYER: u16 = 10;
        /// Function and event signatures of the verified ABI.
        pub const ABI_SIGNATURES: u16 = 11;
        pub const RESPONSE_SHA256: u16 = 12;
        /// The response body, opaque evidence bytes.
        pub const RESPONSE: u16 = 13;
        pub const ERROR: u16 = 14;
        /// The deployment transaction's receipt read on chain:
        /// `CONTRACT_ADDRESS_MATCH`, `CONTRACT_ADDRESS_MISMATCH`, `NOT_CHECKED`
        /// or `READ_FAILED`.
        pub const DEPLOYMENT_RECEIPT: u16 = 15;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.four-freeze.sourcify",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(ADDRESS, "address", Address),
                Field::req(STATUS, "status", Text),
                Field::opt(CREATION_MATCH, "creation_match", Text),
                Field::opt(RUNTIME_MATCH, "runtime_match", Text),
                Field::opt(CONTRACT_NAME, "contract_name", Text),
                Field::opt(COMPILER_VERSION, "compiler_version", Text),
                Field::rep(PROXY_IMPLEMENTATIONS, "proxy_implementations", Address),
                Field::opt(DEPLOYMENT_TX, "deployment_tx", B256),
                Field::opt(DEPLOYMENT_BLOCK, "deployment_block", U64),
                Field::opt(DEPLOYER, "deployer", Address),
                Field::rep(ABI_SIGNATURES, "abi_signatures", Text),
                Field::opt(RESPONSE_SHA256, "response_sha256", B256),
                Field::opt(RESPONSE, "response", Bytes).max(1024 * 1024),
                Field::opt(ERROR, "error", Text),
                Field::req(DEPLOYMENT_RECEIPT, "deployment_receipt", Text),
            ],
        };
    }

    /// One contract of the family at the reference block.
    pub mod contract {
        use super::*;

        pub const ROLE: u16 = 1;
        pub const ADDRESS: u16 = 2;
        /// `TABLE` (published address) or the getter it was resolved through.
        pub const RESOLUTION: u16 = 3;
        /// `FROZEN_SURFACE`, `VALIDATION_ONLY`, `LEGACY_NOT_FROZEN`,
        /// `DERIVATION_INPUT` or `DEPENDENCY`.
        pub const SCOPE: u16 = 4;
        /// alloy keccak256 of the runtime code (the EVM code hash).
        pub const CODE_KECCAK256: u16 = 5;
        pub const CODE_LEN: u16 = 6;
        /// Supplemental byte fingerprint, not an EVM code hash.
        pub const CODE_SHA256: u16 = 7;
        /// The EIP-1967 implementation slot, when non-zero.
        pub const IMPLEMENTATION: u16 = 8;
        pub const IMPLEMENTATION_CODE_KECCAK256: u16 = 9;
        pub const IMPLEMENTATION_CODE_LEN: u16 = 10;
        /// The 2026-09-27 values, where archived.
        pub const ARCHIVED_ADDRESS: u16 = 11;
        pub const ARCHIVED_IMPLEMENTATION: u16 = 12;
        pub const ARCHIVED_CODE_KECCAK256: u16 = 13;
        pub const ARCHIVED_IMPLEMENTATION_CODE_KECCAK256: u16 = 14;
        /// `UNCHANGED`, `NOT_ARCHIVED` or the changed parts.
        pub const CHANGE: u16 = 15;
        /// The archived published ABI the dispatcher was checked against.
        pub const ABI_SOURCE: u16 = 16;
        pub const ABI_FUNCTIONS: u16 = 17;
        pub const ABI_FUNCTIONS_IN_DISPATCHER: u16 = 18;
        pub const ABI_FUNCTIONS_ABSENT: u16 = 19;
        /// CREATE/CREATE2 instructions of the executing code (implementation
        /// when a proxy), e.g. `CREATE=false CREATE2=true`.
        pub const CREATE_OPCODES: u16 = 20;
        /// Sourcify lookups: the address itself, then its implementation.
        pub const SOURCIFY: u16 = 21;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.four-freeze.contract",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(ROLE, "role", Text),
                Field::req(ADDRESS, "address", Address),
                Field::req(RESOLUTION, "resolution", Text),
                Field::req(SCOPE, "scope", Text),
                Field::req(CODE_KECCAK256, "code_keccak256", B256),
                Field::req(CODE_LEN, "code_len", U64),
                Field::req(CODE_SHA256, "code_sha256", B256),
                Field::opt(IMPLEMENTATION, "implementation", Address),
                Field::opt(
                    IMPLEMENTATION_CODE_KECCAK256,
                    "implementation_code_keccak256",
                    B256,
                ),
                Field::opt(IMPLEMENTATION_CODE_LEN, "implementation_code_len", U64),
                Field::opt(ARCHIVED_ADDRESS, "archived_address", Address),
                Field::opt(ARCHIVED_IMPLEMENTATION, "archived_implementation", Address),
                Field::opt(ARCHIVED_CODE_KECCAK256, "archived_code_keccak256", B256),
                Field::opt(
                    ARCHIVED_IMPLEMENTATION_CODE_KECCAK256,
                    "archived_implementation_code_keccak256",
                    B256,
                ),
                Field::req(CHANGE, "change", Text),
                Field::opt(ABI_SOURCE, "abi_source", Text),
                Field::opt(ABI_FUNCTIONS, "abi_functions", U64),
                Field::opt(
                    ABI_FUNCTIONS_IN_DISPATCHER,
                    "abi_functions_in_dispatcher",
                    U64,
                ),
                Field::rep(ABI_FUNCTIONS_ABSENT, "abi_functions_absent", Text),
                Field::req(CREATE_OPCODES, "create_opcodes", Text),
                Field::nested(SOURCIFY, "sourcify", Presence::Repeated, &sourcify::SCHEMA)
                    .max(4 * 1024 * 1024),
            ],
        };
    }

    /// One function selector or event topic.
    pub mod selector {
        use super::*;

        /// `FUNCTION` or `EVENT`.
        pub const KIND: u16 = 1;
        /// `LAUNCH`, `BUY`, `SELL`, `ESTIMATE`, `STATE`, `GETTER`,
        /// `VALIDATION`, `LAUNCH_EVENT`, `BUY_EVENT`, `SELL_EVENT`,
        /// `LIFECYCLE_EVENT`.
        pub const ROLE: u16 = 2;
        /// The contract role whose dispatcher was checked.
        pub const CONTRACT: u16 = 3;
        pub const SIGNATURE: u16 = 4;
        /// 4-byte selector or 32-byte topic.
        pub const VALUE: u16 = 5;
        pub const DERIVATION: u16 = 6;
        /// The signature in the archived published ABI, re-hashed with alloy:
        /// `MATCH`, `MISMATCH` or `NOT_IN_ARCHIVED_ABI`.
        pub const ABI_ARCHIVE: u16 = 7;
        /// The archived independent `web3_sha3` reference: `MATCH`,
        /// `MISMATCH` or `NOT_IN_REFERENCE`.
        pub const KECCAK_REFERENCE: u16 = 8;
        /// Functions: `PUSH4_PRESENT`, `PUSH<n>_PRESENT` or `ABSENT` in the
        /// executing dispatcher. Events (supporting evidence only):
        /// `TOPIC_PUSH32_PRESENT` or `TOPIC_NOT_A_PUSH32`.
        pub const DISPATCHER: u16 = 9;
        /// `SEEN_IN_CANONICAL_FIXTURE` or `NOT_OBSERVED`.
        pub const FIXTURE: u16 = 10;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.four-freeze.selector",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(KIND, "kind", Text),
                Field::req(ROLE, "role", Text),
                Field::req(CONTRACT, "contract", Text),
                Field::req(SIGNATURE, "signature", Text),
                Field::req(VALUE, "value", Bytes).max(32),
                Field::req(DERIVATION, "derivation", Text),
                Field::req(ABI_ARCHIVE, "abi_archive", Text),
                Field::req(KECCAK_REFERENCE, "keccak_reference", Text),
                Field::req(DISPATCHER, "dispatcher", Text),
                Field::req(FIXTURE, "fixture", Text),
            ],
        };
    }

    /// A state read the ABI exposes for quotes, state or mode, and its
    /// probe at the reference block.
    pub mod state_read {
        use super::*;

        pub const CONTRACT: u16 = 1;
        pub const SIGNATURE: u16 = 2;
        pub const SELECTOR: u16 = 3;
        /// `QUOTE`, `STATE`, `MODE`, `DERIVATION` or `VALIDATION`.
        pub const PURPOSE: u16 = 4;
        pub const MUTABILITY: u16 = 5;
        pub const DISPATCHER: u16 = 6;
        /// `CALL_OK`, `REVERTED`, `CALL_FAILED` or `NOT_PROBED`, with detail.
        pub const PROBE: u16 = 7;
        pub const PROBE_INPUT: u16 = 8;
        pub const OUTPUT: u16 = 9;
        pub const SEMANTICS: u16 = 10;
        /// `SUPPORTED` (in the executing dispatcher) or `UNSUPPORTED` (listed
        /// by the published ABI, absent from the executing dispatcher).
        pub const STATUS: u16 = 11;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.four-freeze.state-read",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(CONTRACT, "contract", Text),
                Field::req(SIGNATURE, "signature", Text),
                Field::req(SELECTOR, "selector", Bytes).max(4),
                Field::req(PURPOSE, "purpose", Text),
                Field::req(MUTABILITY, "mutability", Text),
                Field::req(DISPATCHER, "dispatcher", Text),
                Field::req(PROBE, "probe", Text),
                Field::opt(PROBE_INPUT, "probe_input", Bytes).max(8192),
                Field::opt(OUTPUT, "output", Bytes).max(8192),
                Field::req(SEMANTICS, "semantics", Text),
                Field::req(STATUS, "status", Text),
            ],
        };
    }

    /// A named check and its result.
    pub mod check {
        use super::*;

        pub const NAME: u16 = 1;
        pub const PASS: u16 = 2;
        pub const DETAIL: u16 = 3;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.four-freeze.check",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(NAME, "name", Text),
                Field::req(PASS, "pass", Bool),
                Field::req(DETAIL, "detail", Text),
            ],
        };
    }

    /// One decoded calldata parameter (derived view of `fixture.input`).
    pub mod param {
        use super::*;

        pub const NAME: u16 = 1;
        pub const VALUE: u16 = 2;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.four-freeze.param",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(NAME, "name", Text),
                Field::req(VALUE, "value", Text),
            ],
        };
    }

    /// A canonical launch or BUY transaction with its decoded event.
    pub mod fixture {
        use super::*;

        /// `LAUNCH` or `BUY`.
        pub const KIND: u16 = 1;
        /// `ARCHIVED_2026_09_27_REVERIFIED` or `RECEIPT_SCAN`.
        pub const SOURCE: u16 = 2;
        pub const TX_HASH: u16 = 3;
        pub const BLOCK_NUMBER: u16 = 4;
        pub const BLOCK_HASH: u16 = 5;
        pub const TX_INDEX: u16 = 6;
        pub const LOG_INDEX: u16 = 7;
        pub const RECEIPT_STATUS: u16 = 8;
        pub const FROM: u16 = 9;
        pub const TO: u16 = 10;
        pub const VALUE_WEI: u16 = 11;
        pub const INPUT: u16 = 12;
        pub const SELECTOR: u16 = 13;
        pub const CALL_SIGNATURE: u16 = 14;
        pub const EVENT_SIGNATURE: u16 = 15;
        pub const LOG_TOPICS: u16 = 16;
        pub const LOG_DATA: u16 = 17;
        pub const TOKEN: u16 = 18;
        /// Creator of a launch, buyer of a BUY.
        pub const ACCOUNT: u16 = 19;
        pub const PARAMS: u16 = 20;
        pub const CHECKS: u16 = 21;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.four-freeze.fixture",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(KIND, "kind", Text),
                Field::req(SOURCE, "source", Text),
                Field::req(TX_HASH, "tx_hash", B256),
                Field::req(BLOCK_NUMBER, "block_number", U64),
                Field::req(BLOCK_HASH, "block_hash", B256),
                Field::req(TX_INDEX, "tx_index", U64),
                Field::req(LOG_INDEX, "log_index", U64),
                Field::req(RECEIPT_STATUS, "receipt_status", U64),
                Field::req(FROM, "from", Address),
                Field::req(TO, "to", Address),
                Field::req(VALUE_WEI, "value_wei", U256),
                Field::req(INPUT, "input", Bytes).max(128 * 1024),
                Field::req(SELECTOR, "selector", Bytes).max(4),
                Field::req(CALL_SIGNATURE, "call_signature", Text),
                Field::req(EVENT_SIGNATURE, "event_signature", Text),
                Field::rep(LOG_TOPICS, "log_topics", B256),
                Field::req(LOG_DATA, "log_data", Bytes).max(128 * 1024),
                Field::req(TOKEN, "token", Address),
                Field::req(ACCOUNT, "account", Address),
                Field::nested(PARAMS, "params", Presence::Repeated, &param::SCHEMA),
                Field::nested(CHECKS, "checks", Presence::Repeated, &check::SCHEMA),
            ],
        };
    }

    /// One reproduction of a fixture token address from its pending inputs.
    pub mod derivation_attempt {
        use super::*;

        pub const FIXTURE_TX: u16 = 1;
        /// The salt read from the create calldata.
        pub const SALT: u16 = 2;
        /// The clone template the address is predicted with.
        pub const TEMPLATE: u16 = 3;
        pub const INIT_CODE_HASH: u16 = 4;
        pub const PREDICTED: u16 = 5;
        /// The token of the canonical create event.
        pub const ACTUAL: u16 = 6;
        /// The EIP-1167 target of the token's runtime at the reference block.
        pub const OBSERVED_TEMPLATE: u16 = 7;
        pub const MATCH: u16 = 8;
        pub const DETAIL: u16 = 9;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.four-freeze.derivation-attempt",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(FIXTURE_TX, "fixture_tx", B256),
                Field::opt(SALT, "salt", B256),
                Field::opt(TEMPLATE, "template", Address),
                Field::opt(INIT_CODE_HASH, "init_code_hash", B256),
                Field::opt(PREDICTED, "predicted", Address),
                Field::req(ACTUAL, "actual", Address),
                Field::opt(OBSERVED_TEMPLATE, "observed_template", Address),
                Field::req(MATCH, "match", Bool),
                Field::req(DETAIL, "detail", Text),
            ],
        };
    }

    /// Whether the token address is knowable before execution.
    pub mod derivation {
        use super::*;

        /// `PROVEN` or `UNKNOWN`.
        pub const STATUS: u16 = 1;
        pub const REASON: u16 = 2;
        pub const METHOD: u16 = 3;
        pub const DEPLOYER: u16 = 4;
        pub const DEPLOYER_SOURCE: u16 = 5;
        pub const TEMPLATE: u16 = 6;
        pub const TEMPLATE_SOURCE: u16 = 7;
        pub const SALT_LOCATION: u16 = 8;
        pub const ATTEMPTS: u16 = 9;
        /// What would invalidate a precomputed address.
        pub const INVALIDATED_BY: u16 = 10;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.four-freeze.derivation",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(STATUS, "status", Text),
                Field::req(REASON, "reason", Text),
                Field::req(METHOD, "method", Text),
                Field::opt(DEPLOYER, "deployer", Address),
                Field::req(DEPLOYER_SOURCE, "deployer_source", Text),
                Field::opt(TEMPLATE, "template", Address),
                Field::req(TEMPLATE_SOURCE, "template_source", Text),
                Field::req(SALT_LOCATION, "salt_location", Text),
                Field::nested(
                    ATTEMPTS,
                    "attempts",
                    Presence::Repeated,
                    &derivation_attempt::SCHEMA,
                ),
                Field::rep(INVALIDATED_BY, "invalidated_by", Text),
            ],
        };
    }

    /// The bounded receipt scan for canonical fixtures (no `eth_getLogs`).
    pub mod scan {
        use super::*;

        pub const METHOD: u16 = 1;
        pub const FROM_BLOCK: u16 = 2;
        /// The newest block searched for fixtures.
        pub const SEARCH_TO_BLOCK: u16 = 3;
        /// The headers are parent-linked up to the reference block.
        pub const LINKED_TO_BLOCK: u16 = 4;
        pub const HEADERS: u16 = 5;
        pub const PARENT_LINKED: u16 = 6;
        pub const BLOOM_CANDIDATE_BLOCKS: u16 = 7;
        pub const RECEIPT_BLOCKS_FETCHED: u16 = 8;
        pub const RECEIPT_BLOCK_BUDGET: u16 = 9;
        /// `FOUND` or `UNKNOWN` with the reason.
        pub const LAUNCH_FIXTURE: u16 = 10;
        pub const BUY_FIXTURE: u16 = 11;
        /// Successful create events of the family in the fetched receipts.
        pub const LAUNCH_EVENTS_SEEN: u16 = 12;
        /// Of these, created through another top-level contract.
        pub const INDIRECT_LAUNCHES_SEEN: u16 = 13;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.four-freeze.scan",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(METHOD, "method", Text),
                Field::req(FROM_BLOCK, "from_block", U64),
                Field::req(SEARCH_TO_BLOCK, "search_to_block", U64),
                Field::req(LINKED_TO_BLOCK, "linked_to_block", U64),
                Field::req(HEADERS, "headers", U64),
                Field::req(PARENT_LINKED, "parent_linked", Bool),
                Field::req(BLOOM_CANDIDATE_BLOCKS, "bloom_candidate_blocks", U64),
                Field::req(RECEIPT_BLOCKS_FETCHED, "receipt_blocks_fetched", U64),
                Field::req(RECEIPT_BLOCK_BUDGET, "receipt_block_budget", U64),
                Field::req(LAUNCH_FIXTURE, "launch_fixture", Text),
                Field::req(BUY_FIXTURE, "buy_fixture", Text),
                Field::req(LAUNCH_EVENTS_SEEN, "launch_events_seen", U64),
                Field::req(INDIRECT_LAUNCHES_SEEN, "indirect_launches_seen", U64),
            ],
        };

        /// v2 (Phase E review): the SELL scan, every OpenFour preset in use
        /// and the bounded lifecycle-event scan.
        pub const SELL_FIXTURE: u16 = 14;
        /// Successful SELL events of the family in the fetched receipts.
        pub const SELL_EVENTS_SEEN: u16 = 15;
        /// `preset <id>: <n> launches (<d> direct), fixture <yes|no>`.
        pub const PRESETS: u16 = 16;
        /// `<signature>: <n> logs, fixture <tx|none>`.
        pub const LIFECYCLE: u16 = 17;

        pub const V2: Schema = Schema {
            name: "darkbot.g01.four-freeze.scan",
            record_type: 0,
            schema_version: 2,
            fields: &[
                Field::req(METHOD, "method", Text),
                Field::req(FROM_BLOCK, "from_block", U64),
                Field::req(SEARCH_TO_BLOCK, "search_to_block", U64),
                Field::req(LINKED_TO_BLOCK, "linked_to_block", U64),
                Field::req(HEADERS, "headers", U64),
                Field::req(PARENT_LINKED, "parent_linked", Bool),
                Field::req(BLOOM_CANDIDATE_BLOCKS, "bloom_candidate_blocks", U64),
                Field::req(RECEIPT_BLOCKS_FETCHED, "receipt_blocks_fetched", U64),
                Field::req(RECEIPT_BLOCK_BUDGET, "receipt_block_budget", U64),
                Field::req(LAUNCH_FIXTURE, "launch_fixture", Text),
                Field::req(BUY_FIXTURE, "buy_fixture", Text),
                Field::req(LAUNCH_EVENTS_SEEN, "launch_events_seen", U64),
                Field::req(INDIRECT_LAUNCHES_SEEN, "indirect_launches_seen", U64),
                Field::req(SELL_FIXTURE, "sell_fixture", Text),
                Field::req(SELL_EVENTS_SEEN, "sell_events_seen", U64),
                Field::rep(PRESETS, "presets", Text),
                Field::rep(LIFECYCLE, "lifecycle", Text),
            ],
        };

        /// v3 (Phase E review pass 2, N1): the coverage the scan really
        /// had. Every block from `examined_from_block` to `search_to_block`
        /// was examined; older blocks of the window were not when the
        /// receipt budget ran out.
        pub const EXAMINED_FROM_BLOCK: u16 = 18;
        /// `WINDOW_COMPLETE` or `RECEIPT_BUDGET_EXHAUSTED`.
        pub const STOP_REASON: u16 = 19;
        /// Blocks of the whole window whose bloom can hold any scanned
        /// event, fetched or not.
        pub const WINDOW_BLOOM_CANDIDATES: u16 = 20;
        /// OpenFour: the launch-only pass over the whole window (its range,
        /// bloom candidates, receipt blocks fetched of its budget, stop
        /// reason).
        pub const LAUNCH_PASS: u16 = 21;

        pub const V3: Schema = Schema {
            name: "darkbot.g01.four-freeze.scan",
            record_type: 0,
            schema_version: 3,
            fields: &[
                Field::req(METHOD, "method", Text),
                Field::req(FROM_BLOCK, "from_block", U64),
                Field::req(SEARCH_TO_BLOCK, "search_to_block", U64),
                Field::req(LINKED_TO_BLOCK, "linked_to_block", U64),
                Field::req(HEADERS, "headers", U64),
                Field::req(PARENT_LINKED, "parent_linked", Bool),
                Field::req(BLOOM_CANDIDATE_BLOCKS, "bloom_candidate_blocks", U64),
                Field::req(RECEIPT_BLOCKS_FETCHED, "receipt_blocks_fetched", U64),
                Field::req(RECEIPT_BLOCK_BUDGET, "receipt_block_budget", U64),
                Field::req(LAUNCH_FIXTURE, "launch_fixture", Text),
                Field::req(BUY_FIXTURE, "buy_fixture", Text),
                Field::req(LAUNCH_EVENTS_SEEN, "launch_events_seen", U64),
                Field::req(INDIRECT_LAUNCHES_SEEN, "indirect_launches_seen", U64),
                Field::req(SELL_FIXTURE, "sell_fixture", Text),
                Field::req(SELL_EVENTS_SEEN, "sell_events_seen", U64),
                Field::rep(PRESETS, "presets", Text),
                Field::rep(LIFECYCLE, "lifecycle", Text),
                Field::req(EXAMINED_FROM_BLOCK, "examined_from_block", U64),
                Field::req(STOP_REASON, "stop_reason", Text),
                Field::req(WINDOW_BLOOM_CANDIDATES, "window_bloom_candidates", U64),
                Field::opt(LAUNCH_PASS, "launch_pass", Text),
            ],
        };
    }

    /// One required G0.1 property and its proof status.
    pub mod property {
        use super::*;

        pub const NAME: u16 = 1;
        /// `PROVEN`, `UNKNOWN` or `UNSUPPORTED`.
        pub const STATUS: u16 = 2;
        pub const EVIDENCE: u16 = 3;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.four-freeze.property",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(NAME, "name", Text),
                Field::req(STATUS, "status", Text),
                Field::req(EVIDENCE, "evidence", Text),
            ],
        };
    }

    /// `CLASSIC` or `OPENFOUR`.
    pub const FAMILY: u16 = 1;
    pub const CHAIN_ID: u16 = 2;
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
    pub const CONTRACTS: u16 = 15;
    pub const SELECTORS: u16 = 16;
    pub const STATE_READS: u16 = 17;
    pub const FIXTURES: u16 = 18;
    pub const DERIVATION: u16 = 19;
    pub const SCAN: u16 = 20;
    pub const PROPERTIES: u16 = 21;
    /// `COMPLETE` when every required property is `PROVEN`, else `PARTIAL`.
    pub const FREEZE_STATUS: u16 = 22;
    pub const NON_CLAIMS: u16 = 23;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g01.four-freeze.v1",
        record_type: 51,
        schema_version: 1,
        fields: &[
            Field::req(FAMILY, "family", Text),
            Field::req(CHAIN_ID, "chain_id", U64),
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
                &archive_input::SCHEMA,
            ),
            Field::nested(
                CONTRACTS,
                "contracts",
                Presence::Repeated,
                &contract::SCHEMA,
            )
            .max(8 * 1024 * 1024),
            Field::nested(
                SELECTORS,
                "selectors",
                Presence::Repeated,
                &selector::SCHEMA,
            ),
            Field::nested(
                STATE_READS,
                "state_reads",
                Presence::Repeated,
                &state_read::SCHEMA,
            ),
            Field::nested(FIXTURES, "fixtures", Presence::Repeated, &fixture::SCHEMA),
            Field::nested(
                DERIVATION,
                "derivation",
                Presence::Required,
                &derivation::SCHEMA,
            ),
            Field::nested(SCAN, "scan", Presence::Required, &scan::SCHEMA),
            Field::nested(
                PROPERTIES,
                "properties",
                Presence::Repeated,
                &property::SCHEMA,
            ),
            Field::req(FREEZE_STATUS, "freeze_status", Text),
            Field::rep(NON_CLAIMS, "non_claims", Text),
        ],
    };

    /// v2 (Phase E review): the scan carries SELL, presets and lifecycle.
    pub const V2: Schema = Schema {
        name: "darkbot.g01.four-freeze.v2",
        record_type: 51,
        schema_version: 2,
        fields: &[
            Field::req(FAMILY, "family", Text),
            Field::req(CHAIN_ID, "chain_id", U64),
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
                &archive_input::SCHEMA,
            ),
            Field::nested(
                CONTRACTS,
                "contracts",
                Presence::Repeated,
                &contract::SCHEMA,
            )
            .max(8 * 1024 * 1024),
            Field::nested(
                SELECTORS,
                "selectors",
                Presence::Repeated,
                &selector::SCHEMA,
            ),
            Field::nested(
                STATE_READS,
                "state_reads",
                Presence::Repeated,
                &state_read::SCHEMA,
            ),
            Field::nested(FIXTURES, "fixtures", Presence::Repeated, &fixture::SCHEMA),
            Field::nested(
                DERIVATION,
                "derivation",
                Presence::Required,
                &derivation::SCHEMA,
            ),
            Field::nested(SCAN, "scan", Presence::Required, &scan::V2),
            Field::nested(
                PROPERTIES,
                "properties",
                Presence::Repeated,
                &property::SCHEMA,
            ),
            Field::req(FREEZE_STATUS, "freeze_status", Text),
            Field::rep(NON_CLAIMS, "non_claims", Text),
        ],
    };

    /// v3 (Phase E review pass 2): the scan states its real coverage and
    /// the OpenFour launch pass.
    pub const V3: Schema = Schema {
        name: "darkbot.g01.four-freeze.v3",
        record_type: 51,
        schema_version: 3,
        fields: &[
            Field::req(FAMILY, "family", Text),
            Field::req(CHAIN_ID, "chain_id", U64),
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
                &archive_input::SCHEMA,
            ),
            Field::nested(
                CONTRACTS,
                "contracts",
                Presence::Repeated,
                &contract::SCHEMA,
            )
            .max(8 * 1024 * 1024),
            Field::nested(
                SELECTORS,
                "selectors",
                Presence::Repeated,
                &selector::SCHEMA,
            ),
            Field::nested(
                STATE_READS,
                "state_reads",
                Presence::Repeated,
                &state_read::SCHEMA,
            ),
            Field::nested(FIXTURES, "fixtures", Presence::Repeated, &fixture::SCHEMA),
            Field::nested(
                DERIVATION,
                "derivation",
                Presence::Required,
                &derivation::SCHEMA,
            ),
            Field::nested(SCAN, "scan", Presence::Required, &scan::V3),
            Field::nested(
                PROPERTIES,
                "properties",
                Presence::Repeated,
                &property::SCHEMA,
            ),
            Field::req(FREEZE_STATUS, "freeze_status", Text),
            Field::rep(NON_CLAIMS, "non_claims", Text),
        ],
    };
}

pub const ALL: &[&Schema] = &[&four_freeze::SCHEMA, &four_freeze::V2, &four_freeze::V3];

#[cfg(test)]
mod tests {
    use super::four_freeze as f;
    use crate::{Draft, Value, decode};

    /// A minimal record of every nested kind round-trips byte-identically
    /// and fails closed on a flipped byte.
    #[test]
    fn a_family_record_round_trips() {
        let mut property = Draft::new(&f::property::SCHEMA);
        property
            .set(f::property::NAME, "token_address_derivation")
            .set(f::property::STATUS, "UNKNOWN")
            .set(f::property::EVIDENCE, "no fixture");
        let mut scan = Draft::new(&f::scan::SCHEMA);
        scan.set(f::scan::METHOD, "HEADER_BLOOM_THEN_BLOCK_RECEIPTS")
            .set(f::scan::FROM_BLOCK, 1u64)
            .set(f::scan::SEARCH_TO_BLOCK, 2u64)
            .set(f::scan::LINKED_TO_BLOCK, 3u64)
            .set(f::scan::HEADERS, 3u64)
            .set(f::scan::PARENT_LINKED, true)
            .set(f::scan::BLOOM_CANDIDATE_BLOCKS, 0u64)
            .set(f::scan::RECEIPT_BLOCKS_FETCHED, 0u64)
            .set(f::scan::RECEIPT_BLOCK_BUDGET, 10u64)
            .set(f::scan::LAUNCH_FIXTURE, "UNKNOWN: none within 2 blocks")
            .set(f::scan::BUY_FIXTURE, "UNKNOWN: none within 2 blocks")
            .set(f::scan::LAUNCH_EVENTS_SEEN, 0u64)
            .set(f::scan::INDIRECT_LAUNCHES_SEEN, 0u64);
        let mut derivation = Draft::new(&f::derivation::SCHEMA);
        derivation
            .set(f::derivation::STATUS, "UNKNOWN")
            .set(f::derivation::REASON, "no fixture")
            .set(f::derivation::METHOD, "CREATE2_EIP1167_CLONE")
            .set(f::derivation::DEPLOYER_SOURCE, "TABLE")
            .set(f::derivation::TEMPLATE_SOURCE, "TABLE")
            .set(f::derivation::SALT_LOCATION, "args head word 1");
        let mut d = Draft::new(&f::SCHEMA);
        d.set(f::FAMILY, "OPENFOUR")
            .set(f::CHAIN_ID, 56u64)
            .set(f::PROVENANCE_TOOL, "g01_four_freeze")
            .set(f::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(f::REFERENCE_BLOCK, 3u64)
            .set(f::REFERENCE_BLOCK_HASH, Value::B256([3; 32]))
            .set(f::REFERENCE_TIMESTAMP, 9u64)
            .set(f::PIN_RECHECKED, true)
            .set(f::ENDPOINT_HOST, "bsc-dataseed.bnbchain.org")
            .set(f::TOOL_SHA256, Value::B256([1; 32]))
            .set(f::ARCHIVE_ROOT, "prestudy/archive")
            .set(f::DERIVATION, derivation.into_nested())
            .set(f::SCAN, scan.into_nested())
            .push(f::PROPERTIES, property.into_nested())
            .set(f::FREEZE_STATUS, "PARTIAL")
            .push(f::NON_CLAIMS, "NO_SIGNING");
        let bytes = d.encode().unwrap();
        let record = decode(&f::SCHEMA, &bytes).unwrap();
        assert_eq!(record.text(f::FAMILY).unwrap(), "OPENFOUR");
        assert_eq!(
            record.nested_all(f::PROPERTIES).unwrap()[0]
                .text(f::property::STATUS)
                .unwrap(),
            "UNKNOWN"
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
