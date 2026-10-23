//! Secret and evidence handoffs of the G0.6a DRY harness (plan v9.1 D1–D3),
//! offline: no RPC, no key read, no signature, no send.
//!
//! ```text
//! g06a_dry_handoff selection --profile SELF_A|SELF_B
//! g06a_dry_handoff exec-salt --candidate <id> --generation <n> --preflight <preflight.dbrec> -- <consumer...>
//! g06a_dry_handoff prepare --run <prepare_dir> --profile SELF_A|SELF_B --frozen <frozen_dir>
//! g06a_dry_handoff verify --prepare <prepare_dir> --frozen <frozen_dir> --out <new sign_dir>
//! ```
//!
//! `prepare` (D3) closes a PREPARE run: its R0 record must be coherent
//! (shared R0 check, anchored at the pinned frozen preflight) and end at the
//! run's H1 preflight block; it writes `prepare_handoff.dbrec` with only
//! that record's SHA-256 and chain and the selected candidate's reference.
//! `verify` (D3) is the SIGN side: the handoff record, the R0 record's
//! SHA-256, schema and coherence, and an unchanged store selection; it then
//! creates the new private SIGN directory (0700) with a copy of the R0
//! record (to extend at H2) and of the public candidate record. Nothing else
//! of H1 crosses; the PREPARE run is never written.
//!
//! `selection` (D2) verifies the selected candidate of the expected profile
//! in the private store (salt re-derived) and prints only its public
//! reference: profile, ID, generation, token and salt commitment.
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
use darkbot_bsc_record::{Value, decode};
use inputs::*;
use shared::*;
use std::os::unix::fs::OpenOptionsExt;
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
/// SHA-256 of the frozen DRY's preflight: the trusted start of every R0 chain.
const FROZEN_PREFLIGHT_SHA256: &str =
    "2b018137099c69bfd1069abfe08f320d19ca0ab97d1ee7905d5c43c594de33e1";
const R0_RECORD: &str = "portal_upgrade.dbrec";
const HANDOFF_RECORD: &str = "prepare_handoff.dbrec";
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

fn selection(args: &[String]) -> Result<i32, String> {
    let profile = Profile::parse(flag(args, "--profile")?)?;
    repository_root_check(Path::new("."))?;
    let store =
        Store::existing(Path::new(CANDIDATE_STATE_ROOT))?.ok_or("no G0.7 candidate state")?;
    let selection = store.selection()?.ok_or("no G0.7 candidate selected")?;
    if selection.profile != profile {
        return Err(format!(
            "the selected candidate is {}, not {}",
            selection.profile.name(),
            profile.name()
        ));
    }
    let (candidate, _) = store
        .load(profile)?
        .ok_or_else(|| format!("candidate {} does not exist", profile.name()))?;
    if candidate.id != selection.candidate_id {
        return Err("the selection names another candidate than the stored one".into());
    }
    // Public values only (the candidate record holds them as well).
    let mut out = std::io::stdout().lock();
    writeln!(out, "profile={}", profile.name()).map_err(|e| e.to_string())?;
    writeln!(out, "candidate_id={}", candidate.id).map_err(|e| e.to_string())?;
    writeln!(out, "generation={}", selection.generation).map_err(|e| e.to_string())?;
    writeln!(out, "token={:#x}", candidate.token).map_err(|e| e.to_string())?;
    writeln!(out, "salt_commitment={}", candidate.salt_commitment).map_err(|e| e.to_string())?;
    Ok(0)
}

/// The frozen DRY's preflight, by its pinned SHA-256.
fn frozen_preflight(frozen_dir: &str) -> Result<Record, String> {
    let bytes = fs::read(Path::new(frozen_dir).join("preflight.dbrec"))
        .map_err(|e| format!("frozen preflight: {e}"))?;
    if hex::encode(darkbot_bsc_record::sha256(&bytes)) != FROZEN_PREFLIGHT_SHA256 {
        return Err("the frozen preflight is not the pinned one".into());
    }
    decode(&schema::g06a::preflight::SCHEMA, &bytes).map_err(|e| format!("frozen preflight: {e}"))
}

