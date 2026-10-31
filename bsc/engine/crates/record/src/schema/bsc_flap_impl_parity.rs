//! BSC3-065: the observed Flap Portal implementation against the
//! source-verified one it replaced, on the paths DarkBot trades, read-only.
//! Real Portal calls of a block range (launch, BUY, SELL, reverted), sampled
//! evenly over the range per class, are each executed twice by the BSC node
//! (`eth_simulateV1`) on the state of their parent block, once with the
//! Portal's EIP-1967 implementation slot holding each implementation,
//! followed in the same simulated block by the Portal's and the token's
//! views of the state the call left; and each launch once more followed by
//! the BUY the C3 path sends for its token (another sender, the production
//! encoder and amount). The two executions are compared: status, return
//! data and logs of every call (up to addresses only one execution creates)
//! and gas. The bytecode facts of both implementations (dispatcher
//! selectors, embedded addresses) are recorded beside them. Written by the
//! read-only `g01_flap_impl_parity` qualification binary; nothing is signed
//! or sent.

use crate::{Field, Kind::*, Presence, Schema};

pub mod flap_impl_parity {
    use super::*;

    /// An address only one execution's logs name, paired with the other's.
    pub mod renamed {
        use super::*;

        pub const SOURCE: u16 = 1;
        pub const OBSERVED: u16 = 2;
        /// Both had no code at the parent block (archive `eth_getCode`):
        /// `CREATED_BY_THE_CALL`, else what was found.
        pub const PROOF: u16 = 3;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.flap-impl-parity.renamed",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(SOURCE, "source", Address),
                Field::req(OBSERVED, "observed", Address),
                Field::req(PROOF, "proof", Text),
            ],
        };
    }

    /// One real Portal call executed under both implementations.
    pub mod case {
        use super::*;

        pub const BLOCK_NUMBER: u16 = 1;
        pub const BLOCK_HASH: u16 = 2;
        /// The pin of both executions.
        pub const PARENT_HASH: u16 = 3;
        pub const TX_INDEX: u16 = 4;
        pub const TX_HASH: u16 = 5;
        pub const FROM: u16 = 6;
        pub const VALUE_WEI: u16 = 7;
        pub const SELECTOR: u16 = 8;
        /// `LAUNCH`, `BUY`, `SELL`, `REVERTED` or `OTHER` (production
        /// `recognize` and the receipt), or `LAUNCH_THEN_BUY`: the launch
        /// `TX_HASH` followed by the C3 BUY of its token (the executions'
        /// fields are of that BUY, `PRECEDING_*` of the launch).
        pub const CLASS: u16 = 9;
        pub const RECEIPT_STATUS: u16 = 10;
        pub const RECEIPT_GAS_USED: u16 = 11;
        /// The source-verified implementation's execution.
        pub const SOURCE_STATUS: u16 = 12;
        pub const SOURCE_RETURN_KECCAK: u16 = 13;
        pub const SOURCE_LOGS: u16 = 14;
        pub const SOURCE_LOGS_KECCAK: u16 = 15;
        pub const SOURCE_GAS: u16 = 16;
        /// The observed implementation's execution.
        pub const OBSERVED_STATUS: u16 = 17;
        pub const OBSERVED_RETURN_KECCAK: u16 = 18;
        pub const OBSERVED_LOGS: u16 = 19;
        pub const OBSERVED_LOGS_KECCAK: u16 = 20;
        pub const OBSERVED_GAS: u16 = 21;
        pub const GAS_DELTA: u16 = 22;
        pub const RENAMED: u16 = 23;
        /// `IDENTICAL`, `IDENTICAL_BUT_CREATED` or `DIFF`: every call of
        /// the simulated block (the case's and the views after it).
        pub const OUTCOME: u16 = 24;
        pub const DETAIL: u16 = 25;
        /// The calls of the simulated block, the case's first.
        pub const CALLS: u16 = 26;
        /// Keccak-256 of every call's status, return data and logs.
        pub const SOURCE_ALL_KECCAK: u16 = 27;
        pub const OBSERVED_ALL_KECCAK: u16 = 28;
        /// The token the views read (the call's, a launch's return).
        pub const TOKEN: u16 = 29;
        /// `LAUNCH_THEN_BUY`: the launch's status under each implementation,
        /// the BUY's sender and amount.
        pub const PRECEDING_SOURCE_STATUS: u16 = 30;
        pub const PRECEDING_OBSERVED_STATUS: u16 = 31;
        pub const BUYER: u16 = 32;
        pub const BUY_VALUE_WEI: u16 = 33;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.flap-impl-parity.case",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(BLOCK_NUMBER, "block_number", U64),
                Field::req(BLOCK_HASH, "block_hash", B256),
                Field::req(PARENT_HASH, "parent_hash", B256),
                Field::req(TX_INDEX, "tx_index", U64),
                Field::req(TX_HASH, "tx_hash", B256),
                Field::req(FROM, "from", Address),
                Field::req(VALUE_WEI, "value_wei", U256),
                Field::req(SELECTOR, "selector", Bytes),
                Field::req(CLASS, "class", Text),
                Field::req(RECEIPT_STATUS, "receipt_status", U64),
                Field::opt(RECEIPT_GAS_USED, "receipt_gas_used", U64),
                Field::req(SOURCE_STATUS, "source_status", U64),
                Field::req(SOURCE_RETURN_KECCAK, "source_return_keccak", B256),
                Field::req(SOURCE_LOGS, "source_logs", U64),
                Field::req(SOURCE_LOGS_KECCAK, "source_logs_keccak", B256),
                Field::req(SOURCE_GAS, "source_gas", U64),
                Field::req(OBSERVED_STATUS, "observed_status", U64),
                Field::req(OBSERVED_RETURN_KECCAK, "observed_return_keccak", B256),
                Field::req(OBSERVED_LOGS, "observed_logs", U64),
                Field::req(OBSERVED_LOGS_KECCAK, "observed_logs_keccak", B256),
                Field::req(OBSERVED_GAS, "observed_gas", U64),
                Field::req(GAS_DELTA, "gas_delta", I64),
                Field::nested(RENAMED, "renamed", Presence::Repeated, &renamed::SCHEMA),
                Field::req(OUTCOME, "outcome", Text),
                Field::req(DETAIL, "detail", Text),
                Field::req(CALLS, "calls", U64),
                Field::req(SOURCE_ALL_KECCAK, "source_all_keccak", B256),
                Field::req(OBSERVED_ALL_KECCAK, "observed_all_keccak", B256),
                Field::opt(TOKEN, "token", Address),
                Field::opt(PRECEDING_SOURCE_STATUS, "preceding_source_status", U64),
                Field::opt(PRECEDING_OBSERVED_STATUS, "preceding_observed_status", U64),
                Field::opt(BUYER, "buyer", Address),
                Field::opt(BUY_VALUE_WEI, "buy_value_wei", U256),
            ],
        };
    }

    /// The calls of one class in the range and those executed of them,
    /// spread evenly over the class's calls in the range.
    pub mod sample {
        use super::*;

        pub const CLASS: u16 = 1;
        pub const SEEN: u16 = 2;
        pub const SAMPLED: u16 = 3;
        /// Sampled cases whose every traded call succeeded under both
        /// implementations (status 1): only these count towards a minimum.
        pub const EXECUTED: u16 = 4;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.flap-impl-parity.sample",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(CLASS, "class", Text),
                Field::req(SEEN, "seen", U64),
                Field::req(SAMPLED, "sampled", U64),
                Field::req(EXECUTED, "executed", U64),
            ],
        };
    }

    /// The gas of one class under both implementations.
    pub mod gas {
        use super::*;

        pub const CLASS: u16 = 1;
        pub const CASES: u16 = 2;
        pub const EQUAL: u16 = 3;
        pub const MIN_DELTA: u16 = 4;
        pub const MAX_DELTA: u16 = 5;
        pub const MAX_OBSERVED_GAS: u16 = 6;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.flap-impl-parity.gas",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(CLASS, "class", Text),
                Field::req(CASES, "cases", U64),
                Field::req(EQUAL, "equal", U64),
                Field::req(MIN_DELTA, "min_delta", I64),
                Field::req(MAX_DELTA, "max_delta", I64),
                Field::req(MAX_OBSERVED_GAS, "max_observed_gas", U64),
            ],
        };
    }

    pub const CHAIN_ID: u16 = 1;
    pub const PORTAL: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    /// Hosts only: never a path or query.
    pub const ENDPOINT_HOST: u16 = 5;
    pub const ARCHIVE_HOST: u16 = 6;
    pub const TOOL_SHA256: u16 = 7;
    pub const GIT_HEAD: u16 = 8;
    /// The source-verified implementation (the production deployment's
    /// predecessor) and the observed one.
    pub const SOURCE_IMPLEMENTATION: u16 = 9;
    pub const SOURCE_VERSION: u16 = 10;
    pub const SOURCE_RUNTIME_SHA256: u16 = 11;
    pub const SOURCE_CODE_LEN: u16 = 12;
    pub const OBSERVED_IMPLEMENTATION: u16 = 13;
    pub const OBSERVED_VERSION: u16 = 14;
    pub const OBSERVED_RUNTIME_SHA256: u16 = 15;
    pub const OBSERVED_CODE_LEN: u16 = 16;
    /// The block whose state both codes were read at.
    pub const CODE_BLOCK: u16 = 17;
    pub const CODE_BLOCK_HASH: u16 = 18;
    /// Dispatcher selectors (PUSH4 … EQ) only one implementation has.
    pub const SELECTORS_ONLY_SOURCE: u16 = 19;
    pub const SELECTORS_ONLY_OBSERVED: u16 = 20;
    /// Every selector the production code builds or recognizes, in the
    /// observed dispatcher.
    pub const PRODUCTION_SELECTORS: u16 = 21;
    pub const PRODUCTION_SELECTORS_PRESENT: u16 = 22;
    /// Addresses embedded as 32-byte immutables only one implementation has.
    pub const IMMUTABLES_ONLY_SOURCE: u16 = 23;
    pub const IMMUTABLES_ONLY_OBSERVED: u16 = 24;
    pub const FROM_BLOCK: u16 = 25;
    pub const TO_BLOCK: u16 = 26;
    pub const BLOCKS_READ: u16 = 27;
    /// Every case's parent block answered `version()` = the observed
    /// version without an override.
    pub const CHAIN_VERSION_CHECKED: u16 = 28;
    pub const CASES: u16 = 29;
    pub const GAS: u16 = 30;
    pub const LAUNCHES: u16 = 31;
    pub const BUYS: u16 = 32;
    pub const SELLS: u16 = 33;
    pub const REVERTED: u16 = 34;
    pub const OTHER: u16 = 35;
    /// Executed cases a verdict needs per traded path (with
    /// `MIN_LAUNCH_BUYS`; `sample::EXECUTED` counts them).
    pub const MIN_LAUNCHES: u16 = 36;
    pub const MIN_BUYS: u16 = 37;
    pub const MIN_SELLS: u16 = 38;
    pub const IDENTICAL: u16 = 39;
    pub const IDENTICAL_BUT_CREATED: u16 = 40;
    pub const DIFFERENT: u16 = 41;
    /// `BEHAVIOUR_PARITY`, `DIFF` or `INSUFFICIENT`.
    pub const VERDICT: u16 = 42;
    pub const DETAIL: u16 = 43;
    pub const NON_CLAIMS: u16 = 44;
    pub const LAUNCH_BUYS: u16 = 45;
    pub const MIN_LAUNCH_BUYS: u16 = 46;
    pub const SAMPLES: u16 = 47;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g01.flap-impl-parity.v1",
        record_type: 70,
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
            Field::req(ENDPOINT_HOST, "endpoint_host", Text),
            Field::req(ARCHIVE_HOST, "archive_host", Text),
            Field::req(TOOL_SHA256, "tool_sha256", B256),
            Field::opt(GIT_HEAD, "git_head", Text),
            Field::req(SOURCE_IMPLEMENTATION, "source_implementation", Address),
            Field::req(SOURCE_VERSION, "source_version", Text),
            Field::req(SOURCE_RUNTIME_SHA256, "source_runtime_sha256", B256),
            Field::req(SOURCE_CODE_LEN, "source_code_len", U64),
            Field::req(OBSERVED_IMPLEMENTATION, "observed_implementation", Address),
            Field::req(OBSERVED_VERSION, "observed_version", Text),
            Field::req(OBSERVED_RUNTIME_SHA256, "observed_runtime_sha256", B256),
            Field::req(OBSERVED_CODE_LEN, "observed_code_len", U64),
            Field::req(CODE_BLOCK, "code_block", U64),
            Field::req(CODE_BLOCK_HASH, "code_block_hash", B256),
            Field::rep(SELECTORS_ONLY_SOURCE, "selectors_only_source", Bytes),
            Field::rep(SELECTORS_ONLY_OBSERVED, "selectors_only_observed", Bytes),
            Field::rep(PRODUCTION_SELECTORS, "production_selectors", Bytes),
            Field::req(
                PRODUCTION_SELECTORS_PRESENT,
                "production_selectors_present",
                Bool,
            ),
            Field::rep(IMMUTABLES_ONLY_SOURCE, "immutables_only_source", Address),
            Field::rep(
                IMMUTABLES_ONLY_OBSERVED,
                "immutables_only_observed",
                Address,
            ),
            Field::req(FROM_BLOCK, "from_block", U64),
            Field::req(TO_BLOCK, "to_block", U64),
            Field::req(BLOCKS_READ, "blocks_read", U64),
            Field::req(CHAIN_VERSION_CHECKED, "chain_version_checked", Bool),
            Field::nested(CASES, "cases", Presence::Repeated, &case::SCHEMA),
            Field::nested(GAS, "gas", Presence::Repeated, &gas::SCHEMA),
            Field::req(LAUNCHES, "launches", U64),
            Field::req(BUYS, "buys", U64),
            Field::req(SELLS, "sells", U64),
            Field::req(REVERTED, "reverted", U64),
            Field::req(OTHER, "other", U64),
            Field::req(MIN_LAUNCHES, "min_launches", U64),
            Field::req(MIN_BUYS, "min_buys", U64),
            Field::req(MIN_SELLS, "min_sells", U64),
            Field::req(IDENTICAL, "identical", U64),
            Field::req(IDENTICAL_BUT_CREATED, "identical_but_created", U64),
            Field::req(DIFFERENT, "different", U64),
            Field::req(VERDICT, "verdict", Text),
            Field::req(DETAIL, "detail", Text),
            Field::rep(NON_CLAIMS, "non_claims", Text),
            Field::req(LAUNCH_BUYS, "launch_buys", U64),
            Field::req(MIN_LAUNCH_BUYS, "min_launch_buys", U64),
            Field::nested(SAMPLES, "samples", Presence::Repeated, &sample::SCHEMA),
        ],
    };
}

