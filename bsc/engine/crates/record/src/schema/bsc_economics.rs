//! Phase G economics records: the versioned economics snapshot (Economics
//! gate §15: the model, its policies, the bound sources and evidence, the
//! explicit support of every observed scope) and the local quote latency
//! measured against it (§16).

use crate::{Field, Kind::*, Presence, Schema};

pub mod economics_snapshot {
    use super::*;

    /// One bound file (a model source or its evidence) and its bytes' hash.
    pub mod file {
        use super::*;

        pub const PATH: u16 = 1;
        pub const SHA256: u16 = 2;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.economics-snapshot.file",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(PATH, "path", Text),
                Field::req(SHA256, "sha256", B256),
            ],
        };
    }

    /// One venue/version/mode/path: PRICED, COSTED or UNSUPPORTED, with the
    /// rule or the reason.
    pub mod scope {
        use super::*;

        pub const VENUE: u16 = 1;
        pub const SURFACE: u16 = 2;
        pub const PATH: u16 = 3;
        pub const SUPPORT: u16 = 4;
        pub const RULE: u16 = 5;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.economics-snapshot.scope",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(VENUE, "venue", Text),
                Field::req(SURFACE, "surface", Text),
                Field::req(PATH, "path", Text),
                Field::req(SUPPORT, "support", Text),
                Field::req(RULE, "rule", Text),
            ],
        };
    }

    pub mod policy {
        use super::*;

        pub const NAME: u16 = 1;
        pub const RULE: u16 = 2;

        pub const SCHEMA: Schema = Schema {
            name: "darkbot.bsc.economics-snapshot.policy",
            record_type: 0,
            schema_version: 1,
            fields: &[
                Field::req(NAME, "name", Text),
                Field::req(RULE, "rule", Text),
            ],
        };
    }

    /// The sha256 of the snapshot's canonical manifest: its identity.
    pub const SNAPSHOT_SHA256: u16 = 1;
    pub const CHAIN_ID: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    pub const MODEL_VERSION: u16 = 5;
    pub const GIT_HEAD: u16 = 6;
    pub const FILES: u16 = 7;
    pub const SCOPES: u16 = 8;
    pub const POLICIES: u16 = 9;
    pub const NON_CLAIMS: u16 = 10;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.economics-snapshot.v1",
        record_type: 60,
        schema_version: 1,
        fields: &[
            Field::req(SNAPSHOT_SHA256, "snapshot_sha256", B256),
            Field::req(CHAIN_ID, "chain_id", U64),
            Field::req(PROVENANCE_TOOL, "_provenance.tool", Text),
            Field::req(
                PROVENANCE_OBSERVED_UNIX_MS,
                "_provenance.observed_unix_ms",
                U64,
            ),
            Field::req(MODEL_VERSION, "model_version", Text),
            Field::req(GIT_HEAD, "git_head", Text),
            Field::nested(FILES, "files", Presence::Repeated, &file::SCHEMA),
            Field::nested(SCOPES, "scopes", Presence::Repeated, &scope::SCHEMA),
            Field::nested(POLICIES, "policies", Presence::Repeated, &policy::SCHEMA),
            Field::rep(NON_CLAIMS, "non_claims", Text),
        ],
    };
}

/// The local deterministic quote's latency on this machine (§16): the same
/// stage statistics as the ingest latency, against one economics snapshot.
pub mod quote_latency {
    use super::*;
    use crate::schema::bsc_capture::ingest_latency::stage;

    pub const HOST: u16 = 1;
    pub const BUILD: u16 = 2;
    pub const PROVENANCE_TOOL: u16 = crate::schema::PROVENANCE_TOOL;
    pub const PROVENANCE_OBSERVED_UNIX_MS: u16 = crate::schema::PROVENANCE_OBSERVED_UNIX_MS;
    pub const GIT_HEAD: u16 = 5;
    pub const WARMUP: u16 = 6;
    pub const STAGES: u16 = 7;
    /// Two clock reads back to back: the floor of every sample.
    pub const CLOCK_OVERHEAD_P50_NS: u16 = 8;
    /// The economics snapshot whose model was measured.
    pub const SNAPSHOT_SHA256: u16 = 9;
    pub const NON_CLAIMS: u16 = 10;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.bsc.quote-latency.v1",
        record_type: 61,
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
            Field::req(WARMUP, "warmup", U64),
            Field::nested(STAGES, "stages", Presence::Repeated, &stage::SCHEMA),
            Field::req(CLOCK_OVERHEAD_P50_NS, "clock_overhead_p50_ns", U64),
            Field::req(SNAPSHOT_SHA256, "snapshot_sha256", B256),
            Field::rep(NON_CLAIMS, "non_claims", Text),
        ],
    };
}

pub const ALL: &[&Schema] = &[&economics_snapshot::SCHEMA, &quote_latency::SCHEMA];
