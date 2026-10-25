//! FE-04: `p0ctl approval show` over the real binary — a versioned one-shot approval profile
//! (attempt or exit) is read strictly, checked alone and shown with what it binds; it exits 0
//! only for a valid profile bound to this project's clean HEAD and these binaries. Offline, no
//! state change, LIVE stays locked.
use std::process::Command;

fn p0ctl(project: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    p0ctl_at(
        std::path::Path::new(env!("CARGO_BIN_EXE_p0ctl")),
        project,
        args,
    )
}

fn p0ctl_at(
    bin: &std::path::Path,
    project: &std::path::Path,
    args: &[&str],
) -> (i32, String, String) {
    let out = Command::new(bin)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .arg("--project")
        .arg(project)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn key(n: u8) -> String {
    solana_sdk::pubkey::Pubkey::new_from_array([n; 32]).to_string()
}

fn attempt(code_sha: &str, binary_sha256: &str) -> serde_json::Value {
    serde_json::json!({
        "schema": 1, "authorized": true, "approval_id": "pilot-1",
        "code_sha": code_sha, "binary_sha256": binary_sha256,
        "wallet": key(1), "program": key(2), "program_data_hash": "deployed",
        "funding_class": "ZERO_PREFUND_SAME_TX", "allowed_targets": [key(3)],
        "expires_unix_s": 4_000_000_000u64, "max_state_age_ms": 500,
        "max_capital_usd_micro": 10_000_000, "max_total_sol_debit_lamports": 50_000,
        "max_tip_lamports": 0, "max_rent_lamports": 3_000_000,
        "max_loss_usd_micro": 1_000_000, "min_net_usd_micro": 100, "max_slippage_bps": 50})
}

fn exit(code_sha: &str, binary_sha256: &str) -> serde_json::Value {
    serde_json::json!({
        "schema": 1, "authorized": true, "approval_id": "exit-1",
        "attempt_approval_id": "pilot-1", "code_sha": code_sha,
        "binary_sha256": binary_sha256, "wallet": key(1), "mint": key(4),
        "route_program": key(5), "expires_unix_s": 4_000_000_000u64, "max_state_age_ms": 500,
        "min_out_lamports": 9_000_000, "max_slippage_bps": 100, "max_fee_lamports": 10_000})
}

fn write(dir: &std::path::Path, name: &str, v: &serde_json::Value) -> String {
    let p = dir.join(name);
    std::fs::write(&p, serde_json::to_vec(v).unwrap()).unwrap();
    p.display().to_string()
}

/// Every profile is shown with its bindings; an unauthorized, expired, oversized or foreign one
/// is refused (exit 2); outside a clean git project the code binding is UNKNOWN (exit 2).
#[test]
fn approval_show_refuses_invalid_profiles_and_unproven_bindings() {
    let d = std::env::temp_dir().join(format!("p0ctl_approval_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let (a, b) = ("a".repeat(40), "b".repeat(64));
    let (code, out, err) = p0ctl(
        &d,
        &["approval", "show", &write(&d, "a.json", &attempt(&a, &b))],
    );
    assert_eq!(code, 2, "{out}{err}");
    for line in [
        "approval: attempt",
        "approval_id: pilot-1",
        "max_total_sol_debit_lamports: 50000",
        "max_loss_usd_micro: 1000000",
        "valid: yes",
        "UNKNOWN (this project: clean HEAD",
        "live: LOCKED",
    ] {
        assert!(out.contains(line), "{out} lacks {line}");
    }
    let (code, out, _) = p0ctl(
        &d,
        &["approval", "show", &write(&d, "e.json", &exit(&a, &b))],
    );
    assert_eq!(code, 2);
    for line in [
        "approval: exit",
        "attempt_approval_id: pilot-1",
        &format!("mint: {}", key(4)),
        "min_out_lamports: 9000000",
        "valid: yes",
    ] {
        assert!(out.contains(line), "{out} lacks {line}");
    }
    let mut unauthorized = attempt(&a, &b);
    unauthorized["authorized"] = false.into();
    let (code, out, _) = p0ctl(
        &d,
        &["approval", "show", &write(&d, "u.json", &unauthorized)],
    );
    assert!(
        code == 2 && out.contains("valid: no (INVALID_OR_UNAUTHORIZED_APPROVAL)"),
        "{out}"
    );
    let mut expired = exit(&a, &b);
    expired["expires_unix_s"] = 1.into();
    let (code, out, _) = p0ctl(&d, &["approval", "show", &write(&d, "x.json", &expired)]);
    assert!(
        code == 2 && out.contains("valid: no (APPROVAL_EXPIRED)"),
        "{out}"
    );
    let mut foreign = attempt(&a, &b);
    foreign["live_enabled"] = true.into();
    let (code, out, err) = p0ctl(&d, &["approval", "show", &write(&d, "f.json", &foreign)]);
    assert!(
        code == 2 && out.is_empty() && err.contains("APPROVAL_UNREADABLE"),
        "{out}{err}"
    );
    std::fs::write(d.join("big.json"), vec![b' '; 65 * 1024]).unwrap();
    let (code, _, err) = p0ctl(
        &d,
        &[
            "approval",
            "show",
            &d.join("big.json").display().to_string(),
        ],
    );
    assert!(code == 2 && err.contains("APPROVAL_UNREADABLE"), "{err}");
    let _ = std::fs::remove_dir_all(&d);
}

/// Bound to this build: a profile whose `code_sha` is the clean HEAD of the project and whose
/// `binary_sha256` is the digest of these binaries passes (exit 0); a dirty tree or another
/// binary digest does not.
#[cfg(feature = "crate_health")]
#[test]
fn approval_show_passes_only_for_this_clean_build() {
    let d = std::env::temp_dir().join(format!("p0ctl_approval_bound_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&d)
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    git(&["init", "-q"]);
    std::fs::write(d.join("README"), "p0").unwrap();
    git(&["add", "README"]);
    git(&["commit", "-q", "-m", "p0"]);
    let head = git(&["rev-parse", "HEAD"]);
    // The bound identity is p0ctl plus the runtime next to it; a copied p0ctl beside a stand-in
    // runtime keeps the test hermetic (the runtime binary is built by the reporter crate).
    let bin = d.with_extension("bin");
    let _ = std::fs::remove_dir_all(&bin);
    std::fs::create_dir_all(&bin).unwrap();
    let ctl = bin.join("p0ctl");
    std::fs::copy(env!("CARGO_BIN_EXE_p0ctl"), &ctl).unwrap();
    std::fs::write(bin.join("p0-longtail"), b"runtime stand-in").unwrap();
    let binary = darkbot_runner::orchestration::boot_audit::binary_identity_of(
        &ctl,
        &bin.join("p0-longtail"),
    )
    .unwrap();
    // Profiles live outside the project tree (they would make it dirty).
    let files = d.with_extension("profiles");
    let _ = std::fs::remove_dir_all(&files);
    std::fs::create_dir_all(&files).unwrap();
    let bound = write(&files, "a.json", &attempt(&head, &binary));
    let (code, out, err) = p0ctl_at(&ctl, &d, &["approval", "show", &bound]);
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(out.matches(" MATCH").count(), 2, "{out}");
    let other = write(&files, "o.json", &attempt(&head, &"c".repeat(64)));
    let (code, out, _) = p0ctl_at(&ctl, &d, &["approval", "show", &other]);
    assert!(code == 2 && out.contains("MISMATCH (this build"), "{out}");
    std::fs::write(d.join("README"), "changed").unwrap();
    let (code, out, _) = p0ctl_at(&ctl, &d, &["approval", "show", &bound]);
    assert!(code == 2 && out.contains("UNKNOWN (this project"), "{out}");
    let _ = std::fs::remove_dir_all(&d);
    let _ = std::fs::remove_dir_all(&files);
    let _ = std::fs::remove_dir_all(&bin);
}
