//! P0-027: a deployed release names its source by the identity record `p0ctl release identity`
//! wrote from the checkout it was built from, beside its executables; BOOT-01 reads it when the
//! project (the desktop app's data home) is no checkout — for exactly the executables it names.
//! Only a build from unchanged build inputs gets a record; a data home inside a foreign
//! repository is no checkout; an econ4h/shadow flow needs a checkout.
use darkbot_runner::orchestration::boot_audit::{
    identity_of, release_identity, SafetyFlags, RELEASE_BINARIES, RELEASE_IDENTITY,
};
use std::path::Path;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.email=t@t", "-c", "user.name=t"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{args:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn p0_027_a_release_names_its_source_for_exactly_its_executables() {
    let base = std::env::temp_dir().join(format!("p0_027_release_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (checkout, release, home) = (
        base.join("checkout"),
        base.join("release"),
        base.join("home"),
    );
    for d in [&checkout, &release, &home, &checkout.join("native")] {
        std::fs::create_dir_all(d).unwrap();
    }
    git(&checkout, &["init", "-q"]);
    std::fs::write(checkout.join("native/lib.rs"), b"fn main() {}").unwrap();
    git(&checkout, &["add", "native/lib.rs"]);
    git(&checkout, &["commit", "-q", "-m", "c"]);
    let head = git(&checkout, &["rev-parse", "HEAD"]);
    let exe = release.join("p0ctl");
    let record = release.join(RELEASE_IDENTITY);
    // Refused: a project that is no checkout, a missing executable, a changed build input.
    let e = release_identity(&home, &exe, &record).unwrap_err();
    assert!(e.starts_with("RELEASE_SOURCE_NOT_A_CHECKOUT"), "{e}");
    let e = release_identity(&checkout, &exe, &record).unwrap_err();
    assert!(e.starts_with("RELEASE_BINARY_MISSING: p0ctl"), "{e}");
    for name in RELEASE_BINARIES {
        std::fs::write(release.join(name), format!("binary {name}")).unwrap();
    }
    std::fs::write(checkout.join("native/new.rs"), b"// untracked").unwrap();
    let e = release_identity(&checkout, &exe, &record).unwrap_err();
    assert!(e.starts_with("RELEASE_SOURCE_DIRTY"), "{e}");
    std::fs::remove_file(checkout.join("native/new.rs")).unwrap();
    let r = release_identity(&checkout, &exe, &record).unwrap();
    assert_eq!(r.source_head, head);
    assert_eq!(r.binaries.len(), RELEASE_BINARIES.len());
    // Written once.
    assert!(release_identity(&checkout, &exe, &record)
        .unwrap_err()
        .starts_with("RELEASE_IDENTITY_STORE"));
    let config = darkbot_solana::runtime_config::RuntimeConfig::default();
    let flags = SafetyFlags {
        live_enabled: false,
        submission_enabled: false,
    };
    // The data home is no checkout: the release's source, for each of its executables.
    for name in RELEASE_BINARIES {
        let id = identity_of(&home, flags, &config, Some(&release.join(name))).unwrap();
        assert_eq!(id.source_head.as_deref(), Some(head.as_str()), "{name}");
        assert_eq!(
            id.source_dirty_sha256.as_deref(),
            Some(r.source_dirty_sha256.as_str())
        );
    }
    // A data home inside a foreign repository is no checkout: still the release's source.
    let outer = base.join("outer");
    let nested = outer.join("Library/DarkBot");
    std::fs::create_dir_all(&nested).unwrap();
    git(&outer, &["init", "-q"]);
    std::fs::write(outer.join("f"), b"x").unwrap();
    git(&outer, &["add", "f"]);
    git(&outer, &["commit", "-q", "-m", "foreign"]);
    let id = identity_of(&nested, flags, &config, Some(&exe)).unwrap();
    assert_eq!(id.source_head.as_deref(), Some(head.as_str()));
    // A checkout keeps its own git identity whatever lies beside the executable.
    let own = identity_of(&checkout, flags, &config, Some(&exe)).unwrap();
    assert_eq!(own.source_head.as_deref(), Some(head.as_str()));
    // Any executable of the release swapped, or no record: no source identity (BOOT-01 not PASS).
    std::fs::write(
        release.join("p0-market-capture-reported"),
        b"another capture",
    )
    .unwrap();
    let id = identity_of(&home, flags, &config, Some(&exe)).unwrap();
    assert_eq!((id.source_head, id.source_dirty_sha256), (None, None));
    std::fs::remove_file(&record).unwrap();
    let id = identity_of(&home, flags, &config, Some(&exe)).unwrap();
    assert_eq!(id.source_head, None);
    let _ = std::fs::remove_dir_all(&base);
}

/// P0-027: an econ4h or shadow flow starts through a checkout's launcher; on a data home that is
/// no checkout it is refused before any audit.
#[test]
fn p0_027_an_econ4h_flow_needs_a_checkout() {
    let home = std::env::temp_dir().join(format!("p0_027_home_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    for mode in ["econ4h", "shadow"] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_p0ctl"))
            .arg("--project")
            .arg(&home)
            .args(["boot", "offline", "--mode", mode, "--duration", "60s"])
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("P0_PROJECT_DIR", &home)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success() && text.contains("BOOT_MODE_NEEDS_CHECKOUT"),
            "{mode}: {text}"
        );
    }
    let _ = std::fs::remove_dir_all(&home);
}
