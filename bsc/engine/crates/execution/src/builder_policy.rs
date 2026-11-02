//! Freshness of the bound builder policy (BSC-004, BSC_Bug_V2 P2-D3/P3-F3):
//! its oldest fetched source must be at most 24 h old at the moment of the
//! check, never in the future. An import carries no fetched sources and is
//! never fresh. Readiness and the G0.7 runner (right before the attempt
//! latch, the authorizing release boundary) apply the same rule; a refresh
//! is a new bound policy, never a silent swap.

use darkbot_bsc_record::schema::g06a::builder_policy as b;
use darkbot_bsc_record::{Record, decode};
use std::path::Path;

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

/// The builder policy `path` under `root`: a native re-determination or
/// the import.
pub fn read(root: &Path, path: &str) -> Result<Record, String> {
    let bytes = std::fs::read(root.join(path)).map_err(|e| e.to_string())?;
    decode(&b::NATIVE, &bytes)
        .or_else(|_| decode(&b::SCHEMA, &bytes))
        .map_err(|e| format!("{e:?}"))
}

/// REV.1.0 R4: the bound builder policy `path` (under `root`) a LIVE arm
/// signing at `gas_price_wei` rests on at `now_unix_ms`: fresh
/// ([`require_fresh`]), both builders verified, the price at its strictest
/// verified floor or above; else why not.
pub fn live_bound(
    root: &Path,
    path: &str,
    gas_price_wei: u64,
    now_unix_ms: u64,
) -> Result<(), String> {
    use crate::sources::{
        LIVE_BUILDER_POLICY_STALE, LIVE_BUILDER_POLICY_UNVERIFIED,
        LIVE_GAS_PRICE_BELOW_BUILDER_FLOOR,
    };
    let unverified = |e: String| format!("{LIVE_BUILDER_POLICY_UNVERIFIED}: {path}: {e}");
    let policy = read(root, path).map_err(unverified)?;
    require_fresh(&policy, now_unix_ms).map_err(|e| format!("{LIVE_BUILDER_POLICY_STALE}: {e}"))?;
    if policy.bool(b::BLOCKRAZOR_VERIFIED) != Ok(true)
        || policy.bool(b::FORTYEIGHTCLUB_VERIFIED) != Ok(true)
    {
        return Err(unverified("its builders are not both verified".into()));
    }
    let floor = policy
        .u256(b::STRICTEST_FLOOR_WEI)
        .map_err(|e| unverified(format!("{e:?}")))?;
    if alloy_primitives::U256::from(gas_price_wei) < alloy_primitives::U256::from_be_bytes(floor) {
        return Err(format!(
            "{LIVE_GAS_PRICE_BELOW_BUILDER_FLOOR}: {gas_price_wei} below {}",
            darkbot_bsc_record::u256_to_decimal(&floor)
        ));
    }
    Ok(())
}

/// The tests' bound builder policy: written by the LIVE fixtures, fresh
/// at their clock.
#[cfg(test)]
pub(crate) const TEST_POLICY: &str = "ev/builder_policy.dbrec";

#[cfg(test)]
pub(crate) fn frozen() -> Record {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../native/evidence/g06a/g07_binary_import_20261016/builder_policy.dbrec");
    decode(&b::SCHEMA, &std::fs::read(path).unwrap()).unwrap()
}

/// A native policy from the frozen import with sources fetched at `ms`.
#[cfg(test)]
pub(crate) fn native(fetched_ms: &[u64]) -> Record {
    decode(&b::NATIVE, &native_bytes(fetched_ms)).unwrap()
}

/// [`native`]'s bytes.
#[cfg(test)]
pub(crate) fn native_bytes(fetched_ms: &[u64]) -> Vec<u8> {
    native_draft(fetched_ms).encode().unwrap()
}

/// [`native`] as a draft.
#[cfg(test)]
pub(crate) fn native_draft(fetched_ms: &[u64]) -> darkbot_bsc_record::Draft {
    use darkbot_bsc_record::schema::{PROVENANCE_OBSERVED_UNIX_MS, PROVENANCE_TOOL};
    use darkbot_bsc_record::{Draft, Value};
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
    d
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// R4: a LIVE BUY's bound builder policy: fresh, both builders
    /// verified, the signed gas price at its strictest floor or above.
    #[test]
    fn a_live_buy_wants_a_fresh_verified_builder_policy_at_its_floor() {
        use crate::sources::{
            LIVE_BUILDER_POLICY_STALE, LIVE_BUILDER_POLICY_UNVERIFIED,
            LIVE_GAS_PRICE_BELOW_BUILDER_FLOOR,
        };
        use darkbot_bsc_record::{Value, u256_from_u128};
        let root = std::env::temp_dir().join(format!("bsc-builder-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let t = 1_800_000_000u64;
        let floor = 50_000_000u64;
        let write = |d: &darkbot_bsc_record::Draft| {
            std::fs::write(root.join("p.dbrec"), d.encode().unwrap()).unwrap()
        };
        let bound = |gas: u64, now_s: u64| live_bound(&root, "p.dbrec", gas, now_s * 1000);
        let refused = |r: Result<(), String>, why: &str| {
            assert!(r.as_ref().is_err_and(|e| e.starts_with(why)), "{r:?}")
        };
        refused(bound(floor, t), LIVE_BUILDER_POLICY_UNVERIFIED);
        let mut d = native_draft(&[t * 1000]);
        d.set(
            b::STRICTEST_FLOOR_WEI,
            Value::U256(u256_from_u128(floor.into())),
        );
        write(&d);
        assert_eq!(bound(floor, t + MAX_AGE_S), Ok(()));
        refused(bound(floor - 1, t), LIVE_GAS_PRICE_BELOW_BUILDER_FLOOR);
        refused(bound(floor, t + MAX_AGE_S + 1), LIVE_BUILDER_POLICY_STALE);
        for verified in [b::BLOCKRAZOR_VERIFIED, b::FORTYEIGHTCLUB_VERIFIED] {
            let mut one = native_draft(&[t * 1000]);
            one.set(verified, false);
            write(&one);
            refused(bound(u64::MAX, t), LIVE_BUILDER_POLICY_UNVERIFIED);
        }
        std::fs::copy(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
                "../../../native/evidence/g06a/g07_binary_import_20261016/builder_policy.dbrec",
            ),
            root.join("p.dbrec"),
        )
        .unwrap();
        refused(bound(u64::MAX, t), LIVE_BUILDER_POLICY_STALE);
        std::fs::write(root.join("p.dbrec"), b"not a record").unwrap();
        refused(bound(u64::MAX, t), LIVE_BUILDER_POLICY_UNVERIFIED);
        std::fs::remove_dir_all(root).unwrap();
    }
}