/// A coherent R0 record (the shared check) from its exact bytes.
fn coherent_r0(bytes: &[u8], frozen: &Record) -> Result<Record, String> {
    let scan = decode(&schema::g06a::portal_upgrade::SCHEMA, bytes)
        .map_err(|e| format!("R0 record: {e}"))?;
    let problems = darkbot_bsc_execution::r0::chain_problems(&scan, frozen);
    if !problems.is_empty() {
        return Err(format!(
            "R0 record is not coherent: {}",
            problems.join("; ")
        ));
    }
    Ok(scan)
}

/// The verified selection of `profile`: (candidate ID, generation, the
/// public candidate record bytes).
fn verified_selection(profile: Profile) -> Result<(B256, u64, Vec<u8>), String> {
    let store =
        Store::existing(Path::new(CANDIDATE_STATE_ROOT))?.ok_or("no G0.7 candidate state")?;
    let selection = store.selection()?.ok_or("no G0.7 candidate selected")?;
    if selection.profile != profile {
        return Err(format!(
            "the selected candidate is {}, not {}",
            selection.profile.name(),
            profile.name()
        ));
    }
    let (candidate, _) = store
        .load(profile)?
        .ok_or_else(|| format!("candidate {} does not exist", profile.name()))?;
    if candidate.id != selection.candidate_id {
        return Err("the selection names another candidate than the stored one".into());
    }
    Ok((
        selection.candidate_id,
        selection.generation,
        candidate.draft().encode().map_err(|e| e.to_string())?,
    ))
}

fn prepare(args: &[String]) -> Result<i32, String> {
    use schema::g06a::{portal_upgrade as up, preflight as pf, prepare_handoff as h};
    let run = Path::new(flag(args, "--run")?);
    let profile = Profile::parse(flag(args, "--profile")?)?;
    repository_root_check(Path::new("."))?;
    let frozen = frozen_preflight(flag(args, "--frozen")?)?;
    let r0 = fs::read(run.join(R0_RECORD)).map_err(|e| format!("{R0_RECORD}: {e}"))?;
    let scan = coherent_r0(&r0, &frozen)?;
    let pre_bytes = fs::read(run.join("preflight.dbrec")).map_err(|e| format!("preflight: {e}"))?;
    let pre = decode(&pf::SCHEMA, &pre_bytes).map_err(|e| format!("preflight: {e}"))?;
    let e = |e: darkbot_bsc_record::RecordError| e.to_string();
    if scan.u64(up::SCANNED_TO_BLOCK).map_err(e)? != pre.u64(pf::LATEST_BLOCK_NUMBER).map_err(e)?
        || scan.b256(up::LAST_BLOCK_HASH).map_err(e)?
            != pre.b256(pf::LATEST_BLOCK_HASH).map_err(e)?
    {
        return Err("the R0 record does not end at this run's H1 preflight block".into());
    }
    let (id, generation, _) = verified_selection(profile)?;
    let mut d = Draft::new(&h::SCHEMA);
    d.set(schema::PROVENANCE_TOOL, "g06a_dry_handoff prepare")
        .set(schema::PROVENANCE_OBSERVED_UNIX_MS, unix_time_ms())
        .set(h::MODE, h::MODE_TEXT)
        .set(h::R0_RECORD, R0_RECORD)
        .set(
            h::R0_RECORD_SHA256,
            Value::B256(darkbot_bsc_record::sha256(&r0)),
        )
        .set(
            h::R0_FROM_BLOCK,
            scan.u64(up::SCANNED_FROM_BLOCK).map_err(e)?,
        )
        .set(h::R0_TO_BLOCK, scan.u64(up::SCANNED_TO_BLOCK).map_err(e)?)
        .set(
            h::R0_FIRST_PARENT_HASH,
            Value::B256(scan.b256(up::FIRST_PARENT_HASH).map_err(e)?),
        )
        .set(
            h::R0_LAST_BLOCK_HASH,
            Value::B256(scan.b256(up::LAST_BLOCK_HASH).map_err(e)?),
        )
        .set(h::CANDIDATE_PROFILE, profile.name())
        .set(h::CANDIDATE_ID, Value::B256(id.0))
        .set(h::CANDIDATE_GENERATION, generation)
        .set(
            h::H1_PREFLIGHT_SHA256,
            Value::B256(darkbot_bsc_record::sha256(&pre_bytes)),
        );
    let bytes = d.encode().map_err(|e| e.to_string())?;
    create_new(&run.join(HANDOFF_RECORD), &bytes)?;
    Ok(0)
}

