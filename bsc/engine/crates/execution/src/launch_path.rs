//! The launch-path code identity snapshot of a requalification (v9.1 C1/R1).
//!
//! The deployment drift check pins the Portal proxy, its implementation and
//! the Tax-V3 implementation, but the launch and first BUY also execute an
//! EIP-1967 proxy, a slot-0 proxy and storage-selected clone targets whose
//! code can change without any pinned hash changing. The snapshot lists every
//! contract with code at the pinned block that executed in the local EVM run
//! (code SHA-256) and every storage word that selects an executed code
//! address. [`check`] binds it to the preflight of the same run; [`drift`]
//! compares it with the chain at one pinned block, re-read by the caller.

use darkbot_bsc_record::schema::g06a::{launch_path as lp, preflight as pf};
use darkbot_bsc_record::{Record, RecordError, Value, decode};
use std::collections::BTreeSet;

/// EIP-1967 implementation slot (keccak256("eip1967.proxy.implementation") - 1).
pub const EIP1967_IMPLEMENTATION_SLOT: [u8; 32] = [
    0x36, 0x08, 0x94, 0xa1, 0x3b, 0xa1, 0xa3, 0x21, 0x06, 0x67, 0xc8, 0x28, 0x49, 0x2d, 0xb9, 0x8d,
    0xca, 0x3e, 0x20, 0x76, 0xcc, 0x37, 0x35, 0xa9, 0x20, 0xa3, 0xca, 0x50, 0x5d, 0x38, 0x2b, 0xbc,
];
/// A bound on the snapshot size: the traced path has a few dozen entries.
const MAX_ENTRIES: usize = 512;

/// A checked snapshot: what [`drift`] re-verifies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub block_number: u64,
    pub block_hash: [u8; 32],
    pub token: [u8; 20],
    pub code: Vec<([u8; 20], [u8; 32])>,
    pub storage: Vec<([u8; 20], [u8; 32], [u8; 32])>,
}

/// The values of one repeated field, in order.
fn repeated(r: &Record, tag: u16) -> Vec<&Value> {
    r.fields()
        .iter()
        .filter(|(t, _)| *t == tag)
        .map(|(_, v)| v)
        .collect()
}

fn addresses(r: &Record, tag: u16) -> Result<Vec<[u8; 20]>, String> {
    repeated(r, tag)
        .into_iter()
        .map(|v| match v {
            Value::Address(a) => Ok(*a),
            _ => Err("launch path: not an address".to_string()),
        })
        .collect()
}

fn words(r: &Record, tag: u16) -> Result<Vec<[u8; 32]>, String> {
    repeated(r, tag)
        .into_iter()
        .map(|v| match v {
            Value::B256(w) => Ok(*w),
            _ => Err("launch path: not a word".to_string()),
        })
        .collect()
}

fn word_of(address: [u8; 20]) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[12..].copy_from_slice(&address);
    w
}

