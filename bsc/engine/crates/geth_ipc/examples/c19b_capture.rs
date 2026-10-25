//! C1.9b capture: every EARLY_TX_SEEN record of the peer lanes, as a raw
//! binary table.
//!
//! Runs next to a pinned geth with the C1 overlay on Testnet while the
//! controlled launcher sends known launches. It is the Rust consumer of that
//! run: it holds the Rust role lease, reads every peer lane through the
//! borrowed guard, acknowledges drained CLOSING lanes so geth can reuse them,
//! and writes one row per record for `c19b_report` (darkbot-bsc-qualification).
//!
//! Environment:
//!
//! ```text
//! DARKBOT_C19B_DIR            the Fast-Lane directory of the geth run
//! DARKBOT_C19B_GENERATION     its generation
//! DARKBOT_C19B_PEER_LANES     max_peers + churn_reserve of the geth run
//! DARKBOT_C19B_BOOT_ID        this host's boot id (e.g. kern.bootsessionuuid)
//! DARKBOT_C19B_OUT            the observation table to write (DBTAB001)
//! ```
//!
//! Next to the table it writes `<DARKBOT_C19B_OUT>.capture.dbrec`
//! (`darkbot.c1.c19b-capture.v1`): the boot id, the counts and the table's
//! SHA-256. Neither file may exist yet. It stops when
//! `<DARKBOT_C19B_OUT>.stop` exists. It records observations only; it
//! decides nothing and sends nothing. Exit 1: the capture is incomplete
//! (invalid records or lane faults); exit 2: it could not run or write.

use darkbot_bsc_geth_ipc::RecordKind;
use darkbot_bsc_geth_ipc::ingress::{C14LanePlan, ingress_times_borrowed};
use darkbot_bsc_geth_ipc::protocol_v2::{
    BorrowedRecordV2, CLOCK_ID_MONOTONIC, LANE_STATE_CLOSING, LaneSetV2, RoleLeaseV2,
    RuntimeRoleV2, V2Error, arena_path_v2,
};
use darkbot_bsc_record::schema::c1_qualification::{c19b_capture as capture, table};
use darkbot_bsc_record::{Draft, Value, observed_now, sha256};
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

fn required(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("{name} is not set"))
}

fn parsed<T: std::str::FromStr>(name: &str) -> Result<T, String> {
    let value = required(name)?;
    value
        .parse()
        .map_err(|_| format!("{name}={value:?} is not a number"))
}

/// The row of a valid EARLY_TX_SEEN record on `CLOCK_MONOTONIC`; `None`
/// for anything else, which the capture counts as invalid.
fn observation(record: &BorrowedRecordV2<'_>) -> Option<table::Observation> {
    if record.kind != RecordKind::EarlyTxSeen || record.clock_id != CLOCK_ID_MONOTONIC {
        return None;
    }
    let times = ingress_times_borrowed(record).ok()?;
    Some(table::Observation {
        tx_hash: record.hash.try_into().ok()?,
        peer: record.peer.try_into().ok()?,
        source: record.source.try_into().ok()?,
        t_packet_decoded_ns: times.packet_decoded_ns,
        t_ingress_ns: times.ingress_ns,
        lane_epoch: record.lane_epoch,
        lane_sequence: record.sequence,
        clock_id: record.clock_id,
        lane_id: record.lane_id,
    })
}

/// Writes a new file; an existing one is never replaced.
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

struct Counts {
    rows: u64,
    invalid: u64,
    faults: u64,
}

fn main() -> ExitCode {
    match run() {
        Ok(counts) => {
            println!(
                "C19B_CAPTURE_DONE rows={} invalid_records={} lane_faults={}",
                counts.rows, counts.invalid, counts.faults
            );
            // An invalid record or a lane fault makes the capture incomplete.
            if counts.invalid != 0 || counts.faults != 0 {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(problem) => {
            eprintln!("C19B_CAPTURE_FAIL {problem}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<Counts, String> {
    let dir = PathBuf::from(required("DARKBOT_C19B_DIR")?);
    let generation: u64 = parsed("DARKBOT_C19B_GENERATION")?;
    let peer_lanes: usize = parsed("DARKBOT_C19B_PEER_LANES")?;
    let boot_id = required("DARKBOT_C19B_BOOT_ID")?;
    let out = PathBuf::from(required("DARKBOT_C19B_OUT")?);
    let record_path = PathBuf::from(format!("{}.capture.dbrec", out.display()));
    let stop = PathBuf::from(format!("{}.stop", out.display()));
    if boot_id.is_empty() {
        return Err("DARKBOT_C19B_BOOT_ID is empty".into());
    }
    for path in [&out, &record_path] {
        if path.exists() {
            return Err(format!(
                "{} exists; a capture never replaces one",
                path.display()
            ));
        }
    }

    let _lease = RoleLeaseV2::acquire(&dir, RuntimeRoleV2::RustOwner, generation)
        .map_err(|e| format!("Rust role lease: {e:?}"))?;
    let lanes = C14LanePlan::production(peer_lanes).lanes();
    let peers = lanes.get(1..).ok_or("the lane plan has no peer lanes")?;
    let mut set = LaneSetV2::open(&arena_path_v2(&dir, generation), generation, peers)
        .map_err(|e| format!("peer lanes: {e:?}"))?;
    let mut retired = vec![false; set.len()];
    let mut rows = Vec::new();
    let mut counts = Counts {
        rows: 0,
        invalid: 0,
        faults: 0,
    };

    while !stop.exists() {
        let mut idle = true;
        for index in 0..set.len() {
            if retired[index] {
                continue;
            }
            let acquired = match set.try_acquire(index) {
                Ok(acquired) => acquired,
                Err(V2Error::Shutdown) => {
                    retired[index] = true;
                    continue;
                }
                Err(err) => {
                    eprintln!("C19B_LANE_FAULT lane={} {err}", peers[index].lane_id);
                    retired[index] = true;
                    counts.faults += 1;
                    continue;
                }
            };
            if acquired.is_none() {
                // The empty acquisition ends its borrow of the lane set here.
                drop(acquired);
                if set.lifecycle_state(index) == LANE_STATE_CLOSING {
                    let _ = set.acknowledge_drained_if_closing(index);
                }
                continue;
            }
            let Some(guard) = acquired else {
                continue;
            };
            idle = false;
            match observation(&guard.record()) {
                Some(row) => {
                    row.write(&mut rows);
                    counts.rows += 1;
                }
                None => counts.invalid += 1,
            }
            if guard.release().is_err() {
                retired[index] = true;
                counts.faults += 1;
            }
        }
        if idle {
            std::thread::sleep(Duration::from_micros(200));
        }
    }

    let observations = table::encode(&table::C19B_OBSERVATION, &rows).map_err(|e| e.0)?;
    write_new(&out, &observations)?;
    let mut summary = Draft::new(&capture::SCHEMA);
    summary
        .set(capture::HOST_BOOT_ID, boot_id)
        .set(capture::GENERATION, generation)
        .set(capture::PROVENANCE_TOOL, "geth_ipc example c19b_capture")
        .set(capture::PROVENANCE_OBSERVED_UNIX_MS, observed_now()?)
        .set(capture::PEER_LANES, peer_lanes as u64)
        .set(capture::ROWS, counts.rows)
        .set(capture::INVALID_RECORDS, counts.invalid)
        .set(capture::LANE_FAULTS, counts.faults)
        .set(
            capture::OBSERVATIONS_SHA256,
            Value::B256(sha256(&observations)),
        );
    let bytes = summary.encode().map_err(|e| e.0)?;
    write_new(&record_path, &bytes)?;
    Ok(counts)
}
