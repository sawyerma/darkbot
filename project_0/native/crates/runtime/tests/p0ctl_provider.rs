//! DEC-18: `p0ctl provider …` over the real binary — the provider registry decided by the CLI,
//! staged, under request ids with redacted receipts; a credential's secret only through the
//! standard input into the credential store (here the `test-support` directory store; the
//! production build has the macOS keychain alone), never in an argument, an output or a record.
//! A debug build only: in a release build the test store does not exist and the binary would
//! reach the real keychain.
#![cfg(debug_assertions)]
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// The secret of the one item of credential `id` in the test store (`<uid>%<id>@<rev>.<nonce>`).
fn item(store: &Path, id: &str, revision: u64) -> Vec<u8> {
    let marker = format!("%{id}@{revision}.");
    let found: Vec<_> = std::fs::read_dir(store)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.file_name().unwrap().to_string_lossy().contains(&marker))
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    std::fs::read(&found[0]).unwrap()
}

struct Out {
    ok: bool,
    text: String,
}

fn p0ctl(project: &Path, store: &Path, args: &[&str], stdin: Option<&[u8]>) -> Out {
    let mut child = Command::new(env!("CARGO_BIN_EXE_p0ctl"))
        .arg("--project")
        .arg(project)
        .arg("provider")
        .args(args)
        .env(darkbot_solana::providers::TEST_STORE_ENV, store)
        .env_remove("P0_ENV_FILE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        if let Some(bytes) = stdin {
            input.write_all(bytes).unwrap();
        }
    }
    let out = child.wait_with_output().unwrap();
    Out {
        ok: out.status.success(),
        text: format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    }
}

