//! C3 local execution measurement (PHASENPLAN §11, master plan §13 C3) of
//! the production path in PAPER mode on one host: per-stage statistics of
//! `T_ingress → T_shadow_sink` (never `T_send`), the cold first sample,
//! allocations per BUY and the raw totals. Written by the release harness
//! `execution/tests/c3_paper_latency.rs`; nothing is signed with a real key
//! or sent.

use super::c1_qualification::latency_stats;
use crate::{Field, Kind::*, Presence, Schema};

pub mod c3_latency {
    use super::*;

    pub const HOST: u16 = 1;
    pub const BUILD: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    pub const GIT_HEAD: u16 = 5;
    /// Launches measured, after `WARMUP` unmeasured ones.
    pub const LAUNCHES: u16 = 6;
    pub const WARMUP: u16 = 7;
    /// One statistics record per stage; `total` is
    /// `T_ingress → T_shadow_sink`.
    pub const STAGES: u16 = 8;
    /// The first launch after arming (cold caches), its total.
    pub const COLD_TOTAL_NS: u16 = 9;
    /// Heap allocations of one BUY's measured region.
    pub const ALLOCATIONS_MIN: u16 = 10;
    pub const ALLOCATIONS_MAX: u16 = 11;
    /// The measured totals in launch order, `u64` little endian each.
    pub const TOTALS_NS: u16 = 12;
    /// Launches the transport start deadline refused (counted, not
    /// measured).
    pub const DEADLINE_MISSES: u16 = 13;
    pub const NON_CLAIMS: u16 = 14;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.c3.paper-latency.v1",
        record_type: 54,
        schema_version: 1,
        fields: &[
            Field::req(HOST, "host", Text),
            Field::req(BUILD, "build", Text),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::opt(GIT_HEAD, "git_head", Text),
            Field::req(LAUNCHES, "launches", U64),
            Field::req(WARMUP, "warmup", U64),
            Field::nested(STAGES, "stages", Presence::Repeated, &latency_stats::SCHEMA),
            Field::req(COLD_TOTAL_NS, "cold_total_ns", U64),
            Field::req(ALLOCATIONS_MIN, "allocations_min", U64),
            Field::req(ALLOCATIONS_MAX, "allocations_max", U64),
            Field::req(TOTALS_NS, "totals_ns", Bytes).max(8 * 65_536),
            Field::req(DEADLINE_MISSES, "deadline_misses", U64),
            Field::rep(NON_CLAIMS, "non_claims", Text),
        ],
    };
}

pub const ALL: &[&Schema] = &[&c3_latency::SCHEMA];

#[cfg(test)]
mod tests {
    use super::c3_latency as c;
    use super::latency_stats as s;
    use crate::{Draft, Value, decode};

    #[test]
    fn a_c3_latency_record_round_trips() {
        let mut stage = Draft::new(&s::SCHEMA);
        stage.set(s::SEGMENT, "total").set(s::SAMPLES, 2u64);
        let mut d = Draft::new(&c::SCHEMA);
        d.set(c::HOST, "test")
            .set(c::BUILD, "release")
            .set(c::PROVENANCE_TOOL, "c3_paper_latency")
            .set(c::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(c::LAUNCHES, 2u64)
            .set(c::WARMUP, 1u64)
            .push(c::STAGES, stage.into_nested())
            .set(c::COLD_TOTAL_NS, 9u64)
            .set(c::ALLOCATIONS_MIN, 3u64)
            .set(c::ALLOCATIONS_MAX, 4u64)
            .set(
                c::TOTALS_NS,
                Value::Bytes([5u64, 6].iter().flat_map(|v| v.to_le_bytes()).collect()),
            )
            .set(c::DEADLINE_MISSES, 0u64)
            .push(c::NON_CLAIMS, "NOT_T_SEND");
        let bytes = d.encode().unwrap();
        assert_eq!(
            decode(&c::SCHEMA, &bytes)
                .unwrap()
                .u64(c::LAUNCHES)
                .unwrap(),
            2
        );
        assert!(crate::schema::decode_any(&bytes).is_ok());
    }
}
