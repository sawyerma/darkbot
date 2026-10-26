//! Phase G (Economics gate §15): a frozen economics snapshot verifies only
//! against the files it was frozen from, and the newest committed one is
//! the snapshot of this tree (a change of the economics needs a new one).

use darkbot_bsc_qualification::economics_snapshot::{current, freeze, verify};
use darkbot_bsc_trading::economics::snapshot::BOUND_FILES;
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

/// A copy of the bound files verifies; one changed byte makes it STALE and
/// names the file.
#[test]
fn a_frozen_snapshot_verifies_only_against_its_files() {
    let root = repo_root();
    let copy = std::env::temp_dir().join(format!("bsc-econ-snapshot-{}", std::process::id()));
    let _ = fs::remove_dir_all(&copy);
    for p in BOUND_FILES {
        let to = copy.join(p);
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::copy(root.join(p), &to).unwrap();
    }
    let (bytes, hash) = freeze(&copy, 1, "test").unwrap();
    assert_eq!(verify(&copy, &bytes).unwrap(), Ok(hash));
    assert_eq!(current(&copy).unwrap(), hash);
    let changed = copy.join(BOUND_FILES[2]);
    let mut text = fs::read(&changed).unwrap();
    text.push(b'\n');
    fs::write(&changed, text).unwrap();
    assert_eq!(
        verify(&copy, &bytes).unwrap(),
        Err(vec![BOUND_FILES[2].to_string()])
    );
    fs::remove_dir_all(&copy).unwrap();
}

/// The newest frozen snapshot in `bsc/native/evidence/g/` is the one of
/// this tree.
#[test]
fn the_newest_frozen_snapshot_is_this_trees() {
    let root = repo_root();
    let dir = root.join("bsc/native/evidence/g");
    let newest = fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("economics_snapshot_"))
        })
        .max()
        .expect("no frozen economics snapshot");
    let bytes = fs::read(newest.join("snapshot.dbrec")).unwrap();
    assert!(
        verify(&root, &bytes).unwrap().is_ok(),
        "the economics changed since {}: freeze a new snapshot (g_economics_snapshot)",
        newest.display()
    );
}
