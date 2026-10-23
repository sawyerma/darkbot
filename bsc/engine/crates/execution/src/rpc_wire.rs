//! JSON-RPC over curl without the endpoint or the request on curl's argv:
//! both reach curl as a config on stdin (`-K -`). A credential-bearing RPC
//! URL (API key in path or query) or a launch calldata is never visible in
//! the process list, and transport errors are redacted. The caller passes
//! its own environment-cleared `Command` for the curl program.

use std::io::Write;
use std::process::{Command, Stdio};

/// curl's time bounds; `compressed` asks for a compressed response.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub connect_timeout_s: u32,
    pub max_time_s: u32,
    pub compressed: bool,
}

fn quoted(what: &str, text: &str) -> Result<String, String> {
    if text.chars().any(char::is_control) {
        return Err(format!("{what} contains a control character"));
    }
    Ok(format!(
        "\"{}\"",
        text.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

/// The curl config of one JSON POST: URL, content type and body.
pub fn config(url: &str, body: &str) -> Result<String, String> {
    Ok(format!(
        "url = {}\nheader = \"Content-Type: application/json\"\ndata-binary = {}\n",
        quoted("RPC URL", url)?,
        quoted("request", body)?
    ))
}

/// `text` without the endpoint: the URL, everything after its scheme, its
/// path and query, and any other URL-like word.
pub fn redact(url: &str, text: &str) -> String {
    let mut out = text.to_string();
    let mut parts = vec![url];
    if let Some((_, rest)) = url.split_once("://") {
        parts.push(rest);
        if let Some(at) = rest.find(['/', '?']) {
            parts.push(&rest[at..]);
        }
    }
    for part in parts {
        if part.len() > 1 {
            out = out.replace(part, "[endpoint]");
        }
    }
    out.split_whitespace()
        .map(|w| if w.contains("://") { "[endpoint]" } else { w })
        .collect::<Vec<_>>()
        .join(" ")
}

/// One POST through `curl` (an environment-cleared command for the curl
/// program). Returns the response body; never puts `url` or `body` on argv.
pub fn post(mut curl: Command, url: &str, body: &str, limits: Limits) -> Result<Vec<u8>, String> {
    let config = config(url, body)?;
    // -q first: no .curlrc, whatever the environment.
    curl.arg("-q")
        .arg("-sS")
        .args(["--connect-timeout", &limits.connect_timeout_s.to_string()])
        .args(["--max-time", &limits.max_time_s.to_string()]);
    if limits.compressed {
        curl.arg("--compressed");
    }
    curl.args(["-K", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = curl.spawn().map_err(|e| format!("execute curl: {e}"))?;
    let written = child
        .stdin
        .take()
        .ok_or_else(|| "curl stdin unavailable".to_string())
        .and_then(|mut stdin| {
            stdin
                .write_all(config.as_bytes())
                .map_err(|e| format!("curl config: {e}"))
        });
    let out = child.wait_with_output().map_err(|e| format!("curl: {e}"))?;
    written?;
    if !out.status.success() {
        let detail: String = redact(url, &String::from_utf8_lossy(&out.stderr))
            .chars()
            .take(300)
            .collect();
        return Err(format!(
            "curl: transport failure (exit {:?}): {detail}",
            out.status.code()
        ));
    }
    Ok(out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const SECRET_URL: &str = "https://rpc.example.org/v1/APIKEYSECRET?token=TOKENSECRET";

    #[test]
    fn the_config_escapes_and_refuses_control_characters() {
        let c = config(SECRET_URL, r#"{"a":"b\c"}"#).unwrap();
        assert!(c.contains(r#"data-binary = "{\"a\":\"b\\c\"}""#));
        assert!(config("https://x/\nheader = evil", "{}").is_err());
        assert!(config(SECRET_URL, "{\"a\":1}\r").is_err());
    }

    #[test]
    fn errors_never_show_the_endpoint() {
        let text = format!(
            "curl: (6) Could not resolve host for {SECRET_URL} and /v1/APIKEYSECRET?token=TOKENSECRET"
        );
        let clean = redact(SECRET_URL, &text);
        assert!(
            !clean.contains("APIKEYSECRET") && !clean.contains("TOKENSECRET"),
            "{clean}"
        );
    }

    #[test]
    fn every_rpc_transport_posts_through_stdin() {
        for (name, source) in [
            (
                "G0.7",
                include_str!("bin/g07_mainnet_self_launch_buy/chain_reads.rs"),
            ),
            ("R0", include_str!("bin/g06a_preflight_record/coverage.rs")),
            (
                "G0.8",
                include_str!("../../qualification/src/bin/g08_mainnet_verify.rs"),
            ),
        ] {
            assert!(source.contains("rpc_wire::post("), "{name}");
            for argv in ["\"--data\"", "\"--data-binary\"", "\"-H\""] {
                assert!(!source.contains(argv), "{name} still passes {argv} on argv");
            }
        }
    }

    /// A stand-in curl that records its argv and stdin, then fails with the
    /// endpoint on stderr.
    #[test]
    fn neither_url_nor_request_reach_argv_and_a_failure_is_redacted() {
        let dir = std::env::temp_dir().join(format!("rpc-wire-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("curl");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{0}/argv'\ncat > '{0}/stdin'\necho \"failed for {SECRET_URL}\" >&2\nexit 7\n",
                dir.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut cmd = Command::new(&fake);
        cmd.env_clear();
        let body =
            r#"{"jsonrpc":"2.0","method":"eth_estimateGas","params":[{"data":"0xSALTCALLDATA"}]}"#;
        let limits = Limits {
            connect_timeout_s: 2,
            max_time_s: 5,
            compressed: false,
        };
        let error = post(cmd, SECRET_URL, body, limits).unwrap_err();
        let argv = std::fs::read_to_string(dir.join("argv")).unwrap();
        let stdin = std::fs::read_to_string(dir.join("stdin")).unwrap();
        assert!(argv.starts_with("-q\n"), "{argv}");
        for secret in ["APIKEYSECRET", "TOKENSECRET", "SALTCALLDATA", "https://"] {
            assert!(!argv.contains(secret), "{secret} on argv: {argv}");
            assert!(!error.contains(secret), "{secret} in error: {error}");
        }
        assert!(stdin.contains("APIKEYSECRET") && stdin.contains("SALTCALLDATA"));
        assert!(error.contains("exit Some(7)"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