/// Decodes the snapshot and checks it against the preflight of the same
/// requalification run: same block, Portal, implementation and Tax-V3 code
/// hashes, the Portal's EIP-1967 slot, a successful launch returning the
/// token and a successful BUY, and every storage selector pointing at a
/// listed code address. A missing, stale, foreign or contradictory snapshot
/// fails.
pub fn check(bytes: &[u8], preflight: &Record) -> Result<Snapshot, String> {
    let r = decode(&lp::SCHEMA, bytes).map_err(|e| format!("launch path: {e}"))?;
    let e = |e: RecordError| format!("launch path: {e}");
    let p = |e: RecordError| format!("launch path preflight: {e}");
    if r.text(lp::METHOD).map_err(e)? != lp::TRACE_METHOD {
        return Err("launch path: unknown trace method".into());
    }
    let block_number = r.u64(lp::BLOCK_NUMBER).map_err(e)?;
    let block_hash = r.b256(lp::BLOCK_HASH).map_err(e)?;
    if block_number != preflight.u64(pf::LATEST_BLOCK_NUMBER).map_err(p)?
        || block_hash != preflight.b256(pf::LATEST_BLOCK_HASH).map_err(p)?
    {
        return Err("launch path: not taken at the preflight's block (stale or foreign)".into());
    }
    let portal = r.address(lp::PORTAL).map_err(e)?;
    if portal != preflight.address(pf::PORTAL).map_err(p)? {
        return Err("launch path: another Portal than the preflight's".into());
    }
    let token = r.address(lp::TOKEN).map_err(e)?;
    if !r.bool(lp::LAUNCH_SUCCESS).map_err(e)?
        || !r.bool(lp::BUY_SUCCESS).map_err(e)?
        || r.address(lp::LAUNCH_RETURN_TOKEN).map_err(e)? != token
    {
        return Err("launch path: the traced launch/BUY did not succeed with the token".into());
    }
    let code_addresses = addresses(&r, lp::CODE_ADDRESS)?;
    let code_hashes = words(&r, lp::CODE_SHA256)?;
    if code_hashes.len() != code_addresses.len()
        || repeated(&r, lp::CODE_BYTES).len() != code_addresses.len()
        || repeated(&r, lp::CODE_ROLES).len() != code_addresses.len()
    {
        return Err("launch path: code lists of different length".into());
    }
    let mut known = BTreeSet::new();
    let mut code = Vec::new();
    for (address, sha) in code_addresses.into_iter().zip(code_hashes) {
        if !known.insert(address) {
            return Err("launch path: duplicate code entry".into());
        }
        code.push((address, sha));
    }
    let owners = addresses(&r, lp::STORAGE_ADDRESS)?;
    let slot_list = words(&r, lp::STORAGE_SLOT)?;
    let values = words(&r, lp::STORAGE_VALUE)?;
    let selected = addresses(&r, lp::STORAGE_SELECTS)?;
    if [slot_list.len(), values.len(), selected.len()] != [owners.len(); 3] {
        return Err("launch path: storage lists of different length".into());
    }
    let mut slots = BTreeSet::new();
    let mut storage = Vec::new();
    for i in 0..owners.len() {
        let (address, slot, value, selects) = (owners[i], slot_list[i], values[i], selected[i]);
        if !slots.insert((address, slot))
            || !known.contains(&address)
            || !known.contains(&selects)
            || value != word_of(selects)
        {
            return Err("launch path: storage selector without its code entries".into());
        }
        storage.push((address, slot, value));
    }
    if code.is_empty() || code.len() + storage.len() > MAX_ENTRIES {
        return Err("launch path: empty or oversized snapshot".into());
    }
    let implementation = preflight.address(pf::PORTAL_IMPLEMENTATION).map_err(p)?;
    let required_code = [
        (portal, preflight.b256(pf::PORTAL_CODE_SHA256).map_err(p)?),
        (
            implementation,
            preflight
                .b256(pf::PORTAL_IMPLEMENTATION_CODE_SHA256)
                .map_err(p)?,
        ),
        (
            preflight.address(pf::TAX_V3_IMPL).map_err(p)?,
            preflight.b256(pf::TAX_V3_IMPL_CODE_SHA256).map_err(p)?,
        ),
    ];
    for entry in required_code {
        if !code.contains(&entry) {
            return Err(
                "launch path: Portal, implementation or Tax-V3 code differs from the preflight"
                    .into(),
            );
        }
    }
    if !storage.contains(&(portal, EIP1967_IMPLEMENTATION_SLOT, word_of(implementation))) {
        return Err("launch path: the Portal implementation slot is not bound".into());
    }
    Ok(Snapshot {
        block_number,
        block_hash,
        token,
        code,
        storage,
    })
}

/// Every difference of the chain from `snapshot`: the caller re-reads each
/// code hash and storage word at one pinned block. A read error fails; an
/// empty result means the launch-path identity is unchanged.
pub fn drift(
    snapshot: &Snapshot,
    code_sha256_at: &mut dyn FnMut([u8; 20]) -> Result<[u8; 32], String>,
    storage_at: &mut dyn FnMut([u8; 20], [u8; 32]) -> Result<[u8; 32], String>,
) -> Result<Vec<String>, String> {
    let hex = |b: &[u8]| darkbot_bsc_record::hex(b);
    let mut differences = Vec::new();
    for (address, sha) in &snapshot.code {
        if code_sha256_at(*address)? != *sha {
            differences.push(format!("code of 0x{} changed", hex(address)));
        }
    }
    for (address, slot, value) in &snapshot.storage {
        let now = storage_at(*address, *slot)?;
        if now != *value {
            differences.push(format!(
                "0x{} slot 0x{} now selects 0x{}",
                hex(address),
                hex(slot),
                hex(&now[12..])
            ));
        }
    }
    Ok(differences)
}

