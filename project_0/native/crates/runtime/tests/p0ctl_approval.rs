//! FE-04: `p0ctl approval show` over the real binary — a versioned one-shot approval profile
//! (attempt or exit) is read strictly, checked alone and shown with what it binds; it exits 0
//! only for a valid profile bound to this project's clean HEAD and these binaries. Offline, no
//! state change, LIVE stays locked.
mod support;
use std::process::Command;
use support::Serialized;

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
        .serialized_output()
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

type Fields = Vec<(&'static str, String)>;

/// A valid ProgramData hash (base58) and an expiry within the lifetime bound.
const HASH: &str = "11111111111111111111111111111111";
fn expires() -> String {
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3_600)
        .to_string()
}

/// DEC-10: an attempt approval in the native `key = value` text.
fn attempt(code_sha: &str, binary_sha256: &str) -> Fields {
    vec![
        ("schema", "Project0/FE04Approval/1".into()),
        ("authorized", "true".into()),
        ("approval_id", "pilot-1".into()),
        ("code_sha", code_sha.into()),
        ("binary_sha256", binary_sha256.into()),
        ("wallet", key(1)),
        ("program", key(2)),
        ("program_data_hash", HASH.into()),
        ("funding_class", "ZERO_PREFUND_SAME_TX".into()),
        ("allowed_targets", key(3)),
        ("expires_unix_s", expires()),
        ("max_state_age_ms", "500".into()),
        ("max_capital_usd_micro", "10000000".into()),
        ("max_total_sol_debit_lamports", "50000".into()),
        ("max_tip_lamports", "0".into()),
        ("max_rent_lamports", "3000000".into()),
        ("max_loss_usd_micro", "1000000".into()),
        ("min_net_usd_micro", "100".into()),
        ("max_slippage_bps", "50".into()),
    ]
}

fn exit(code_sha: &str, binary_sha256: &str) -> Fields {
    vec![
        ("schema", "Project0/FE04ExitApproval/1".into()),
        ("authorized", "true".into()),
        ("approval_id", "exit-1".into()),
        ("attempt_approval_id", "pilot-1".into()),
        ("code_sha", code_sha.into()),
        ("binary_sha256", binary_sha256.into()),
        ("wallet", key(1)),
        ("mint", key(4)),
        ("route_program", key(5)),
        ("expires_unix_s", expires()),
        ("max_state_age_ms", "500".into()),
        ("min_out_lamports", "9000000".into()),
        ("max_slippage_bps", "100".into()),
        ("max_fee_lamports", "10000".into()),
    ]
}

/// `fields` with `key` set to `value` (appended when absent).
fn with(mut fields: Fields, key: &'static str, value: &str) -> Fields {
    match fields.iter_mut().find(|(k, _)| *k == key) {
        Some(entry) => entry.1 = value.into(),
        None => fields.push((key, value.into())),
    }
    fields
}

