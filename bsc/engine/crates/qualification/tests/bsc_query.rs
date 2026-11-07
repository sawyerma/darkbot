//! BSC-V8: the deployed reporter's `--query` arm end to end, spawned as the
//! gateway's `POST /api/v1/bsc/query` spawns it (a cleared environment, the
//! request on stdin): the answer on stdout, the run directory untouched.
use darkbot_bsc_qualification::status::wire::bsc as w;
use prost::Message;
use std::io::Write;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const REPORTER: &str = env!("CARGO_BIN_EXE_bsc-status-reporter");

/// Every path under `dir` with its size.
fn listing(dir: &Path) -> Vec<(PathBuf, u64)> {
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let e = e.unwrap();
            let meta = e.metadata().unwrap();
            if meta.is_dir() {
                todo.push(e.path());
            }
            out.push((e.path(), meta.len()));
        }
    }
    out.sort();
    out
}

#[test]
fn the_reporter_answers_a_query_on_stdin_and_writes_nothing() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let run = std::env::temp_dir().join(format!("bsc-query-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&run);
    std::fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(run.join("bsc-sources"))
        .unwrap();
    let before = listing(&run);
    let ask = |request: &[u8]| {
        let mut child = Command::new(REPORTER)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("DARKBOT_BSC_ROOT", &root)
            .env("DARKBOT_RUNDIR", &run)
            .arg("--query")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(request).unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        w::BscQueryResponse::decode(out.stdout.as_slice()).unwrap()
    };
    let r = ask(&w::BscQueryRequest {
        request_id: "q1".into(),
        query: Some(w::bsc_query_request::Query::StrategySettings(
            w::StrategyCSettingsQuery {
                include_features: true,
            },
        )),
    }
    .encode_to_vec());
    assert_eq!(
        (r.request_id.as_str(), r.availability),
        ("q1", w::Availability::Available as i32)
    );
    let Some(w::bsc_query_response::Result::StrategySettings(s)) = r.result else {
        panic!("no settings state")
    };
    // No generation on file: nothing stored, the file's basis effective.
    assert_eq!(s.stored_revision, 0);
    assert_eq!(s.effective.unwrap().max_open, Some(5));
    assert_eq!(s.apply_status, w::SettingsApplyStatus::NotRunning as i32);
    assert!(!s.features.is_empty());
    // A request that does not decode is answered, never a crash.
    let junk = ask(&[0xff, 0xff]);
    assert_eq!(junk.availability, w::Availability::Unavailable as i32);
    assert_eq!(listing(&run), before, "the query wrote nothing");
    std::fs::remove_dir_all(run).unwrap();
}
