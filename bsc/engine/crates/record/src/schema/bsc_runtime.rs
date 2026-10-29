//! The native configuration of the BSC runtime (C0/C1, plan §5a.2, order V2
//! §5): the five signal sources, the runtime components and the three venues,
//! each with the mode the user requested (`OFF | SHADOW | LIVE`) and the mode
//! the native owner made effective (`OFF | SHADOW | BLOCKED`, with its
//! reason), and the wallet and economic switches (plan 27.10.,
//! `DISABLED | ENABLED`). One record per generation; every request leaves
//! one receipt.

use crate::{Field, Kind::*, Presence, Schema};

/// One generation of the source modes.
pub mod source_modes {
    use super::*;

    /// One source of a generation.
    pub mod entry {
        use super::*;

        /// `BSC_INGRESS_GETH`, `_PUBLICNODE`, `_DRPC`, `_BLOCKREQ`, `_48CLUB`.
        pub const SOURCE_ID: u16 = 1;
        /// What the user requested: `OFF`, `SHADOW` or `LIVE`.
        pub const REQUESTED: u16 = 2;
        /// What the native owner made of it: `OFF`, `SHADOW` or `BLOCKED`.
        pub const EFFECTIVE: u16 = 3;
        /// Why a request is not effective as requested.
        pub const BLOCK_REASON: u16 = 4;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.source-mode",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(SOURCE_ID, "source_id", Text),
                Field::req(REQUESTED, "requested", Text),
                Field::req(EFFECTIVE, "effective", Text),
                Field::opt(BLOCK_REASON, "block_reason", Text),
            ],
        };
    }

    /// One switchable runtime component of a generation (C2 state, ...):
    /// requested and effective mode like a source.
    pub mod component {
        use super::*;

        /// `BSC_STATE_C2`, ...
        pub const COMPONENT_ID: u16 = 1;
        pub const REQUESTED: u16 = 2;
        pub const EFFECTIVE: u16 = 3;
        pub const BLOCK_REASON: u16 = 4;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.component-mode",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(COMPONENT_ID, "component_id", Text),
                Field::req(REQUESTED, "requested", Text),
                Field::req(EFFECTIVE, "effective", Text),
                Field::opt(BLOCK_REASON, "block_reason", Text),
            ],
        };
    }

    /// One venue of a generation (plan v9 §2.3): `FLAP`, `FOUR_CLASSIC`,
    /// `OPENFOUR`, each `OFF | SHADOW | LIVE` like a source.
    pub mod venue {
        use super::*;

        /// `BSC_VENUE_FLAP`, `BSC_VENUE_FOUR_CLASSIC`, `BSC_VENUE_OPENFOUR`.
        pub const VENUE_ID: u16 = 1;
        pub const REQUESTED: u16 = 2;
        pub const EFFECTIVE: u16 = 3;
        pub const BLOCK_REASON: u16 = 4;
        /// The request that last set this venue (none while never requested).
        pub const REQUEST_ID: u16 = 5;
        /// The generation that request wrote.
        pub const SET_GENERATION: u16 = 6;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.venue-mode",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(VENUE_ID, "venue_id", Text),
                Field::req(REQUESTED, "requested", Text),
                Field::req(EFFECTIVE, "effective", Text),
                Field::opt(BLOCK_REASON, "block_reason", Text),
                Field::opt(REQUEST_ID, "request_id", Text),
                Field::opt(SET_GENERATION, "set_generation", U64),
            ],
        };
    }

    /// One wallet or economic switch of a generation (plan 27.10.):
    /// `DISABLED | ENABLED`, DISABLED initially and after every restart.
    pub mod switch {
        use super::*;

        /// `BSC_WALLET_LAUNCHER`, `BSC_WALLET_BUYER`, `BSC_ECON_SIGN`,
        /// `BSC_ECON_ARM`, `BSC_ECON_SEND`, `BSC_ECON_ATTEMPT`, `BSC_ECON_LIVE`.
        pub const SWITCH_ID: u16 = 1;
        pub const REQUESTED: u16 = 2;
        pub const EFFECTIVE: u16 = 3;
        /// Why a requested ENABLED is not effective (a restart reset).
        pub const BLOCK_REASON: u16 = 4;
        /// The request (or restart reset) that last set this switch.
        pub const REQUEST_ID: u16 = 5;
        /// The generation that request wrote.
        pub const SET_GENERATION: u16 = 6;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.control-switch",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(SWITCH_ID, "switch_id", Text),
                Field::req(REQUESTED, "requested", Text),
                Field::req(EFFECTIVE, "effective", Text),
                Field::opt(BLOCK_REASON, "block_reason", Text),
                Field::opt(REQUEST_ID, "request_id", Text),
                Field::opt(SET_GENERATION, "set_generation", U64),
            ],
        };
    }

    /// The runtime mode `BSC_RUNTIME_MODE` (`OFF | SHADOW | PAPER | LIVE`)
    /// and the daily BUY attempt limit `BSC_BUY_ATTEMPTS_PER_DAY` (decimal):
    /// requested, effective, and the request that set them.
    pub mod setting {
        use super::*;

        pub const SETTING_ID: u16 = 1;
        pub const REQUESTED: u16 = 2;
        pub const EFFECTIVE: u16 = 3;
        pub const BLOCK_REASON: u16 = 4;
        pub const REQUEST_ID: u16 = 5;
        pub const SET_GENERATION: u16 = 6;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.owner-setting",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(SETTING_ID, "setting_id", Text),
                Field::req(REQUESTED, "requested", Text),
                Field::req(EFFECTIVE, "effective", Text),
                Field::opt(BLOCK_REASON, "block_reason", Text),
                Field::opt(REQUEST_ID, "request_id", Text),
                Field::opt(SET_GENERATION, "set_generation", U64),
            ],
        };
    }

    /// The operator's LIVE grant (plan 28.10., `0a647175`, schema version
    /// 5): the one economic approval of a LIVE scope, or its revocation.
    /// It counts only in the boot session of the generation that holds it
    /// and until it expires; a restart revokes it.
    pub mod live {
        use super::*;

        /// `GRANTED` or `REVOKED`.
        pub const STATE: u16 = 1;
        /// The venue the grant may buy on (`BSC_VENUE_FLAP`).
        pub const VENUE: u16 = 2;
        /// `C3`, `STRATEGY_C` or `C3+STRATEGY_C`.
        pub const STRATEGIES: u16 = 3;
        /// The sources whose verified full launches may act (repeated).
        pub const SOURCES: u16 = 4;
        /// The inclusion adapters it may send through (repeated).
        pub const ADAPTERS: u16 = 5;
        /// The largest input of one BUY.
        pub const MAX_INPUT_WEI: u16 = 6;
        /// The most the grant may spend in all (inputs, gas, fees).
        pub const MAX_SPEND_WEI: u16 = 7;
        /// The most BUY attempts under this grant.
        pub const MAX_ATTEMPTS: u16 = 8;
        pub const EXPIRES_UNIX_MS: u16 = 9;
        /// SHA-256 of the canonical scope text the operator confirmed.
        pub const SCOPE_SHA256: u16 = 10;
        pub const REQUEST_ID: u16 = 11;
        pub const SET_GENERATION: u16 = 12;
        /// The grant admits the exits of open positions only, no BUY
        /// (BSC3-058); absent: BUYs and exits.
        pub const EXIT_ONLY: u16 = 13;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.source-modes.live",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(STATE, "state", Text),
                Field::opt(VENUE, "venue", Text),
                Field::opt(STRATEGIES, "strategies", Text),
                Field::rep(SOURCES, "sources", Text),
                Field::rep(ADAPTERS, "adapters", Text),
                Field::opt(MAX_INPUT_WEI, "max_input_wei", U256),
                Field::opt(MAX_SPEND_WEI, "max_spend_wei", U256),
                Field::opt(MAX_ATTEMPTS, "max_attempts", U64),
                Field::opt(EXPIRES_UNIX_MS, "expires_unix_ms", U64),
                Field::opt(SCOPE_SHA256, "scope_sha256", B256),
                Field::opt(REQUEST_ID, "request_id", Text),
                Field::opt(SET_GENERATION, "set_generation", U64),
                Field::opt(EXIT_ONLY, "exit_only", Bool),
            ],
        };
    }

    /// Strictly increasing; the CAS every change states.
    pub const GENERATION: u16 = 1;
    pub const SOURCES: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    /// The request this generation applied (none for the initial one).
    pub const REQUEST_ID: u16 = 5;
    /// The switchable components; a generation written before a component
    /// existed has it OFF (the initial state, nothing ever requested).
    pub const COMPONENTS: u16 = 6;
    /// The venues; a generation written before them has every venue OFF.
    pub const VENUES: u16 = 7;
    /// The wallet and economic switches; a generation written before them
    /// has every switch DISABLED.
    pub const SWITCHES: u16 = 8;
    /// The host boot session this generation was written in: an ENABLED
    /// switch counts only within it (a restart disables every switch).
    /// Bytes, so the host identity never leaves the reporter.
    pub const BOOT_SESSION: u16 = 9;
    /// The runtime mode and the daily BUY limit (schema version 4), the
    /// campaign's total BUY limit (version 5).
    pub const SETTINGS: u16 = 10;
    /// The LIVE grant or its revocation (schema version 5).
    pub const LIVE: u16 = 11;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.source-modes.v1",
        record_type: 38,
        schema_version: 5,
        fields: &[
            Field::req(GENERATION, "generation", U64),
            Field::nested(SOURCES, "sources", Presence::Repeated, &entry::SCHEMA),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::opt(REQUEST_ID, "request_id", Text),
            Field::nested(
                COMPONENTS,
                "components",
                Presence::Repeated,
                &component::SCHEMA,
            ),
            Field::nested(VENUES, "venues", Presence::Repeated, &venue::SCHEMA),
            Field::nested(SWITCHES, "switches", Presence::Repeated, &switch::SCHEMA),
            Field::opt(BOOT_SESSION, "boot_session", Bytes).max(128),
            Field::nested(SETTINGS, "settings", Presence::Repeated, &setting::SCHEMA),
            Field::nested(LIVE, "live", Presence::Optional, &live::SCHEMA),
        ],
    };

    /// Generations written before the LIVE grant and the total BUY limit
    /// (read only; no grant, the total limit 0).
    pub const SCHEMA_V4: Schema = Schema {
        name: "darkbot.bsc.source-modes.v1",
        record_type: 38,
        schema_version: 4,
        fields: &[
            Field::req(GENERATION, "generation", U64),
            Field::nested(SOURCES, "sources", Presence::Repeated, &entry::SCHEMA),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::opt(REQUEST_ID, "request_id", Text),
            Field::nested(
                COMPONENTS,
                "components",
                Presence::Repeated,
                &component::SCHEMA,
            ),
            Field::nested(VENUES, "venues", Presence::Repeated, &venue::SCHEMA),
            Field::nested(SWITCHES, "switches", Presence::Repeated, &switch::SCHEMA),
            Field::opt(BOOT_SESSION, "boot_session", Bytes).max(128),
            Field::nested(SETTINGS, "settings", Presence::Repeated, &setting::SCHEMA),
        ],
    };

    /// Generations written before the runtime mode and the daily BUY limit
    /// (read only; the runtime is OFF and the limit 0).
    pub const SCHEMA_V3: Schema = Schema {
        name: "darkbot.bsc.source-modes.v1",
        record_type: 38,
        schema_version: 3,
        fields: &[
            Field::req(GENERATION, "generation", U64),
            Field::nested(SOURCES, "sources", Presence::Repeated, &entry::SCHEMA),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::opt(REQUEST_ID, "request_id", Text),
            Field::nested(
                COMPONENTS,
                "components",
                Presence::Repeated,
                &component::SCHEMA,
            ),
            Field::nested(VENUES, "venues", Presence::Repeated, &venue::SCHEMA),
            Field::nested(SWITCHES, "switches", Presence::Repeated, &switch::SCHEMA),
            Field::opt(BOOT_SESSION, "boot_session", Bytes).max(128),
        ],
    };

    /// Generations written before the venues and switches (read only; their
    /// venues are OFF and their switches DISABLED).
    pub const SCHEMA_V2: Schema = Schema {
        name: "darkbot.bsc.source-modes.v1",
        record_type: 38,
        schema_version: 2,
        fields: &[
            Field::req(GENERATION, "generation", U64),
            Field::nested(SOURCES, "sources", Presence::Repeated, &entry::SCHEMA),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::opt(REQUEST_ID, "request_id", Text),
            Field::nested(
                COMPONENTS,
                "components",
                Presence::Repeated,
                &component::SCHEMA,
            ),
        ],
    };

    /// Generations written before components existed (read only; their
    /// components are OFF).
    pub const SCHEMA_V1: Schema = Schema {
        name: "darkbot.bsc.source-modes.v1",
        record_type: 38,
        schema_version: 1,
        fields: &[
            Field::req(GENERATION, "generation", U64),
            Field::nested(SOURCES, "sources", Presence::Repeated, &entry::SCHEMA),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::opt(REQUEST_ID, "request_id", Text),
        ],
    };
}

