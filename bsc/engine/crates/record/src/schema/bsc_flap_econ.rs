//! The G0.6a native economics reconfirmed read-only on the current Flap
//! Portal (Phase D gate review HIGH-1): the witnessed native launch and the
//! native BUY executed on state pinned to one block, by the BSC node
//! (`eth_simulateV1`) and by an independent local EVM (EthereumJS, the G0.6a
//! method), compared with the production quote (`quote_native_buy`) and the
//! production derivation. Written by the read-only `g01_flap_econ`
//! qualification binary; nothing is signed or sent.

use super::bsc_four_freeze::four_freeze as shared;
use crate::{Field, Kind::*, Presence, Schema};

pub mod flap_econ {
    use super::*;

    /// One BUY amount: the three executions and the production quote.
    pub mod case {
        use super::*;

        pub const BUY_VALUE_WEI: u16 = 1;
        /// Why this amount: the admission's qualified input, or the amount of
        /// the committed G0.6a record.
        pub const PURPOSE: u16 = 2;
        /// The production `encode_swap_exact_input_native` bytes (minimum
        /// output 1, as G0.6a: the EVM decides the output).
        pub const BUY_CALLDATA: u16 = 3;
        pub const RUST_EXPECTED_OUTPUT: u16 = 4;
        pub const RUST_PROTOCOL_FEE: u16 = 5;
        pub const RUST_BONDING_CURVE_TAX: u16 = 6;
        pub const RUST_COMBINED_FEE: u16 = 7;
        pub const RUST_NET_RESERVE_INPUT: u16 = 8;
        /// Node execution (`eth_simulateV1` at the pinned block).
        pub const NODE_LAUNCH_STATUS: u16 = 9;
        pub const NODE_TOKEN: u16 = 10;
        pub const NODE_LAUNCH_GAS: u16 = 11;
        pub const NODE_BUY_STATUS: u16 = 12;
        pub const NODE_BUY_RETURN: u16 = 13;
        pub const NODE_BUYER_BALANCE: u16 = 14;
        /// `TokenBought(ts, token, buyer, amount, eth, fee, postPrice)`.
        pub const NODE_BOUGHT_AMOUNT: u16 = 15;
        pub const NODE_BOUGHT_ETH: u16 = 16;
        pub const NODE_BOUGHT_FEE: u16 = 17;
        pub const NODE_BOUGHT_POST_PRICE: u16 = 18;
        /// `TaxV2OnBondingCurvePaid` / `TaxOnBondingCurvePaid` amount.
        pub const NODE_TAX_PAID: u16 = 19;
        pub const NODE_BUY_GAS: u16 = 20;
        pub const NODE_ERROR: u16 = 21;
        /// Local EVM execution (EthereumJS over pinned RPC state).
        pub const LOCAL_STATUS: u16 = 22;
        pub const LOCAL_TOKEN: u16 = 23;
        pub const LOCAL_LAUNCH_GAS: u16 = 24;
        pub const LOCAL_BUY_RETURN: u16 = 25;
        pub const LOCAL_BUYER_BALANCE: u16 = 26;
        pub const LOCAL_BOUGHT_AMOUNT: u16 = 27;
        pub const LOCAL_BOUGHT_ETH: u16 = 28;
        pub const LOCAL_BOUGHT_FEE: u16 = 29;
        pub const LOCAL_TAX_PAID: u16 = 30;
        pub const LOCAL_BUY_GAS: u16 = 31;
        /// Keccak-256 of the simulator's own (ethers) encodings.
        pub const LOCAL_LAUNCH_CALLDATA_KECCAK: u16 = 32;
        pub const LOCAL_BUY_CALLDATA_KECCAK: u16 = 33;
        /// The committed G0.6a output for this amount, if it has one.
        pub const COMMITTED_OUTPUT: u16 = 34;
        pub const CHECKS: u16 = 35;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.g01.flap-econ.case",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(BUY_VALUE_WEI, "buy_value_wei", U256),
                Field::req(PURPOSE, "purpose", Text),
                Field::req(BUY_CALLDATA, "buy_calldata", Bytes),
                Field::opt(RUST_EXPECTED_OUTPUT, "rust_expected_output", U256),
                Field::opt(RUST_PROTOCOL_FEE, "rust_protocol_fee", U256),
                Field::opt(RUST_BONDING_CURVE_TAX, "rust_bonding_curve_tax", U256),
                Field::opt(RUST_COMBINED_FEE, "rust_combined_fee", U256),
                Field::opt(RUST_NET_RESERVE_INPUT, "rust_net_reserve_input", U256),
                Field::opt(NODE_LAUNCH_STATUS, "node_launch_status", U64),
                Field::opt(NODE_TOKEN, "node_token", Address),
                Field::opt(NODE_LAUNCH_GAS, "node_launch_gas", U64),
                Field::opt(NODE_BUY_STATUS, "node_buy_status", U64),
                Field::opt(NODE_BUY_RETURN, "node_buy_return", U256),
                Field::opt(NODE_BUYER_BALANCE, "node_buyer_balance", U256),
                Field::opt(NODE_BOUGHT_AMOUNT, "node_bought_amount", U256),
                Field::opt(NODE_BOUGHT_ETH, "node_bought_eth", U256),
                Field::opt(NODE_BOUGHT_FEE, "node_bought_fee", U256),
                Field::opt(NODE_BOUGHT_POST_PRICE, "node_bought_post_price", U256),
                Field::opt(NODE_TAX_PAID, "node_tax_paid", U256),
                Field::opt(NODE_BUY_GAS, "node_buy_gas", U64),
                Field::opt(NODE_ERROR, "node_error", Text),
                Field::req(LOCAL_STATUS, "local_status", Text),
                Field::opt(LOCAL_TOKEN, "local_token", Address),
                Field::opt(LOCAL_LAUNCH_GAS, "local_launch_gas", U64),
                Field::opt(LOCAL_BUY_RETURN, "local_buy_return", U256),
                Field::opt(LOCAL_BUYER_BALANCE, "local_buyer_balance", U256),
                Field::opt(LOCAL_BOUGHT_AMOUNT, "local_bought_amount", U256),
                Field::opt(LOCAL_BOUGHT_ETH, "local_bought_eth", U256),
                Field::opt(LOCAL_BOUGHT_FEE, "local_bought_fee", U256),
                Field::opt(LOCAL_TAX_PAID, "local_tax_paid", U256),
                Field::opt(LOCAL_BUY_GAS, "local_buy_gas", U64),
                Field::opt(
                    LOCAL_LAUNCH_CALLDATA_KECCAK,
                    "local_launch_calldata_keccak",
                    B256,
                ),
                Field::opt(LOCAL_BUY_CALLDATA_KECCAK, "local_buy_calldata_keccak", B256),
                Field::opt(COMMITTED_OUTPUT, "committed_output", U256),
                Field::nested(CHECKS, "checks", Presence::Repeated, &shared::check::SCHEMA),
            ],
        };
    }

    pub const CHAIN_ID: u16 = 1;
    pub const PORTAL: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    pub const REFERENCE_BLOCK: u16 = 5;
    pub const REFERENCE_BLOCK_HASH: u16 = 6;
    /// The reference block still had the pinned hash after both executions.
    pub const PIN_RECHECKED: u16 = 7;
    /// Hosts only: never a path or query.
    pub const ENDPOINT_HOST: u16 = 8;
    pub const LOCAL_EVM_ENDPOINT_HOST: u16 = 9;
    pub const TOOL_SHA256: u16 = 10;
    pub const GIT_HEAD: u16 = 11;
    /// The local EVM simulator script and its SHA-256.
    pub const SIMULATOR_PATH: u16 = 12;
    pub const SIMULATOR_SHA256: u16 = 13;
    pub const PORTAL_VERSION: u16 = 14;
    pub const IMPLEMENTATION: u16 = 15;
    pub const IMPLEMENTATION_RUNTIME_SHA256: u16 = 16;
    /// `bsc_mainnet_v5_27_0().validate(observed)`: `VALID` or the error.
    pub const DEPLOYMENT_VALIDATION: u16 = 17;
    /// The committed G0.6a record the launch inputs come from.
    pub const G06A_RECORD_PATH: u16 = 18;
    pub const G06A_RECORD_SHA256: u16 = 19;
    pub const G06A_RECORD_BLOCK: u16 = 20;
    pub const SALT: u16 = 21;
    pub const LAUNCHER: u16 = 22;
    pub const BUYER: u16 = 23;
    /// `predicted_token` of the production code.
    pub const PREDICTED_TOKEN: u16 = 24;
    pub const COMMITTED_TOKEN: u16 = 25;
    /// The production `encode_new_token_v6` bytes and their Keccak-256.
    pub const LAUNCH_CALLDATA: u16 = 26;
    pub const LAUNCH_CALLDATA_KECCAK: u16 = 27;
    /// The production `recognize` of those bytes.
    pub const RECOGNIZED: u16 = 28;
    /// The production `classify_launch` on the validated v5.27.0 deployment.
    pub const CLASSIFIED: u16 = 29;
    pub const NODE_LAUNCH_GAS_ESTIMATE: u16 = 30;
    pub const LAUNCH_GAS_LIMIT: u16 = 31;
    pub const CASES: u16 = 32;
    /// `RECONFIRMED`, `MISMATCH` or `NOT_RECONFIRMED`.
    pub const STATUS: u16 = 33;
    pub const DETAIL: u16 = 34;
    pub const NON_CLAIMS: u16 = 35;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g01.flap-econ.v1",
        record_type: 53,
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
            Field::req(PIN_RECHECKED, "pin_rechecked", Bool),
            Field::req(ENDPOINT_HOST, "endpoint_host", Text),
            Field::req(LOCAL_EVM_ENDPOINT_HOST, "local_evm_endpoint_host", Text),
            Field::req(TOOL_SHA256, "tool_sha256", B256),
            Field::opt(GIT_HEAD, "git_head", Text),
            Field::req(SIMULATOR_PATH, "simulator_path", Text),
            Field::req(SIMULATOR_SHA256, "simulator_sha256", B256),
            Field::req(PORTAL_VERSION, "portal_version", Text),
            Field::req(IMPLEMENTATION, "implementation", Address),
            Field::req(
                IMPLEMENTATION_RUNTIME_SHA256,
                "implementation_runtime_sha256",
                B256,
            ),
            Field::req(DEPLOYMENT_VALIDATION, "deployment_validation", Text),
            Field::req(G06A_RECORD_PATH, "g06a_record_path", Text),
            Field::req(G06A_RECORD_SHA256, "g06a_record_sha256", B256),
            Field::req(G06A_RECORD_BLOCK, "g06a_record_block", U64),
            Field::req(SALT, "salt", B256),
            Field::req(LAUNCHER, "launcher", Address),
            Field::req(BUYER, "buyer", Address),
            Field::opt(PREDICTED_TOKEN, "predicted_token", Address),
            Field::req(COMMITTED_TOKEN, "committed_token", Address),
            Field::req(LAUNCH_CALLDATA, "launch_calldata", Bytes),
            Field::req(LAUNCH_CALLDATA_KECCAK, "launch_calldata_keccak", B256),
            Field::req(RECOGNIZED, "recognized", Text),
            Field::req(CLASSIFIED, "classified", Text),
            Field::opt(NODE_LAUNCH_GAS_ESTIMATE, "node_launch_gas_estimate", U64),
            Field::req(LAUNCH_GAS_LIMIT, "launch_gas_limit", U64),
            Field::nested(CASES, "cases", Presence::Repeated, &case::SCHEMA),
            Field::req(STATUS, "status", Text),
            Field::req(DETAIL, "detail", Text),
            Field::rep(NON_CLAIMS, "non_claims", Text),
        ],
    };
}

