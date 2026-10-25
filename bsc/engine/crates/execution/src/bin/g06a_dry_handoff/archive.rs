//! BSC-006 (BSC_Bug_V2 P3-F5): the private evidence of a SIGN run
//! (`bsc/.local/g06a_sign/<run>`, which a v5 contract binds by path and
//! SHA-256) archived and restored, offline. No key is read, nothing is sent.
//!
//! `archive --sign <bound run> --to <dir>` copies a complete, unchanged and
//! clean run into `<dir>/<run>` (0700, files 0400): never into the
//! repository outside `bsc/.local`, never over an existing archive.
//! Complete and unchanged: exactly the files its `SHA256SUMS` lists, each a
//! private regular file of that SHA-256. Clean: every record is of a known
//! evidence schema and none a salt record; no file holds raw-transaction-
//! sized hex, the private key of an address the records or the contract
//! name, or (SELF_B) the candidate's salt in any spelling. It prints
//! `SUMS_SHA256`, the hash of the manifest; keep it apart from the archive.
//!
//! `restore --from <archive>/<run> --expect <SUMS_SHA256>` checks the archive
//! the same way (its manifest must hash to the anchor) and restores it only
//! at its bound path: atomically when absent, never over other content. A
//! v5 contract binding that run must then be reproduced read-only from the
//! restored evidence (proposal or approved variant); otherwise the run is
//! moved aside to `<run>.unbound-<ms>` and nothing binds it.

use super::*;
use darkbot_bsc_execution::acceptance_contract::{
    Authorization, BUYER, LAUNCHER, REQUAL_DRY, attempt1_v5, attempt1_v5_evidence,
};
use darkbot_bsc_execution::acceptance_guard::V5_RELATIVE_PATH;
use darkbot_bsc_record::schema::g07::contract as c;
use darkbot_bsc_record::schema::g07_candidate::{candidate as cand, secret};
use std::collections::BTreeSet;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};

const SIGN_ROOT: &str = "bsc/.local/g06a_sign";
const SUMS: &str = "SHA256SUMS";
const CANDIDATE_RECORD: &str = "candidate.dbrec";

/// A run read completely into memory: its listed files and its manifest.
struct Run {
    files: Vec<(String, Vec<u8>)>,
    sums: Vec<u8>,
}

/// `g06a_sign_<YYYYMMDD>T<HHMMSS>Z`, the name the harness gives a run.
fn run_name(name: &str) -> Result<(), String> {
    let stamp = name
        .strip_prefix("g06a_sign_")
        .unwrap_or_default()
        .as_bytes();
    let digits = |r: std::ops::Range<usize>| stamp[r].iter().all(u8::is_ascii_digit);
    if stamp.len() != 16 || stamp[8] != b'T' || stamp[15] != b'Z' || !digits(0..8) || !digits(9..15)
    {
        return Err(format!("{name:?} is not a SIGN run name"));
    }
    Ok(())
}

/// A real file or directory of the repository's owner, closed to group and
/// others.
fn private(path: &Path, label: &str, dir: bool) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("{label}: {e}"))?;
    let kind = if dir { meta.is_dir() } else { meta.is_file() };
    if !kind {
        let what = if dir { "a directory" } else { "a regular file" };
        return Err(format!("{label} is not {what}"));
    }
    if meta.mode() & 0o077 != 0 || meta.uid() != owner_uid(Path::new("."))? {
        return Err(format!(
            "{label} is not private (mode {:o}, or another owner)",
            meta.mode() & 0o777
        ));
    }
    Ok(())
}

