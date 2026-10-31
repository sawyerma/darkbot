//! BSC3-009: the runtime's hot path under its real load, read from a run's
//! journal (read only, nothing is written): every PAPER BUY the PAPER sink
//! took (`Q` records, `runtime_journal::paper_handoff_payload`) carries the
//! production loop's stage times — the source received the launch's frame
//! (`T_ingress`), decoded, admitted, signed, sink (`T_shadow_sink`). The
//! first stage holds the wait in the source's queue and whatever the loop
//! did before it took the event (canonical drain, reconcile, status,
//! checkpoint, C2), as it happened in that run. Run with
//! `BSC_HOT_PATH_RUNDIR=<rundir> BSC_HOT_PATH_CONFIG=<ingress.public.env> cargo test --release -p darkbot-bsc-execution --features runtime --test hot_path_load -- --ignored --nocapture`.
//! A local metric of that host and run: no transport, no T0→T_buy claim.
#![cfg(feature = "runtime")]

use darkbot_bsc_execution::runtime::{RuntimeConfig, journal_max_payload};
use darkbot_bsc_execution::runtime_journal::{PAPER_HANDOFF_HEAD, RUN_MANIFEST};
use darkbot_bsc_journal::storage::run::RunReader;
use darkbot_bsc_journal::{CaptureRecord, JournalError, JournalReader, RecordKind};
use std::fs;
use std::path::Path;

/// `Q | sequence u64 | launch hash [32] | BUY hash [32] | token [20] |
/// ingress, decoded, admitted, signed, sink ns u64 | signed bytes`.
const TIMES_AT: usize = PAPER_HANDOFF_HEAD - 5 * 8;

fn times(r: &CaptureRecord) -> Option<[u64; 5]> {
    let p = &r.payload;
    if r.kind != RecordKind::Runtime || p.first() != Some(&b'Q') || p.len() < PAPER_HANDOFF_HEAD {
        return None;
    }
    let mut t = [0u64; 5];
    for (i, v) in t.iter_mut().enumerate() {
        let at = TIMES_AT + 8 * i;
        *v = u64::from_le_bytes(p[at..at + 8].try_into().ok()?);
    }
    Some(t)
}

fn line(segment: &str, values: &mut [u64]) -> String {
    values.sort_unstable();
    let n = values.len();
    let rank = |p: f64| values[(((n as f64) * p).ceil() as usize).clamp(1, n) - 1];
    let at_least = |ns: u64| values.iter().filter(|x| **x >= ns).count();
    format!(
        "{segment}: n={n} p50={} p95={} p99={} max={} ns; >=1ms {} >=3ms {}",
        rank(0.50),
        rank(0.95),
        rank(0.99),
        values[n - 1],
        at_least(1_000_000),
        at_least(3_000_000)
    )
}

#[test]
#[ignore = "hot path under a run's real load; needs BSC_HOT_PATH_RUNDIR, run in release"]
fn hot_path_under_the_runs_load() {
    let rundir = std::env::var("BSC_HOT_PATH_RUNDIR").expect("BSC_HOT_PATH_RUNDIR");
    let config_path = std::env::var("BSC_HOT_PATH_CONFIG").expect("BSC_HOT_PATH_CONFIG");
    let config = RuntimeConfig::parse(&fs::read_to_string(&config_path).unwrap()).unwrap();
    let max_payload = journal_max_payload(&config);
    let dir = Path::new(&rundir).join("bsc-runtime/journal");
    // The journals as the census reads them: `<started_unix_ms>/` runs and
    // `<started_unix_ms>.dbjnl` files of an older build.
    let mut names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| {
            n.trim_end_matches(".dbjnl").parse::<u64>().is_ok()
                && (n.ends_with(".dbjnl") || dir.join(n).is_dir())
        })
        .collect();
    names.sort();
    let last = names.last().cloned();
    let mut stages: [Vec<u64>; 5] = Default::default();
    let mut unordered = 0usize;
    for name in &names {
        let path = dir.join(name);
        let mut next: Box<dyn FnMut() -> Result<Option<CaptureRecord>, JournalError>> =
            if path.is_dir() {
                // A sealed run is read with at least the bound it was
                // written with (as the census does).
                let written = fs::read(path.join(RUN_MANIFEST))
                    .ok()
                    .and_then(|b| {
                        darkbot_bsc_record::schema::bsc_capture::capture_run::decode_run(&b).ok()
                    })
                    .and_then(|m| {
                        m.opt_u64(darkbot_bsc_record::schema::bsc_capture::capture_run::MAX_PAYLOAD)
                            .ok()
                            .flatten()
                    })
                    .and_then(|v| usize::try_from(v).ok());
                let bound = written.map_or(max_payload, |w| w.max(max_payload));
                let mut r = RunReader::open(&path, bound).unwrap();
                Box::new(move || r.next_record())
            } else {
                let file = fs::File::open(&path).unwrap();
                let mut r = JournalReader::new(std::io::BufReader::new(file), max_payload).unwrap();
                Box::new(move || r.next_record())
            };
        let mut buys = 0usize;
        // The run being written ends in a frame not complete yet.
        let end = loop {
            match next() {
                Ok(Some(r)) => {
                    let Some(t) = times(&r) else { continue };
                    if t.windows(2).any(|w| w[1] < w[0]) {
                        unordered += 1;
                        continue;
                    }
                    buys += 1;
                    for (i, s) in stages.iter_mut().take(4).enumerate() {
                        s.push(t[i + 1] - t[i]);
                    }
                    stages[4].push(t[4] - t[0]);
                }
                Ok(None) => break "end".to_string(),
                // Only the run being written may end in a frame not
                // complete yet; any other error is no sample.
                Err(JournalError::Truncated) if Some(name) == last.as_ref() => {
                    break "the run being written".to_string();
                }
                Err(e) => panic!("{name}: {e}"),
            }
        };
        println!("journal {name}: {buys} PAPER BUYs ({end})");
    }
    assert!(!stages[4].is_empty(), "no PAPER BUY in {rundir}");
    assert_eq!(unordered, 0, "stage times out of order");
    for (segment, values) in [
        "ingress→decoded (queue wait, loop work, decode)",
        "decoded→admitted",
        "admitted→signed",
        "signed→sink",
        "ingress→sink",
    ]
    .into_iter()
    .zip(stages.iter_mut())
    {
        println!("MEASURE {}", line(segment, values));
    }
}
