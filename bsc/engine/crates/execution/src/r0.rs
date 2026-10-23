//! The intrinsic consistency of an R0 Portal-upgrade coverage record (plan
//! v9.1 D3), offline: its header chain starts right after the frozen DRY's
//! block (anchored at that block's hash), covers a contiguous range with one
//! verified header per block, and lists a gapless, ordered chain of
//! successful upgrades inside the range from the frozen implementation to
//! its stated current one. Shared by the PREPARE → SIGN handoff and
//! `g06a_requal_check`; the end anchors (preflight block, hash and
//! implementation) are the caller's. Hashes alone do not make a record valid.

use darkbot_bsc_record::schema::g06a::{portal_upgrade as up, preflight as pf};
use darkbot_bsc_record::{Record, RecordError};

/// Every inconsistency of `scan` against the frozen DRY's preflight `frozen`;
/// empty when the record is internally coherent.
pub fn chain_problems(scan: &Record, frozen: &Record) -> Vec<String> {
    problems(scan, frozen).unwrap_or_else(|e| vec![format!("unreadable coverage record: {e}")])
}

fn problems(scan: &Record, frozen: &Record) -> Result<Vec<String>, RecordError> {
    let mut p = Vec::new();
    if scan.text(up::COVERAGE_METHOD)? != up::METHOD {
        p.push(format!("coverage method is not {}", up::METHOD));
    }
    if scan.address(up::PROXY)? != frozen.address(pf::PORTAL)? {
        p.push("scanned proxy is not the frozen Portal".into());
    }
    let head = frozen.u64(pf::LATEST_BLOCK_NUMBER)?;
    let (from, to) = (
        scan.u64(up::SCANNED_FROM_BLOCK)?,
        scan.u64(up::SCANNED_TO_BLOCK)?,
    );
    if from != head + 1 {
        p.push(format!(
            "scan starts at {from}, not right after the frozen head {head}"
        ));
    }
    if scan.b256(up::FIRST_PARENT_HASH)? != frozen.b256(pf::LATEST_BLOCK_HASH)? {
        p.push("the hash chain does not start at the frozen DRY's block hash".into());
    }
    let blocks = (to + 1).saturating_sub(from);
    let headers = scan.u64(up::HEADERS_VERIFIED)?;
    if blocks == 0 || headers != blocks {
        p.push(format!(
            "{headers} verified headers do not cover the {blocks} blocks scanned"
        ));
    }
    let baseline = frozen.address(pf::PORTAL_IMPLEMENTATION)?;
    if scan.address(up::BASELINE_IMPLEMENTATION)? != baseline {
        p.push("baseline implementation is not the frozen DRY's".into());
    }
    // Each upgrade starts at the implementation the previous one left (the
    // first at the frozen one), in strict chain order, inside the range.
    let mut expected_before = baseline;
    let mut previous = None;
    for (index, u) in scan.nested_all(up::UPGRADES)?.iter().enumerate() {
        let block = u.u64(up::upgrade::BLOCK)?;
        let position = (block, u.u64(up::upgrade::LOG_INDEX)?);
        if previous.is_some_and(|before| position <= before) {
            p.push(format!("upgrade {index} is not after the previous one"));
        }
        previous = Some(position);
        if !(from <= block && block <= to) {
            p.push(format!("upgrade block {block} outside the scanned range"));
        }
        if u.u64(up::upgrade::RECEIPT_STATUS)? != 1 {
            p.push(format!("upgrade {index} receipt status is not 1"));
        }
        if u.address(up::upgrade::IMPLEMENTATION)? != u.address(up::upgrade::SLOT_AFTER)? {
            p.push(format!(
                "upgrade {index} event target differs from the slot after it"
            ));
        }
        if u.address(up::upgrade::SLOT_BEFORE)? != expected_before {
            p.push(format!(
                "upgrade {index} does not start at the implementation left by the previous upgrade (the first at the frozen one)"
            ));
        }
        expected_before = u.address(up::upgrade::SLOT_AFTER)?;
    }
    if scan.address(up::CURRENT_IMPLEMENTATION)? != expected_before {
        p.push("the upgrade chain does not end at the current implementation".into());
    }
    Ok(p)
}