fn create_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .and_then(|mut f| f.write_all(bytes).and_then(|()| f.sync_all()))
        .map_err(|e| format!("create {}: {e}", path.display()))
}

fn verify(args: &[String]) -> Result<i32, String> {
    use schema::g06a::{portal_upgrade as up, prepare_handoff as h};
    use std::os::unix::fs::DirBuilderExt;
    let prepared = Path::new(flag(args, "--prepare")?);
    let out = Path::new(flag(args, "--out")?);
    repository_root_check(Path::new("."))?;
    let frozen = frozen_preflight(flag(args, "--frozen")?)?;
    let handoff = decode(
        &h::SCHEMA,
        &fs::read(prepared.join(HANDOFF_RECORD)).map_err(|e| format!("{HANDOFF_RECORD}: {e}"))?,
    )
    .map_err(|e| format!("{HANDOFF_RECORD}: {e}"))?;
    let e = |e: darkbot_bsc_record::RecordError| e.to_string();
    if handoff.text(h::MODE).map_err(e)? != h::MODE_TEXT
        || handoff.text(h::R0_RECORD).map_err(e)? != R0_RECORD
    {
        return Err("not a PREPARE handoff of an R0 record".into());
    }
    let r0 = fs::read(prepared.join(R0_RECORD)).map_err(|e| format!("{R0_RECORD}: {e}"))?;
    if darkbot_bsc_record::sha256(&r0) != handoff.b256(h::R0_RECORD_SHA256).map_err(e)? {
        return Err(
            "the R0 record is not the one the handoff binds (modified or substituted)".into(),
        );
    }
    let scan = coherent_r0(&r0, &frozen)?;
    let stated = (
        scan.u64(up::SCANNED_FROM_BLOCK).map_err(e)?,
        scan.u64(up::SCANNED_TO_BLOCK).map_err(e)?,
        scan.b256(up::FIRST_PARENT_HASH).map_err(e)?,
        scan.b256(up::LAST_BLOCK_HASH).map_err(e)?,
    );
    let bound = (
        handoff.u64(h::R0_FROM_BLOCK).map_err(e)?,
        handoff.u64(h::R0_TO_BLOCK).map_err(e)?,
        handoff.b256(h::R0_FIRST_PARENT_HASH).map_err(e)?,
        handoff.b256(h::R0_LAST_BLOCK_HASH).map_err(e)?,
    );
    if stated != bound {
        return Err("the R0 record's chain differs from the handoff".into());
    }
    let profile = Profile::parse(handoff.text(h::CANDIDATE_PROFILE).map_err(e)?)?;
    let (id, generation, record) = verified_selection(profile)?;
    if id.0 != handoff.b256(h::CANDIDATE_ID).map_err(e)?
        || generation != handoff.u64(h::CANDIDATE_GENERATION).map_err(e)?
    {
        return Err(
            "the selected candidate or its generation changed since PREPARE (stale)".into(),
        );
    }
    // A new private directory; never an existing one.
    let parent = out.parent().ok_or("--out needs a parent directory")?;
    if !fs::symlink_metadata(parent).is_ok_and(|m| m.file_type().is_dir()) {
        return Err("the SIGN parent directory must be a real directory".into());
    }
    fs::DirBuilder::new()
        .mode(0o700)
        .create(out)
        .map_err(|e| format!("create {}: {e}", out.display()))?;
    create_new(&out.join("portal_upgrade_scan.dbrec"), &r0)?;
    create_new(&out.join("candidate.dbrec"), &record)?;
    let mut stdout = std::io::stdout().lock();
    for line in [
        format!("profile={}", profile.name()),
        format!("candidate_id={id}"),
        format!("generation={generation}"),
        format!("r0_to_block={}", stated.1),
    ] {
        writeln!(stdout, "{line}").map_err(|e| e.to_string())?;
    }
    Ok(0)
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
        Some("selection") => selection(&args[1..]),
        Some("exec-salt") => exec_salt(&args[1..]),
        Some("prepare") => prepare(&args[1..]),
        Some("verify") => verify(&args[1..]),
        _ => Err("usage: g06a_dry_handoff selection | exec-salt | prepare | verify (see the source header)".into()),
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