/// One consumed slot of the daily BUY attempt limit (plan 27.10.,
/// `8b4721bb`): `buy-attempts/<UTC day>/<slot>.dbrec`, created create-new,
/// so a slot is taken once; the same attempt again finds its slot.
pub mod buy_attempt_slot {
    use super::*;

    pub const ATTEMPT_ID: u16 = 1;
    /// The UTC calendar day, `YYYY-MM-DD` (the contract's day boundary).
    pub const DAY_UTC: u16 = 2;
    /// Days since 1970-01-01 UTC.
    pub const DAY_INDEX: u16 = 5;
    /// 0-based; below the limit in force when taken.
    pub const SLOT: u16 = 6;
    pub const LIMIT: u16 = 7;
    /// The owner generation the limit was read from.
    pub const GENERATION: u16 = 8;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.buy-attempt-slot.v1",
        record_type: 64,
        schema_version: 1,
        fields: &[
            Field::req(ATTEMPT_ID, "attempt_id", Text),
            Field::req(DAY_UTC, "day_utc", Text),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(DAY_INDEX, "day_index", U64),
            Field::req(SLOT, "slot", U64),
            Field::req(LIMIT, "limit", U64),
            Field::req(GENERATION, "generation", U64),
        ],
    };
}

/// One consumed slot of the campaign's total BUY attempt limit (plan
/// 28.10., `0a647175`: at most ten real BUY attempts, G0.7 included):
/// `buy-attempts-total/<slot>.dbrec`, created create-new; never freed.
pub mod buy_attempt_total_slot {
    use super::*;

    pub const ATTEMPT_ID: u16 = 1;
    /// 0-based; below the total limit in force when taken.
    pub const SLOT: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    pub const LIMIT: u16 = 5;
    /// The owner generation the limit was read from.
    pub const GENERATION: u16 = 6;
    /// The generation of the LIVE grant the attempt ran under (0: none,
    /// a G0.7 attempt).
    pub const GRANT_GENERATION: u16 = 7;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.buy-attempt-total-slot.v1",
        record_type: 65,
        schema_version: 1,
        fields: &[
            Field::req(ATTEMPT_ID, "attempt_id", Text),
            Field::req(SLOT, "slot", U64),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(LIMIT, "limit", U64),
            Field::req(GENERATION, "generation", U64),
            Field::req(GRANT_GENERATION, "grant_generation", U64),
        ],
    };
}

/// One LIVE intent the native owner admitted (plan 28.10.): written
/// create-new before any byte of it can leave, its most the grant may
/// lose by it (a BUY's input and gas, an approval's or SELL's gas). The
/// grant's spend is the sum of its entries; an entry is never removed, so a
/// possibly sent intent always counts. Its token and nonce (BSC3-024) make
/// it the write-ahead record of the transaction: an entry the LIVE ledger
/// has no fact of after a crash is recovered there as UNKNOWN.
pub mod live_spend {
    use super::*;

    /// The signed transaction's hash, hex: one entry per transaction.
    pub const INTENT_ID: u16 = 1;
    /// `BUY`, `APPROVE` or `SELL`.
    pub const KIND: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    /// The component it acts for (`BSC_EXEC`, `BSC_STRATEGY_C`).
    pub const COMPONENT: u16 = 5;
    /// The owner generation of the grant it was admitted under.
    pub const GRANT_GENERATION: u16 = 6;
    pub const INPUT_WEI: u16 = 7;
    /// Input and gas at the signed gas price and limit.
    pub const MAX_WEI: u16 = 8;
    /// The token it trades and the sender's nonce it is signed at (absent
    /// in entries written before BSC3-024: such an entry cannot be
    /// recovered and keeps LIVE from arming).
    pub const TOKEN: u16 = 9;
    pub const NONCE: u16 = 10;
    /// The last block its bundle may be included in (BSC3-048): after it,
    /// with no inclusion, the nonce may be taken again (absent in entries
    /// written before: the window is unknown and nothing reuses the nonce).
    pub const MAX_BLOCK: u16 = 11;
    /// A BUY: the most the exits of the open positions and of its own may
    /// still cost, the room it left the grant (BSC3-057); absent in entries
    /// written before.
    pub const EXIT_RESERVE_WEI: u16 = 12;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.live-spend.v1",
        record_type: 67,
        schema_version: 1,
        fields: &[
            Field::req(INTENT_ID, "intent_id", Text),
            Field::req(KIND, "kind", Text),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(COMPONENT, "component", Text),
            Field::req(GRANT_GENERATION, "grant_generation", U64),
            Field::req(INPUT_WEI, "input_wei", U256),
            Field::req(MAX_WEI, "max_wei", U256),
            Field::opt(TOKEN, "token", Address),
            Field::opt(NONCE, "nonce", U64),
            Field::opt(MAX_BLOCK, "max_block", U64),
            Field::opt(EXIT_RESERVE_WEI, "exit_reserve_wei", U256),
        ],
    };
}

/// One fact of the LIVE ledger (plan 28.10.): a LIVE transaction whose
/// bytes may have left, and what the canonical chain made of it. Written
/// create-new in order, never changed: the positions, their costs and the
/// net result are the fold of these facts, after a crash as before.
pub mod live_ledger {
    use super::*;