pub const ALL: &[&Schema] = &[&flap_impl_parity::SCHEMA];

#[cfg(test)]
mod tests {
    use super::flap_impl_parity as p;
    use crate::{Draft, Value, decode};

    #[test]
    fn a_parity_record_round_trips() {
        let mut renamed = Draft::new(&p::renamed::SCHEMA);
        renamed
            .set(p::renamed::SOURCE, Value::Address([1; 20]))
            .set(p::renamed::OBSERVED, Value::Address([2; 20]))
            .set(p::renamed::PROOF, "CREATED_BY_THE_CALL");
        let mut case = Draft::new(&p::case::SCHEMA);
        case.set(p::case::BLOCK_NUMBER, 2u64)
            .set(p::case::BLOCK_HASH, Value::B256([2; 32]))
            .set(p::case::PARENT_HASH, Value::B256([1; 32]))
            .set(p::case::TX_INDEX, 0u64)
            .set(p::case::TX_HASH, Value::B256([3; 32]))
            .set(p::case::FROM, Value::Address([4; 20]))
            .set(p::case::VALUE_WEI, Value::u128(1))
            .set(
                p::case::SELECTOR,
                Value::Bytes(vec![0x8c, 0xb5, 0x77, 0x2c]),
            )
            .set(p::case::CLASS, "LAUNCH")
            .set(p::case::RECEIPT_STATUS, 1u64)
            .set(p::case::SOURCE_STATUS, 1u64)
            .set(p::case::SOURCE_RETURN_KECCAK, Value::B256([5; 32]))
            .set(p::case::SOURCE_LOGS, 28u64)
            .set(p::case::SOURCE_LOGS_KECCAK, Value::B256([6; 32]))
            .set(p::case::SOURCE_GAS, 5_587_631u64)
            .set(p::case::OBSERVED_STATUS, 1u64)
            .set(p::case::OBSERVED_RETURN_KECCAK, Value::B256([5; 32]))
            .set(p::case::OBSERVED_LOGS, 28u64)
            .set(p::case::OBSERVED_LOGS_KECCAK, Value::B256([7; 32]))
            .set(p::case::OBSERVED_GAS, 5_587_609u64)
            .set(p::case::GAS_DELTA, Value::I64(-22))
            .push(p::case::RENAMED, renamed.into_nested())
            .set(p::case::OUTCOME, "IDENTICAL_BUT_CREATED")
            .set(p::case::DETAIL, "test")
            .set(p::case::CALLS, 2u64)
            .set(p::case::SOURCE_ALL_KECCAK, Value::B256([11; 32]))
            .set(p::case::OBSERVED_ALL_KECCAK, Value::B256([12; 32]))
            .set(p::case::TOKEN, Value::Address([13; 20]));
        let mut sample = Draft::new(&p::sample::SCHEMA);
        sample
            .set(p::sample::CLASS, "LAUNCH")
            .set(p::sample::SEEN, 3u64)
            .set(p::sample::SAMPLED, 1u64)
            .set(p::sample::EXECUTED, 1u64);
        let mut gas = Draft::new(&p::gas::SCHEMA);
        gas.set(p::gas::CLASS, "LAUNCH")
            .set(p::gas::CASES, 1u64)
            .set(p::gas::EQUAL, 0u64)
            .set(p::gas::MIN_DELTA, Value::I64(-22))
            .set(p::gas::MAX_DELTA, Value::I64(-22))
            .set(p::gas::MAX_OBSERVED_GAS, 5_587_609u64);
        let mut d = Draft::new(&p::SCHEMA);
        d.set(p::CHAIN_ID, 56u64)
            .set(p::PORTAL, Value::Address([0xe2; 20]))
            .set(p::PROVENANCE_TOOL, "g01_flap_impl_parity")
            .set(p::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(p::ENDPOINT_HOST, "bsc-dataseed1.defibit.io")
            .set(p::ARCHIVE_HOST, "bsc-mainnet.public.blastapi.io")
            .set(p::TOOL_SHA256, Value::B256([8; 32]))
            .set(p::SOURCE_IMPLEMENTATION, Value::Address([0x6a; 20]))
            .set(p::SOURCE_VERSION, "v5.27.0")
            .set(p::SOURCE_RUNTIME_SHA256, Value::B256([9; 32]))
            .set(p::SOURCE_CODE_LEN, 20_643u64)
            .set(p::OBSERVED_IMPLEMENTATION, Value::Address([0x6f; 20]))
            .set(p::OBSERVED_VERSION, "v5.27.2")
            .set(p::OBSERVED_RUNTIME_SHA256, Value::B256([10; 32]))
            .set(p::OBSERVED_CODE_LEN, 20_643u64)
            .set(p::CODE_BLOCK, 3u64)
            .set(p::CODE_BLOCK_HASH, Value::B256([3; 32]))
            .push(
                p::SELECTORS_ONLY_SOURCE,
                Value::Bytes(vec![0x66, 0xfd, 0xb9, 0x7b]),
            )
            .push(
                p::PRODUCTION_SELECTORS,
                Value::Bytes(vec![0xef, 0x7e, 0xc2, 0xe7]),
            )
            .set(p::PRODUCTION_SELECTORS_PRESENT, true)
            .push(p::IMMUTABLES_ONLY_OBSERVED, Value::Address([0x02; 20]))
            .set(p::FROM_BLOCK, 1u64)
            .set(p::TO_BLOCK, 2u64)
            .set(p::BLOCKS_READ, 2u64)
            .set(p::CHAIN_VERSION_CHECKED, true)
            .push(p::CASES, case.into_nested())
            .push(p::GAS, gas.into_nested())
            .set(p::LAUNCHES, 1u64)
            .set(p::BUYS, 0u64)
            .set(p::SELLS, 0u64)
            .set(p::REVERTED, 0u64)
            .set(p::OTHER, 0u64)
            .set(p::MIN_LAUNCHES, 1u64)
            .set(p::MIN_BUYS, 1u64)
            .set(p::MIN_SELLS, 1u64)
            .set(p::IDENTICAL, 0u64)
            .set(p::IDENTICAL_BUT_CREATED, 1u64)
            .set(p::DIFFERENT, 0u64)
            .set(p::VERDICT, "INSUFFICIENT")
            .set(p::DETAIL, "test")
            .push(p::NON_CLAIMS, "READ_ONLY")
            .set(p::LAUNCH_BUYS, 0u64)
            .set(p::MIN_LAUNCH_BUYS, 1u64)
            .push(p::SAMPLES, sample.into_nested());
        let bytes = d.encode().unwrap();
        let r = decode(&p::SCHEMA, &bytes).unwrap();
        assert_eq!(r.nested_all(p::CASES).unwrap().len(), 1);
        assert_eq!(r.text(p::VERDICT).unwrap(), "INSUFFICIENT");
        assert!(crate::schema::decode_any(&bytes).is_ok());
    }
}
