//! C1.9b capture: every EARLY_TX_SEEN record of the peer lanes, as raw CSV.
//!
//! Runs next to a pinned geth with the C1 overlay on Testnet while the
//! controlled launcher sends known launches. It is the Rust consumer of that
//! run: it holds the Rust role lease, reads every peer lane through the
//! borrowed guard, acknowledges drained CLOSING lanes so geth can reuse them,
//! and writes one row per record for `c19b_hook_path.py`.
//!
//! Environment:
//!
//! ```text
//! DARKBOT_C19B_DIR            the Fast-Lane directory of the geth run
//! DARKBOT_C19B_GENERATION     its generation
//! DARKBOT_C19B_PEER_LANES     max_peers + churn_reserve of the geth run
//! DARKBOT_C19B_BOOT_ID        this host's boot id (e.g. kern.bootsessionuuid)
//! DARKBOT_C19B_OUT            the CSV to write
//! ```
//!
//! It stops when `<DARKBOT_C19B_OUT>.stop` exists. It records observations
//! only; it decides nothing and sends nothing.

use darkbot_fastlane::RecordKind;
use darkbot_fastlane::ingress::{C14LanePlan, ingress_times_borrowed};
use darkbot_fastlane::v2::{
    CLOCK_ID_MONOTONIC, LANE_STATE_CLOSING, LaneSetV2, RoleLeaseV2, RuntimeRoleV2, V2Error,
    arena_path_v2,
};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

fn required(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn source_name(source: &[u8]) -> String {
    let end = source
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(source.len());
    String::from_utf8_lossy(&source[..end]).into_owned()
}

fn main() {
    let dir = PathBuf::from(required("DARKBOT_C19B_DIR"));
    let generation: u64 = required("DARKBOT_C19B_GENERATION").parse().unwrap();
    let peer_lanes: usize = required("DARKBOT_C19B_PEER_LANES").parse().unwrap();
    let boot_id = required("DARKBOT_C19B_BOOT_ID");
    let out = PathBuf::from(required("DARKBOT_C19B_OUT"));
    let stop = PathBuf::from(format!("{}.stop", out.display()));
    assert!(!boot_id.is_empty() && !boot_id.contains(','), "boot id");

    let _lease = RoleLeaseV2::acquire(&dir, RuntimeRoleV2::RustOwner, generation).unwrap();
    let lanes = C14LanePlan::production(peer_lanes).lanes();
    let peers = &lanes[1..];
    let mut set = LaneSetV2::open(&arena_path_v2(&dir, generation), generation, peers).unwrap();
    let mut retired = vec![false; set.len()];
    let mut csv = String::from(
        "tx_hash,peer,source,t_packet_decoded_ns,t_ingress_ns,clock_id,host_boot_id,\
         lane_id,lane_epoch,lane_sequence\n",
    );
    let (mut rows, mut invalid, mut faults) = (0u64, 0u64, 0u64);

    while !stop.exists() {
        let mut idle = true;
        for index in 0..set.len() {
            if retired[index] {
                continue;
            }
            let acquired = match set.try_acquire(index) {
                Ok(guard) => guard,
                Err(V2Error::Shutdown) => {
                    retired[index] = true;
                    continue;
                }
                Err(err) => {
                    eprintln!("C19B_LANE_FAULT lane={} {err}", peers[index].lane_id);
                    retired[index] = true;
                    faults += 1;
                    continue;
                }
            };
            if acquired.is_none() {
                drop(acquired);
                if set.lifecycle_state(index) == LANE_STATE_CLOSING {
                    let _ = set.acknowledge_drained_if_closing(index);
                }
                continue;
            }
            let guard = acquired.unwrap();
            idle = false;
            let record = guard.record();
            let times = ingress_times_borrowed(&record);
            match (record.kind, record.clock_id, times) {
                (RecordKind::EarlyTxSeen, CLOCK_ID_MONOTONIC, Ok(times)) => {
                    let _ = writeln!(
                        csv,
                        "{},{},{},{},{},{},{boot_id},{},{},{}",
                        hex(record.hash),
                        hex(record.peer),
                        source_name(record.source),
                        times.packet_decoded_ns,
                        times.ingress_ns,
                        record.clock_id,
                        record.lane_id,
                        record.lane_epoch,
                        record.sequence
                    );
                    rows += 1;
                }
                _ => invalid += 1,
            }
            if guard.release().is_err() {
                retired[index] = true;
                faults += 1;
            }
        }
        if idle {
            std::thread::sleep(Duration::from_micros(200));
        }
    }
    fs::write(&out, csv).unwrap();
    println!("C19B_CAPTURE_DONE rows={rows} invalid_records={invalid} lane_faults={faults}");
    // An invalid record or a lane fault makes the capture incomplete.
    if invalid != 0 || faults != 0 {
        std::process::exit(1);
    }
}