    /// `RELEASED`, `UNKNOWN` (its handoff did not complete, or it was
    /// admitted and the process ended before its outcome was written),
    /// `WITHDRAWN` (admitted, but no byte of it left: never on chain),
    /// `INCLUDED`, `REVERTED`, `REPLACED` (another transaction took its
    /// nonce) or `ORPHANED` (its block left the canonical chain).
    pub const KIND: u16 = 1;
    pub const TX_HASH: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    /// `BUY`, `APPROVE` or `SELL`.
    pub const INTENT: u16 = 5;
    /// `BSC_EXEC` or `BSC_STRATEGY_C`.
    pub const COMPONENT: u16 = 6;
    pub const TOKEN: u16 = 7;
    pub const NONCE: u16 = 8;
    pub const GRANT_GENERATION: u16 = 9;
    /// The signed native value (a BUY's input).
    pub const INPUT_WEI: u16 = 10;
    /// The signed input and gas limit at the signed price: the most the
    /// transaction can cost.
    pub const MAX_WEI: u16 = 11;
    pub const BLOCK_NUMBER: u16 = 12;
    pub const BLOCK_HASH: u16 = 13;
    pub const BLOCK_MS: u16 = 14;
    /// Gas used times the effective price, from the receipt.
    pub const GAS_WEI: u16 = 15;
    /// A BUY's tokens received, a SELL's tokens sold (the Portal's trade).
    pub const TOKENS: u16 = 16;
    /// A BUY's native amount spent, a SELL's received (the Portal's trade).
    pub const ETH_WEI: u16 = 17;
    /// A released transaction's window: the last block its bundle may be
    /// included in (BSC3-048).
    pub const MAX_BLOCK: u16 = 18;
    /// `REPLACED`: the transaction that took its nonce (its block is the
    /// fact's block).
    pub const BY_TX_HASH: u16 = 19;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.live-ledger.v1",
        record_type: 68,
        schema_version: 1,
        fields: &[
            Field::req(KIND, "kind", Text),
            Field::req(TX_HASH, "tx_hash", B256),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(INTENT, "intent", Text),
            Field::req(COMPONENT, "component", Text),
            Field::req(TOKEN, "token", Address),
            Field::req(NONCE, "nonce", U64),
            Field::req(GRANT_GENERATION, "grant_generation", U64),
            Field::opt(INPUT_WEI, "input_wei", U256),
            Field::opt(MAX_WEI, "max_wei", U256),
            Field::opt(BLOCK_NUMBER, "block_number", U64),
            Field::opt(BLOCK_HASH, "block_hash", B256),
            Field::opt(BLOCK_MS, "block_ms", U64),
            Field::opt(GAS_WEI, "gas_wei", U256),
            Field::opt(TOKENS, "tokens", U256),
            Field::opt(ETH_WEI, "eth_wei", U256),
            Field::opt(MAX_BLOCK, "max_block", U64),
            Field::opt(BY_TX_HASH, "by_tx_hash", B256),
        ],
    };
}

/// The LIVE qualification of a scope (plan 28.10., `0a647175`): what this
/// product qualified about venues, sources and inclusion adapters, its
/// acquisition and its operator's risk decision, each bound by path and
/// SHA-256 to the evidence records it rests on. The native owner's LIVE
/// gate counts only what verifies.
pub mod live_qualification {
    use super::*;

    /// One evidence record: its repository path, file SHA-256 and schema.
    pub mod evidence {
        use super::*;

        pub const PATH: u16 = 1;
        pub const SHA256: u16 = 2;
        pub const SCHEMA_NAME: u16 = 3;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.live-qualification.evidence",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(PATH, "path", Text),
                Field::req(SHA256, "sha256", B256),
                Field::req(SCHEMA_NAME, "schema", Text),
            ],
        };
    }

    /// A qualified venue or source.
    pub mod item {
        use super::*;

        /// `VENUE` or `SOURCE`.
        pub const KIND: u16 = 1;
        pub const ID: u16 = 2;
        pub const EVIDENCE: u16 = 3;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.live-qualification.item",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(KIND, "kind", Text),
                Field::req(ID, "id", Text),
                Field::nested(EVIDENCE, "evidence", Presence::Repeated, &evidence::SCHEMA),
            ],
        };
    }

    /// A qualified inclusion adapter and its measured findings
    /// (`PROPERTY:<NAME>=YES|NO`, `FEATURE:<NAME>`, `MODE:<NAME>`,
    /// `SEND_TIME`).
    pub mod adapter {
        use super::*;

        pub const ADAPTER: u16 = 1;
        pub const FINDINGS: u16 = 2;
        pub const EVIDENCE: u16 = 3;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.live-qualification.adapter",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(ADAPTER, "adapter", Text),
                Field::rep(FINDINGS, "findings", Text),
                Field::nested(EVIDENCE, "evidence", Presence::Repeated, &evidence::SCHEMA),
            ],
        };
    }

    /// The operator's documented J risk decision for the scope.
    pub mod risk {
        use super::*;

        pub const DECIDED_BY: u16 = 1;
        pub const DECIDED_UNIX_MS: u16 = 2;
        pub const STATEMENT: u16 = 3;
        pub const EVIDENCE: u16 = 4;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.live-qualification.risk",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(DECIDED_BY, "decided_by", Text),
                Field::req(DECIDED_UNIX_MS, "decided_unix_ms", U64),
                Field::req(STATEMENT, "statement", Text),
                Field::nested(EVIDENCE, "evidence", Presence::Repeated, &evidence::SCHEMA),
            ],
        };
    }

    pub const ITEMS: u16 = 1;
    pub const ADAPTERS: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    /// The G0.8 result whose verdict PASS is the scope's acquisition.
    pub const ACQUISITION: u16 = 5;
    pub const RISK: u16 = 6;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.live-qualification.v1",
        record_type: 66,
        schema_version: 1,
        fields: &[
            Field::nested(ITEMS, "items", Presence::Repeated, &item::SCHEMA),
            Field::nested(ADAPTERS, "adapters", Presence::Repeated, &adapter::SCHEMA),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::nested(
                ACQUISITION,
                "acquisition",
                Presence::Optional,
                &evidence::SCHEMA,
            ),
            Field::nested(RISK, "risk", Presence::Optional, &risk::SCHEMA),
        ],
    };
}

/// The receipt of one source-mode request.
pub mod source_mode_receipt {
    use super::*;

    pub const REQUEST_ID: u16 = 1;
    /// The source or component the request was for.
    pub const SOURCE_ID: u16 = 2;
    pub const REQUESTED: u16 = 3;
    pub const EXPECTED_GENERATION: u16 = 4;
    /// `APPLIED` or `REJECTED_STALE`.
    pub const OUTCOME: u16 = 5;
    /// The generation now current (the new one when applied).
    pub const GENERATION: u16 = 6;
    pub const EFFECTIVE: u16 = 7;
    pub const BLOCK_REASON: u16 = 8;
    pub const OBSERVED_UNIX_MS: u16 = 9;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.source-mode-receipt.v1",
        record_type: 39,
        schema_version: 1,
        fields: &[
            Field::req(REQUEST_ID, "request_id", Text),
            Field::req(SOURCE_ID, "source_id", Text),
            Field::req(REQUESTED, "requested", Text),
            Field::req(EXPECTED_GENERATION, "expected_generation", U64),
            Field::req(OUTCOME, "outcome", Text),
            Field::req(GENERATION, "generation", U64),
            Field::req(EFFECTIVE, "effective", Text),
            Field::opt(BLOCK_REASON, "block_reason", Text),
            Field::req(OBSERVED_UNIX_MS, "observed_unix_ms", U64),
        ],
    };
}

/// The running runtime's state, rewritten while it runs: per source its
/// effective mode, derived health and counters (plan §5a.2/§5a.3).
pub mod runtime_status {
    use super::*;

    /// One source of the runtime status.
    pub mod source {
        use super::*;

