use super::*;
use crate::strategy_profiles::{Owner as Profiles, Request};
use std::sync::atomic::AtomicBool;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn config() -> String {
    std::fs::read_to_string(root().join(crate::sources::RUNTIME_CONFIG_PATH)).unwrap()
}

/// The configuration file's values, `set` over them.
fn values(set: &[(&str, &str)]) -> Vec<(String, String)> {
    settings::effective_values(&config(), &settings::Settings::default())
        .into_iter()
        .map(|(k, v)| {
            let v = set
                .iter()
                .find(|(s, _)| *s == k)
                .map_or(v, |(_, x)| x.to_string());
            (k.to_string(), v)
        })
        .collect()
}

/// A run directory with its owner state, a recorded run `77` of two Flap
/// launches and a profile p000001 at revision 1 on Flap (the
/// configuration file's values).
fn setup(tag: &str) -> (PathBuf, PathBuf) {
    use std::os::unix::fs::DirBuilderExt;
    let run = std::env::temp_dir().join(format!("bsc-training-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&run);
    let sources = run.join("bsc-sources");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(&sources)
        .unwrap();
    crate::runtime_strategy::tests::recorded_run(&run.join("bsc-runtime/journal"), 77, 2);
    profile(&sources, "c1", "fast", 0, &[]);
    (run, sources)
}

fn profile(sources: &Path, request: &str, name: &str, catalog: u64, set: &[(&str, &str)]) {
    let create = Request::Create {
        name: name.into(),
        venues: vec!["BSC_VENUE_FLAP".into()],
        values: values(set),
    };
    let r = Profiles::open(sources)
        .unwrap()
        .apply(request, &create, catalog, &config(), &|_| None, 1)
        .unwrap();
    assert!(r.applied(), "{}", r.line());
}

fn start(plan: (&str, Option<u64>)) -> Start {
    Start {
        profile_id: "p000001".into(),
        mode: "REPLAY".into(),
        venues: vec!["BSC_VENUE_FLAP".into()],
        stop_kind: plan.0.into(),
        bound: plan.1,
        replay_source: "77".into(),
    }
}

const UNTIL: (&str, Option<u64>) = ("UNTIL_STOPPED", None);
const NONE: Job = Job::None;

/// BSC-V8-02: a start is checked whole (the profile at its revision, its
/// venues, the catalogue, the mode, exactly one bound); one run at a time
/// while its job runs; the same request again is its receipt, another
/// under its ID an error.
#[test]
fn a_start_is_one_checked_run_at_a_time() {
    let (run, sources) = setup("start");
    let o = Owner::open(&sources).unwrap();
    let text = config();
    let refused = |r: Receipt| (r.outcome, r.reason.unwrap_or_default());
    let shadow = Start {
        mode: "SHADOW".into(),
        ..start(UNTIL)
    };
    let (outcome, why) = refused(o.start("s0", &shadow, 1, NONE, &run, &text, 1).unwrap());
    assert_eq!(outcome, "REJECTED_INVALID");
    assert!(why.contains("live inputs"), "{why}");
    let (outcome, _) = refused(
        o.start("s1", &start(UNTIL), 2, NONE, &run, &text, 1)
            .unwrap(),
    );
    assert_eq!(outcome, "REJECTED_STALE");
    let other = Start {
        replay_source: "78".into(),
        ..start(UNTIL)
    };
    let r = o.start("s2", &other, 1, NONE, &run, &text, 1).unwrap();
    assert_eq!(r.outcome, "REJECTED_NOT_FOUND");
    let four = Start {
        venues: vec!["BSC_VENUE_FOUR_CLASSIC".into()],
        ..start(UNTIL)
    };
    let r = o.start("s3", &four, 1, NONE, &run, &text, 1).unwrap();
    assert_eq!(r.outcome, "REJECTED_INVALID");
    for bad in [
        ("DURATION", None),
        ("UNTIL_STOPPED", Some(5)),
        ("LAUNCH_COUNT", Some(0)),
        ("FOREVER", None),
    ] {
        assert!(
            o.start("s4", &start(bad), 1, NONE, &run, &text, 1).is_err(),
            "{bad:?}"
        );
    }
    let bounded = start(("LAUNCH_COUNT", Some(1)));
    let r = o.start("s5", &bounded, 1, NONE, &run, &text, 2).unwrap();
    assert_eq!(
        (r.outcome, r.run_id.as_deref()),
        ("APPLIED", Some("t000001"))
    );
    assert_eq!(
        r.line(),
        "outcome=APPLIED action=START run_id=t000001 request_id=s5"
    );
    assert_eq!(o.start("s5", &bounded, 1, NONE, &run, &text, 3).unwrap(), r);
    assert!(
        o.start("s5", &start(UNTIL), 1, NONE, &run, &text, 3)
            .is_err()
    );
    // One run at a time while its job runs.
    let r = o
        .start("s6", &start(UNTIL), 1, Job::Running, &run, &text, 3)
        .unwrap();
    assert_eq!(
        refused(r),
        ("REJECTED_ACTIVE", "training run t000001 is PENDING".into())
    );
    // Review M1/M2: a lost receipt is recovered from the run (never its own
    // run ended for want of a job); another CAS under its ID is refused.
    std::fs::remove_file(sources.join(DIR).join("receipts/s5.dbrec")).unwrap();
    assert!(o.start("s5", &bounded, 2, NONE, &run, &text, 4).is_err());
    let again = o.start("s5", &bounded, 1, NONE, &run, &text, 4).unwrap();
    assert_eq!(
        (again.outcome, again.run_id.as_deref()),
        ("APPLIED", Some("t000001"))
    );
    let p = runs(&sources).unwrap();
    assert_eq!(p.len(), 1);
    assert_eq!(
        (p[0].status, p[0].profile_revision, p[0].values.len()),
        (Status::Pending, 1, KEYS.len())
    );
    // Another request while no job runs: the run left behind ends FAILED,
    // the new one starts.
    let r = o
        .start("s7", &start(UNTIL), 1, NONE, &run, &text, 5)
        .unwrap();
    assert_eq!(
        (r.outcome, r.run_id.as_deref()),
        ("APPLIED", Some("t000002"))
    );
    let left = o.run("t000001").unwrap().unwrap();
    assert_eq!(
        (left.status, left.stop_reason.as_deref()),
        (Status::Failed, Some("its job was gone at the next start"))
    );
    // A run directory a crash left without a state is no run.
    std::fs::create_dir(sources.join(DIR).join("runs/t000003")).unwrap();
    assert_eq!(o.newest().unwrap().unwrap().run_id, "t000002");
    std::fs::remove_dir_all(run).unwrap();
}

/// BSC-V8-02/03: the job takes its PENDING run up, trains to its plan and
/// writes its end with the result and why (the proto's words); the
/// profile stays in use while it is active; a stopped PENDING run ends at
/// once; a job restarted on a RUNNING run, or signalled without a stop
/// request, ends it INTERRUPTED; a run whose job is gone ends.
#[test]
fn the_job_runs_its_run_to_its_end() {
    let (run, sources) = setup("job");
    let o = Owner::open(&sources).unwrap();
    let text = config();
    let quiet = AtomicBool::new(false);
    let now = || 5_000u64;
    o.start("s1", &start(UNTIL), 1, NONE, &run, &text, 1)
        .unwrap();
    let profile = profiles::current_file(&sources).unwrap().0.profiles[0].clone();
    assert_eq!(
        in_use(&sources, &profile).unwrap().as_deref(),
        Some("training run t000001 is PENDING")
    );
    let done = o
        .execute("t000001", &root(), &run, &text, &quiet, &now)
        .unwrap();
    assert_eq!(
        (
            done.status,
            done.seq,
            done.started_unix_ms,
            done.stop_reason.as_deref()
        ),
        (Status::Completed, 3, Some(5_000), Some("JOURNAL_END"))
    );
    assert!(done.config_file_sha256.is_some());
    let t = done.result.clone().unwrap();
    // The profile is the configuration file's: its filters (two buys)
    // decide both launches (none had a buy), none passes.
    assert_eq!(
        (t.counters.launches, t.decided(), t.counters.passed, t.end),
        (2, 2, 0, Some(TrainingEnd::JournalEnd))
    );
    assert!(t.manifest && t.followed_at_end == 0);
    assert_eq!(in_use(&sources, &profile).unwrap(), None);
    // Its states are kept, the last read back whole.
    assert_eq!(o.run("t000001").unwrap().unwrap(), done);
    // A second job on the ended run changes nothing.
    assert_eq!(
        o.execute("t000001", &root(), &run, &text, &quiet, &now)
            .unwrap(),
        done
    );
    // Bounded at one launch, and by duration.
    o.start(
        "s2",
        &start(("LAUNCH_COUNT", Some(1))),
        1,
        NONE,
        &run,
        &text,
        6,
    )
    .unwrap();
    let r = o
        .execute("t000002", &root(), &run, &text, &quiet, &now)
        .unwrap();
    let t = r.result.clone().unwrap();
    assert_eq!((t.decided(), t.end), (1, Some(TrainingEnd::Bound)));
    assert_eq!(r.stop_reason.as_deref(), Some("PLAN_LAUNCH_COUNT"));
    o.start("s3", &start(("DURATION", Some(1))), 1, NONE, &run, &text, 7)
        .unwrap();
    let r = o
        .execute("t000003", &root(), &run, &text, &quiet, &now)
        .unwrap();
    assert_eq!(
        (r.status, r.stop_reason.as_deref()),
        (Status::Completed, Some("PLAN_DURATION"))
    );
    // The recorded launches are 100 ms apart: one within 1 ms.
    assert_eq!(r.result.unwrap().decided(), 1);
    // A stop while PENDING ends it at once; its job then does nothing.
    o.start("s4", &start(UNTIL), 1, NONE, &run, &text, 8)
        .unwrap();
    assert_eq!(
        o.stop("x4", "t000004", "operator", 9).unwrap().outcome,
        "APPLIED"
    );
    let stopped = o.run("t000004").unwrap().unwrap();
    assert_eq!(
        (stopped.status, stopped.stop_reason.as_deref()),
        (Status::Stopped, Some("STOP_REQUESTED: operator"))
    );
    let again = o
        .execute("t000004", &root(), &run, &text, &quiet, &now)
        .unwrap();
    assert_eq!(again.status, Status::Stopped);
    assert_eq!(
        o.stop("x5", "t000004", "", 9).unwrap().outcome,
        "REJECTED_ENDED"
    );
    assert_eq!(
        o.stop("x6", "t000099", "", 9).unwrap().outcome,
        "REJECTED_NOT_FOUND"
    );
    // A job restarted on a RUNNING run: INTERRUPTED.
    o.start("s5", &start(UNTIL), 1, NONE, &run, &text, 10)
        .unwrap();
    let mut running = o.run("t000005").unwrap().unwrap().next(Status::Running);
    running.started_unix_ms = Some(10);
    o.write(&running, 10).unwrap();
    let r = o
        .execute("t000005", &root(), &run, &text, &quiet, &now)
        .unwrap();
    assert_eq!(r.status, Status::Interrupted);
    // Review L2: signalled without a stop request (a logout): INTERRUPTED.
    o.start("s6", &start(UNTIL), 1, NONE, &run, &text, 11)
        .unwrap();
    let signalled = AtomicBool::new(true);
    let r = o
        .execute("t000006", &root(), &run, &text, &signalled, &now)
        .unwrap();
    assert_eq!(
        (r.status, r.stop_reason.as_deref()),
        (
            Status::Interrupted,
            Some("its job was signalled without a stop request")
        )
    );
    assert_eq!(r.result.unwrap().records, 1);
    // The stop request of a RUNNING run stays for its job; with no job
    // the controller ends the run INTERRUPTED.
    o.start("s7", &start(UNTIL), 1, NONE, &run, &text, 12)
        .unwrap();
    let mut taken = o.run("t000007").unwrap().unwrap().next(Status::Running);
    taken.started_unix_ms = Some(12);
    o.write(&taken, 12).unwrap();
    assert_eq!(
        o.stop("x7", "t000007", "enough", 13).unwrap().outcome,
        "APPLIED"
    );
    assert_eq!(o.run("t000007").unwrap().unwrap().status, Status::Running);
    let r = o
        .end_orphans("stopped while its job was gone", 14)
        .unwrap()
        .unwrap();
    assert_eq!(
        (r.run_id.as_str(), r.status),
        ("t000007", Status::Interrupted)
    );
    assert_eq!(
        o.end_orphans("again", 15).unwrap().unwrap().status,
        Status::Interrupted
    );
    // A stop request the job finds when it takes the run up: STOPPED with
    // its reason, nothing replayed.
    o.start("s8", &start(UNTIL), 1, NONE, &run, &text, 16)
        .unwrap();
    o.stop("x8", "t000008", "", 16).unwrap();
    let r = o
        .execute("t000008", &root(), &run, &text, &quiet, &now)
        .unwrap();
    assert_eq!(
        (r.status, r.stop_reason.as_deref()),
        (Status::Stopped, Some("STOP_REQUESTED"))
    );
    let ids: Vec<String> = runs(&sources)
        .unwrap()
        .into_iter()
        .map(|r| r.run_id)
        .collect();
    assert_eq!(ids.first().map(String::as_str), Some("t000008"));
    std::fs::remove_dir_all(run).unwrap();
}

/// Review M1: a stop's ID is one request's even without its receipt: the
/// same stop again is APPLIED, another payload under its ID refused.
#[test]
fn a_stop_is_one_request_even_without_its_receipt() {
    let (run, sources) = setup("stop");
    let o = Owner::open(&sources).unwrap();
    let text = config();
    o.start("s1", &start(UNTIL), 1, NONE, &run, &text, 1)
        .unwrap();
    assert!(o.stop("x1", "t000001", "enough", 2).unwrap().applied());
    std::fs::remove_file(sources.join(DIR).join("receipts/x1.dbrec")).unwrap();
    // Its run ended meanwhile (STOPPED at once): still this request's stop.
    let again = o.stop("x1", "t000001", "enough", 3).unwrap();
    assert_eq!(again.outcome, "APPLIED");
    std::fs::remove_file(sources.join(DIR).join("receipts/x1.dbrec")).unwrap();
    assert!(o.stop("x1", "t000001", "other", 3).is_err());
    o.start("s2", &start(UNTIL), 1, NONE, &run, &text, 4)
        .unwrap();
    assert!(o.stop("x1", "t000002", "enough", 5).is_err());
    std::fs::remove_dir_all(run).unwrap();
}

/// BSC-V8-03 PASS: two profiles with different settings give their own
/// results, the same each time they run on the same recorded run.
#[test]
fn two_profiles_give_their_own_stable_results() {
    let (run, sources) = setup("profiles");
    // One that needs no buy: both launches pass.
    profile(&sources, "c2", "open", 1, &[("BSC_STRATEGY_C_FILTERS", "")]);
    let o = Owner::open(&sources).unwrap();
    let text = config();
    let quiet = AtomicBool::new(false);
    let mut results = Vec::new();
    for (i, id) in ["p000001", "p000002", "p000001", "p000002"]
        .into_iter()
        .enumerate()
    {
        let s = Start {
            profile_id: id.into(),
            ..start(UNTIL)
        };
        let r = o
            .start(&format!("s{i}"), &s, 1, NONE, &run, &text, 1)
            .unwrap();
        let run_id = r.run_id.unwrap();
        let done = o
            .execute(&run_id, &root(), &run, &text, &quiet, &|| 2)
            .unwrap();
        results.push(done.result.unwrap());
    }
    assert_eq!(results[0], results[2]);
    assert_eq!(results[1], results[3]);
    assert_eq!(
        (results[0].counters.passed, results[1].counters.passed),
        (0, 2)
    );
    std::fs::remove_dir_all(run).unwrap();
}

/// The catalogue: the recorded runs under the run directory, newest first,
/// their manifest's counts.
#[test]
fn the_catalogue_is_the_recorded_runs() {
    let (run, _) = setup("catalogue");
    crate::runtime_strategy::tests::recorded_run(&run.join("bsc-runtime/journal"), 120, 1);
    let list = replay_sources(&run).unwrap();
    let ids: Vec<&str> = list.iter().map(|s| s.run_id.as_str()).collect();
    assert_eq!(ids, ["120", "77"]);
    assert!(
        list.iter()
            .all(|s| s.manifest && s.records.is_some_and(|n| n > 0))
    );
    std::fs::remove_dir_all(run).unwrap();
}

/// The command line takes each flag from its pair, the bound only with a
/// bounded plan's flag, the controller's view of the job.
#[test]
fn the_training_command_line_takes_each_flag_from_its_pair() {
    let line = |w: &[&str]| w.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let places = ["--state", "/s", "--rundir", "/r", "--config", "/c"];
    let mut a = line(&[
        "--profile",
        "p000001",
        "--expect",
        "2",
        "--mode",
        "REPLAY",
        "--venues",
        "BSC_VENUE_FLAP",
        "--stop-kind",
        "DURATION",
        "--bound",
        "60000",
        "--replay-source",
        "77",
        "--job",
        "NONE",
        "--request-id",
        "--state",
    ]);
    a.extend(line(&places));
    let (c, at) = command("start", &a).unwrap();
    assert_eq!(at.rundir, "/r");
    match c {
        Command::Start {
            start,
            expected,
            job,
            request_id,
        } => {
            assert_eq!(
                (expected, job, request_id.as_str(), start.bound),
                (2, Job::None, "--state", Some(60000))
            );
        }
        other => panic!("{other:?}"),
    }
    let mut odd = a.clone();
    let at = odd.iter().position(|x| x == "NONE").unwrap();
    odd[at] = "MAYBE".into();
    assert!(command("start", &odd).is_err());
    let mut stop = line(&["--run", "t000001", "--reason", "", "--request-id", "x1"]);
    stop.extend(line(&places));
    assert!(matches!(
        command("stop", &stop).unwrap().0,
        Command::Stop { .. }
    ));
    let mut twice = stop.clone();
    twice.extend(line(&["--run", "t2"]));
    assert!(command("stop", &twice).is_err());
    assert!(command("stop", &stop[..stop.len() - 2]).is_err());
    assert!(command("resume", &stop).is_err());
}
