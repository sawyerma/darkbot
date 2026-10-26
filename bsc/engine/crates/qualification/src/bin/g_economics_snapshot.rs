//! Phase G (Economics gate §15): freezes the versioned economics snapshot
//! of the tree, or verifies a frozen one against it. Offline: it reads the
//! repository's files only. Run from the repository root.
//!
//! ```text
//! g_economics_snapshot [--out-root bsc/native/evidence/g]
//!     writes <out-root>/economics_snapshot_<UTC>/snapshot.dbrec
//! g_economics_snapshot --verify <snapshot.dbrec>
//!     MATCH, or STALE with the changed bound files (exit 2)
//! ```
#![forbid(unsafe_code)]

use darkbot_bsc_qualification::c1_common::{Args, git_head};
use darkbot_bsc_qualification::economics_snapshot::{freeze, verify};
use std::path::Path;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "g01_common/util.rs"]
#[allow(dead_code)]
mod util;

/// `--out-root`.
const OUT_ROOT: &str = "bsc/native/evidence/g";

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("g_economics_snapshot: {e}");
            ExitCode::from(2)
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn run() -> Result<ExitCode, String> {
    let args = Args::parse(std::env::args().skip(1));
    args.only(&["out-root", "verify"])?;
    let root = Path::new(".");
    if !root.join("bsc/engine/crates/trading").is_dir() {
        return Err("run from the repository root".into());
    }
    if let Some(path) = args.opt("verify")? {
        let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        return Ok(match verify(root, &bytes)? {
            Ok(hash) => {
                println!("economics snapshot {}: MATCH", hex(&hash));
                ExitCode::SUCCESS
            }
            Err(changed) => {
                println!(
                    "economics snapshot {path}: STALE ({})",
                    if changed.is_empty() {
                        "scopes, policies or model version changed".to_string()
                    } else {
                        changed.join(", ")
                    }
                );
                ExitCode::from(2)
            }
        });
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    let observed = u64::try_from(now.as_millis()).map_err(|e| e.to_string())?;
    let head = git_head(root).ok_or("no git HEAD")?;
    let (bytes, hash) = freeze(root, observed, &head)?;
    let dir = Path::new(args.opt("out-root")?.unwrap_or(OUT_ROOT))
        .join(format!("economics_snapshot_{}", util::utc_stamp(observed)));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let out = dir.join("snapshot.dbrec");
    std::fs::write(&out, &bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    println!(
        "economics snapshot {} (git {head}) -> {}",
        hex(&hash),
        out.display()
    );
    Ok(ExitCode::SUCCESS)
}