        pub const SOURCE_ID: u16 = 1;
        /// `OFF`, `SHADOW` or `BLOCKED` (the owner's, as applied by the runtime).
        pub const EFFECTIVE: u16 = 2;
        /// `OFF`, `UNQUALIFIED`, `DISCONNECTED`, `RATE_LIMITED`, `DEGRADED`, `HEALTHY`.
        pub const HEALTH: u16 = 3;
        pub const CONNECTS: u16 = 4;
        pub const DISCONNECTS: u16 = 5;
        pub const RATE_LIMITED: u16 = 6;
        pub const FULL: u16 = 7;
        pub const HASH_ONLY: u16 = 8;
        pub const REJECTED: u16 = 9;
        pub const DROPPED: u16 = 10;
        pub const DUPLICATES: u16 = 11;
        pub const FIRST_ANY: u16 = 12;
        pub const FIRST_FULL: u16 = 13;
        pub const LAUNCH_SIGHTINGS: u16 = 14;
        pub const LAUNCH_FIRST: u16 = 15;
        /// Milliseconds since its last observation; absent before the first.
        pub const LAST_EVENT_AGE_MS: u16 = 16;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.runtime-status.source",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(SOURCE_ID, "source_id", Text),
                Field::req(EFFECTIVE, "effective", Text),
                Field::req(HEALTH, "health", Text),
                Field::req(CONNECTS, "connects", U64),
                Field::req(DISCONNECTS, "disconnects", U64),
                Field::req(RATE_LIMITED, "rate_limited", U64),
                Field::req(FULL, "full", U64),
                Field::req(HASH_ONLY, "hash_only", U64),
                Field::req(REJECTED, "rejected", U64),
                Field::req(DROPPED, "dropped", U64),
                Field::req(DUPLICATES, "duplicates", U64),
                Field::req(FIRST_ANY, "first_any", U64),
                Field::req(FIRST_FULL, "first_full", U64),
                Field::req(LAUNCH_SIGHTINGS, "launch_sightings", U64),
                Field::req(LAUNCH_FIRST, "launch_first", U64),
                Field::opt(LAST_EVENT_AGE_MS, "last_event_age_ms", U64),
            ],
        };
    }

    /// The canonical chain follower (the G0.4 denominator): what really
    /// landed, joined with the pending sightings by local monotonic time.
    pub mod canonical {
        use super::*;

        /// `CONNECTING`, `FOLLOWING`, `RATE_LIMITED`, `FAILING` or `STOPPED`.
        pub const STATE: u16 = 1;
        /// The last canonical block; absent before the first.
        pub const HEAD: u16 = 2;
        /// Milliseconds since the follower first had that block's header.
        pub const HEAD_AGE_MS: u16 = 3;
        pub const BLOCKS: u16 = 4;
        pub const REORGS: u16 = 5;
        /// Reorgs deeper than the kept window, and head gaps: resynced, never silent.
        pub const RESYNCS: u16 = 6;
        pub const RPC_ERRORS: u16 = 7;
        pub const RATE_LIMITED: u16 = 8;
        /// Canonical Flap launches, orphaned ones removed.
        pub const FLAP_LAUNCHES: u16 = 9;
        pub const FOUR_LAUNCHES: u16 = 10;
        /// Launches of blocks a reorg orphaned (taken out of the counts above).
        pub const ORPHANED_LAUNCHES: u16 = 11;
        /// Flap launches with a verified full pending sighting before the
        /// block time (header ms; local wall-clock estimate of the sighting).
        pub const SEEN_FULL_BEFORE: u16 = 12;
        /// Flap launches only seen as a hash (not actionable) before the block.
        pub const SEEN_HASH_ONLY_BEFORE: u16 = 13;
        /// Flap launches first seen pending only after the block.
        pub const SEEN_AFTER: u16 = 14;
        /// Flap launches no source delivered.
        pub const MISSED: u16 = 15;
        /// External buys after the launch in the launch block, summed over the
        /// Flap launches whose buys are known (see `buys_unknown`).
        pub const EXTERNAL_BUYS: u16 = 16;
        /// Flap launches whose external buys are unknown (never counted as 0).
        pub const BUYS_UNKNOWN: u16 = 17;
        /// Flap launches whose deciding sighting has no wall-clock estimate.
        pub const CLOCK_UNKNOWN: u16 = 18;
        /// Flap launches of the C1 scope (direct Portal `newTokenV6`), and
        /// those seen as a verified full transaction before the block time.
        pub const C1_SCOPE_LAUNCHES: u16 = 19;
        pub const C1_SCOPE_FULL_BEFORE: u16 = 20;
        /// Canonical blocks not followed (gaps, resyncs).
        pub const BLOCKS_SKIPPED: u16 = 21;
        /// Launches received and waiting for their join.
        pub const WAITING: u16 = 22;
        /// The head's fetch lag: local wall ms at the header fetch minus the
        /// block's ms (negative: local clock behind).
        pub const HEAD_FETCH_LAG_MS: u16 = 23;
        /// Launches (any venue) whose transaction is a type/form the
        /// verifier does not rebuild (EIP-4844/7702, pre-EIP-155).
        pub const TX_UNVERIFIABLE: u16 = 24;
        /// Launches (any venue) whose provider answers were refused every
        /// time they were fetched: unknown, not counted as C1 scope.
        pub const TX_ANSWER_REFUSED: u16 = 25;
        /// Blocks followed after a restart that were mined before this
        /// process started (the C2 resume backfill), and their launches:
        /// outside the observation window, never part of the counts above.
        pub const BACKFILL_BLOCKS: u16 = 26;
        pub const BACKFILL_LAUNCHES: u16 = 27;
        /// Four.meme launches (Classic and OpenFour, in `four_launches`)
        /// whose verified signed bytes the production Four decoder
        /// recognizes as a launch with the canonical token derived before
        /// execution (Phase E).
        pub const FOUR_PENDING_DECODED: u16 = 28;
        /// v7 (Phase E review M1): Four.meme launches decoded pending
        /// without a proven derivation (unfrozen OpenFour preset), and with
        /// a derived token that is not the canonical one.
        pub const FOUR_PENDING_UNKNOWN: u16 = 29;
        pub const FOUR_PENDING_MISMATCH: u16 = 30;
        /// Four.meme launches seen in full before their block whose token the
        /// decoder derived from the launch bytes (actionable before execution).
        pub const FOUR_ACTIONABLE: u16 = 31;
        /// Launch calls included and reverted (no token).
        pub const REVERTED_LAUNCHES: u16 = 32;
        /// v9 (Phase E review L3): the Four.meme drift guard's last verdict
        /// (`UNPROBED`, `MATCH at block n`, `DRIFT at block n: ...`,
        /// `PROBE_FAILED: ...`).
        pub const FOUR_DEPLOYMENT: u16 = 33;
        /// v10 (Paket 8 review M1): Four Classic launches whose token is
        /// one of the frozen pairs' candidates (not derivable).
        pub const FOUR_PENDING_CANDIDATE: u16 = 34;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.runtime-status.canonical",
            record_type: 0,
            schema_version: 10,
            fields: &[
                Field::req(STATE, "state", Text),
                Field::opt(HEAD, "head", U64),
                Field::opt(HEAD_AGE_MS, "head_age_ms", U64),
                Field::req(BLOCKS, "blocks", U64),
                Field::req(REORGS, "reorgs", U64),
                Field::req(RESYNCS, "resyncs", U64),
                Field::req(RPC_ERRORS, "rpc_errors", U64),
                Field::req(RATE_LIMITED, "rate_limited", U64),
                Field::req(FLAP_LAUNCHES, "flap_launches", U64),
                Field::req(FOUR_LAUNCHES, "four_launches", U64),
                Field::req(ORPHANED_LAUNCHES, "orphaned_launches", U64),
                Field::req(SEEN_FULL_BEFORE, "seen_full_before", U64),
                Field::req(SEEN_HASH_ONLY_BEFORE, "seen_hash_only_before", U64),
                Field::req(SEEN_AFTER, "seen_after", U64),
                Field::req(MISSED, "missed", U64),
                Field::req(EXTERNAL_BUYS, "external_buys", U64),
                Field::req(BUYS_UNKNOWN, "buys_unknown", U64),
                Field::req(CLOCK_UNKNOWN, "clock_unknown", U64),
                Field::req(C1_SCOPE_LAUNCHES, "c1_scope_launches", U64),
                Field::req(C1_SCOPE_FULL_BEFORE, "c1_scope_full_before", U64),
                Field::req(BLOCKS_SKIPPED, "blocks_skipped", U64),
                Field::req(WAITING, "waiting", U64),
                Field::opt(HEAD_FETCH_LAG_MS, "head_fetch_lag_ms", I64),
                Field::req(TX_UNVERIFIABLE, "tx_unverifiable", U64),
                Field::req(TX_ANSWER_REFUSED, "tx_answer_refused", U64),
                Field::req(BACKFILL_BLOCKS, "backfill_blocks", U64),
                Field::req(BACKFILL_LAUNCHES, "backfill_launches", U64),
                Field::req(FOUR_PENDING_DECODED, "four_pending_decoded", U64),
                Field::req(FOUR_PENDING_UNKNOWN, "four_pending_unknown", U64),
                Field::req(FOUR_PENDING_MISMATCH, "four_pending_mismatch", U64),
                Field::req(FOUR_ACTIONABLE, "four_actionable", U64),
                Field::req(REVERTED_LAUNCHES, "reverted_launches", U64),
                Field::opt(FOUR_DEPLOYMENT, "four_deployment", Text),
                Field::req(FOUR_PENDING_CANDIDATE, "four_pending_candidate", U64),
            ],
        };
    }

    /// The C2 chain-state owner (component `BSC_STATE_C2` effective
    /// SHADOW): canonical effects, pending candidates and per-sender
    /// confidence. Every applied, refused, overflowed, late or unknown item
    /// is a counter here; nothing is dropped silently.
    pub mod state {
        use super::*;

        /// The retained canonical head; absent while unanchored.
        pub const HEAD: u16 = 1;
        /// A snapshot can be issued: head known, no refused canonical input
        /// since the last accepted one, every watched sender clean.
        pub const COHERENT: u16 = 2;
        pub const WATCHED: u16 = 3;
        pub const RETAINED_BLOCKS: u16 = 4;
        pub const HELD_TXS: u16 = 5;
        pub const HELD_CANDIDATES: u16 = 6;
        pub const HELD_UNCERTAIN: u16 = 7;
        pub const HELD_SENDERS: u16 = 8;
        pub const UNKNOWN_SENDERS: u16 = 9;
        pub const QUARANTINED_SENDERS: u16 = 10;
        pub const INPUTS: u16 = 11;
        pub const EFFECTS: u16 = 12;
        pub const PENDING_OBSERVATIONS: u16 = 13;
        pub const REPEAT_SIGHTINGS: u16 = 14;
        pub const EARLIER_FIRST_SEEN: u16 = 15;
        pub const LATE_SIGHTINGS: u16 = 16;
        pub const STALE_PENDING: u16 = 17;
        pub const COMPETING_CANDIDATES: u16 = 18;
        pub const BLOCKS: u16 = 19;
        pub const INCLUDED: u16 = 20;
        pub const REVERTED: u16 = 21;
        pub const REPLACED: u16 = 22;
        pub const REORGS: u16 = 23;
        pub const REORGS_BEYOND_WINDOW: u16 = 24;
        pub const REORGS_TOO_DEEP: u16 = 25;
        pub const UNMATCHED_ORPHANED_BLOCKS: u16 = 26;
        pub const ORPHANED_TXS: u16 = 27;
        pub const GAPS: u16 = 28;
        pub const UNCERTAIN_MARKED: u16 = 29;
        pub const FINALIZED_BLOCKS: u16 = 30;
        pub const FINALIZED_TXS: u16 = 31;
        pub const NONCE_FACTS: u16 = 32;
        pub const IGNORED_NONCE_FACTS: u16 = 33;
        pub const RECONCILIATIONS: u16 = 34;
        pub const RECONCILIATION_REFUSALS: u16 = 35;
        pub const QUARANTINES: u16 = 36;
        pub const CONFLICTS: u16 = 37;
        pub const REFUSED_INPUTS: u16 = 38;
        pub const OVERFLOW_TXS: u16 = 39;
        pub const OVERFLOW_CANDIDATES: u16 = 40;
        pub const EVICTED: u16 = 41;
        /// Blocks replayed from the checkpoint at start.
        pub const RESTORED_BLOCKS: u16 = 42;
        /// `NONE` (no checkpoint), `RESTORED` or `REFUSED` (unreadable or
        /// contradicting checkpoint: started empty).
        pub const CHECKPOINT_RESTORE: u16 = 43;
        pub const CHECKPOINT_WRITES: u16 = 44;
        pub const CHECKPOINT_ERRORS: u16 = 45;
        /// The latest conflict, as its kind.
        pub const LAST_CONFLICT: u16 = 46;
        /// Blocks that did not extend the head (or were not the expected
        /// anchor): the retained chain is dropped and the block re-anchors.
        pub const DISCONTINUITIES: u16 = 47;
        /// Candidates whose nonce is proven consumed (an own account's nonce
        /// fact, or any sender's later nonce included) without them being
        /// seen included; uncertain txs whose nonce is so proven consumed by
        /// an unknown tx; of all, those resolved by a later inclusion; and
        /// resolutions undone because their evidence block left the chain.
        pub const CONSUMED_NOT_SEEN: u16 = 48;
        pub const CONSUMED_UNATTRIBUTED: u16 = 49;
        pub const RESOLUTIONS_REVERTED: u16 = 50;
        /// The latest refused input, as its kind.
        pub const LAST_REFUSED: u16 = 51;
        pub const RESOLVED_BY_LATER_INCLUSION: u16 = 52;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.runtime-status.state",
            record_type: 0,
            schema_version: 3,
            fields: &[
                Field::opt(HEAD, "head", U64),
                Field::req(COHERENT, "coherent", Bool),
                Field::req(WATCHED, "watched", U64),
                Field::req(RETAINED_BLOCKS, "retained_blocks", U64),
                Field::req(HELD_TXS, "held_txs", U64),
                Field::req(HELD_CANDIDATES, "held_candidates", U64),
                Field::req(HELD_UNCERTAIN, "held_uncertain", U64),
                Field::req(HELD_SENDERS, "held_senders", U64),
                Field::req(UNKNOWN_SENDERS, "unknown_senders", U64),
                Field::req(QUARANTINED_SENDERS, "quarantined_senders", U64),
                Field::req(INPUTS, "inputs", U64),
                Field::req(EFFECTS, "effects", U64),
                Field::req(PENDING_OBSERVATIONS, "pending_observations", U64),
                Field::req(REPEAT_SIGHTINGS, "repeat_sightings", U64),
                Field::req(EARLIER_FIRST_SEEN, "earlier_first_seen", U64),
                Field::req(LATE_SIGHTINGS, "late_sightings", U64),
                Field::req(STALE_PENDING, "stale_pending", U64),
                Field::req(COMPETING_CANDIDATES, "competing_candidates", U64),
                Field::req(BLOCKS, "blocks", U64),
                Field::req(INCLUDED, "included", U64),
                Field::req(REVERTED, "reverted", U64),
                Field::req(REPLACED, "replaced", U64),
                Field::req(REORGS, "reorgs", U64),
                Field::req(REORGS_BEYOND_WINDOW, "reorgs_beyond_window", U64),
                Field::req(REORGS_TOO_DEEP, "reorgs_too_deep", U64),
                Field::req(UNMATCHED_ORPHANED_BLOCKS, "unmatched_orphaned_blocks", U64),
                Field::req(ORPHANED_TXS, "orphaned_txs", U64),
                Field::req(GAPS, "gaps", U64),
                Field::req(UNCERTAIN_MARKED, "uncertain_marked", U64),
                Field::req(FINALIZED_BLOCKS, "finalized_blocks", U64),
                Field::req(FINALIZED_TXS, "finalized_txs", U64),
                Field::req(NONCE_FACTS, "nonce_facts", U64),
                Field::req(IGNORED_NONCE_FACTS, "ignored_nonce_facts", U64),
                Field::req(RECONCILIATIONS, "reconciliations", U64),
                Field::req(RECONCILIATION_REFUSALS, "reconciliation_refusals", U64),
                Field::req(QUARANTINES, "quarantines", U64),
                Field::req(CONFLICTS, "conflicts", U64),
                Field::req(REFUSED_INPUTS, "refused_inputs", U64),
                Field::req(OVERFLOW_TXS, "overflow_txs", U64),
                Field::req(OVERFLOW_CANDIDATES, "overflow_candidates", U64),
                Field::req(EVICTED, "evicted", U64),
                Field::req(RESTORED_BLOCKS, "restored_blocks", U64),
                Field::req(CHECKPOINT_RESTORE, "checkpoint_restore", Text),
                Field::req(CHECKPOINT_WRITES, "checkpoint_writes", U64),
                Field::req(CHECKPOINT_ERRORS, "checkpoint_errors", U64),
                Field::opt(LAST_CONFLICT, "last_conflict", Text),
                Field::req(DISCONTINUITIES, "discontinuities", U64),
                Field::req(CONSUMED_NOT_SEEN, "consumed_not_seen", U64),
                Field::req(CONSUMED_UNATTRIBUTED, "consumed_unattributed", U64),
                Field::req(RESOLUTIONS_REVERTED, "resolutions_reverted", U64),
                Field::opt(LAST_REFUSED, "last_refused", Text),
                Field::req(
                    RESOLVED_BY_LATER_INCLUSION,
                    "resolved_by_later_inclusion",
                    U64,
                ),
            ],
        };
    }

    /// The capture's storage (Phase F): the segmented journal's writer,
    /// its durability watermark and the byte budget of the queued records.
    pub mod capture {
        use super::*;

        /// Frames appended to sealed or open segments.
        pub const RECORDS: u16 = 1;
        pub const BYTES: u16 = 2;
        /// Records the gap spans written say were lost.
        pub const LOST_RECORDS: u16 = 3;
        pub const SEGMENTS_SEALED: u16 = 4;
        pub const SYNCS: u16 = 5;
        /// The last sequence known to be on disk.
        pub const DURABLE_SEQUENCE: u16 = 6;
        pub const BUDGET_MAX_BYTES: u16 = 7;
        pub const BUDGET_IN_USE_BYTES: u16 = 8;
        pub const BUDGET_PEAK_BYTES: u16 = 9;
        /// Records the byte budget refused (each a gap).
        pub const BUDGET_REFUSED: u16 = 10;
        pub const WRITE_ERRORS: u16 = 11;
        /// `HEALTHY` or `FAILED` (a write, sync or seal failed: the capture
        /// is no longer complete).
        pub const STATE: u16 = 12;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.runtime-status.capture",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(RECORDS, "records", U64),
                Field::req(BYTES, "bytes", U64),
                Field::req(LOST_RECORDS, "lost_records", U64),
                Field::req(SEGMENTS_SEALED, "segments_sealed", U64),
                Field::req(SYNCS, "syncs", U64),
                Field::opt(DURABLE_SEQUENCE, "durable_sequence", U64),
                Field::req(BUDGET_MAX_BYTES, "budget_max_bytes", U64),
                Field::req(BUDGET_IN_USE_BYTES, "budget_in_use_bytes", U64),
                Field::req(BUDGET_PEAK_BYTES, "budget_peak_bytes", U64),
                Field::req(BUDGET_REFUSED, "budget_refused", U64),
                Field::req(WRITE_ERRORS, "write_errors", U64),
                Field::req(STATE, "state", Text),
            ],
        };
    }

    /// The C3 execution path in PAPER mode (component `BSC_EXEC` effective
    /// SHADOW): from the pending launch to the PAPER sink (`T_shadow_sink`,
    /// never `T_send`).
    /// Phase H: Strategy C (component `BSC_STRATEGY_C`, SHADOW: a PAPER
    /// ledger) on the journaled canonical records.
    pub mod strategy {
        use super::*;

        /// `SHADOW`, or `GAS_EVIDENCE_REFUSED` with its reason.
        pub const STATE: u16 = 1;
        pub const REASON: u16 = 2;
        /// Tokens followed now (observing, entering, open or exiting).
        pub const FOLLOWED: u16 = 3;
        pub const LAUNCHES: u16 = 4;
        /// Decisions by verdict.
        pub const PASSED: u16 = 5;
        pub const REJECTED: u16 = 6;
        pub const DATA_INCOMPLETE: u16 = 7;
        pub const NOT_TRACKED: u16 = 8;
        pub const SLOT_LIMIT: u16 = 9;
        pub const CREATOR_SOLD: u16 = 10;
        /// PAPER entries and cancels, exits by reason.
        pub const ENTERED: u16 = 11;
        pub const CANCELLED: u16 = 12;
        pub const EXITS_CREATOR_SELL: u16 = 13;
        pub const EXITS_TARGET: u16 = 14;
        pub const EXITS_STOP: u16 = 15;
        pub const EXITS_TIMEOUT: u16 = 16;
        /// Closed PAPER positions with a gain or a loss after every cost,
        /// and their sums (wei).
        pub const GAINS: u16 = 17;
        pub const LOSSES: u16 = 18;
        pub const GAIN_WEI: u16 = 19;
        pub const LOSS_WEI: u16 = 20;
        /// Open positions whose curve stopped being exact: never valued.
        pub const UNRESOLVED: u16 = 21;
        /// The configured limits.
        pub const OBSERVATION_MS: u16 = 22;
        pub const MAX_OPEN: u16 = 23;
        pub const INPUT_WEI: u16 = 24;
        /// v2: the inputs the engine took: trades, terms, migrations and
        /// the records that made every followed curve incomplete (a refused
        /// Portal log, a reorg, a resync).
        pub const TRADES: u16 = 25;
        pub const TERMS: u16 = 26;
        pub const MIGRATIONS: u16 = 27;
        pub const INCOMPLETE: u16 = 28;
        /// v3: PAPER positions entered and not yet closed.
        pub const OPEN: u16 = 29;
        /// v4 (Phase I): decisions for the PAPER path without an armed C3
        /// executor (counted, never executed).
        pub const PAPER_UNARMED: u16 = 30;
        /// v5 (Phase I gate L4): decisions without an exact curve or
        /// position when their block was complete (never executed).
        pub const PAPER_UNPRICED: u16 = 31;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.runtime-status.strategy",
            record_type: 0,
            schema_version: 5,
            fields: &[
                Field::req(STATE, "state", Text),
                Field::opt(REASON, "reason", Text),
                Field::req(FOLLOWED, "followed", U64),
                Field::req(LAUNCHES, "launches", U64),
                Field::req(PASSED, "passed", U64),
                Field::req(REJECTED, "rejected", U64),
                Field::req(DATA_INCOMPLETE, "data_incomplete", U64),
                Field::req(NOT_TRACKED, "not_tracked", U64),
                Field::req(SLOT_LIMIT, "slot_limit", U64),
                Field::req(CREATOR_SOLD, "creator_sold", U64),
                Field::req(ENTERED, "entered", U64),
                Field::req(CANCELLED, "cancelled", U64),
                Field::req(EXITS_CREATOR_SELL, "exits_creator_sell", U64),
                Field::req(EXITS_TARGET, "exits_target", U64),
                Field::req(EXITS_STOP, "exits_stop", U64),
                Field::req(EXITS_TIMEOUT, "exits_timeout", U64),
                Field::req(GAINS, "gains", U64),
                Field::req(LOSSES, "losses", U64),
                Field::req(GAIN_WEI, "gain_wei", U256),
                Field::req(LOSS_WEI, "loss_wei", U256),
                Field::req(UNRESOLVED, "unresolved", U64),
                Field::req(OBSERVATION_MS, "observation_ms", U64),
                Field::req(MAX_OPEN, "max_open", U64),
                Field::req(INPUT_WEI, "input_wei", U256),
                Field::req(TRADES, "trades", U64),
                Field::req(TERMS, "terms", U64),
                Field::req(MIGRATIONS, "migrations", U64),
                Field::req(INCOMPLETE, "incomplete", U64),
                Field::req(OPEN, "open", U64),
                Field::req(PAPER_UNARMED, "paper_unarmed", U64),
                Field::req(PAPER_UNPRICED, "paper_unpriced", U64),
            ],
        };
    }

    pub mod exec {
        use super::*;

        /// `OFF`, `IDENTITY_REFUSED`, `WAITING_C2`, `PROBING`,
        /// `PROBE_FAILED`, `DEPLOYMENT_DRIFT`, `TERMS_DRIFT`,
        /// `ECONOMICS_UNQUALIFIED`, `FUNDED`, `WAITING_NONCE`, `ARM_FAILED`,
        /// `ARMED` or `EXHAUSTED`.
        pub const STATE: u16 = 1;
        /// Why the state is not `ARMED`.
        pub const REASON: u16 = 2;
        /// The PAPER identity's address (never a production account; its
        /// balance is `PAPER_BALANCE_WEI` at the probe block).
        pub const PAPER_SENDER: u16 = 3;
        /// The live Portal version and the block the probe pinned.
        pub const PORTAL_VERSION: u16 = 4;
        pub const PROBE_BLOCK: u16 = 5;
        pub const LANES_ARMED: u16 = 6;
        pub const LANES_LEFT: u16 = 7;
        /// Pending C1-scope launches handed to the path.
        pub const LAUNCHES: u16 = 8;
        /// Signed PAPER BUYs taken by the sink.
        pub const HANDOFFS: u16 = 9;
        /// Refusals by stage.
        pub const REFUSED_NOT_DECODED: u16 = 10;
        pub const REFUSED_PROFILE: u16 = 11;
        pub const REFUSED_NO_SNAPSHOT: u16 = 12;
        pub const REFUSED_ADMISSION: u16 = 13;
        pub const REFUSED_INTENT: u16 = 14;
        pub const REFUSED_LANES_EXHAUSTED: u16 = 15;
        pub const REFUSED_LANE: u16 = 16;
        pub const REFUSED_BUILD: u16 = 17;
        /// `T_ingress` → `T_shadow_sink` of the kept samples (ns).
        pub const LATENCY_SAMPLES: u16 = 18;
        pub const LATENCY_P50_NS: u16 = 19;
        pub const LATENCY_P99_NS: u16 = 20;
        pub const LATENCY_MAX_NS: u16 = 21;
        /// v2 (C3 gate review): the stages of the production submission
        /// plan, frame and dispatch, the session's shared cap and token
        /// dedup, the probe's view of the PAPER balance, re-probes.
        pub const REFUSED_DUPLICATE_TOKEN: u16 = 22;
        pub const REFUSED_SESSION_CAP: u16 = 23;
        pub const REFUSED_PLAN: u16 = 24;
        pub const REFUSED_DEADLINE: u16 = 25;
        pub const REFUSED_DISPATCH: u16 = 26;
        /// Exposure dispatched in this session (against the shared cap).
        pub const SESSION_SPENT_WEI: u16 = 27;
        /// The PAPER sender's balance at the probe block.
        pub const PAPER_BALANCE_WEI: u16 = 28;
        /// Probes started (the first and the re-probes while armed).
        pub const PROBES: u16 = 29;
        /// Disarms whose fence could not be removed.
        pub const DISARM_ERRORS: u16 = 30;
        /// v3 (Phase I): Strategy C's PAPER transactions: asked for (an
        /// exit counts once), at the sink (an exit's approval and SELL
        /// count twice), refused by stage, the strategy lanes left (BUYs and
        /// approvals, SELLs) and the strategy exposure dispatched.
        pub const STRATEGY_INTENTS: u16 = 31;
        pub const STRATEGY_HANDOFFS: u16 = 32;
        pub const STRATEGY_REFUSED: u16 = 33;
        pub const STRATEGY_LANES_LEFT: u16 = 34;
        /// Open exposure plus the gas spent (v4: an exit releases its
        /// position's input).
        pub const STRATEGY_SPENT_WEI: u16 = 36;
        /// The refusals by stage, `STAGE=n` each.
        pub const STRATEGY_REFUSALS: u16 = 37;
        /// v4 (Phase I gate M3): the strategy's next nonce in the PAPER
        /// sender's sequence, and whether a nonce hole stopped its intents
        /// (field 35, the v3 SELL lanes, is retired).
        pub const STRATEGY_NEXT_NONCE: u16 = 38;
        pub const STRATEGY_NONCE_GAP: u16 = 39;
        /// v5 (Phase I gate M2): the PAPER paths that take the sender
        /// (`BSC_EXEC_SENDER_PATHS`), and every C3 refusal by stage,
        /// `STAGE=n` each (NONCE_TAKEN, PATH_OFF included).
        pub const SENDER_PATHS: u16 = 40;
        pub const REFUSALS: u16 = 41;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.runtime-status.exec",
            record_type: 0,
            schema_version: 5,
            fields: &[
                Field::req(STATE, "state", Text),
                Field::opt(REASON, "reason", Text),
                Field::opt(PAPER_SENDER, "paper_sender", Text),
                Field::opt(PORTAL_VERSION, "portal_version", Text),
                Field::opt(PROBE_BLOCK, "probe_block", U64),
                Field::req(LANES_ARMED, "lanes_armed", U64),
                Field::req(LANES_LEFT, "lanes_left", U64),
                Field::req(LAUNCHES, "launches", U64),
                Field::req(HANDOFFS, "handoffs", U64),
                Field::req(REFUSED_NOT_DECODED, "refused_not_decoded", U64),
                Field::req(REFUSED_PROFILE, "refused_profile", U64),
                Field::req(REFUSED_NO_SNAPSHOT, "refused_no_snapshot", U64),
                Field::req(REFUSED_ADMISSION, "refused_admission", U64),
                Field::req(REFUSED_INTENT, "refused_intent", U64),
                Field::req(REFUSED_LANES_EXHAUSTED, "refused_lanes_exhausted", U64),
                Field::req(REFUSED_LANE, "refused_lane", U64),
                Field::req(REFUSED_BUILD, "refused_build", U64),
                Field::req(LATENCY_SAMPLES, "latency_samples", U64),
                Field::opt(LATENCY_P50_NS, "latency_p50_ns", U64),
                Field::opt(LATENCY_P99_NS, "latency_p99_ns", U64),
                Field::opt(LATENCY_MAX_NS, "latency_max_ns", U64),
                Field::req(REFUSED_DUPLICATE_TOKEN, "refused_duplicate_token", U64),
                Field::req(REFUSED_SESSION_CAP, "refused_session_cap", U64),
                Field::req(REFUSED_PLAN, "refused_plan", U64),
                Field::req(REFUSED_DEADLINE, "refused_deadline", U64),
                Field::req(REFUSED_DISPATCH, "refused_dispatch", U64),
                Field::req(SESSION_SPENT_WEI, "session_spent_wei", U256),
                Field::opt(PAPER_BALANCE_WEI, "paper_balance_wei", U256),
                Field::req(PROBES, "probes", U64),
                Field::req(DISARM_ERRORS, "disarm_errors", U64),
                Field::req(STRATEGY_INTENTS, "strategy_intents", U64),
                Field::req(STRATEGY_HANDOFFS, "strategy_handoffs", U64),
                Field::req(STRATEGY_REFUSED, "strategy_refused", U64),
                Field::req(STRATEGY_LANES_LEFT, "strategy_lanes_left", U64),
                Field::req(STRATEGY_SPENT_WEI, "strategy_spent_wei", U256),
                Field::rep(STRATEGY_REFUSALS, "strategy_refusals", Text),
                Field::req(STRATEGY_NEXT_NONCE, "strategy_next_nonce", U64),
                Field::req(STRATEGY_NONCE_GAP, "strategy_nonce_gap", Bool),
                Field::opt(SENDER_PATHS, "sender_paths", Text),
                Field::rep(REFUSALS, "refusals", Text),
            ],
        };
    }

    /// The LIVE path (plan 28.10.): its arming, what it released and what
    /// C2's evidence did to it.
    pub mod live {
        use super::*;

        pub const STATE: u16 = 1;
        pub const REASON: u16 = 2;
        /// The buyer wallet, `0x` hex.
        pub const BUYER: u16 = 3;
        pub const BALANCE_WEI: u16 = 4;
        pub const NEXT_NONCE: u16 = 5;
        /// The owner generation of the grant the path is armed under.
        pub const GRANT_GENERATION: u16 = 6;
        /// `BUILDERS` or `LOOPBACK`.
        pub const ROUTE: u16 = 7;
        /// Transactions whose bytes may have left, and of them the UNKNOWN.
        pub const RELEASED: u16 = 8;
        pub const UNKNOWN: u16 = 9;
        /// C2 evidence applied to the path's intents.
        pub const INCLUDED: u16 = 10;
        pub const REVERTED: u16 = 11;
        pub const REPLACED: u16 = 12;
        pub const FINAL: u16 = 13;
        pub const ORPHANED: u16 = 14;
        /// The owner's or the release's last reason to refuse an intent.
        pub const LAST_REFUSAL: u16 = 15;
        /// Refusals by stage, `NAME=n`.
        pub const REFUSALS: u16 = 16;
        pub const PROBES: u16 = 17;
        /// The LIVE ledger's positions (its fold).
        pub const POSITIONS_OPEN: u16 = 18;
        pub const POSITIONS_CLOSED: u16 = 19;
        pub const POSITIONS_FAILED: u16 = 20;
        /// Closed positions' proceeds minus input minus all gas, and the
        /// gas of failed BUYs, signed decimal wei.
        pub const REALIZED_PNL_WEI: u16 = 21;
        /// All gas the ledger's included or reverted transactions paid.
        pub const GAS_WEI: u16 = 22;
        /// Why the ledger could not be read (then nothing above counts).
        pub const LEDGER_ERROR: u16 = 23;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.runtime-status.live",
            record_type: 0,
            schema_version: 2,
            fields: &[
                Field::req(STATE, "state", Text),
                Field::opt(REASON, "reason", Text),
                Field::opt(BUYER, "buyer", Text),
                Field::opt(BALANCE_WEI, "balance_wei", U256),
                Field::opt(NEXT_NONCE, "next_nonce", U64),
                Field::opt(GRANT_GENERATION, "grant_generation", U64),
                Field::req(ROUTE, "route", Text),
                Field::req(RELEASED, "released", U64),
                Field::req(UNKNOWN, "unknown", U64),
                Field::req(INCLUDED, "included", U64),
                Field::req(REVERTED, "reverted", U64),
                Field::req(REPLACED, "replaced", U64),
                Field::req(FINAL, "final", U64),
                Field::req(ORPHANED, "orphaned", U64),
                Field::opt(LAST_REFUSAL, "last_refusal", Text),
                Field::rep(REFUSALS, "refusals", Text),
                Field::req(PROBES, "probes", U64),
                Field::req(POSITIONS_OPEN, "positions_open", U64),
                Field::req(POSITIONS_CLOSED, "positions_closed", U64),
                Field::req(POSITIONS_FAILED, "positions_failed", U64),
                Field::req(REALIZED_PNL_WEI, "realized_pnl_wei", Text),
                Field::req(GAS_WEI, "gas_wei", U256),
                Field::opt(LEDGER_ERROR, "ledger_error", Text),
            ],
        };
    }

    /// The source-modes generation the runtime applies.
    pub const GENERATION: u16 = 1;
    pub const SOURCES: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    pub const STARTED_UNIX_MS: u16 = 5;
    /// Distinct C1-scope Flap launches (direct Portal `newTokenV6`) seen as
    /// verified full pending transactions.
    pub const LAUNCHES: u16 = 6;
    /// The journal of this process, relative to the run directory.
    pub const JOURNAL_PATH: u16 = 7;
    pub const JOURNAL_RECORDS: u16 = 8;
    /// Records the bounded journal queue could not take (never silent).
    pub const JOURNAL_LOST: u16 = 9;
    /// `READ_ONLY`: observation only, no build, signature or send exists;
    /// `LIVE` while the LIVE path is armed (plan 28.10.).
    pub const MODE: u16 = 10;
    /// Absent when no canonical endpoint is configured (then no denominator).
    pub const CANONICAL: u16 = 11;
    /// How often this record is rewritten; a reader derives staleness from it.
    pub const STATUS_INTERVAL_MS: u16 = 12;
    /// Present while the C2 chain-state owner runs.
    pub const STATE: u16 = 13;
    /// Present while a PAPER execution path is configured.
    pub const EXEC: u16 = 14;
    /// The capture's storage health (Phase F).
    pub const CAPTURE: u16 = 15;
    /// Present while `BSC_STRATEGY_C` runs or is refused (Phase H).
    pub const STRATEGY: u16 = 16;
    /// Present while a LIVE path is configured (plan 28.10.).
    pub const LIVE: u16 = 17;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.runtime-status.v1",
        record_type: 40,
        schema_version: 20,
        fields: &[
            Field::req(GENERATION, "generation", U64),
            Field::nested(SOURCES, "sources", Presence::Repeated, &source::SCHEMA),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(STARTED_UNIX_MS, "started_unix_ms", U64),
            Field::req(LAUNCHES, "launches", U64),
            Field::req(JOURNAL_PATH, "journal_path", Text),
            Field::req(JOURNAL_RECORDS, "journal_records", U64),
            Field::req(JOURNAL_LOST, "journal_lost", U64),
            Field::req(MODE, "mode", Text),
            Field::nested(
                CANONICAL,
                "canonical",
                Presence::Optional,
                &canonical::SCHEMA,
            ),
            Field::req(STATUS_INTERVAL_MS, "status_interval_ms", U64),
            Field::nested(STATE, "state", Presence::Optional, &state::SCHEMA),
            Field::nested(EXEC, "exec", Presence::Optional, &exec::SCHEMA),
            Field::nested(CAPTURE, "capture", Presence::Optional, &capture::SCHEMA),
            Field::nested(STRATEGY, "strategy", Presence::Optional, &strategy::SCHEMA),
            Field::nested(LIVE, "live", Presence::Optional, &live::SCHEMA),
        ],
    };
}