/// The run in `dir`: exactly the files its manifest lists, each private and
/// of its listed SHA-256.
fn read_run(dir: &Path) -> Result<Run, String> {
    private(dir, "the run directory", true)?;
    private(&dir.join(SUMS), SUMS, false)?;
    let sums = fs::read(dir.join(SUMS)).map_err(|e| format!("{SUMS}: {e}"))?;
    let text = std::str::from_utf8(&sums).map_err(|_| format!("{SUMS} is not text"))?;
    let mut listed = Vec::new();
    for row in text.lines() {
        let (digest, name) = row
            .split_once("  ")
            .ok_or_else(|| format!("malformed {SUMS} line"))?;
        let name = name.strip_prefix("./").unwrap_or(name);
        let plain = !name.is_empty() && !name.contains('/') && name != SUMS;
        let known = [".dbrec", ".txt", ".md"].iter().any(|x| name.ends_with(x));
        let digest = B256::from_str(digest).map_err(|_| format!("malformed {SUMS} digest"))?;
        if !plain || !known || listed.iter().any(|(n, _)| n == name) {
            return Err(format!(
                "{SUMS} lists an unexpected or repeated name {name:?}"
            ));
        }
        listed.push((name.to_string(), digest));
    }
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let name = entry.map_err(|e| e.to_string())?.file_name();
        let name = name.to_string_lossy();
        if name != SUMS && !listed.iter().any(|(n, _)| *n == name) {
            return Err(format!("{name:?} is not in {SUMS}"));
        }
    }
    let mut files = Vec::new();
    for (name, digest) in listed {
        private(&dir.join(&name), &name, false)?;
        let bytes = fs::read(dir.join(&name)).map_err(|e| format!("{name}: {e}"))?;
        if darkbot_bsc_record::sha256(&bytes) != digest.0 {
            return Err(format!("{name} does not match its SHA-256 in {SUMS}"));
        }
        files.push((name, bytes));
    }
    Ok(Run { files, sums })
}

/// Every address and 32-byte word of a record, nested ones included.
fn walk(fields: &[(u16, Value)], addresses: &mut BTreeSet<Address>, words: &mut BTreeSet<B256>) {
    for (_, value) in fields {
        match value {
            Value::Address(a) => {
                addresses.insert(Address::from(*a));
            }
            Value::B256(w) | Value::U256(w) => {
                words.insert(B256::from(*w));
            }
            Value::Nested(inner) => walk(inner, addresses, words),
            _ => {}
        }
    }
}

/// The SELF_B salt of the run's candidate, from the private store; `None`
/// for SELF_A (public salt). Required when archiving; a restore checks it
/// where the store still holds that candidate (the archive is anchored).
fn b_salt(run: &Run, required: bool) -> Result<Option<B256>, String> {
    let (_, bytes) = run
        .files
        .iter()
        .find(|(n, _)| n == CANDIDATE_RECORD)
        .ok_or("the run has no candidate record; its profile and salt cannot be checked")?;
    let record = decode(&cand::SCHEMA, bytes).map_err(|e| format!("{CANDIDATE_RECORD}: {e}"))?;
    let e = |e: darkbot_bsc_record::RecordError| e.to_string();
    if record.text(cand::PROFILE).map_err(e)? != Profile::B.name() {
        return Ok(None);
    }
    let stored = Store::existing(Path::new(CANDIDATE_STATE_ROOT))
        .and_then(|s| s.map(|s| s.load(Profile::B)).transpose())
        .map(Option::flatten);
    match stored {
        Ok(Some((candidate, salt)))
            if candidate.id.0 == record.b256(cand::CANDIDATE_ID).map_err(e)? =>
        {
            Ok(Some(salt.launch_word()))
        }
        _ if !required => Ok(None),
        _ => Err(
            "the store does not hold this run's SELF_B candidate; its salt cannot be checked"
                .into(),
        ),
    }
}