/// DEC-18 §1.2/§1.3 Nr. 5–7: a Helius provider through the CLI — an endpoint carrying its key
/// is refused (the key stays out of the error); the provider, its credential through the
/// standard input, the selection; the secret only in the store, never in an output, a record or
/// a receipt; a secret as an argument is no option; the same request id is the same receipt;
/// the one-time `.env` import, never twice; the status names hosts, never paths or keys.
#[test]
fn dec18_a_provider_and_its_credential_through_the_cli() {
    let base = std::env::temp_dir().join(format!("p0ctl_provider_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (project, store) = (base.join("project"), base.join("keychain"));
    std::fs::create_dir_all(&project).unwrap();
    let run = |args: &[&str], stdin: Option<&[u8]>| p0ctl(&project, &store, args, stdin);
    let status = run(&["status"], None);
    assert!(
        status.ok && status.text.contains("PROVIDER_REGISTRY version=0"),
        "{}",
        status.text
    );

    let add = |endpoint: &str| {
        run(
            &[
                "add",
                "--id",
                "helius-rpc",
                "--kind",
                "rpc",
                "--endpoint",
                endpoint,
                "--auth",
                "query:api-key",
                "--credential",
                "helius",
                "--plan",
                "free",
                "--entitlement",
                "helius-free",
                "--capacity-rps",
                "10",
                "--request-id",
                "add-1",
            ],
            None,
        )
    };
    let refused = add("https://mainnet.helius-rpc.com/?api-key=IN-URL-KEY");
    assert!(
        !refused.ok && refused.text.contains("PROVIDER_URL_QUERY"),
        "{}",
        refused.text
    );
    assert!(!refused.text.contains("IN-URL-KEY"), "{}", refused.text);
    let added = add("https://mainnet.helius-rpc.com");
    assert!(
        added.ok && added.text.contains("applied=true version=1"),
        "{}",
        added.text
    );
    assert_eq!(
        add("https://mainnet.helius-rpc.com").text,
        added.text,
        "same request id, same receipt"
    );

    let select = |id: &str| {
        run(
            &[
                "select",
                "--kind",
                "rpc",
                "--id",
                "helius-rpc",
                "--request-id",
                id,
            ],
            None,
        )
    };
    let early = select("sel-1");
    assert!(
        !early.ok && early.text.contains("CREDENTIAL_UNKNOWN"),
        "{}",
        early.text
    );

    let no_flag = run(
        &[
            "credential",
            "set",
            "--credential",
            "helius",
            "--secret",
            "ARG-KEY",
        ],
        None,
    );
    assert!(
        !no_flag.ok && no_flag.text.contains("PROVIDER_OPTION_UNKNOWN: --secret"),
        "{}",
        no_flag.text
    );
    let set = run(
        &["credential", "set", "--credential", "helius"],
        Some(b"KEY-ONE\n"),
    );
    assert!(
        set.ok && set.text.contains("applied=true version=2"),
        "{}",
        set.text
    );
    assert!(!set.text.contains("KEY-ONE"), "{}", set.text);
    assert_eq!(item(&store, "helius", 1), b"KEY-ONE");
    let selected = select("sel-2");
    assert!(
        selected.ok && selected.text.contains("version=3"),
        "{}",
        selected.text
    );

    // The one-time `.env` import.
    let env = base.join("capture.private.env");
    std::fs::write(&env, "HELIUS_API_KEY=ENV-KEY\nP0_KEYPAIR_PATH=/x\n").unwrap();
    let import = |credential: &str| {
        run(
            &[
                "credential",
                "import-env",
                "--credential",
                credential,
                "--variable",
                "HELIUS_API_KEY",
                "--env-file",
                env.to_str().unwrap(),
            ],
            None,
        )
    };
    let imported = import("helius-env");
    assert!(
        imported.ok
            && imported
                .text
                .contains("credential set helius-env env:HELIUS_API_KEY"),
        "{}",
        imported.text
    );
    assert_eq!(item(&store, "helius-env", 1), b"ENV-KEY");
    let again = import("helius-env2");
    assert!(
        !again.ok && again.text.contains("CREDENTIAL_ENV_IMPORTED"),
        "{}",
        again.text
    );
    // A key typed where an identifier belongs is never echoed nor persisted.
    let slip = run(
        &[
            "credential",
            "revoke",
            "--credential",
            "0b6c2d4e-1f3a-4b5c-8d9e-0a1b2c3d4e5f",
        ],
        None,
    );
    assert!(
        !slip.ok && slip.text.contains("CREDENTIAL_ID_INVALID") && !slip.text.contains("0b6c2d4e"),
        "{}",
        slip.text
    );
    let stray = run(&["remove", "SECRET-STRAY"], None);
    assert!(
        !stray.ok && !stray.text.contains("SECRET-STRAY"),
        "{}",
        stray.text
    );
    // A number or an id that is no request is never echoed; a non-credential receipt carries
    // no sweep note.
    let bad_number = run(
        &["priority", "--id", "helius-rpc", "--priority", "SECRET-NUM"],
        None,
    );
    assert!(
        !bad_number.ok && !bad_number.text.contains("SECRET-NUM"),
        "{}",
        bad_number.text
    );
    let hex_key = "0123456789abcdef0123456789abcdef";
    let bad_update = run(&["update", "--id", hex_key], None);
    assert!(
        !bad_update.ok && !bad_update.text.contains(hex_key),
        "{}",
        bad_update.text
    );
    assert!(
        !added.text.contains("sweep") && !selected.text.contains("sweep"),
        "{}",
        selected.text
    );
    assert!(
        std::fs::read_to_string(&env).unwrap().contains("ENV-KEY"),
        "the original is kept"
    );

    let status = run(&["status"], None);
    assert!(status.ok, "{}", status.text);
    assert!(status.text.contains("provider helius-rpc kind=rpc host=mainnet.helius-rpc.com auth=query:api-key credential=helius@1"), "{}", status.text);
    assert!(status.text.contains("selected=true"), "{}", status.text);
    assert!(
        status
            .text
            .contains("credential helius-env revision=1 state=active origin=env:HELIUS_API_KEY"),
        "{}",
        status.text
    );
    // No secret anywhere in the outputs or the registry's records.
    for secret in ["KEY-ONE", "ENV-KEY"] {
        assert!(!status.text.contains(secret));
        for e in std::fs::read_dir(project.join("data/providers")).unwrap() {
            let bytes = std::fs::read(e.unwrap().path()).unwrap();
            assert!(
                !bytes.windows(secret.len()).any(|w| w == secret.as_bytes()),
                "{secret} in a record"
            );
        }
    }
    let _ = std::fs::remove_dir_all(base);
}