/// Phase H: the replay of one run's journal through Strategy C
/// (`bsc_runtime strategy-replay`): the outputs it reproduces against the
/// `Y` records the runtime journaled, and the PAPER result (EV) of the
/// replay.
pub mod strategy_replay {
    use super::*;

    pub const RUN_ID: u16 = 1;
    pub const CHAIN_ID: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = 3;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = 4;
    /// `PASS` (every journaled output reproduced, nothing else),
    /// `DIFFERS`, `INCOMPLETE` (the journal does not read to its end) or
    /// `NOT_RUN` (the strategy never ran in this run).
    pub const VERDICT: u16 = 5;
    pub const REASONS: u16 = 6;
    pub const RECORDS: u16 = 7;
    /// How often the strategy was switched on in the run.
    pub const STARTS: u16 = 8;
    pub const JOURNALED: u16 = 9;
    pub const REPLAYED: u16 = 10;
    /// The index of the first output that differs.
    pub const FIRST_DIFFERENCE: u16 = 11;
    /// The run's configuration is the one replayed with.
    pub const CONFIG_MATCH: u16 = 12;
    pub const LAUNCHES: u16 = 13;
    pub const PASSED: u16 = 14;
    pub const REJECTED: u16 = 15;
    pub const DATA_INCOMPLETE: u16 = 16;
    pub const NOT_TRACKED: u16 = 17;
    pub const SLOT_LIMIT: u16 = 18;
    pub const CREATOR_SOLD: u16 = 19;
    pub const ENTERED: u16 = 20;
    pub const CANCELLED: u16 = 21;
    pub const EXITS_CREATOR_SELL: u16 = 22;
    pub const EXITS_TARGET: u16 = 23;
    pub const EXITS_STOP: u16 = 24;
    pub const EXITS_TIMEOUT: u16 = 25;
    pub const GAINS: u16 = 26;
    pub const LOSSES: u16 = 27;
    pub const GAIN_WEI: u16 = 28;
    pub const LOSS_WEI: u16 = 29;
    pub const UNRESOLVED: u16 = 30;
    /// The inputs the replay took (as the status's strategy section).
    pub const TRADES: u16 = 31;
    pub const TERMS: u16 = 32;
    pub const MIGRATIONS: u16 = 33;
    pub const INCOMPLETE: u16 = 34;
    /// v2 (H gate L10): PAPER positions still open when the strategy's
    /// periods ended (not valued). v2 decisions carry their features.
    pub const OPEN_AT_END: u16 = 35;
    /// v3 (Phase I gate, PHASENPLAN §17 J input): each entry's position
    /// against the foreign BUYs of its token, and the totals: entries
    /// eligible for the block after the decided one, entries with a
    /// foreign BUY before or in their eligible block, entries without a
    /// journaled eligible block.
    pub const POSITIONS: u16 = 36;
    pub const POSITIONS_NEXT_BLOCK: u16 = 37;
    pub const POSITIONS_CONTESTED: u16 = 38;
    pub const POSITIONS_UNKNOWN: u16 = 39;

