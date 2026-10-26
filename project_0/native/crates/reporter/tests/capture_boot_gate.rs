//! DEC-07: the desktop raw capture over the real binary — without an admitted boot flow (mode
//! market) it refuses itself with BOOT_AUDIT_REQUIRED before any network read: the configured
//! WebSocket never sees a connection, and the result names the refusal.
use std::net::TcpListener;
use std::process::Command;
use std::time::Duration;

#[test]
fn dec07_an_unapproved_raw_capture_reads_nothing() {
    let dir = std::env::temp_dir().join(format!("p0_capture_gate_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let ws = format!("ws://{}", listener.local_addr().unwrap());
    let result = dir.join("capture.result");
    let base = |cmd: &mut Command| {
        cmd.env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("P0_WS_URL", &ws)
            .env("P0_PROGRAM_ID", "11111111111111111111111111111111")
            .env("P0_MARKET_SOURCE", "logs")
            .env("P0_MARKET_OUT_DIR", dir.join("data"))
            .env("P0_MARKET_DURATION_SECS", "60")
            .env("P0_RESULT_FILE", &result)
            .env("P0_LAUNCH_ID", "gate-test");
    };
    // Autostart/terminal: no admission at all; and a market start whose record is missing.
    for admission in [None, Some(dir.join("absent.p0j"))] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_p0-market-capture-reported"));
        base(&mut cmd);
        if let Some(record) = &admission {
            cmd.env("P0_MODE", "market")
                .env("P0_NETWORK_AUDIT", record)
                .env("P0_NETWORK_AUDIT_TOKEN", "0123456789abcdef")
                .env("P0_NETWORK_AUDIT_APPROVED_AT", "1")
                .env("P0_MEASUREMENT_DURATION", "1m")
                .env("P0_PROJECT_DIR", &dir);
        }
        let out = cmd.output().unwrap();
        assert!(!out.status.success(), "{admission:?}");
        let text = std::fs::read_to_string(&result).unwrap_or_default();
        let expected = if admission.is_none() {
            "BOOT_AUDIT_REQUIRED"
        } else {
            "BOOT_NETWORK_EVIDENCE_INVALID"
        };
        assert!(
            text.contains(expected),
            "{admission:?}: {text}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    std::thread::sleep(Duration::from_millis(50));
    assert!(
        listener.accept().is_err(),
        "no connection was ever made to the configured WebSocket"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
