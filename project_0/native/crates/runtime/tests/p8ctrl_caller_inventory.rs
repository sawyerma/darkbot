//! P8-CTRL.7 caller inventory: every real signature and every network egress of the runtime's
//! production code sits behind the one permit boundary. The production sources (each file up to
//! its test module; test-only files excluded) are scanned for signing and submission calls; each
//! call site must be one of the inventoried ones — a new signer or sender anywhere else fails here
//! until it is put behind `SignPermit`/`EgressPermit` and inventoried. Offline, source only.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The production part of a source file: up to its first test module.
fn production(text: &str) -> &str {
    let cut = [
        "#[cfg(test)]\nmod ",
        "#[cfg(test)]\npub(crate) mod ",
        "#[cfg(test)]\npub mod ",
    ]
    .iter()
    .filter_map(|m| text.find(m))
    .min()
    .unwrap_or(text.len());
    &text[..cut]
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs")
            && !path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("tests.rs")
        {
            out.push(path);
        }
    }
}

/// Call sites per (pattern, file relative to `src/`).
fn inventory(patterns: &[&str]) -> BTreeMap<(String, String), usize> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src, &mut files);
    let mut found = BTreeMap::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let code: String = production(&text)
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let rel = file
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        for p in patterns {
            let n = code.matches(p).count();
            if n > 0 {
                found.insert((p.to_string(), rel.clone()), n);
            }
        }
    }
    found
}

#[test]
fn p8ctrl03_every_real_signer_and_sender_is_behind_the_permit_boundary() {
    let found = inventory(&[
        "tx::sign(",
        "sign_transaction(",
        "VersionedTransaction::try_new(",
        "new_signed_with_payer(",
        ".submit(",
        ".submit_bundle(",
        "send_and_confirm_transaction(",
        "send_transaction(",
    ]);
    let expected: BTreeMap<(String, String), usize> = [
        // The wallet's own signature: only `SignPermit::sign`.
        (
            ("sign_transaction(", "orchestration/capability_control.rs"),
            1,
        ),
        // Egress: the FE-04 transport under its `EgressPermit` (attempt, exit) and the adapters'
        // own forwarding below it (ObserveOnly → OwnerSubmitter → RPC/Jito).
        ((".submit(", "execution/one_shot/transport.rs"), 2),
        ((".submit_bundle(", "execution/one_shot/transport.rs"), 1),
        ((".submit(", "execution/observe_only.rs"), 1),
        ((".submit_bundle(", "execution/observe_only.rs"), 1),
        ((".submit(", "execution/bundle_path.rs"), 1),
        ((".submit_bundle(", "execution/bundle_path.rs"), 1),
        // Not an egress: the market journal writer's own `submit`.
        ((".submit(", "market/sink.rs"), 1),
        // P8-CTRL-03d (open): the operator tools that sign or send outside the owner — inventoried
        // here until they are put behind the same boundary.
        (("new_signed_with_payer(", "bin/p0-prewarm-atas.rs"), 1),
        (
            ("send_and_confirm_transaction(", "bin/p0-prewarm-atas.rs"),
            1,
        ),
        (("tx::sign(", "bin/p0-probe-costs.rs"), 1),
    ]
    .into_iter()
    .map(|((p, f), n)| ((p.to_string(), f.to_string()), n))
    .collect();
    assert_eq!(found, expected);
}
