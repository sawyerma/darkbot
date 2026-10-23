//! G0.7 self-launch candidates (plan v8/v9): profile A keeps the frozen,
//! publicly known salt, profile B a fresh CSPRNG salt. The public identity
//! never carries the salt; the salt record is private local state and is
//! deliberately absent from `schema::all()`, so generic decoders and views
//! cannot render it.

use crate::{Field, Kind::*, Schema};

/// The public identity of one candidate.
pub mod candidate {
    use super::*;

    /// `SELF_A` or `SELF_B`.
    pub const PROFILE: u16 = 1;
    pub const CANDIDATE_ID: u16 = 2;
    pub const CHAIN_ID: u16 = 3;
    pub const PORTAL: u16 = 4;
    /// The deployment the address was derived against: the bound preflight.
    pub const PORTAL_VERSION: u16 = 5;
    pub const PORTAL_IMPLEMENTATION: u16 = 6;
    pub const PORTAL_IMPLEMENTATION_CODE_SHA256: u16 = 7;
    pub const TAX_V3_IMPLEMENTATION: u16 = 8;
    pub const TAX_V3_IMPLEMENTATION_CODE_SHA256: u16 = 9;
    pub const DEPLOYMENT_RECORD_SHA256: u16 = 10;
    pub const LAUNCHER: u16 = 11;
    pub const TOKEN: u16 = 12;
    /// SHA-256 over a domain tag and the salt; never the salt itself.
    pub const SALT_COMMITMENT: u16 = 13;
    /// `FROZEN_PUBLIC_ECONOMICS_RECORD` or `OS_CSPRNG`.
    pub const SALT_ORIGIN: u16 = 14;
    pub const LAUNCH_CALLDATA_SHA256: u16 = 15;
    /// Profile A only: the frozen evidence the salt and hashes come from.
    pub const ECONOMICS_RECORD_SHA256: u16 = 16;
    pub const DRY_RECORD_SHA256: u16 = 17;
    pub const LAUNCH_TX_HASH: u16 = 18;
    pub const BUY_TX_HASH: u16 = 19;
    /// `UNQUALIFIED` until a requalification binds this exact candidate.
    pub const QUALIFICATION: u16 = 20;
    pub const PROVENANCE: u16 = 21;
    pub const CREATED_UNIX_MS: u16 = 22;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g07.self-launch-candidate",
        record_type: 27,
        schema_version: 1,
        fields: &[
            Field::req(PROFILE, "profile", Text),
            Field::req(CANDIDATE_ID, "candidate_id", B256),
            Field::req(CHAIN_ID, "chain_id", U64),
            Field::req(PORTAL, "portal", Address),
            Field::req(PORTAL_VERSION, "deployment.portal_version", Text),
            Field::req(
                PORTAL_IMPLEMENTATION,
                "deployment.portal_implementation",
                Address,
            ),
            Field::req(
                PORTAL_IMPLEMENTATION_CODE_SHA256,
                "deployment.portal_implementation_code_sha256",
                B256,
            ),
            Field::req(
                TAX_V3_IMPLEMENTATION,
                "deployment.tax_v3_implementation",
                Address,
            ),
            Field::req(
                TAX_V3_IMPLEMENTATION_CODE_SHA256,
                "deployment.tax_v3_implementation_code_sha256",
                B256,
            ),
            Field::req(DEPLOYMENT_RECORD_SHA256, "deployment.record_sha256", B256),
            Field::req(LAUNCHER, "launcher", Address),
            Field::req(TOKEN, "token", Address),
            Field::req(SALT_COMMITMENT, "salt_commitment", B256),
            Field::req(SALT_ORIGIN, "salt_origin", Text),
            Field::req(LAUNCH_CALLDATA_SHA256, "launch_calldata_sha256", B256),
            Field::opt(
                ECONOMICS_RECORD_SHA256,
                "evidence.economics_record_sha256",
                B256,
            ),
            Field::opt(DRY_RECORD_SHA256, "evidence.dry_record_sha256", B256),
            Field::opt(LAUNCH_TX_HASH, "evidence.launch_tx_hash", B256),
            Field::opt(BUY_TX_HASH, "evidence.buy_tx_hash", B256),
            Field::req(QUALIFICATION, "qualification", Text),
            Field::req(PROVENANCE, "provenance", Text),
            Field::req(CREATED_UNIX_MS, "created_unix_ms", U64),
        ],
    };
}

/// The private salt of one candidate (mode 0600, never in Git or a view).
pub mod secret {
    use super::*;

    pub const CANDIDATE_ID: u16 = 1;
    pub const SALT: u16 = 2;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g07.self-launch-candidate.salt",
        record_type: 28,
        schema_version: 1,
        fields: &[
            Field::req(CANDIDATE_ID, "candidate_id", B256),
            Field::req(SALT, "salt", B256),
        ],
    };
}

/// The selected candidate: one monotonic generation per switch.
pub mod selection {
    use super::*;

    pub const GENERATION: u16 = 1;
    pub const PROFILE: u16 = 2;
    pub const CANDIDATE_ID: u16 = 3;
    pub const PREVIOUS_CANDIDATE_ID: u16 = 4;
    pub const ACTIVATED_UNIX_MS: u16 = 5;
    /// What the switch invalidated for the new candidate.
    pub const INVALIDATED: u16 = 6;
    pub const GUARD_AT_SWITCH: u16 = 7;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g07.self-launch-candidate.selection",
        record_type: 29,
        schema_version: 1,
        fields: &[
            Field::req(GENERATION, "generation", U64),
            Field::req(PROFILE, "profile", Text),
            Field::req(CANDIDATE_ID, "candidate_id", B256),
            Field::opt(PREVIOUS_CANDIDATE_ID, "previous_candidate_id", B256),
            Field::req(ACTIVATED_UNIX_MS, "activated_unix_ms", U64),
            Field::req(INVALIDATED, "invalidated", Text),
            Field::req(GUARD_AT_SWITCH, "guard_at_switch", Text),
        ],
    };
}

/// The decodable public schemas; the salt record is excluded on purpose.
pub const ALL: &[&Schema] = &[&candidate::SCHEMA, &selection::SCHEMA];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_salt_record_is_well_formed_but_never_generically_decodable() {
        secret::SCHEMA.check().unwrap();
        assert!(crate::schema::all().iter().all(|s| {
            s.record_type != secret::SCHEMA.record_type && s.name != secret::SCHEMA.name
        }));
    }
}