fn write(dir: &std::path::Path, name: &str, fields: &Fields) -> String {
    let p = dir.join(name);
    let text: String = fields.iter().map(|(k, v)| format!("{k} = {v}\n")).collect();
    std::fs::write(&p, text).unwrap();
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
    let journals = d.join("fe04");
    let journals = journals.display().to_string();
    let attempt_file = write(&d, "a.conf", &attempt(&a, &b));
    // DEC-10: without `--journal-dir` single use is unchecked: never valid.
    let (code, out, err) = p0ctl(&d, &["approval", "show", &attempt_file]);
    assert!(
        code == 2 && out.contains("status: INVALID (SINGLE_USE_NOT_CHECKED"),
        "{out}{err}"
    );
    let (code, out, err) = p0ctl(
        &d,
        &[
            "approval",
            "show",
            &attempt_file,
            "--journal-dir",
            &journals,
        ],
    );
    assert_eq!(code, 2, "{out}{err}");
    for line in [
        "approval: attempt",
        "approval_id: pilot-1",
        "max_total_sol_debit_lamports: 50000",
        "max_loss_usd_micro: 1000000",
        "approval_archived: false",
        "status: BINDING_MISMATCH (code_sha,binary_sha256)",
        "UNKNOWN (this project: clean HEAD",
        "live: LOCKED",
    ] {
        assert!(out.contains(line), "{out} lacks {line}");
    }
    let (code, out, _) = p0ctl(
        &d,
        &[
            "approval",
            "show",
            &write(&d, "e.conf", &exit(&a, &b)),
            "--journal-dir",
            &journals,
        ],
    );
    assert_eq!(code, 2);
    for line in [
        "approval: exit",
        "attempt_approval_id: pilot-1",
        &format!("mint: {}", key(4)),
        "min_out_lamports: 9000000",
        // An exit approval needs the open position of its attempt.
        "status: INVALID (EXIT_ATTEMPT_UNKNOWN)",
    ] {
        assert!(out.contains(line), "{out} lacks {line}");
    }
    let expired = with(exit(&a, &b), "expires_unix_s", "1");
    let (code, out, _) = p0ctl(&d, &["approval", "show", &write(&d, "x.conf", &expired)]);
    assert!(
        code == 2 && out.contains("valid: no (APPROVAL_EXPIRED)"),
        "{out}"
    );
    // DEC-10: the strict parser refuses before anything is shown.
    let mut duplicate = attempt(&a, &b);
    duplicate.push(("approval_id", "pilot-2".into()));
    let mut missing = attempt(&a, &b);
    missing.retain(|(k, _)| *k != "max_loss_usd_micro");
    for (name, fields, code_text) in [
        (
            "u",
            with(attempt(&a, &b), "authorized", "false"),
            "APPROVAL_INVALID: INVALID_OR_UNAUTHORIZED_APPROVAL",
        ),
        (
            "f",
            with(attempt(&a, &b), "live_enabled", "true"),
            "APPROVAL_FIELD_UNKNOWN: live_enabled",
        ),
        ("m", missing, "APPROVAL_FIELD_MISSING: max_loss_usd_micro"),
        ("dup", duplicate, "duplicate config key approval_id"),
        (
            "z",
            with(attempt(&a, &b), "max_tip_lamports", "01"),
            "APPROVAL_FIELD_INVALID: max_tip_lamports",
        ),
        (
            "n",
            with(attempt(&a, &b), "max_capital_usd_micro", "-1"),
            "APPROVAL_FIELD_INVALID: max_capital_usd_micro",
        ),
        (
            "t",
            with(
                attempt(&a, &b),
                "allowed_targets",
                &format!("{},{}", key(3), key(3)),
            ),
            "APPROVAL_FIELD_INVALID: allowed_targets",
        ),
        (
            "s",
            with(attempt(&a, &b), "schema", "Project0/FE04Approval/2"),
            "APPROVAL_SCHEMA_UNKNOWN",
        ),
        (
            "b",
            with(attempt(&a, &b), "max_slippage_bps", "10001"),
            "APPROVAL_INVALID",
        ),
        (
            "i",
            with(attempt(&a, &b), "approval_id", "../x"),
            "APPROVAL_FIELD_INVALID: approval_id",
        ),
        (
            "c",
            with(attempt(&a, &b), "max_capital_usd_micro", "0"),
            "APPROVAL_FIELD_INVALID: max_capital_usd_micro",
        ),
        (
            "h",
            with(attempt(&a, &b), "program_data_hash", "deployed"),
            "APPROVAL_FIELD_INVALID: program_data_hash",
        ),
    ] {
        let (code, out, err) = p0ctl(
            &d,
            &[
                "approval",
                "show",
                &write(&d, &format!("{name}.conf"), &fields),
            ],
        );
        assert!(
            code == 2 && out.is_empty() && err.contains(code_text),
            "{name}: {out}{err}"
        );
    }
    // A JSON approval is historical: refused by the standard build, shown by the historical
    // build as never authorizing (never adopted).
    let json = d.join("historical.json");
    std::fs::write(&json, br#"{"schema":1,"authorized":true}"#).unwrap();
    let (code, out, err) = p0ctl(&d, &["approval", "show", &json.display().to_string()]);
    assert_eq!(code, 2);
    if cfg!(feature = "historical") {
        assert!(err.contains("APPROVAL_UNREADABLE"), "{out}{err}");
        let full = serde_json::json!({
            "schema": 1, "authorized": true, "approval_id": "pilot-1", "code_sha": a, "binary_sha256": b,
            "wallet": key(1), "program": key(2), "program_data_hash": "deployed",
            "funding_class": "ZERO_PREFUND_SAME_TX", "allowed_targets": [key(3)],
            "expires_unix_s": 4_000_000_000u64, "max_state_age_ms": 500,
            "max_capital_usd_micro": 10_000_000, "max_total_sol_debit_lamports": 50_000,
            "max_tip_lamports": 0, "max_rent_lamports": 3_000_000,
            "max_loss_usd_micro": 1_000_000, "min_net_usd_micro": 100, "max_slippage_bps": 50});
        std::fs::write(&json, serde_json::to_vec(&full).unwrap()).unwrap();
        let (code, out, err) = p0ctl(&d, &["approval", "show", &json.display().to_string()]);
        assert!(
            code == 2
                && out.contains("format: HISTORICAL_JSON")
                && out.contains("valid: no (HISTORICAL_JSON_NEVER_AUTHORIZES)"),
            "{out}{err}"
        );
    } else {
        assert!(err.contains("APPROVAL_JSON_REFUSED"), "{out}{err}");
    }
    std::fs::write(d.join("big.conf"), vec![b' '; 65 * 1024]).unwrap();
    let (code, _, err) = p0ctl(
        &d,
        &[
            "approval",
            "show",
            &d.join("big.conf").display().to_string(),
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
            .serialized_output()
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
    support::install(env!("CARGO_BIN_EXE_p0ctl"), &ctl);
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
    let bound = write(&files, "a.conf", &attempt(&head, &binary));
    let journals = files.join("fe04").display().to_string();
    let show = |file: &str| {
        p0ctl_at(
            &ctl,
            &d,
            &["approval", "show", file, "--journal-dir", &journals],
        )
    };
    let (code, out, err) = show(&bound);
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(out.matches(" MATCH").count(), 2, "{out}");
    let other = write(&files, "o.conf", &attempt(&head, &"c".repeat(64)));
    let (code, out, _) = show(&other);
    assert!(code == 2 && out.contains("MISMATCH (this build"), "{out}");
    std::fs::write(d.join("README"), "changed").unwrap();
    let (code, out, _) = show(&bound);
    assert!(code == 2 && out.contains("UNKNOWN (this project"), "{out}");
    let _ = std::fs::remove_dir_all(&d);
    let _ = std::fs::remove_dir_all(&files);
    let _ = std::fs::remove_dir_all(&bin);
}
