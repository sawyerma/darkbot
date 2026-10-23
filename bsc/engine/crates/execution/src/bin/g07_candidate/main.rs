//! G0.7 self-launch candidate manager A/B (plan v8/v9), offline only.
//!
//! ```text
//! g07_candidate init-a --economics <dbrec> --dry <dbrec> --preflight <dbrec>
//! g07_candidate create-b --dry <dbrec> --preflight <dbrec>
//! g07_candidate select A|B --expect-generation <n>
//! g07_candidate show [--readiness <dbrec>]
//! g07_candidate qualify A|B --requal-dir <requalification run>
//! g07_candidate apply-requests --rundir <dir>
//! g07_candidate request A|B --expect-generation <n> --request-id <uuid> --rundir <dir> --snapshot <dir>
//! ```
//!
//! `request` is the runtime side of a GUI selection (staged release, no
//! repository): it only records a typed request; `apply-requests`, run from
//! the desktop sync, lets the native store apply or reject it.
//!
//! Runs at the root of the main checkout; state in bsc/.local/g07_candidates.
//! No RPC, no key, no signature, no send; the guard is only read. Prints the
//! public identity, never a salt. Exit 2 on every refusal.
#![allow(dead_code, unused_imports)]
#[path = "../g07_mainnet_self_launch_buy/builder_submission.rs"]
mod builder_submission;
mod candidate;
#[path = "../g07_mainnet_self_launch_buy/chain_reads.rs"]
mod chain_reads;
#[path = "../g07_mainnet_self_launch_buy/inputs.rs"]
mod inputs;
#[path = "../g07_mainnet_self_launch_buy/shared.rs"]
mod shared;
#[path = "../g07_mainnet_self_launch_buy/transactions.rs"]
mod transactions;

use builder_submission::*;
use candidate::*;
use chain_reads::*;
use darkbot_bsc_execution::acceptance_guard::FROZEN_APPROVED_V4_SHA256;
use darkbot_bsc_record::decode;
use inputs::*;
use shared::*;
use transactions::*;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn flag<'a>(args: &'a [String], name: &str) -> Result<&'a str, String> {
    let at = args
        .iter()
        .position(|a| a == name)
        .ok_or_else(|| format!("{name} is required"))?;
    args.get(at + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("{name} needs a value"))
}

fn read_input(path: &str) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("read {path}: {e}"))
}

fn print_candidate(candidate: &Candidate) {
    let p = candidate.profile.name();
    let dep = &candidate.deployment;
    println!("{p}.state = PRESENT");
    println!("{p}.candidate_id = {}", candidate.id);
    println!("{p}.token = {}", candidate.token);
    println!("{p}.launcher = {}", candidate.launcher);
    println!("{p}.chain_id = {}", dep.chain_id);
    println!("{p}.deployment.portal = {}", dep.portal);
    println!("{p}.deployment.portal_version = {}", dep.portal_version);
    println!(
        "{p}.deployment.portal_implementation = {}",
        dep.portal_implementation
    );
    println!("{p}.deployment.tax_v3_implementation = {}", dep.tax_v3);
    println!("{p}.deployment.record_sha256 = {}", dep.record_sha256);
    println!(
        "{p}.launch_calldata_sha256 = {}",
        candidate.launch_calldata_sha256
    );
    println!("{p}.salt_commitment = {}", candidate.salt_commitment);
    match &candidate.evidence {
        Some(e) => {
            println!(
                "{p}.evidence.economics_record_sha256 = {}",
                e.economics_sha256
            );
            println!("{p}.evidence.dry_record_sha256 = {}", e.dry_sha256);
            println!("{p}.evidence.launch_tx_hash = {}", e.launch_tx_hash);
            println!("{p}.evidence.buy_tx_hash = {}", e.buy_tx_hash);
        }
        None => println!("{p}.evidence = NONE"),
    }
    println!("{p}.qualification = {}", candidate.qualification);
    println!("{p}.provenance = {}", candidate.provenance);
}

