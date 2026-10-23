//! Secret and evidence handoffs of the G0.6a DRY harness (plan v9.1 D1–D3),
//! offline: no RPC, no key read, no signature, no send.
//!
//! ```text
//! g06a_dry_handoff exec-salt --candidate <id> --generation <n> --preflight <preflight.dbrec> -- <consumer...>
//! ```
//!
//! `exec-salt` (D1) runs exactly one allowlisted local EVM/DRY consumer with
//! the selected SELF_B candidate's secret salt in that child's environment
//! only. The candidate comes from the private store (salt re-derived against
//! its commitment, token and calldata hash); the selection must still name
//! exactly that candidate and generation, and the run's preflight must be of
//! the candidate's deployment. Non-signing consumers never see wallet keys.
//! The salt is never on argv, never printed and never part of an error; a
//! wrong, missing or stale candidate STOPs (exit 2) without a fallback to A.
//! Runs at the root of the main checkout.
#![allow(dead_code, unused_imports)]
#[path = "g07_mainnet_self_launch_buy/builder_submission.rs"]
mod builder_submission;
#[path = "g07_candidate/candidate.rs"]
mod candidate;
#[path = "g07_mainnet_self_launch_buy/chain_reads.rs"]
mod chain_reads;
#[path = "g07_mainnet_self_launch_buy/inputs.rs"]
mod inputs;
#[path = "g07_mainnet_self_launch_buy/shared.rs"]
mod shared;
#[path = "g07_mainnet_self_launch_buy/transactions.rs"]
mod transactions;

// candidate.rs resolves its helpers through these, as in g07_candidate.
use builder_submission::*;
use candidate::{CANDIDATE_STATE_ROOT, Deployment, Profile, Store};
use chain_reads::*;
use inputs::*;
use shared::*;
use std::str::FromStr;
use transactions::*;

/// Wallet keys a non-signing consumer never receives.
const KEYS: [&str; 4] = [
    "DARKBOT_G06A_LAUNCHER_PRIVATE_KEY",
    "DARKBOT_G06A_BUYER_PRIVATE_KEY",
    "BSC_LAUNCHER_PRIVATE_KEY",
    "BSC_BUYER_PRIVATE_KEY",
];
const SALT_ENV: &str = "DARKBOT_G06A_SALT";
const BUNDLE: &str = "bsc/engine/target/debug/g06a_self_launch_bundle";

/// Whether `argv` is one of the exact salt consumers, and whether it signs
/// (and so keeps the keys its caller passed).
fn consumer(argv: &[String], root: &Path) -> Result<bool, String> {
    let a: Vec<&str> = argv.iter().map(String::as_str).collect();
    let tool = |name: &str| -> Result<(), String> {
        let meta = fs::symlink_metadata(root.join(name)).map_err(|e| format!("{name}: {e}"))?;
        if meta.file_type().is_file() {
            Ok(())
        } else {
            Err(format!("{name} must be a regular file"))
        }
    };
    let bundle = root.canonicalize().map_err(|e| e.to_string())?.join(BUNDLE);
    match a.as_slice() {
        [
            "node",
            script @ "bsc/tools/g06a_economics_evm_crosscheck.cjs",
            _,
            _,
            "--private",
        ]
        | [
            "node",
            script @ "bsc/tools/g06a_launch_path_trace.cjs",
            _,
            _,
        ] => {
            tool(script)?;
            Ok(false)
        }
        ["node", script @ "bsc/tools/g06a_local_simulate.cjs", _] => {
            tool(script)?;
            Ok(true)
        }
        [program, "--verify-evidence", _, "--out", _]
            if *program == BUNDLE || Path::new(program) == bundle =>
        {
            tool(BUNDLE)?;
            Ok(true)
        }
        _ => Err("not an allowlisted salt consumer".into()),
    }
}

/// The consumer's environment: the caller's without any inherited salt, the
/// keys only for a signing consumer, and the candidate's salt.
fn child_environment(
    inherited: impl Iterator<Item = (String, String)>,
    signing: bool,
    salt: &str,
) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = inherited
        .filter(|(k, _)| k != SALT_ENV && (signing || !KEYS.contains(&k.as_str())))
        .collect();
    env.push((SALT_ENV.into(), salt.into()));
    env
}

fn flag<'a>(args: &'a [String], name: &str) -> Result<&'a str, String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
        .ok_or_else(|| format!("{name} <value> is required"))
}

