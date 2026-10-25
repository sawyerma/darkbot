//! Freshness of the bound builder policy (BSC-004, BSC_Bug_V2 P2-D3/P3-F3):
//! its oldest fetched source must be at most 24 h old at the moment of the
//! check, never in the future. An import carries no fetched sources and is
//! never fresh. Readiness and the G0.7 runner (right before the attempt
//! latch, the authorizing release boundary) apply the same rule; a refresh
//! is a new bound policy, never a silent swap.

use darkbot_bsc_record::Record;
use darkbot_bsc_record::schema::g06a::builder_policy as b;

/// The longest accepted age of the oldest fetched source, in seconds.
pub const MAX_AGE_S: u64 = 86_400;

/// Unix seconds of the oldest fetched source of a native re-determination;
/// `None` for an import or a policy without sources.
pub fn oldest_fetch_s(policy: &Record) -> Option<u64> {
    if policy.schema() != &b::NATIVE {
        return None;
    }
    policy
        .nested_all(b::SOURCES)
        .ok()?
        .iter()
        .map(|s| s.u64(b::source::FETCHED_UNIX_MS).map(|ms| ms / 1000))
        .collect::<Result<Vec<_>, _>>()
        .ok()?
        .into_iter()
        .min()
}

/// Whether `oldest` (unix seconds) is at most [`MAX_AGE_S`] before `now_s`
/// and not after it.
pub fn age_ok(oldest: Option<u64>, now_s: u64) -> bool {
    oldest.is_some_and(|t| now_s.checked_sub(t).is_some_and(|age| age <= MAX_AGE_S))
}

/// The policy's age in seconds when fresh at `now_unix_ms`; else why not.
pub fn require_fresh(policy: &Record, now_unix_ms: u64) -> Result<u64, String> {
    let now_s = now_unix_ms / 1000;
    match oldest_fetch_s(policy) {
        None => Err("the bound builder policy has no fetched sources (import or empty)".into()),
        Some(t) if t > now_s => Err("the bound builder policy was fetched in the future".into()),
        Some(t) if age_ok(Some(t), now_s) => Ok(now_s - t),
        Some(t) => Err(format!(
            "the bound builder policy is {} s old (> {MAX_AGE_S} s); a refresh is a new bound policy",
            now_s - t
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkbot_bsc_record::schema::{PROVENANCE_OBSERVED_UNIX_MS, PROVENANCE_TOOL};
    use darkbot_bsc_record::{Draft, Value, decode};

    fn frozen() -> Record {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../native/evidence/g06a/g07_binary_import_20261016/builder_policy.dbrec");
        decode(&b::SCHEMA, &std::fs::read(path).unwrap()).unwrap()
    }

    /// A native policy from the frozen import with sources fetched at `ms`.
    fn native(fetched_ms: &[u64]) -> Record {
        let import = frozen();
        let mut d = Draft::new(&b::NATIVE);
        for (tag, value) in import.fields() {
            if *tag > 2 && b::NATIVE.field(*tag).is_some() {
                d.push(*tag, value.clone());
            }
        }
        d.set(PROVENANCE_TOOL, "test")
            .set(PROVENANCE_OBSERVED_UNIX_MS, 1u64);
        for ms in fetched_ms {
            let mut s = Draft::new(&b::source::SCHEMA);
            s.set(b::source::URL, "https://example.test/")
                .set(b::source::HTTP_STATUS, 200u64)
                .set(b::source::FETCHED_UNIX_MS, *ms)
                .set(b::source::BODY_SHA256, Value::B256([5; 32]))
                .set(b::source::BODY_BYTES, 1u64)
                .set(b::source::FINDING, "test");
            d.push(b::SOURCES, s.into_nested());
        }
        decode(&b::NATIVE, &d.encode().unwrap()).unwrap()
    }

    #[test]
    fn the_oldest_source_bounds_the_age_and_imports_or_future_fetches_are_never_fresh() {
        let t = 1_800_000_000u64;
        let policy = native(&[(t + 5) * 1000, t * 1000]);
        assert_eq!(
            require_fresh(&policy, (t + MAX_AGE_S) * 1000),
            Ok(MAX_AGE_S)
        );
        assert!(
            require_fresh(&policy, (t + MAX_AGE_S + 1) * 1000)
                .unwrap_err()
                .contains("old")
        );
        assert!(
            require_fresh(&policy, (t - 1) * 1000)
                .unwrap_err()
                .contains("future")
        );
        assert!(
            require_fresh(&native(&[]), t * 1000)
                .unwrap_err()
                .contains("no fetched")
        );
        assert!(
            require_fresh(&frozen(), t * 1000)
                .unwrap_err()
                .contains("no fetched")
        );
        assert!(age_ok(Some(10), 86_410) && !age_ok(Some(10), 86_411) && !age_ok(Some(11), 10));
        assert!(!age_ok(None, 10));
    }
}