/// A snapshot the acceptance guard read at its v5-bound path, matched to
/// its bound SHA-256 and checked against the bound preflight. Only the guard
/// creates it.
///
/// ```compile_fail
/// let _ = darkbot_bsc_execution::launch_path::Bound { record_sha256: [0; 32], snapshot: todo!() };
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bound {
    record_sha256: [u8; 32],
    snapshot: Snapshot,
}

impl Bound {
    pub(crate) fn new(record_sha256: [u8; 32], snapshot: Snapshot) -> Self {
        Self {
            record_sha256,
            snapshot,
        }
    }

    pub fn record_sha256(&self) -> [u8; 32] {
        self.record_sha256
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
}

/// Proof that a guard-bound snapshot showed no drift at one pinned block.
/// Only [`verify_current`] creates it.
///
/// ```compile_fail
/// let _ = darkbot_bsc_execution::launch_path::CurrentProof { record_sha256: [0; 32], block_hash: [0; 32] };
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrentProof {
    record_sha256: [u8; 32],
    block_hash: [u8; 32],
}

impl CurrentProof {
    pub fn record_sha256(&self) -> [u8; 32] {
        self.record_sha256
    }

    pub fn block_hash(&self) -> [u8; 32] {
        self.block_hash
    }
}

/// The current chain against a bound snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Current {
    Unchanged(CurrentProof),
    Drift(Vec<String>),
}

