//! G0.7 self-launch candidates (plan v8/v9): profile A keeps the frozen,
//! publicly known salt, profile B a fresh CSPRNG salt. The public identity
//! never carries the salt; the salt record is private local state and is
//! deliberately absent from `schema::all()`, so generic decoders and views
//! cannot render it.

use crate::{Field, Kind::*, Record, RecordError, Schema};

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

/// A selection request from the runtime (gateway command). Never
/// authority: the native store applies or rejects it.
pub mod request {
    use super::*;

    pub const REQUEST_ID: u16 = 1;
    pub const PROFILE: u16 = 2;
    pub const EXPECTED_GENERATION: u16 = 3;
    pub const TARGET_CANDIDATE_ID: u16 = 4;
    pub const REQUESTED_UNIX_MS: u16 = 5;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g07.self-launch-candidate.request",
        record_type: 30,
        schema_version: 1,
        fields: &[
            Field::req(REQUEST_ID, "request_id", Text),
            Field::req(PROFILE, "profile", Text),
            Field::req(EXPECTED_GENERATION, "expected_generation", U64),
            Field::req(TARGET_CANDIDATE_ID, "target_candidate_id", B256),
            Field::req(REQUESTED_UNIX_MS, "requested_unix_ms", U64),
        ],
    };
}

/// The native decision on one request.
pub mod request_result {
    use super::*;

    pub const REQUEST_ID: u16 = 1;
    /// `APPLIED` or `REJECTED`.
    pub const OUTCOME: u16 = 2;
    pub const DETAIL: u16 = 3;
    pub const GENERATION: u16 = 4;
    pub const DECIDED_UNIX_MS: u16 = 5;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g07.self-launch-candidate.request-result",
        record_type: 31,
        schema_version: 1,
        fields: &[
            Field::req(REQUEST_ID, "request_id", Text),
            Field::req(OUTCOME, "outcome", Text),
            Field::req(DETAIL, "detail", Text),
            Field::opt(GENERATION, "generation", U64),
            Field::req(DECIDED_UNIX_MS, "decided_unix_ms", U64),
        ],
    };
}

/// A candidate bound to one PASSing requalification of exactly its token,
/// launcher and signed transactions, against the deployment of that run.
/// Not economic approval; a later deployment needs a new requalification.
pub mod qualification {
    use super::*;

    pub const CANDIDATE_ID: u16 = 1;
    pub const PROFILE: u16 = 2;
    pub const REQUAL_RECORD_SHA256: u16 = 3;
    pub const DRY_RECORD_SHA256: u16 = 4;
    pub const PREFLIGHT_RECORD_SHA256: u16 = 5;
    pub const PORTAL_VERSION: u16 = 6;
    pub const PORTAL_IMPLEMENTATION: u16 = 7;
    pub const PORTAL_IMPLEMENTATION_CODE_SHA256: u16 = 8;
    pub const TOKEN: u16 = 9;
    pub const LAUNCH_TX_HASH: u16 = 10;
    pub const BUY_TX_HASH: u16 = 11;
    pub const QUALIFIED_UNIX_MS: u16 = 12;
    pub const SCOPE: u16 = 13;

    pub const SCHEMA: Schema = Schema {
        name: "darkbot.g07.self-launch-candidate.qualification",
        record_type: 32,
        schema_version: 1,
        fields: &[
            Field::req(CANDIDATE_ID, "candidate_id", B256),
            Field::req(PROFILE, "profile", Text),
            Field::req(REQUAL_RECORD_SHA256, "requal_record_sha256", B256),
            Field::req(DRY_RECORD_SHA256, "dry_record_sha256", B256),
            Field::req(PREFLIGHT_RECORD_SHA256, "preflight_record_sha256", B256),
            Field::req(PORTAL_VERSION, "portal_version", Text),
            Field::req(PORTAL_IMPLEMENTATION, "portal_implementation", Address),
            Field::req(
                PORTAL_IMPLEMENTATION_CODE_SHA256,
                "portal_implementation_code_sha256",
                B256,
            ),
            Field::req(TOKEN, "token", Address),
            Field::req(LAUNCH_TX_HASH, "launch_tx_hash", B256),
            Field::req(BUY_TX_HASH, "buy_tx_hash", B256),
            Field::req(QUALIFIED_UNIX_MS, "qualified_unix_ms", U64),
            Field::req(SCOPE, "scope", Text),
        ],
    };
}