/// The verified SELF_B candidate and its salt (as launch-word hex).
fn selected_b(
    store_dir: &Path,
    id: &str,
    generation: u64,
    preflight: &[u8],
) -> Result<String, String> {
    let store = Store::existing(store_dir)?.ok_or("no G0.7 candidate state")?;
    let selection = store.selection()?.ok_or("no G0.7 candidate selected")?;
    if selection.profile != Profile::B {
        return Err(
            "the selected candidate is not SELF_B; only a secret salt is handed off".into(),
        );
    }
    let expected = B256::from_str(id).map_err(|_| "--candidate must be a 32-byte hex ID")?;
    if selection.candidate_id != expected || selection.generation != generation {
        return Err(
            "the selection is not the expected SELF_B candidate and generation (stale)".into(),
        );
    }
    let (candidate, salt) = store
        .load(Profile::B)?
        .ok_or("candidate SELF_B does not exist")?;
    if candidate.id != expected {
        return Err("the stored SELF_B candidate is not the expected one".into());
    }
    let run = Deployment::from_preflight(preflight)?;
    let own = &candidate.deployment;
    if (
        run.chain_id,
        run.portal,
        &run.portal_version,
        run.portal_implementation,
    ) != (
        own.chain_id,
        own.portal,
        &own.portal_version,
        own.portal_implementation,
    ) || run.portal_implementation_code_sha256 != own.portal_implementation_code_sha256
        || run.tax_v3 != own.tax_v3
        || run.tax_v3_code_sha256 != own.tax_v3_code_sha256
    {
        return Err("the run's preflight is not of the candidate's deployment".into());
    }
    Ok(format!("0x{}", hex::encode(salt.launch_word())))
}

fn exec_salt(args: &[String]) -> Result<i32, String> {
    let split = args
        .iter()
        .position(|a| a == "--")
        .ok_or("exec-salt needs -- <consumer>")?;
    let (own, consumer_argv) = (&args[..split], &args[split + 1..]);
    let root = Path::new(".");
    repository_root_check(root)?;
    let signing = consumer(consumer_argv, root)?;
    let generation: u64 = flag(own, "--generation")?
        .parse()
        .map_err(|_| "--generation must be an unsigned integer")?;
    let preflight = flag(own, "--preflight")?;
    let preflight = fs::read(preflight).map_err(|e| format!("read {preflight}: {e}"))?;
    let salt = selected_b(
        Path::new(CANDIDATE_STATE_ROOT),
        flag(own, "--candidate")?,
        generation,
        &preflight,
    )?;
    let status = Command::new(&consumer_argv[0])
        .args(&consumer_argv[1..])
        .env_clear()
        .envs(child_environment(env::vars(), signing, &salt))
        .status()
        .map_err(|e| format!("start the consumer: {e}"))?;
    Ok(status.code().unwrap_or(1))
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("exec-salt") => exec_salt(&args[1..]),
        _ => Err("usage: g06a_dry_handoff exec-salt --candidate <id> --generation <n> --preflight <preflight.dbrec> -- <consumer...>".into()),
    };
    match result {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("STOP: {e}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn only_the_exact_local_consumers_receive_a_salt_and_only_signers_keep_keys() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .canonicalize()
            .unwrap();
        let ok = |a: &[&str]| consumer(&argv(a), &root);
        assert_eq!(
            ok(&["node", "bsc/tools/g06a_launch_path_trace.cjs", "p", "o"]),
            Ok(false)
        );
        assert_eq!(
            ok(&[
                "node",
                "bsc/tools/g06a_economics_evm_crosscheck.cjs",
                "p",
                "o",
                "--private"
            ]),
            Ok(false)
        );
        assert_eq!(
            ok(&["node", "bsc/tools/g06a_local_simulate.cjs", "o"]),
            Ok(true)
        );
        for refused in [
            &[
                "node",
                "bsc/tools/g06a_economics_evm_crosscheck.cjs",
                "p",
                "o",
            ][..],
            &[
                "node",
                "bsc/tools/g06a_launch_path_trace.cjs",
                "p",
                "o",
                "extra",
            ],
            &["node", "-e", "console.log(process.env)"],
            &["node", "./bsc/tools/g06a_launch_path_trace.cjs", "p", "o"],
            &["sh", "-c", "env"],
            &["env"],
            &[],
        ] {
            assert!(ok(refused).is_err(), "{refused:?}");
        }
        let inherited = [
            ("PATH", "/bin"),
            ("DARKBOT_G06A_SALT", "0xold"),
            ("DARKBOT_G06A_LAUNCHER_PRIVATE_KEY", "k1"),
            ("BSC_BUYER_PRIVATE_KEY", "k2"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()));
        let plain = child_environment(inherited.clone().into_iter(), false, "0xnew");
        let names: Vec<_> = plain.iter().map(|(k, v)| format!("{k}={v}")).collect();
        assert_eq!(names, ["PATH=/bin", "DARKBOT_G06A_SALT=0xnew"]);
        let signer = child_environment(inherited.into_iter(), true, "0xnew");
        assert_eq!(signer.len(), 4);
        assert!(!signer.iter().any(|(_, v)| v == "0xold"));
    }
}
