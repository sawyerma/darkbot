//! T5 (BSC-005): the terminal byte-free abort before the release barrier,
//! run for real on every stop this runner has: a bare `origin` as the
//! authority medium, the real latch, evidence file and capture journal.

use super::*;
use std::os::unix::fs::DirBuilderExt;

fn private_dir(tag: &str) -> PathBuf {
    let dir = fs::canonicalize(env::temp_dir())
        .unwrap()
        .join(format!("darkbot-g07-t5-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
    dir
}

/// A bare `origin` and one checkout of it: the authority medium.
fn authority_remote(dir: &Path) -> attempt_authority::Git {
    let origin = dir.join("origin.git");
    let checkout = dir.join("checkout");
    for args in [
        vec!["init", "-q", "--bare", origin.to_str().unwrap()],
        vec!["init", "-q", checkout.to_str().unwrap()],
        vec![
            "-C",
            checkout.to_str().unwrap(),
            "remote",
            "add",
            "origin",
            origin.to_str().unwrap(),
        ],
    ] {
        let out = std::process::Command::new("git")
            .args(&args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    attempt_authority::Git::new(checkout, "origin")
}

/// The events of a capture journal, in order.
fn journal_events(path: &Path) -> Vec<String> {
    let mut reader = darkbot_bsc_journal::JournalReader::new(
        fs::File::open(path).unwrap(),
        darkbot_bsc_execution::qualification_capture::DEFAULT_CAPTURE_MAX_PAYLOAD,
    )
    .unwrap();
    let mut events = Vec::new();
    while let Some(record) = reader.next_record().unwrap() {
        let text = String::from_utf8_lossy(&record.payload).to_string();
        if let Some(line) = text.lines().find(|l| l.starts_with("event=")) {
            events.push(format!("{line}|{text}"));
        }
    }
    events
}

/// Every stage this runner stops at before the release barrier, from its
/// own source: the literal stage of each `pre_egress_abort` call, the two
/// liveness stages of the loop and the guard refusal.
fn pre_egress_stages() -> Vec<String> {
    let source = include_str!("main.rs");
    let mut stages = vec!["live authorization".to_string()];
    for call in source.split("pre_egress_abort(").skip(1) {
        let args: Vec<&str> = call.split(')').next().unwrap().split(',').collect();
        if args.len() < 4 || args[2].trim() != "unsent" {
            continue; // the definition, not a call
        }
        match args[3].trim() {
            "stage" => stages.extend(["48Club liveness", "BlockRazor liveness"].map(String::from)),
            literal => stages.push(literal.trim_matches('"').to_string()),
        }
    }
    stages
}

/// One attempt up to the capture pre-arm, with its consumed authority.
struct Armed {
    git: attempt_authority::Git,
    binding: attempt_authority::Binding,
    attempt_dir: PathBuf,
    evidence: EvidenceFile,
    claim: attempt_authority::Claim,
    capture: CaptureSession,
}

fn armed(base: &Path, n: u8) -> Armed {
    let git = authority_remote(&base.join(format!("remote-{n}")));
    let binding = attempt_authority::Binding {
        attempt_id: format!("t5-attempt-{n}"),
        v5_sha256: [n; 32],
        launch_tx_hash: [0xaa; 32],
        buy_tx_hash: [0xbb; 32],
    };
    attempt_authority::grant(&git, &binding).unwrap();
    let claim = attempt_authority::consume(&git, &binding, "t5").unwrap();
    let attempt_dir = latch_attempt(&base.join("g07"), &binding.attempt_id, "").unwrap();
    let evidence = EvidenceFile::create(
        base.join(format!("g07-{n}.dbrec")).to_str().unwrap(),
        owner_uid(base).unwrap(),
    )
    .unwrap();
    let capture = CaptureSession::prearm(
        &attempt_dir.join(CAPTURE_JOURNAL_FILE),
        &binding.attempt_id,
        "test-clock",
        "test-boot",
        "test-witness",
    )
    .unwrap();
    Armed {
        git,
        binding,
        attempt_dir,
        evidence,
        claim,
        capture,
    }
}

/// The stops whose cause is the capture journal itself: a broken journal
/// cannot prove the stop byte-free, so these keep the authority.
const JOURNAL_STAGES: [&str; 3] = ["journal attempt", "journal prewarm", "journal submission"];

/// Breaks the capture writer the way a real journal fault does (an enqueue
/// it refuses marks the session failed); nothing is faked in the session.
fn break_journal(capture: &mut CaptureSession) {
    let oversize =
        "x".repeat(darkbot_bsc_execution::qualification_capture::DEFAULT_CAPTURE_MAX_PAYLOAD + 1);
    assert!(capture.record_text("OVERSIZE", 0, &oversize).is_err());
    assert!(!capture.healthy());
}

fn renamed_latch(base: &Path, attempt_id: &str, outcome: &str) -> PathBuf {
    let renamed: Vec<PathBuf> = fs::read_dir(base.join("g07"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&format!("{attempt_id}.{outcome}-"))
        })
        .collect();
    assert_eq!(renamed.len(), 1, "{attempt_id}");
    renamed.into_iter().next().unwrap()
}

/// T5, through the real abort for every stop of this runner (stage list
/// from its own source): with an intact journal the byte-free stop is
/// journaled durably, the latch released and exactly this claim returned
/// (AUTHORIZED again), and that is journaled; where the stop's cause is the
/// journal (a refused, failed writer), the latch is released but the
/// authority stays CONSUMED. Nothing retries.
#[test]
fn every_pre_egress_stop_returns_the_authority_only_with_a_durable_journal() {
    let base = private_dir("t5-stages");
    let stages = pre_egress_stages();
    assert_eq!(stages.len(), 15, "{stages:?}");
    assert!(JOURNAL_STAGES.iter().all(|j| stages.iter().any(|s| s == j)));
    for (n, stage) in stages.iter().enumerate() {
        let mut a = armed(&base, n as u8);
        let journal_fault = JOURNAL_STAGES.contains(&stage.as_str());
        if journal_fault {
            break_journal(&mut a.capture);
        }
        let clock = a.capture.clock();
        let outcome = if stage == "live authorization" {
            "refused"
        } else {
            "pre-egress"
        };
        let unsent = Unsent {
            attempt_dir: &a.attempt_dir,
            evidence: &a.evidence,
            authority: a.claim,
            git: &a.git,
        };
        let said = unsent.abort(a.capture, &clock, outcome, stage, "test stop");
        assert!(said.contains("latch released"), "{stage}: {said}");
        assert!(!a.attempt_dir.exists(), "{stage}: latch released");
        let journal =
            renamed_latch(&base, &a.binding.attempt_id, outcome).join(CAPTURE_JOURNAL_FILE);
        let events = journal_events(&journal);
        let names: Vec<&str> = events
            .iter()
            .map(|e| e.split('|').next().unwrap())
            .collect();
        let state = attempt_authority::state(&a.git, &a.binding);
        if journal_fault {
            assert!(
                said.contains("stays consumed") && said.contains("journal"),
                "{stage}: {said}"
            );
            assert!(
                matches!(state, Ok(attempt_authority::State::Other(_))),
                "{stage}"
            );
            assert!(
                !names.contains(&"event=AUTHORITY_RETURNED"),
                "{stage}: {names:?}"
            );
            assert!(attempt_authority::consume(&a.git, &a.binding, "restart").is_err());
        } else {
            assert!(said.contains("was returned"), "{stage}: {said}");
            assert_eq!(state, Ok(attempt_authority::State::Authorized), "{stage}");
            assert_eq!(
                names,
                [
                    "event=SESSION_PREARMED",
                    "event=PRE_EGRESS_ABORT",
                    "event=AUTHORITY_RETURNED"
                ],
                "{stage}"
            );
            assert!(
                events[1].contains(&format!("stage={stage}\n")),
                "{events:?}"
            );
            assert!(
                events[1].contains("bundle_bytes_written=false"),
                "{events:?}"
            );
            assert!(events[2].contains("attempt_authority=AUTHORIZED\nautomatic_retry=false"));
        }
    }
    fs::remove_dir_all(&base).unwrap();
}

/// T5: a durable RELEASED marker in the attempt directory keeps the
/// latch and the authority CONSUMED, and the journal names the marker.
#[test]
fn a_released_marker_keeps_the_latch_and_the_authority() {
    let base = private_dir("t5-marker");
    let a = armed(&base, 1);
    fs::write(a.attempt_dir.join(RELEASED_MARKER_FILE), b"released").unwrap();
    let clock = a.capture.clock();
    let unsent = Unsent {
        attempt_dir: &a.attempt_dir,
        evidence: &a.evidence,
        authority: a.claim,
        git: &a.git,
    };
    let said = unsent.abort(
        a.capture,
        &clock,
        "pre-egress",
        "send head read",
        "test stop",
    );
    assert!(
        said.contains("never released") && said.contains("stays consumed"),
        "{said}"
    );
    assert!(matches!(
        attempt_authority::state(&a.git, &a.binding),
        Ok(attempt_authority::State::Other(_))
    ));
    assert!(a.attempt_dir.join(ATTEMPT_LATCH_FILE).exists());
    let events = journal_events(&a.attempt_dir.join(CAPTURE_JOURNAL_FILE));
    let last = events.last().unwrap();
    assert!(last.starts_with("event=AUTHORITY_KEPT|"), "{events:?}");
    assert!(
        last.contains("attempt_authority=CONSUMED\nreason=the latch was not released (")
            && last.contains("RELEASED marker"),
        "{last}"
    );
    assert!(attempt_authority::consume(&a.git, &a.binding, "restart").is_err());
    fs::remove_dir_all(&base).unwrap();
}

/// T5: no return when the journal did not take the abort or the latch
/// was not released; otherwise the marker of the renamed latch decides.
#[test]
fn the_return_needs_the_journaled_abort_and_the_released_latch() {
    let dir = PathBuf::from("/attempt.pre-egress-1-0");
    assert!(
        return_condition(Err("writer failed".into()), Some(dir.clone()), "released")
            .unwrap_err()
            .contains("journal")
    );
    let e = return_condition(Ok(()), None, "holds the RELEASED marker").unwrap_err();
    assert!(e.contains("latch") && e.contains("RELEASED marker"), "{e}");
    assert_eq!(
        return_condition(Ok(()), Some(dir.clone()), "released"),
        Ok(dir.join(RELEASED_MARKER_FILE))
    );
}

/// Review MEDIUM-1/LOW-3: the release gate opens only with every sender
/// waiting at it; tickets exist only by spending the claim (the remote stays
/// CONSUMED); a sender that never arrives cancels the gate at the deadline
/// and nobody writes, nothing hangs.
#[test]
fn the_release_gate_needs_the_spent_claim_and_every_sender() {
    let base = private_dir("t5-gate");
    let a = armed(&base, 1);
    let (spent, gate, [t1, t2]) = ReleaseGate::spend::<2>(a.claim);
    assert!(matches!(
        attempt_authority::state(&a.git, &a.binding),
        Ok(attempt_authority::State::Other(head)) if head == spent.consumed_commit()
    ));
    let passed = thread::scope(|s| {
        let runs = [t1, t2].map(|t| s.spawn(move || t.pass()));
        gate.release(2, MAX_BUILDER_IDLE).unwrap();
        runs.map(|r| r.join().unwrap())
    });
    assert_eq!(passed, [true, true]);
    // One sender missing: cancelled at the deadline, the other writes nothing.
    let b = armed(&base, 2);
    let (_, gate, [t1, t2]) = ReleaseGate::spend::<2>(b.claim);
    drop(t2);
    let started = Instant::now();
    let passed = thread::scope(|s| {
        let run = s.spawn(move || t1.pass());
        let e = gate.release(2, Duration::from_millis(200)).unwrap_err();
        assert!(
            e.contains("1 of 2 senders") && e.contains("nothing was written"),
            "{e}"
        );
        run.join().unwrap()
    });
    assert!(!passed);
    assert!(started.elapsed() < MAX_BUILDER_IDLE);
    fs::remove_dir_all(&base).unwrap();
}