/// The decodable public schemas; the salt record is excluded on purpose.
pub const ALL: &[&Schema] = &[
    &candidate::SCHEMA,
    &selection::SCHEMA,
    &request::SCHEMA,
    &request_result::SCHEMA,
    &qualification::SCHEMA,
];

const ID_DOMAIN: &[u8] = b"darkbot.g07.candidate.id.v1";

/// The candidate ID over every public identity field, so it can be
/// recomputed without the salt. `addresses` = portal, Portal
/// implementation, Tax-V3 implementation, launcher, token; `words` = the
/// implementation code hashes, deployment record hash, salt commitment,
/// launch calldata hash, then any evidence binding.
pub fn candidate_id(
    profile: &str,
    chain_id: u64,
    addresses: [[u8; 20]; 5],
    portal_version: &str,
    words: &[[u8; 32]],
) -> [u8; 32] {
    let mut p = ID_DOMAIN.to_vec();
    p.extend_from_slice(profile.as_bytes());
    p.extend_from_slice(&chain_id.to_be_bytes());
    for a in addresses {
        p.extend_from_slice(&a);
    }
    p.extend_from_slice(portal_version.as_bytes());
    for w in words {
        p.extend_from_slice(w);
    }
    crate::sha256(&p)
}

/// [`candidate_id`] of a decoded public candidate record.
pub fn candidate_id_of(r: &Record) -> Result<[u8; 32], RecordError> {
    use candidate as c;
    let mut words = vec![
        r.b256(c::PORTAL_IMPLEMENTATION_CODE_SHA256)?,
        r.b256(c::TAX_V3_IMPLEMENTATION_CODE_SHA256)?,
        r.b256(c::DEPLOYMENT_RECORD_SHA256)?,
        r.b256(c::SALT_COMMITMENT)?,
        r.b256(c::LAUNCH_CALLDATA_SHA256)?,
    ];
    for tag in [
        c::ECONOMICS_RECORD_SHA256,
        c::DRY_RECORD_SHA256,
        c::LAUNCH_TX_HASH,
        c::BUY_TX_HASH,
    ] {
        words.extend(r.opt_b256(tag)?);
    }
    Ok(candidate_id(
        r.text(c::PROFILE)?,
        r.u64(c::CHAIN_ID)?,
        [
            r.address(c::PORTAL)?,
            r.address(c::PORTAL_IMPLEMENTATION)?,
            r.address(c::TAX_V3_IMPLEMENTATION)?,
            r.address(c::LAUNCHER)?,
            r.address(c::TOKEN)?,
        ],
        r.text(c::PORTAL_VERSION)?,
        &words,
    ))
}

/// The current selection, as the readiness binding needs it.
pub struct SelectionView<'a> {
    pub generation: u64,
    pub profile: &'a str,
    pub candidate_id: [u8; 32],
    pub activated_unix_ms: u64,
}

/// The detail a PASSing readiness CANDIDATE_SELECTION check carries.
pub fn readiness_binding(selection: &SelectionView<'_>) -> String {
    format!(
        "candidate=0x{} profile={} generation={}",
        crate::hex(&selection.candidate_id),
        selection.profile,
        selection.generation
    )
}

/// The readiness of one candidate: it counts only with a PASSing
/// CANDIDATE_SELECTION for exactly the current selection of this candidate,
/// observed after that switch; otherwise `STALE`, `NOT_SELECTED` or
/// `UNKNOWN`.
pub fn readiness_for(
    readiness: &Record,
    selection: Option<&SelectionView<'_>>,
    candidate_id: &[u8; 32],
) -> &'static str {
    use super::g07::readiness as rr;
    let Some(selection) = selection else {
        return "UNKNOWN";
    };
    if &selection.candidate_id != candidate_id {
        return "NOT_SELECTED";
    }
    let binding = readiness_binding(selection);
    let bound = readiness
        .u64(rr::OBSERVED_UNIX_MS)
        .is_ok_and(|t| t >= selection.activated_unix_ms)
        && readiness.nested_all(rr::CHECKS).is_ok_and(|checks| {
            checks.iter().any(|row| {
                row.text(rr::check::NAME).ok() == Some("CANDIDATE_SELECTION")
                    && row.text(rr::check::STATUS).ok() == Some("PASS")
                    && row.text(rr::check::DETAIL).ok() == Some(binding.as_str())
            })
        });
    if !bound {
        return "STALE";
    }
    match readiness.text(rr::STATUS) {
        Ok("PASS_READ_ONLY") => "PASS_READ_ONLY",
        Ok("BLOCKED") => "BLOCKED",
        Ok("STOP") => "STOP",
        _ => "UNKNOWN",
    }
}

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