/// [`drift`] of a guard-bound snapshot, re-read by the caller at the block
/// `block_hash`; no drift yields the proof the live guard requires.
pub fn verify_current(
    bound: &Bound,
    block_hash: [u8; 32],
    code_sha256_at: &mut dyn FnMut([u8; 20]) -> Result<[u8; 32], String>,
    storage_at: &mut dyn FnMut([u8; 20], [u8; 32]) -> Result<[u8; 32], String>,
) -> Result<Current, String> {
    let differences = drift(&bound.snapshot, code_sha256_at, storage_at)?;
    Ok(if differences.is_empty() {
        Current::Unchanged(CurrentProof {
            record_sha256: bound.record_sha256,
            block_hash,
        })
    } else {
        Current::Drift(differences)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkbot_bsc_record::{Draft, Value};

    fn preflight() -> Record {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../native/evidence/g06a/g07_binary_import_20261016/preflight.dbrec"),
        )
        .unwrap();
        decode(&pf::SCHEMA, &bytes).unwrap()
    }

    const PROXY: [u8; 20] = [0xf1; 20];
    const PROXY_IMPL: [u8; 20] = [0xf2; 20];

    fn push_code(d: &mut Draft, address: [u8; 20], sha: [u8; 32]) {
        d.push(lp::CODE_ADDRESS, Value::Address(address))
            .push(lp::CODE_SHA256, Value::B256(sha))
            .push(lp::CODE_BYTES, 1u64)
            .push(lp::CODE_ROLES, "launch:CALL");
    }

    fn push_storage(d: &mut Draft, address: [u8; 20], slot: [u8; 32], selects: [u8; 20]) {
        d.push(lp::STORAGE_ADDRESS, Value::Address(address))
            .push(lp::STORAGE_SLOT, Value::B256(slot))
            .push(lp::STORAGE_VALUE, Value::B256(word_of(selects)))
            .push(lp::STORAGE_SELECTS, Value::Address(selects));
    }

    /// A snapshot consistent with the frozen preflight plus one extra
    /// EIP-1967 proxy, edited by `edit`.
    fn snapshot(edit: impl FnOnce(&mut Draft, &Record)) -> Vec<u8> {
        let pre = preflight();
        let a = |tag| pre.address(tag).unwrap();
        let w = |tag| pre.b256(tag).unwrap();
        let mut d = Draft::new(&lp::SCHEMA);
        d.set(darkbot_bsc_record::schema::PROVENANCE_TOOL, "test")
            .set(
                darkbot_bsc_record::schema::PROVENANCE_OBSERVED_UNIX_MS,
                1u64,
            )
            .set(lp::BLOCK_NUMBER, pre.u64(pf::LATEST_BLOCK_NUMBER).unwrap())
            .set(lp::BLOCK_HASH, Value::B256(w(pf::LATEST_BLOCK_HASH)))
            .set(lp::PORTAL, Value::Address(a(pf::PORTAL)))
            .set(lp::METHOD, lp::TRACE_METHOD)
            .set(lp::TOKEN, Value::Address([0x77; 20]))
            .set(lp::LAUNCH_SUCCESS, true)
            .set(lp::BUY_SUCCESS, true)
            .set(lp::LAUNCH_RETURN_TOKEN, Value::Address([0x77; 20]));
        for (address, sha) in [
            (a(pf::PORTAL), w(pf::PORTAL_CODE_SHA256)),
            (
                a(pf::PORTAL_IMPLEMENTATION),
                w(pf::PORTAL_IMPLEMENTATION_CODE_SHA256),
            ),
            (a(pf::TAX_V3_IMPL), w(pf::TAX_V3_IMPL_CODE_SHA256)),
            (PROXY, [3; 32]),
            (PROXY_IMPL, [4; 32]),
        ] {
            push_code(&mut d, address, sha);
        }
        for (address, selects) in [
            (a(pf::PORTAL), a(pf::PORTAL_IMPLEMENTATION)),
            (PROXY, PROXY_IMPL),
        ] {
            push_storage(&mut d, address, EIP1967_IMPLEMENTATION_SLOT, selects);
        }
        edit(&mut d, &pre);
        d.encode().unwrap()
    }

    #[test]
    fn the_real_snapshot_of_the_frozen_a_dry_checks_against_its_preflight() {
        let bytes =
            std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
                "../../../native/evidence/g06a/launch_path_frozen_a_20261023/launch_path.dbrec",
            ))
            .unwrap();
        let s = check(&bytes, &preflight()).unwrap();
        assert_eq!(
            (s.block_number, s.code.len(), s.storage.len()),
            (127_865_760, 22, 4)
        );
        // The proven gap: a slot-0 proxy, two EIP-1967 proxies and a
        // storage-selected clone target are bound by their selectors.
        let selected: Vec<_> = s
            .storage
            .iter()
            .map(|(_, _, v)| darkbot_bsc_record::hex(&v[12..]))
            .collect();
        for target in [
            "3e5c63644e683549055b9be8653de26e0b4cd36e",
            "a39a573d33181f014b326c50575589ed8efcee98",
            "88056bfe17e460e4b3240080dbce849b97b984dd",
        ] {
            assert!(selected.iter().any(|x| x == target), "{target}");
        }
    }

    #[test]
    fn a_consistent_snapshot_checks_and_every_defect_fails() {
        let pre = preflight();
        let good = check(&snapshot(|_, _| {}), &pre).unwrap();
        assert_eq!((good.code.len(), good.storage.len()), (5, 2));
        type Defect = Box<dyn Fn(&mut Draft, &Record)>;
        let defects: [(&str, Defect); 7] = [
            (
                "stale block",
                Box::new(|d, p| {
                    d.set(
                        lp::BLOCK_NUMBER,
                        p.u64(pf::LATEST_BLOCK_NUMBER).unwrap() + 1,
                    );
                }),
            ),
            (
                "foreign hash",
                Box::new(|d, _| {
                    d.set(lp::BLOCK_HASH, Value::B256([9; 32]));
                }),
            ),
            (
                "method",
                Box::new(|d, _| {
                    d.set(lp::METHOD, "eth_call");
                }),
            ),
            (
                "failed BUY",
                Box::new(|d, _| {
                    d.set(lp::BUY_SUCCESS, false);
                }),
            ),
            (
                "other return",
                Box::new(|d, _| {
                    d.set(lp::LAUNCH_RETURN_TOKEN, Value::Address([1; 20]));
                }),
            ),
            (
                "no storage",
                Box::new(|d, _| {
                    for tag in [
                        lp::STORAGE_ADDRESS,
                        lp::STORAGE_SLOT,
                        lp::STORAGE_VALUE,
                        lp::STORAGE_SELECTS,
                    ] {
                        d.clear(tag);
                    }
                }),
            ),
            (
                "other Portal",
                Box::new(|d, _| {
                    d.set(lp::PORTAL, Value::Address([5; 20]));
                }),
            ),
        ];
        for (name, defect) in defects {
            assert!(
                check(&snapshot(|d, p| defect(d, p)), &pre).is_err(),
                "{name}"
            );
        }
        assert!(check(b"not a record", &pre).is_err());
    }

    #[test]
    fn contradictions_with_the_preflight_or_unbound_selectors_fail() {
        let pre = preflight();
        // The implementation listed with another code hash.
        let changed = snapshot(|d, p| {
            let hashes: Vec<_> = d.values(lp::CODE_SHA256).to_vec();
            d.clear(lp::CODE_SHA256);
            for (i, h) in hashes.into_iter().enumerate() {
                d.push(
                    lp::CODE_SHA256,
                    if i == 1 { Value::B256([0xee; 32]) } else { h },
                );
            }
            let _ = p;
        });
        assert!(
            check(&changed, &pre)
                .unwrap_err()
                .contains("differs from the preflight")
        );
        // A selector pointing at an address without a code entry.
        let unbound = snapshot(|d, _| push_storage(d, PROXY, [0; 32], [0xab; 20]));
        assert!(
            check(&unbound, &pre)
                .unwrap_err()
                .contains("without its code")
        );
        // Lists of different length.
        let ragged = snapshot(|d, _| {
            d.push(lp::CODE_ADDRESS, Value::Address([0xac; 20]));
        });
        assert!(
            check(&ragged, &pre)
                .unwrap_err()
                .contains("different length")
        );
    }

    #[test]
    fn drift_reports_a_changed_code_or_an_upgraded_proxy_and_fails_on_read_errors() {
        let pre = preflight();
        let s = check(&snapshot(|_, _| {}), &pre).unwrap();
        let code = s.code.clone();
        let storage = s.storage.clone();
        let mut same_code = |a: [u8; 20]| Ok(code.iter().find(|(x, _)| *x == a).unwrap().1);
        let mut same_storage = |a: [u8; 20], slot: [u8; 32]| {
            Ok(storage
                .iter()
                .find(|(x, y, _)| *x == a && *y == slot)
                .unwrap()
                .2)
        };
        assert!(
            drift(&s, &mut same_code, &mut same_storage)
                .unwrap()
                .is_empty()
        );
        // The proxy now points elsewhere: its old implementation code is
        // unchanged, only the selector reveals the upgrade.
        let mut upgraded = |a: [u8; 20], slot: [u8; 32]| {
            if a == PROXY {
                Ok(word_of([0xcc; 20]))
            } else {
                same_storage(a, slot)
            }
        };
        let found = drift(&s, &mut same_code, &mut upgraded).unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("now selects 0xcccc"));
        let mut changed_code = |a: [u8; 20]| {
            if a == PROXY_IMPL {
                Ok([0; 32])
            } else {
                same_code(a)
            }
        };
        assert_eq!(
            drift(&s, &mut changed_code, &mut same_storage)
                .unwrap()
                .len(),
            1
        );
        let mut failing = |_: [u8; 20]| Err("rpc down".to_string());
        assert!(drift(&s, &mut failing, &mut same_storage).is_err());
    }

    #[test]
    fn only_an_unchanged_chain_yields_the_current_proof_of_the_bound_record() {
        let s = check(&snapshot(|_, _| {}), &preflight()).unwrap();
        let bound = Bound::new([7; 32], s.clone());
        let code = |a: [u8; 20]| Ok(s.code.iter().find(|(x, _)| *x == a).unwrap().1);
        let storage = |a: [u8; 20], slot: [u8; 32]| {
            Ok(if a == PROXY {
                word_of([0xcc; 20])
            } else {
                s.storage
                    .iter()
                    .find(|(x, y, _)| *x == a && *y == slot)
                    .unwrap()
                    .2
            })
        };
        let unchanged = |a: [u8; 20], slot: [u8; 32]| {
            Ok(s.storage
                .iter()
                .find(|(x, y, _)| *x == a && *y == slot)
                .unwrap()
                .2)
        };
        match verify_current(&bound, [8; 32], &mut { code }, &mut { unchanged }).unwrap() {
            Current::Unchanged(p) => {
                assert_eq!((p.record_sha256(), p.block_hash()), ([7; 32], [8; 32]))
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            verify_current(&bound, [8; 32], &mut { code }, &mut { storage }).unwrap(),
            Current::Drift(d) if d.len() == 1
        ));
        let mut failing = |_: [u8; 20]| Err("rpc down".to_string());
        assert!(verify_current(&bound, [8; 32], &mut failing, &mut { unchanged }).is_err());
    }
}