    /// One entry's position. Times are wall-clock estimates (the run's
    /// start plus the deciding record's monotonic time) against the
    /// headers' millisecond block times: nothing is sent, no inclusion is
    /// shown; the header time is not the producer's cut-off.
    pub mod position {
        use super::*;

        /// `SHADOW` (a strategy entry) or `PAPER` (a BUY at the sink).
        pub const SOURCE: u16 = 1;
        pub const TOKEN: u16 = 2;
        /// The block whose complete state the entry is priced on.
        pub const DECIDED_BLOCK: u16 = 3;
        pub const DECIDED_UNIX_US: u16 = 4;
        /// The first block whose header time is after the decision.
        pub const ELIGIBLE_BLOCK: u16 = 5;
        /// The header time of the block after the decided one minus the
        /// decision (negative: that block's time had passed).
        pub const LEAD_US: u16 = 6;
        /// Foreign BUYs of the token after the decided block, before the
        /// eligible one (ahead of any BUY of the entry).
        pub const BUYS_BEFORE: u16 = 7;
        /// Foreign BUYs of the token in the eligible block (the order
        /// against the entry's is not known).
        pub const BUYS_IN_ELIGIBLE: u16 = 8;
        /// Blocks from the decided block to the first foreign BUY after it.
        pub const FIRST_BUY_GAP_BLOCKS: u16 = 9;
        /// The action time was not journaled (an older journal): the
        /// deciding record's time stands in, a lower bound.
        pub const TIME_LOWER_BOUND: u16 = 10;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.strategy-position",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(SOURCE, "source", Text),
                Field::req(TOKEN, "token", Address),
                Field::req(DECIDED_BLOCK, "decided_block", U64),
                Field::opt(DECIDED_UNIX_US, "decided_unix_us", U64),
                Field::opt(ELIGIBLE_BLOCK, "eligible_block", U64),
                Field::opt(LEAD_US, "lead_us", I64),
                Field::req(BUYS_BEFORE, "buys_before", U64),
                Field::req(BUYS_IN_ELIGIBLE, "buys_in_eligible", U64),
                Field::opt(FIRST_BUY_GAP_BLOCKS, "first_buy_gap_blocks", U64),
                Field::opt(TIME_LOWER_BOUND, "time_lower_bound", Bool),
            ],
        };
    }

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.strategy-replay.v1",
        record_type: 62,
        schema_version: 1,
        fields: &[
            Field::req(RUN_ID, "run_id", Text),
            Field::req(CHAIN_ID, "chain_id", U64),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(VERDICT, "verdict", Text),
            Field::rep(REASONS, "reasons", Text),
            Field::req(RECORDS, "records", U64),
            Field::req(STARTS, "starts", U64),
            Field::req(JOURNALED, "journaled", U64),
            Field::req(REPLAYED, "replayed", U64),
            Field::opt(FIRST_DIFFERENCE, "first_difference", U64),
            Field::opt(CONFIG_MATCH, "config_match", Bool),
            Field::req(LAUNCHES, "launches", U64),
            Field::req(PASSED, "passed", U64),
            Field::req(REJECTED, "rejected", U64),
            Field::req(DATA_INCOMPLETE, "data_incomplete", U64),
            Field::req(NOT_TRACKED, "not_tracked", U64),
            Field::req(SLOT_LIMIT, "slot_limit", U64),
            Field::req(CREATOR_SOLD, "creator_sold", U64),
            Field::req(ENTERED, "entered", U64),
            Field::req(CANCELLED, "cancelled", U64),
            Field::req(EXITS_CREATOR_SELL, "exits_creator_sell", U64),
            Field::req(EXITS_TARGET, "exits_target", U64),
            Field::req(EXITS_STOP, "exits_stop", U64),
            Field::req(EXITS_TIMEOUT, "exits_timeout", U64),
            Field::req(GAINS, "gains", U64),
            Field::req(LOSSES, "losses", U64),
            Field::req(GAIN_WEI, "gain_wei", U256),
            Field::req(LOSS_WEI, "loss_wei", U256),
            Field::req(UNRESOLVED, "unresolved", U64),
            Field::req(TRADES, "trades", U64),
            Field::req(TERMS, "terms", U64),
            Field::req(MIGRATIONS, "migrations", U64),
            Field::req(INCOMPLETE, "incomplete", U64),
        ],
    };

    pub const V2: Schema = Schema {
        name: "darkbot.bsc.strategy-replay.v1",
        record_type: 62,
        schema_version: 2,
        fields: &[
            Field::req(RUN_ID, "run_id", Text),
            Field::req(CHAIN_ID, "chain_id", U64),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(VERDICT, "verdict", Text),
            Field::rep(REASONS, "reasons", Text),
            Field::req(RECORDS, "records", U64),
            Field::req(STARTS, "starts", U64),
            Field::req(JOURNALED, "journaled", U64),
            Field::req(REPLAYED, "replayed", U64),
            Field::opt(FIRST_DIFFERENCE, "first_difference", U64),
            Field::opt(CONFIG_MATCH, "config_match", Bool),
            Field::req(LAUNCHES, "launches", U64),
            Field::req(PASSED, "passed", U64),
            Field::req(REJECTED, "rejected", U64),
            Field::req(DATA_INCOMPLETE, "data_incomplete", U64),
            Field::req(NOT_TRACKED, "not_tracked", U64),
            Field::req(SLOT_LIMIT, "slot_limit", U64),
            Field::req(CREATOR_SOLD, "creator_sold", U64),
            Field::req(ENTERED, "entered", U64),
            Field::req(CANCELLED, "cancelled", U64),
            Field::req(EXITS_CREATOR_SELL, "exits_creator_sell", U64),
            Field::req(EXITS_TARGET, "exits_target", U64),
            Field::req(EXITS_STOP, "exits_stop", U64),
            Field::req(EXITS_TIMEOUT, "exits_timeout", U64),
            Field::req(GAINS, "gains", U64),
            Field::req(LOSSES, "losses", U64),
            Field::req(GAIN_WEI, "gain_wei", U256),
            Field::req(LOSS_WEI, "loss_wei", U256),
            Field::req(UNRESOLVED, "unresolved", U64),
            Field::req(TRADES, "trades", U64),
            Field::req(TERMS, "terms", U64),
            Field::req(MIGRATIONS, "migrations", U64),
            Field::req(INCOMPLETE, "incomplete", U64),
            Field::req(OPEN_AT_END, "open_at_end", U64),
        ],
    };

    pub const V3: Schema = Schema {
        name: "darkbot.bsc.strategy-replay.v1",
        record_type: 62,
        schema_version: 3,
        fields: &[
            Field::req(RUN_ID, "run_id", Text),
            Field::req(CHAIN_ID, "chain_id", U64),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(VERDICT, "verdict", Text),
            Field::rep(REASONS, "reasons", Text),
            Field::req(RECORDS, "records", U64),
            Field::req(STARTS, "starts", U64),
            Field::req(JOURNALED, "journaled", U64),
            Field::req(REPLAYED, "replayed", U64),
            Field::opt(FIRST_DIFFERENCE, "first_difference", U64),
            Field::opt(CONFIG_MATCH, "config_match", Bool),
            Field::req(LAUNCHES, "launches", U64),
            Field::req(PASSED, "passed", U64),
            Field::req(REJECTED, "rejected", U64),
            Field::req(DATA_INCOMPLETE, "data_incomplete", U64),
            Field::req(NOT_TRACKED, "not_tracked", U64),
            Field::req(SLOT_LIMIT, "slot_limit", U64),
            Field::req(CREATOR_SOLD, "creator_sold", U64),
            Field::req(ENTERED, "entered", U64),
            Field::req(CANCELLED, "cancelled", U64),
            Field::req(EXITS_CREATOR_SELL, "exits_creator_sell", U64),
            Field::req(EXITS_TARGET, "exits_target", U64),
            Field::req(EXITS_STOP, "exits_stop", U64),
            Field::req(EXITS_TIMEOUT, "exits_timeout", U64),
            Field::req(GAINS, "gains", U64),
            Field::req(LOSSES, "losses", U64),
            Field::req(GAIN_WEI, "gain_wei", U256),
            Field::req(LOSS_WEI, "loss_wei", U256),
            Field::req(UNRESOLVED, "unresolved", U64),
            Field::req(TRADES, "trades", U64),
            Field::req(TERMS, "terms", U64),
            Field::req(MIGRATIONS, "migrations", U64),
            Field::req(INCOMPLETE, "incomplete", U64),
            Field::req(OPEN_AT_END, "open_at_end", U64),
            Field::nested(
                POSITIONS,
                "positions",
                Presence::Repeated,
                &position::SCHEMA,
            ),
            Field::req(POSITIONS_NEXT_BLOCK, "positions_next_block", U64),
            Field::req(POSITIONS_CONTESTED, "positions_contested", U64),
            Field::req(POSITIONS_UNKNOWN, "positions_unknown", U64),
        ],
    };

    /// A replay record of either version.
    pub fn decode_replay(bytes: &[u8]) -> Result<crate::Record, crate::RecordError> {
        crate::decode(&V3, bytes)
            .or_else(|_| crate::decode(&V2, bytes))
            .or_else(|_| crate::decode(&SCHEMA, bytes))
    }
}

pub const ALL: &[&Schema] = &[
    &source_modes::SCHEMA,
    &source_modes::SCHEMA_V4,
    &source_modes::SCHEMA_V3,
    &source_modes::SCHEMA_V2,
    &source_modes::SCHEMA_V1,
    &source_mode_receipt::SCHEMA,
    &buy_attempt_slot::SCHEMA,
    &buy_attempt_total_slot::SCHEMA,
    &live_spend::SCHEMA,
    &live_ledger::SCHEMA,
    &live_qualification::SCHEMA,
    &runtime_status::SCHEMA,
    &strategy_replay::SCHEMA,
    &strategy_replay::V2,
    &strategy_replay::V3,
];