pub const ALL: &[&Schema] = &[&flap_econ::SCHEMA];

#[cfg(test)]
mod tests {
    use super::flap_econ as f;
    use crate::{Draft, Value, decode};

    #[test]
    fn an_econ_record_round_trips() {
        let mut case = Draft::new(&f::case::SCHEMA);
        case.set(f::case::BUY_VALUE_WEI, Value::u128(1_000_000_000_000_000))
            .set(f::case::PURPOSE, "QUALIFIED_MAINNET_INPUT_WEI")
            .set(
                f::case::BUY_CALLDATA,
                Value::Bytes(vec![0xef, 0x7e, 0xc2, 0xe7]),
            )
            .set(f::case::LOCAL_STATUS, "NOT_RUN");
        let mut d = Draft::new(&f::SCHEMA);
        d.set(f::CHAIN_ID, 56u64)
            .set(f::PORTAL, Value::Address([0xe2; 20]))
            .set(f::PROVENANCE_TOOL, "g01_flap_econ")
            .set(f::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(f::REFERENCE_BLOCK, 3u64)
            .set(f::REFERENCE_BLOCK_HASH, Value::B256([3; 32]))
            .set(f::PIN_RECHECKED, true)
            .set(f::ENDPOINT_HOST, "bsc-dataseed.bnbchain.org")
            .set(f::LOCAL_EVM_ENDPOINT_HOST, "bsc-dataseed.bnbchain.org")
            .set(f::TOOL_SHA256, Value::B256([1; 32]))
            .set(f::SIMULATOR_PATH, "bsc/tools/g01_flap_econ_evm.cjs")
            .set(f::SIMULATOR_SHA256, Value::B256([2; 32]))
            .set(f::PORTAL_VERSION, "v5.27.0")
            .set(f::IMPLEMENTATION, Value::Address([0x6a; 20]))
            .set(f::IMPLEMENTATION_RUNTIME_SHA256, Value::B256([4; 32]))
            .set(f::DEPLOYMENT_VALIDATION, "VALID")
            .set(f::G06A_RECORD_PATH, "bsc/native/evidence/g06a/x.dbrec")
            .set(f::G06A_RECORD_SHA256, Value::B256([5; 32]))
            .set(f::G06A_RECORD_BLOCK, 1u64)
            .set(f::SALT, Value::B256([6; 32]))
            .set(f::LAUNCHER, Value::Address([0xa1; 20]))
            .set(f::BUYER, Value::Address([0xb0; 20]))
            .set(f::COMMITTED_TOKEN, Value::Address([0x77; 20]))
            .set(
                f::LAUNCH_CALLDATA,
                Value::Bytes(vec![0x8c, 0xb5, 0x77, 0x2c]),
            )
            .set(f::LAUNCH_CALLDATA_KECCAK, Value::B256([7; 32]))
            .set(f::RECOGNIZED, "LaunchV6")
            .set(f::CLASSIFIED, "Err(EconomicsNotWitnessed)")
            .set(f::LAUNCH_GAS_LIMIT, 6_000_000u64)
            .push(f::CASES, case.into_nested())
            .set(f::STATUS, "NOT_RECONFIRMED")
            .set(f::DETAIL, "test")
            .push(f::NON_CLAIMS, "READ_ONLY");
        let bytes = d.encode().unwrap();
        let r = decode(&f::SCHEMA, &bytes).unwrap();
        assert_eq!(r.nested_all(f::CASES).unwrap().len(), 1);
        assert!(crate::schema::decode_any(&bytes).is_ok());
    }
}
