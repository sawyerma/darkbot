//! Offline wiring fixtures only. Never a real chain/requalification attestation.
use std::path::Path;
use darkbot_bsc_execution::acceptance_contract::{FROZEN_DIR, REQUAL_CHECKS, REQUAL_STATUS, DRY_RECORD};
use darkbot_bsc_record::{Draft, Value, sha256};
use darkbot_bsc_record::schema::{g06a::requalification as r, PROVENANCE_TOOL, PROVENANCE_OBSERVED_UNIX_MS};
pub fn fixture(root: &Path, dir: &str, policy: &str, repo: &Path) {
    std::fs::copy(repo.join(DRY_RECORD), root.join(DRY_RECORD)).unwrap();
    std::fs::write(root.join(dir).join("native_quote.dbrec"), b"offline-test quote stand-in; wiring only").unwrap();
    refresh(root, dir, policy);
}
pub fn refresh(root: &Path, dir: &str, policy: &str) {
    let mut d = Draft::new(&r::SCHEMA);
    d.set(PROVENANCE_TOOL, "g06a_requal_check").set(PROVENANCE_OBSERVED_UNIX_MS, 1u64)
        .set(r::VERDICT, "PASS").set(r::REQUAL_DIR, dir).set(r::FROZEN_DIR, FROZEN_DIR);
    for name in REQUAL_CHECKS {
        let mut row = Draft::new(&r::check::SCHEMA);
        row.set(r::check::NAME, *name).set(r::check::STATUS, "PASS").set(r::check::DETAIL, "offline-test synthetic wiring fixture; no chain authority");
        d.push(r::CHECKS, row.into_nested());
    }
    for name in ["preflight.dbrec", "native_quote.dbrec", "economics_crosscheck.dbrec", "local_simulation.dbrec", "dry_result.dbrec", "portal_upgrade.dbrec", policy] {
        let path = if name == policy { root.join(name) } else { root.join(dir).join(name) };
        let mut row = Draft::new(&r::bound::SCHEMA);
        row.set(r::bound::NAME, name).set(r::bound::SHA256, Value::B256(sha256(&std::fs::read(path).unwrap())));
        d.push(r::RECORDS, row.into_nested());
    }
    std::fs::write(root.join(dir).join(REQUAL_STATUS), d.encode().unwrap()).unwrap();
}
