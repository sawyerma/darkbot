//! The native configuration of the BSC runtime (C0/C1, plan §5a.2, order V2
//! §5): the five signal sources, each with the mode the user requested
//! (`OFF | SHADOW | LIVE`) and the mode the native owner made effective
//! (`OFF | SHADOW | BLOCKED`, with its reason). One record per generation;
//! every request leaves one receipt.

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

    pub const SCHEMA: Schema = Schema {
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

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.runtime-status.canonical",
            record_type: 0,
            schema_version: 6,
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

    /// The C3 execution path in PAPER mode (component `BSC_EXEC` effective
    /// SHADOW): from the pending launch to the PAPER sink (`T_shadow_sink`,
    /// never `T_send`).
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

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.runtime-status.exec",
            record_type: 0,
            schema_version: 2,
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
    /// `READ_ONLY`: observation only, no build, signature or send exists.
    pub const MODE: u16 = 10;
    /// Absent when no canonical endpoint is configured (then no denominator).
    pub const CANONICAL: u16 = 11;
    /// How often this record is rewritten; a reader derives staleness from it.
    pub const STATUS_INTERVAL_MS: u16 = 12;
    /// Present while the C2 chain-state owner runs.
    pub const STATE: u16 = 13;
    /// Present while a PAPER execution path is configured.
    pub const EXEC: u16 = 14;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.runtime-status.v1",
        record_type: 40,
        schema_version: 8,
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
        ],
    };
}

pub const ALL: &[&Schema] = &[
    &source_modes::SCHEMA,
    &source_modes::SCHEMA_V1,
    &source_mode_receipt::SCHEMA,
    &runtime_status::SCHEMA,
];