fn generation(args: &[String]) -> Result<u64, String> {
    flag(args, "--expect-generation")?
        .parse::<u64>()
        .map_err(|_| "--expect-generation must be an unsigned integer".into())
}

fn run(args: &[String]) -> Result<(), String> {
    // Runtime side (staged release, no repository, no salt): only records a
    // typed request against the public mirror of its snapshot.
    if args.first().map(String::as_str) == Some("request") {
        let profile = Profile::parse(args.get(1).ok_or("request needs A or B")?)?;
        let expected = generation(args)?;
        write_request(
            Path::new(flag(args, "--rundir")?),
            &Path::new(flag(args, "--snapshot")?).join(PUBLIC_MIRROR_ROOT),
            profile,
            expected,
            flag(args, "--request-id")?,
            FROZEN_APPROVED_V4_SHA256,
            now_ms(),
        )?;
        println!(
            "candidate.request = RECORDED profile={} expected_generation={expected}; effective only after the native apply at the next desktop sync",
            profile.name()
        );
        return Ok(());
    }
    repository_root_check(Path::new("."))?;
    let dir = Path::new(CANDIDATE_STATE_ROOT);
    match args.first().map(String::as_str) {
        Some("init-a") => {
            let (candidate, salt) = frozen_a(
                &read_input(flag(args, "--economics")?)?,
                &read_input(flag(args, "--dry")?)?,
                &read_input(flag(args, "--preflight")?)?,
                now_ms(),
            )?;
            print_candidate(&Store::create(dir)?.init_a(candidate, salt)?);
        }
        Some("create-b") => {
            let store = Store::create(dir)?;
            let (candidate, salt) = fresh_b(
                &read_input(flag(args, "--preflight")?)?,
                &read_input(flag(args, "--dry")?)?,
                now_ms(),
                &mut os_random,
            )?;
            print_candidate(&store.create_b(candidate, salt)?);
        }
        Some("select") => {
            let profile = Profile::parse(args.get(1).ok_or("select needs A or B")?)?;
            let expected = generation(args)?;
            let store = Store::existing(dir)?.ok_or("no candidate state")?;
            let selection = store.select(
                profile,
                expected,
                FROZEN_APPROVED_V4_SHA256,
                Path::new(ATTEMPT_STATE_ROOT),
                now_ms(),
            )?;
            println!("selection.generation = {}", selection.generation);
            println!("selection.profile = {}", selection.profile.name());
            println!("selection.candidate_id = {}", selection.candidate_id);
            println!(
                "selection.invalidated = READINESS,SIGNED_BYTE_AUTHORIZATION,ECONOMIC_APPROVAL"
            );
        }
        Some("qualify") => {
            let profile = Profile::parse(args.get(1).ok_or("qualify needs A or B")?)?;
            let run = Path::new(flag(args, "--requal-dir")?);
            let store = Store::existing(dir)?.ok_or("no candidate state")?;
            let (candidate, _) = store
                .load(profile)?
                .ok_or_else(|| format!("candidate {} does not exist", profile.name()))?;
            let qualification = qualification_for(
                &candidate,
                &read_input(&run.join("requalification_status.dbrec").to_string_lossy())?,
                &read_input(&run.join("dry_result.dbrec").to_string_lossy())?,
                &read_input(&run.join("preflight.dbrec").to_string_lossy())?,
            )?;
            store.qualify(profile, &qualification, now_ms())?;
            println!(
                "{}.qualification = QUALIFIED_FOR {} ({}); not economic approval",
                profile.name(),
                qualification.portal_version,
                qualification.portal_implementation
            );
        }
        Some("apply-requests") => {
            let store = Store::existing(dir)?.ok_or("no candidate state")?;
            for (id, state, detail) in apply_requests(
                &store,
                Path::new(flag(args, "--rundir")?),
                FROZEN_APPROVED_V4_SHA256,
                Path::new(ATTEMPT_STATE_ROOT),
                now_ms(),
            )? {
                println!("candidate.request.{id} = {state}: {detail}");
            }
        }
        Some("show") => {
            let readiness = match args.iter().position(|a| a == "--readiness") {
                Some(_) => Some(
                    decode(
                        &schema::g07::readiness::SCHEMA,
                        &read_input(flag(args, "--readiness")?)?,
                    )
                    .map_err(|e| format!("readiness: {e}"))?,
                ),
                None => None,
            };
            println!("guard = {FROZEN_APPROVED_V4_SHA256}");
            let Some(store) = Store::existing(dir)? else {
                println!("selection = NONE");
                println!("SELF_A.state = ABSENT");
                println!("SELF_B.state = ABSENT");
                return Ok(());
            };
            let selection = store.selection()?;
            match &selection {
                Some(s) => {
                    println!("selection.generation = {}", s.generation);
                    println!("selection.profile = {}", s.profile.name());
                    println!("selection.candidate_id = {}", s.candidate_id);
                    println!("selection.activated_unix_ms = {}", s.activated_unix_ms);
                }
                None => println!("selection = NONE"),
            }
            for profile in [Profile::A, Profile::B] {
                match store.load(profile)? {
                    Some((candidate, _)) => {
                        print_candidate(&candidate);
                        if let Some(bound) = store.qualification(profile)? {
                            println!("{}.qualification_binding = {bound}", profile.name());
                        }
                        if let Some(r) = &readiness {
                            println!(
                                "{}.readiness = {}",
                                profile.name(),
                                readiness_for(r, selection.as_ref(), &candidate)
                            );
                        }
                    }
                    None => println!("{}.state = ABSENT", profile.name()),
                }
            }
        }
        _ => return Err("usage: g07_candidate init-a|create-b|select|show (see source)".into()),
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("G07_CANDIDATE=REFUSED: {e}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence(name: &str) -> Vec<u8> {
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../native/evidence/g06a/g07_binary_import_20261016")
                .join(name),
        )
        .unwrap()
    }

    fn frozen() -> (Candidate, Salt) {
        frozen_a(
            &evidence("economics_crosscheck.dbrec"),
            &evidence("dry_result.dbrec"),
            &evidence("preflight.dbrec"),
            1,
        )
        .unwrap()
    }

    fn private_dir(tag: &str) -> PathBuf {
        let base = fs::canonicalize(env::temp_dir()).unwrap().join(format!(
            "g07-candidate-{tag}-{}-{}",
            std::process::id(),
            now_ms()
        ));
        fs::DirBuilder::new().mode(0o700).create(&base).unwrap();
        base
    }

    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    fn no_mirror() -> &'static Path {
        Path::new("/nonexistent/g07-candidate-mirror")
    }

    fn word(hex: &str) -> B256 {
        hex.parse().unwrap()
    }

    #[test]
    fn profile_a_reproduces_the_frozen_token_calldata_and_signed_hashes() {
        let (a, salt) = frozen();
        assert_eq!(
            a.token,
            "0xe51c4deb1038afcd95ef12d4a649cdbcf6fa7777"
                .parse::<Address>()
                .unwrap()
        );
        let e = a.evidence.as_ref().unwrap();
        assert_eq!(
            e.launch_tx_hash,
            word("0xdb468c01529ee4c9597eae056f420f0d4d6d0753a1f785f8dbb3163028be5146")
        );
        assert_eq!(
            e.buy_tx_hash,
            word("0xb086f683550d1aa221db2add1aa1426444b7ef716de98594c40b9b6da765db32")
        );
        // The exact calldata the G0.7 runner signs for this launcher and salt.
        assert_eq!(
            a.launch_calldata_sha256,
            B256::from(darkbot_bsc_record::sha256(&launch_calldata(
                a.launcher,
                salt.expose()
            )))
        );
        assert_eq!(
            a.launcher,
            "0x3e7b487ae5e1c49ab1cf6222bdb5cf016e86aacd"
                .parse::<Address>()
                .unwrap()
        );
        assert_eq!(
            (a.profile, a.qualification.as_str()),
            (Profile::A, UNQUALIFIED)
        );
        // v5.24.0 evidence never qualifies a later deployment by itself.
        assert!(a.provenance.contains("v5.24.0") && a.provenance.contains("requalification"));
    }

    #[test]
    fn profile_b_draws_a_fresh_vanity_salt_with_the_pinned_create2() {
        let (a, a_salt) = frozen();
        let (b, b_salt) = fresh_b(
            &evidence("preflight.dbrec"),
            &evidence("dry_result.dbrec"),
            2,
            &mut os_random,
        )
        .unwrap();
        assert_eq!(b.token.as_slice()[18..], [0x77, 0x77]);
        assert_eq!(
            b.token,
            predicted_token(b.deployment.portal, b.deployment.tax_v3, b_salt.expose())
        );
        assert_ne!(b.token, a.token);
        assert_ne!(b.salt_commitment, a.salt_commitment);
        assert_ne!(b_salt.expose(), a_salt.expose());
        assert!(b.evidence.is_none() && b.qualification == UNQUALIFIED);
        assert_eq!(b.launcher, a.launcher);
        // Draws without the suffix are skipped, a failing RNG fails.
        let mut n = 0u8;
        let mut fails = || -> Result<[u8; 32], String> {
            n += 1;
            if n > 3 {
                Err("rng down".into())
            } else {
                Ok([n; 32])
            }
        };
        assert!(
            fresh_b(
                &evidence("preflight.dbrec"),
                &evidence("dry_result.dbrec"),
                2,
                &mut fails
            )
            .is_err()
        );
    }

    #[test]
    fn candidate_b_survives_a_restart_and_is_never_rerolled() {
        let root = private_dir("restart");
        let dir = root.join("state");
        let store = Store::create(&dir).unwrap();
        let (b, salt) = fresh_b(
            &evidence("preflight.dbrec"),
            &evidence("dry_result.dbrec"),
            3,
            &mut os_random,
        )
        .unwrap();
        let created = store.create_b(b, salt).unwrap();
        for f in ["b.candidate.dbrec", "b.salt.dbrec"] {
            assert_eq!(
                fs::metadata(dir.join(f)).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        // A new process view of the same directory.
        let reopened = Store::existing(&dir).unwrap().unwrap();
        let (again, _) = reopened.load(Profile::B).unwrap().unwrap();
        assert_eq!(again, created);
        let (other, other_salt) = fresh_b(
            &evidence("preflight.dbrec"),
            &evidence("dry_result.dbrec"),
            4,
            &mut os_random,
        )
        .unwrap();
        assert!(
            reopened
                .create_b(other, other_salt)
                .unwrap_err()
                .contains("never re-rolled")
        );
        assert_eq!(reopened.load(Profile::B).unwrap().unwrap().0, created);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_switch_invalidates_readiness_bound_to_the_other_candidate() {
        let root = private_dir("switch");
        let dir = root.join("state");
        let attempts = root.join("attempts");
        let store = Store::create(&dir).unwrap();
        let (a, a_salt) = frozen();
        let a = store.init_a(a, a_salt).unwrap();
        let (b, b_salt) = fresh_b(
            &evidence("preflight.dbrec"),
            &evidence("dry_result.dbrec"),
            5,
            &mut os_random,
        )
        .unwrap();
        let b = store.create_b(b, b_salt).unwrap();
        let e = a.evidence.clone().unwrap();
        let check = |dir: &Path| {
            contract_candidate_check(
                dir,
                no_mirror(),
                a.token,
                a.launcher,
                e.launch_tx_hash,
                e.buy_tx_hash,
            )
        };

        assert_eq!(check(&dir).0, "BLOCKED"); // nothing selected
        let on_a = store
            .select(Profile::A, 0, "UNARMED", &attempts, 10)
            .unwrap();
        let (status, detail) = check(&dir);
        assert_eq!((status, detail.as_str()), ("PASS", on_a.binding().as_str()));

        let mut readiness = Draft::new(&schema::g07::readiness::SCHEMA);
        readiness
            .set(schema::g07::readiness::STATUS, "BLOCKED")
            .set(schema::g07::readiness::MODE, "READ_ONLY_RPC")
            .set(schema::g07::readiness::OBSERVED_UNIX_MS, 11u64)
            .set(schema::g07::readiness::LIVE_AUTHORIZED, false);
        let mut row = Draft::new(&schema::g07::readiness::check::SCHEMA);
        row.set(schema::g07::readiness::check::NAME, "CANDIDATE_SELECTION")
            .set(schema::g07::readiness::check::STATUS, "PASS")
            .set(schema::g07::readiness::check::DETAIL, detail.as_str());
        readiness.push(schema::g07::readiness::CHECKS, row.into_nested());
        let readiness = decode(
            &schema::g07::readiness::SCHEMA,
            &readiness.encode().unwrap(),
        )
        .unwrap();
        assert_eq!(readiness_for(&readiness, Some(&on_a), &a), "BLOCKED");
        assert_eq!(readiness_for(&readiness, Some(&on_a), &b), "NOT_SELECTED");

        // Switching to B: A's readiness is stale, B has none, v4 (A) blocks.
        let on_b = store
            .select(Profile::B, 1, "UNARMED", &attempts, 20)
            .unwrap();
        assert_eq!((on_b.generation, on_b.previous), (2, Some(a.id)));
        assert_eq!(readiness_for(&readiness, Some(&on_b), &b), "STALE");
        assert_eq!(readiness_for(&readiness, Some(&on_b), &a), "NOT_SELECTED");
        assert_eq!(check(&dir).0, "BLOCKED");
        // Back to A: the old verdict names generation 1, not 3.
        let again = store
            .select(Profile::A, 2, "UNARMED", &attempts, 30)
            .unwrap();
        assert_eq!(readiness_for(&readiness, Some(&again), &a), "STALE");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_generation_tampering_and_missing_candidates_fail_closed() {
        let root = private_dir("tamper");
        let dir = root.join("state");
        let attempts = root.join("attempts");
        let store = Store::create(&dir).unwrap();
        let (a, a_salt) = frozen();
        let a = store.init_a(a, a_salt).unwrap();
        let e = a.evidence.clone().unwrap();
        assert!(
            store
                .select(Profile::B, 0, "UNARMED", &attempts, 1)
                .unwrap_err()
                .contains("does not exist")
        );
        assert!(
            store
                .select(Profile::A, 5, "UNARMED", &attempts, 1)
                .unwrap_err()
                .contains("stale")
        );
        store
            .select(Profile::A, 0, "UNARMED", &attempts, 1)
            .unwrap();
        // Wrong address or deployment against the v4 DRY: contradiction.
        let other = Address::repeat_byte(7);
        assert_eq!(
            contract_candidate_check(
                &dir,
                no_mirror(),
                other,
                a.launcher,
                e.launch_tx_hash,
                e.buy_tx_hash
            )
            .0,
            "STOP"
        );
        assert_eq!(
            contract_candidate_check(
                &dir,
                no_mirror(),
                a.token,
                a.launcher,
                B256::ZERO,
                e.buy_tx_hash
            )
            .0,
            "STOP"
        );
        // A changed public record (token, deployment) or salt never verifies.
        let public = dir.join("a.candidate.dbrec");
        let original = fs::read(&public).unwrap();
        for edit in [
            c_edit(&a, |x| x.token = other),
            c_edit(&a, |x| x.deployment.portal_version = "v5.26.0".into()),
        ] {
            rewrite(&public, &edit);
            assert!(store.load(Profile::A).is_err());
            assert_eq!(
                contract_candidate_check(
                    &dir,
                    no_mirror(),
                    a.token,
                    a.launcher,
                    e.launch_tx_hash,
                    e.buy_tx_hash
                )
                .0,
                "STOP"
            );
        }
        rewrite(&public, &original);
        let mut secret = Draft::new(&schema::g07_candidate::secret::SCHEMA);
        secret
            .set(
                schema::g07_candidate::secret::CANDIDATE_ID,
                darkbot_bsc_record::Value::B256(a.id.0),
            )
            .set(
                schema::g07_candidate::secret::SALT,
                darkbot_bsc_record::Value::B256([9; 32]),
            );
        rewrite(&dir.join("a.salt.dbrec"), &secret.encode().unwrap());
        assert!(store.load(Profile::A).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_gui_request_is_only_applied_by_the_native_store() {
        let root = private_dir("request");
        let dir = root.join("state");
        let run = root.join("run");
        let attempts = root.join("attempts");
        let mirror = root.join("snapshot/bsc/native/state/g07_candidates");
        let store = Store::create(&dir).unwrap();
        let (a, a_salt) = frozen();
        let a = store.init_a(a, a_salt).unwrap();
        let (b, b_salt) = fresh_b(
            &evidence("preflight.dbrec"),
            &evidence("dry_result.dbrec"),
            6,
            &mut os_random,
        )
        .unwrap();
        let b = store.create_b(b, b_salt).unwrap();
        store
            .select(Profile::A, 0, "UNARMED", &attempts, 10)
            .unwrap();
        let sync = || {
            fs::create_dir_all(&mirror).unwrap();
            for f in ["a.candidate.dbrec", "b.candidate.dbrec", "selection.dbrec"] {
                fs::copy(dir.join(f), mirror.join(f)).unwrap();
            }
        };
        sync();
        let id = |n: u8| format!("{n:08x}-0000-4000-8000-000000000000");
        let request = |p, g, rid: &str, guard| write_request(&run, &mirror, p, g, rid, guard, 20);
        assert!(
            request(Profile::B, 0, &id(1), "UNARMED")
                .unwrap_err()
                .contains("stale")
        );
        assert!(
            request(Profile::A, 1, &id(1), "UNARMED")
                .unwrap_err()
                .contains("already selected")
        );
        assert!(
            request(Profile::B, 1, "../x", "UNARMED")
                .unwrap_err()
                .contains("UUID")
        );
        assert!(
            request(Profile::B, 1, &id(1), "6f877ae0")
                .unwrap_err()
                .contains("armed")
        );
        request(Profile::B, 1, &id(1), "UNARMED").unwrap();
        assert!(
            request(Profile::B, 1, &id(2), "UNARMED")
                .unwrap_err()
                .contains("pending")
        );
        // The request alone changes nothing; the native apply does.
        assert_eq!(store.selection().unwrap().unwrap().profile, Profile::A);
        let decided = apply_requests(&store, &run, "UNARMED", &attempts, 30).unwrap();
        assert_eq!(
            (decided[0].0.as_str(), decided[0].1),
            (id(1).as_str(), "APPLIED")
        );
        let now = store.selection().unwrap().unwrap();
        assert_eq!(
            (now.profile, now.generation, now.candidate_id),
            (Profile::B, 2, b.id)
        );
        assert!(
            apply_requests(&store, &run, "UNARMED", &attempts, 31)
                .unwrap()
                .is_empty()
        );

        // A request made against a stale mirror is rejected natively.
        request(Profile::B, 1, &id(3), "UNARMED").unwrap();
        let decided = apply_requests(&store, &run, "UNARMED", &attempts, 40).unwrap();
        assert_eq!(decided[0].1, "REJECTED");
        assert!(decided[0].2.contains("stale"));
        sync();
        fs::create_dir_all(attempts.join("attempt-1")).unwrap();
        request(Profile::A, 2, &id(4), "UNARMED").unwrap();
        let decided = apply_requests(&store, &run, "UNARMED", &attempts, 50).unwrap();
        assert!(decided[0].2.contains("switching is forbidden"));
        assert_eq!(store.selection().unwrap().unwrap().generation, 2);

        // Readiness in a snapshot sees only the public mirror.
        let e = a.evidence.clone().unwrap();
        let none = root.join("no-private");
        let check = |m: &Path| {
            contract_candidate_check(
                &none,
                m,
                a.token,
                a.launcher,
                e.launch_tx_hash,
                e.buy_tx_hash,
            )
        };
        assert_eq!(check(&mirror).0, "BLOCKED"); // B selected
        let public = load_public(&mirror).unwrap().unwrap();
        assert_eq!((public.generation(), public.candidates.len()), (2, 2));
        // A salt record or a changed identity in the mirror is refused.
        fs::copy(dir.join("b.salt.dbrec"), mirror.join("b.salt.dbrec")).unwrap();
        assert!(load_public(&mirror).unwrap_err().contains("salt"));
        assert_eq!(check(&mirror).0, "STOP");
        fs::remove_file(mirror.join("b.salt.dbrec")).unwrap();
        rewrite(
            &mirror.join("a.candidate.dbrec"),
            &c_edit(&a, |x| x.token = Address::repeat_byte(3)),
        );
        assert!(load_public(&mirror).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_public_mirror_passes_readiness_only_for_the_v4_candidate() {
        let root = private_dir("mirror");
        let dir = root.join("state");
        let mirror = root.join("mirror");
        let store = Store::create(&dir).unwrap();
        let (a, a_salt) = frozen();
        let a = store.init_a(a, a_salt).unwrap();
        let on_a = store
            .select(Profile::A, 0, "UNARMED", &root.join("attempts"), 1)
            .unwrap();
        fs::create_dir_all(&mirror).unwrap();
        for f in ["a.candidate.dbrec", "selection.dbrec"] {
            fs::copy(dir.join(f), mirror.join(f)).unwrap();
        }
        let e = a.evidence.clone().unwrap();
        let (status, detail) = contract_candidate_check(
            &root.join("absent"),
            &mirror,
            a.token,
            a.launcher,
            e.launch_tx_hash,
            e.buy_tx_hash,
        );
        assert_eq!((status, detail), ("PASS", on_a.binding()));
        // The private store, when present, is authoritative over the mirror.
        assert_eq!(
            contract_candidate_check(
                &dir,
                &root.join("absent"),
                a.token,
                a.launcher,
                e.launch_tx_hash,
                e.buy_tx_hash
            )
            .0,
            "PASS"
        );
        fs::remove_dir_all(root).unwrap();
    }

    /// A PASSing requalification status that binds the given DRY and
    /// preflight, as g06a_requal_check writes it.
    fn requal(
        verdict: &str,
        dry: &[u8],
        preflight: &[u8],
        hashes: Option<(B256, B256)>,
    ) -> Vec<u8> {
        use darkbot_bsc_record::{Value, schema::g06a::requalification as rqs};
        let mut r = Draft::new(&rqs::SCHEMA);
        r.set(rqs::VERDICT, verdict)
            .set(rqs::REQUAL_DIR, "fixture")
            .set(rqs::FROZEN_DIR, "fixture")
            .set(schema::PROVENANCE_TOOL, "fixture")
            .set(schema::PROVENANCE_OBSERVED_UNIX_MS, 1u64);
        let real = evidence("dry_result.dbrec");
        let dry_record = decode(&schema::g06a::dry::SCHEMA, &real)
            .or_else(|_| decode(&schema::g06a::dry::NATIVE, &real))
            .unwrap();
        let (l, b) = hashes.unwrap_or((
            B256::from(dry_record.b256(schema::g06a::dry::LAUNCH_TX_HASH).unwrap()),
            B256::from(dry_record.b256(schema::g06a::dry::BUY_TX_HASH).unwrap()),
        ));
        r.set(rqs::LAUNCH_TX_HASH, Value::B256(l.0))
            .set(rqs::BUY_TX_HASH, Value::B256(b.0));
        for (name, bytes) in [("dry_result.dbrec", dry), ("preflight.dbrec", preflight)] {
            let mut row = Draft::new(&rqs::bound::SCHEMA);
            row.set(rqs::bound::NAME, name).set(
                rqs::bound::SHA256,
                Value::B256(darkbot_bsc_record::sha256(bytes)),
            );
            r.push(rqs::RECORDS, row.into_nested());
        }
        r.encode().unwrap()
    }

    #[test]
    fn only_a_passing_requalification_of_exactly_the_candidate_qualifies_it() {
        let root = private_dir("qualify");
        let store = Store::create(&root.join("state")).unwrap();
        let (a, a_salt) = frozen();
        let a = store.init_a(a, a_salt).unwrap();
        let (b, b_salt) = fresh_b(
            &evidence("preflight.dbrec"),
            &evidence("dry_result.dbrec"),
            7,
            &mut os_random,
        )
        .unwrap();
        let b = store.create_b(b, b_salt).unwrap();
        // Offline creation is never a qualification.
        assert_eq!(store.qualification(Profile::B).unwrap(), None);
        assert_eq!(b.qualification, UNQUALIFIED);
        let (dry, pre) = (evidence("dry_result.dbrec"), evidence("preflight.dbrec"));
        let pass = requal("PASS", &dry, &pre, None);
        let qa = qualification_for(&a, &pass, &dry, &pre).unwrap();
        assert_eq!((qa.portal_version.as_str(), qa.token), ("v5.24.0", a.token));
        store.qualify(Profile::A, &qa, 8).unwrap();
        assert_eq!(
            store.qualification(Profile::A).unwrap().as_deref(),
            Some("QUALIFIED_FOR v5.24.0")
        );
        // A's run never qualifies B, and a STOP, an unbound record or
        // changed signed hashes never qualify anything.
        assert!(
            qualification_for(&b, &pass, &dry, &pre)
                .unwrap_err()
                .contains("not candidate SELF_B")
        );
        assert!(qualification_for(&a, &requal("STOP", &dry, &pre, None), &dry, &pre).is_err());
        assert!(
            qualification_for(&a, &requal("PASS", b"other", &pre, None), &dry, &pre)
                .unwrap_err()
                .contains("bound")
        );
        let moved = requal(
            "PASS",
            &dry,
            &pre,
            Some((B256::repeat_byte(1), B256::repeat_byte(2))),
        );
        assert!(qualification_for(&a, &moved, &dry, &pre).is_err());
        assert!(
            store
                .qualify(Profile::B, &qa, 9)
                .unwrap_err()
                .contains("another candidate")
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn c_edit(a: &Candidate, f: impl FnOnce(&mut Candidate)) -> Vec<u8> {
        let mut x = a.clone();
        f(&mut x);
        x.draft().encode().unwrap()
    }

    fn rewrite(path: &Path, bytes: &[u8]) {
        fs::write(path, bytes).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[test]
    fn switching_is_refused_while_armed_or_with_attempt_state() {
        let root = private_dir("armed");
        let dir = root.join("state");
        let attempts = root.join("attempts");
        let store = Store::create(&dir).unwrap();
        let (a, a_salt) = frozen();
        store.init_a(a, a_salt).unwrap();
        assert!(
            store
                .select(Profile::A, 0, "6f877ae0", &attempts, 1)
                .unwrap_err()
                .contains("armed")
        );
        fs::create_dir_all(attempts.join("attempt-1")).unwrap();
        assert!(
            store
                .select(Profile::A, 0, "UNARMED", &attempts, 1)
                .unwrap_err()
                .contains("attempt state")
        );
        assert!(store.selection().unwrap().is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