/// No salt record, no unknown record, no raw-transaction-sized hex, no
/// private key of a named address and no SELF_B salt in any file.
fn clean(run: &Run, salt: Option<B256>) -> Result<(), String> {
    let mut addresses: BTreeSet<Address> = [LAUNCHER, BUYER]
        .iter()
        .map(|a| Address::from_str(a).map_err(|_| "contract address".to_string()))
        .collect::<Result<_, _>>()?;
    let mut words = Vec::new();
    let mut texts = Vec::new();
    for (name, bytes) in &run.files {
        if name.ends_with(".dbrec") && decode(&secret::SCHEMA, bytes).is_ok() {
            return Err(format!("{name} is a salt record"));
        }
        if salt.is_some_and(|s| bytes.windows(32).any(|w| w == s.as_slice())) {
            return Err(format!("{name} holds the candidate's salt"));
        }
        if name.ends_with(".dbrec") {
            let record = darkbot_bsc_record::schema::decode_any(bytes)
                .map_err(|e| format!("{name} is not a known evidence record: {e}"))?;
            let mut own = BTreeSet::new();
            walk(record.fields(), &mut addresses, &mut own);
            words.extend(own.into_iter().map(|w| (name.as_str(), w)));
            texts.push((name.as_str(), record.render()));
        } else {
            texts.push((name.as_str(), String::from_utf8_lossy(bytes).into_owned()));
        }
    }
    let salt_hex = salt.map(|s| hex::encode(s.as_slice()));
    for (name, text) in &texts {
        let lower = text.to_ascii_lowercase();
        if salt_hex
            .as_ref()
            .is_some_and(|s| lower.contains(s.as_str()))
        {
            return Err(format!("{name} holds the candidate's salt"));
        }
        for run in lower.split(|c: char| !c.is_ascii_hexdigit()) {
            if run.len() > 200 {
                return Err(format!("{name} holds raw-transaction-sized hex"));
            }
            if run.len() == 64 {
                words.push((name, B256::from_str(run).map_err(|e| e.to_string())?));
            }
        }
    }
    let mut seen = BTreeSet::new();
    for (name, word) in words {
        if seen.insert(word)
            && SigningKey::from_slice(word.as_slice())
                .is_ok_and(|key| addresses.contains(&signer_address(&key)))
        {
            return Err(format!(
                "{name} holds the private key of an address the evidence names"
            ));
        }
    }
    Ok(())
}

/// Writes `run` into the new directory `dest` (0700, files 0400), durably,
/// and reads it back.
fn copy_into(dest: &Path, run: &Run) -> Result<(), String> {
    fs::DirBuilder::new()
        .mode(0o700)
        .create(dest)
        .map_err(|e| format!("create {}: {e}", dest.display()))?;
    let write = |name: &str, bytes: &[u8]| {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o400)
            .open(dest.join(name))
            .and_then(|mut f| f.write_all(bytes).and_then(|()| f.sync_all()))
            .map_err(|e| format!("write {name}: {e}"))
    };
    for (name, bytes) in &run.files {
        write(name, bytes)?;
    }
    write(SUMS, &run.sums)?;
    sync_dir(dest)?;
    let back = read_run(dest)?;
    if back.sums != run.sums || back.files != run.files {
        return Err("the written copy differs from the run".into());
    }
    Ok(())
}

fn sync_dir(dir: &Path) -> Result<(), String> {
    fs::File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| format!("sync {}: {e}", dir.display()))
}

/// The bound run path `bsc/.local/g06a_sign/<run>` of real directories; a
/// restore into a fresh checkout creates the missing ones (0700).
fn bound_path(run: &str, create: bool) -> Result<String, String> {
    run_name(run)?;
    let mut at = PathBuf::new();
    for part in SIGN_ROOT.split('/') {
        at.push(part);
        if create && fs::symlink_metadata(&at).is_err() {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&at)
                .map_err(|e| format!("create {}: {e}", at.display()))?;
        }
        if !fs::symlink_metadata(&at).is_ok_and(|m| m.is_dir()) {
            return Err(format!("{} is not a real directory", at.display()));
        }
    }
    Ok(format!("{SIGN_ROOT}/{run}"))
}

pub(super) fn archive(args: &[String]) -> Result<i32, String> {
    repository_root_check(Path::new("."))?;
    let sign = flag(args, "--sign")?;
    let name = sign
        .strip_prefix(&format!("{SIGN_ROOT}/"))
        .ok_or_else(|| format!("--sign must be a run under {SIGN_ROOT}/"))?;
    let sign = bound_path(name, false)?;
    let to = fs::canonicalize(flag(args, "--to")?).map_err(|e| format!("--to: {e}"))?;
    let root = fs::canonicalize(".").map_err(|e| e.to_string())?;
    if !fs::symlink_metadata(&to).is_ok_and(|m| m.is_dir()) {
        return Err("--to must be a real directory".into());
    }
    if to.starts_with(&root)
        && (!to.starts_with(root.join("bsc/.local")) || to.starts_with(root.join(SIGN_ROOT)))
    {
        return Err(
            "--to lies in the repository outside bsc/.local (or among the runs); an archive is never committable or bound".into(),
        );
    }
    let run = read_run(Path::new(&sign))?;
    clean(&run, b_salt(&run, true)?)?;
    let dest = to.join(name);
    copy_into(&dest, &run)?;
    sync_dir(&to)?;
    let anchor = sha256_hex(&run.sums);
    let mut out = std::io::stdout().lock();
    for line in [
        format!("ARCHIVED={}", dest.display()),
        format!("SUMS_SHA256={anchor}"),
        format!(
            "RESTORE=g06a_dry_handoff restore --from {} --expect {anchor}",
            dest.display()
        ),
    ] {
        writeln!(out, "{line}").map_err(|e| e.to_string())?;
    }
    Ok(0)
}

/// The v5 contract, if one binds `dir`, reproduced read-only from it.
fn reproduce_v5(dir: &str) -> Result<String, String> {
    let bytes = match fs::read(V5_RELATIVE_PATH) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok("V5=NOT_PRESENT".into());
        }
        Err(e) => return Err(format!("{V5_RELATIVE_PATH}: {e}")),
    };
    let contract = decode(&c::V5, &bytes).map_err(|e| format!("{V5_RELATIVE_PATH}: {e}"))?;
    let e = |e: darkbot_bsc_record::RecordError| e.to_string();
    let dry = contract.text(c::DRY_RECORD_PATH).map_err(e)?;
    if dry != format!("{dir}/{REQUAL_DRY}") {
        return Ok(format!("V5=BINDS_ANOTHER_RUN {dry}"));
    }
    let policy = contract.text(c::BUILDER_POLICY_RECORD_PATH).map_err(e)?;
    let evidence = attempt1_v5_evidence(
        Path::new("."),
        dir,
        policy,
        &format!("{dir}/{CANDIDATE_RECORD}"),
    )
    .map_err(|e| format!("the restored run does not reproduce the bound v5 contract: {e}"))?;
    for (authorization, label) in [
        (Authorization::Proposed, "PROPOSAL"),
        (Authorization::Approved, "APPROVED"),
    ] {
        if attempt1_v5(&evidence, authorization)? == bytes {
            return Ok(format!(
                "V5_REPRODUCED={label} sha256={}",
                sha256_hex(&bytes)
            ));
        }
    }
    Err("the restored run does not reproduce the bound v5 contract (neither variant)".into())
}

pub(super) fn restore(args: &[String]) -> Result<i32, String> {
    repository_root_check(Path::new("."))?;
    let from = Path::new(flag(args, "--from")?);
    let expect = B256::from_str(flag(args, "--expect")?)
        .map_err(|_| "--expect must be the archive's 32-byte SUMS_SHA256")?;
    let name = from
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("--from must name a run directory")?;
    run_name(name)?;
    let run = read_run(from)?;
    if darkbot_bsc_record::sha256(&run.sums) != expect.0 {
        return Err(format!("the archive's {SUMS} is not the expected one"));
    }
    clean(&run, b_salt(&run, false)?)?;
    let bound = bound_path(name, true)?;
    let mut out = std::io::stdout().lock();
    if fs::symlink_metadata(&bound).is_ok() {
        let present = read_run(Path::new(&bound))?;
        if present.sums != run.sums || present.files != run.files {
            return Err(format!(
                "{bound} holds other evidence; it is never overwritten"
            ));
        }
        writeln!(out, "ALREADY_PRESENT_IDENTICAL={bound}").map_err(|e| e.to_string())?;
        writeln!(out, "{}", reproduce_v5(&bound)?).map_err(|e| e.to_string())?;
        return Ok(0);
    }
    let staging = format!("{SIGN_ROOT}/.{name}.restore-{}", std::process::id());
    copy_into(Path::new(&staging), &run)?;
    fs::rename(&staging, &bound).map_err(|e| format!("restore {bound}: {e}"))?;
    sync_dir(Path::new(SIGN_ROOT))?;
    writeln!(out, "RESTORED={bound}").map_err(|e| e.to_string())?;
    match reproduce_v5(&bound) {
        Ok(line) => writeln!(out, "{line}").map_err(|e| e.to_string())?,
        Err(e) => {
            let aside = format!("{bound}.unbound-{}", unix_time_ms());
            fs::rename(&bound, &aside).map_err(|x| format!("{e}; moving it aside failed: {x}"))?;
            sync_dir(Path::new(SIGN_ROOT))?;
            return Err(format!("{e}; the restored run was moved aside to {aside}"));
        }
    }
    Ok(0)
}
