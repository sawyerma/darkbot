//! C1.9 native latency consumer.
//!
//! Run by `bsc/native/tools/c19_native_latency.sh`. It consumes the
//! integrated C1.5-C1.8 candidate exactly as HOT0 does (`Hot0Poller`:
//! borrowed guard, Chain lane first, fair multi-lane polling, one launch copy
//! into an `IntentSlot`), publishes one prepared signed BUY per admitted
//! launch on the egress lane, and reads geth's outbound evidence.
//!
//! It records raw boundaries only; `c19_analyze.py` derives every statistic:
//!
//! - `T_packet_decoded`, `T_ingress`: from the record (geth, clock id 1);
//! - L1: geth's pre-commit and post-commit probes around the SPSC Release
//!   commit, the bracket C1.4a used;
//! - L2: the clock read immediately after the poller acquired the record;
//! - egress commit: this process's clock read immediately before the egress
//!   publish; first and every selected-peer handoff: geth's worker clocks.
//!
//! Every timestamp is `CLOCK_MONOTONIC` of this host. Heap allocations of
//! this thread inside the qualification window are counted and reported.

use darkbot_fastlane::hot0::{
    Hot0Config, Hot0Decision, Hot0Outcome, Hot0Poller, IntentBuy, monotonic_ns,
};
use darkbot_fastlane::ingress::{C14_FLAP_SCOPE, decode_tx_target};
use darkbot_fastlane::outbound::{
    HandoffEvidence, OutboundStatus, chain_lane, decode_handoff, egress_lane, evidence_lane,
    publish_buy,
};
use darkbot_fastlane::v2::{
    BorrowedRecordV2, DirectionV2, LaneClassV2, LaneConfigV2, LaneSetV2, ProducerV2, arena_path_v2,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const SUPER_HEADER_SIZE: usize = 256;
const DIRECTORY_ENTRY_SIZE: usize = 128;
const LANE_HEADER_SIZE: usize = 384;
const SH_LANE_COUNT: usize = 24;
const DE_LANE_ID: usize = 0;
const DE_LANE_OFFSET: usize = 16;
const LH_WRITE_SEQUENCE: usize = 0;
const PROBE_PRE_COMMIT: usize = 160;
const PROBE_POST_COMMIT: usize = 168;
const PROBE_MAGIC_OFFSET: usize = 176;
const PROBE_MAGIC: u64 = 0x4331_3950_524f_4245;

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

fn count() {
    let _ = ALLOCATIONS.try_with(|cell| cell.set(cell.get() + 1));
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocations() -> u64 {
    ALLOCATIONS.with(Cell::get)
}

/// A read-only view of the arena for the measurement probes, which live in
/// reserved slot-header bytes outside the record contract.
struct RawMap {
    _file: File,
    ptr: NonNull<u8>,
    len: usize,
}

impl RawMap {
    fn open(path: &Path) -> Self {
        let file = File::options().read(true).write(true).open(path).unwrap();
        let len = usize::try_from(file.metadata().unwrap().len()).unwrap();
        let raw = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        assert_ne!(raw, libc::MAP_FAILED);
        let ptr = NonNull::new(raw.cast::<u8>()).expect("mmap returned null");
        Self {
            _file: file,
            ptr,
            len,
        }
    }

    fn u64_at(&self, offset: usize) -> u64 {
        assert!(offset + 8 <= self.len && offset.is_multiple_of(8));
        let cell = unsafe { &*self.ptr.as_ptr().add(offset).cast::<AtomicU64>() };
        cell.load(Ordering::Acquire)
    }

    fn bytes(&self, offset: usize, len: usize) -> &[u8] {
        assert!(offset + len <= self.len);
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr().add(offset), len) }
    }

    /// Lane id -> lane offset, from the arena directory.
    fn lane_offsets(&self) -> BTreeMap<u16, usize> {
        let mut count = [0u8; 4];
        count.copy_from_slice(self.bytes(SH_LANE_COUNT, 4));
        let mut out = BTreeMap::new();
        for entry in 0..u32::from_le_bytes(count) as usize {
            let base = SUPER_HEADER_SIZE + entry * DIRECTORY_ENTRY_SIZE;
            let mut id = [0u8; 2];
            let mut offset = [0u8; 8];
            id.copy_from_slice(self.bytes(base + DE_LANE_ID, 2));
            offset.copy_from_slice(self.bytes(base + DE_LANE_OFFSET, 8));
            let lane_offset = usize::try_from(u64::from_le_bytes(offset)).unwrap();
            out.insert(u16::from_le_bytes(id), lane_offset);
        }
        out
    }
}

impl Drop for RawMap {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr.as_ptr().cast(), self.len);
        }
    }
}

fn manifest(path: &Path) -> BTreeMap<String, String> {
    let text = fs::read_to_string(path).unwrap();
    let pairs = text.lines().filter_map(|line| line.split_once('='));
    pairs.map(|(k, v)| (k.to_owned(), v.to_owned())).collect()
}

fn number(map: &BTreeMap<String, String>, key: &str) -> u64 {
    let value = map
        .get(key)
        .unwrap_or_else(|| panic!("manifest has no {key}"));
    value.parse().unwrap_or_else(|_| panic!("manifest {key}"))
}

fn unhex(text: &str) -> Vec<u8> {
    let step = (0..text.len()).step_by(2);
    step.map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(120);
    while !path.exists() {
        assert!(Instant::now() < deadline, "timed out: {}", path.display());
        std::thread::sleep(Duration::from_millis(1));
    }
}

struct IngressRow {
    lane_id: u16,
    sequence: u64,
    hash: [u8; 32],
    packet_decoded_ns: u64,
    ingress_ns: u64,
    l1_pre_ns: u64,
    l1_post_ns: u64,
    l2_ns: u64,
    queue_depth: u64,
}

struct OutboundRow {
    egress_commit_ns: u64,
    evidence: HandoffEvidence,
}

fn scope_decision(record: &BorrowedRecordV2<'_>) -> Hot0Decision {
    match decode_tx_target(record.payload) {
        Ok(target) if C14_FLAP_SCOPE.matches(&target) => Hot0Decision::Admit,
        _ => Hot0Decision::Reject,
    }
}

fn main() {
    let dir = std::env::var_os("DARKBOT_C19_DIR").expect("DARKBOT_C19_DIR");
    let dir = Path::new(&dir);
    let m = manifest(&dir.join("manifest.txt"));
    let generation = number(&m, "generation");
    let warmup = number(&m, "warmup_per_peer");
    let samples = number(&m, "samples_per_peer");
    let workers = number(&m, "outbound_workers") as usize;
    let capacity = number(&m, "peer_lane_capacity") as usize;
    let stride = number(&m, "peer_lane_slot_stride") as usize;
    let lane_ids: Vec<u16> = m["lane_ids"]
        .split(',')
        .map(|id| id.parse().unwrap())
        .collect();
    let peers: Vec<LaneConfigV2> = lane_ids
        .iter()
        .map(|lane_id| LaneConfigV2 {
            lane_id: *lane_id,
            direction: DirectionV2::GethToRust,
            class: LaneClassV2::PeerIngress,
            flags: 0,
            capacity,
            slot_stride: stride,
        })
        .collect();
    let total = (warmup + samples) as usize * peers.len();

    // Prepared signed BUYs: `hash raw` in hex, one per admitted launch.
    let buys: Vec<(Vec<u8>, Vec<u8>)> = fs::read_to_string(dir.join("buys.txt"))
        .unwrap()
        .lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(hash, raw)| (unhex(hash), unhex(raw)))
        .collect();
    assert!(
        workers == 0 || buys.len() >= total,
        "{} prepared BUYs",
        buys.len()
    );

    let path = arena_path_v2(dir, generation);
    let raw = RawMap::open(&path);
    let offsets = raw.lane_offsets();
    let config = Hot0Config {
        intent_slots: 8,
        dedup_sets: 4096,
    };
    let chain = Some(chain_lane());
    let mut poller = Hot0Poller::open(&path, generation, &peers, chain, config).unwrap();
    // With outbound workers the egress and evidence lanes must open.
    let open_egress = || ProducerV2::open(&path, generation, egress_lane()).unwrap();
    let mut egress = (workers > 0).then(open_egress);
    let evidence_lanes: Vec<LaneConfigV2> = (0..=workers).map(evidence_lane).collect();
    let open_evidence = || LaneSetV2::open(&path, generation, &evidence_lanes).unwrap();
    let mut evidence = (workers > 0).then(open_evidence);

    // Everything the measured loop writes is allocated before it starts.
    let mut ingress_rows: Vec<IngressRow> = Vec::with_capacity(total);
    let mut outbound_rows: Vec<OutboundRow> = Vec::with_capacity(total * (workers + 2));
    let mut chain_records = 0u64;
    let mut window_allocations = 0u64;
    let mut window_open = None;
    let warmup_total = warmup as usize * peers.len();

    fs::write(dir.join("rust.ready"), b"ok\n").unwrap();
    let deadline = Instant::now() + Duration::from_secs(600);
    while ingress_rows.len() < total {
        assert!(
            Instant::now() < deadline,
            "timed out at {}",
            ingress_rows.len()
        );
        if ingress_rows.len() == warmup_total && window_open.is_none() {
            window_open = Some(allocations());
        }
        let outcome = poller.poll_once(|_| chain_records += 1, scope_decision);
        let Hot0Outcome::Admitted(handle) = outcome else {
            if outcome == Hot0Outcome::Idle {
                std::hint::spin_loop();
            }
            continue;
        };
        let pool = poller.intents_mut();
        let slot = pool.get_mut(handle).expect("live intent");
        let (lane_id, _epoch, sequence) = slot.launch_provenance();
        let lane_offset = offsets[&lane_id];
        let slot_offset = lane_offset + LANE_HEADER_SIZE + (sequence as usize % capacity) * stride;
        // The probes are written just after the commit; wait for the marker.
        let probe_deadline = Instant::now() + Duration::from_millis(200);
        while raw.u64_at(slot_offset + PROBE_MAGIC_OFFSET) != PROBE_MAGIC {
            assert!(Instant::now() < probe_deadline, "L1 probe missing");
            std::hint::spin_loop();
        }
        let write = raw.u64_at(lane_offset + LH_WRITE_SEQUENCE);
        ingress_rows.push(IngressRow {
            lane_id,
            sequence,
            hash: *slot.launch_hash(),
            packet_decoded_ns: slot.packet_decoded_ns(),
            ingress_ns: slot.ingress_ns(),
            l1_pre_ns: raw.u64_at(slot_offset + PROBE_PRE_COMMIT),
            l1_post_ns: raw.u64_at(slot_offset + PROBE_POST_COMMIT),
            l2_ns: slot.received_ns(),
            queue_depth: write.wrapping_sub(sequence),
        });

        if let (Some(egress), Some(evidence)) = (egress.as_mut(), evidence.as_mut()) {
            let (hash, signed) = &buys[ingress_rows.len() - 1];
            let mut buy_hash = [0u8; 32];
            buy_hash.copy_from_slice(hash);
            let buy = IntentBuy {
                intent_id: ingress_rows.len() as u64,
                generation,
                policy_generation: 1,
                nonce: ingress_rows.len() as u64,
                adapter_permit_mask: 1,
                buy_hash,
                signed_buy: signed,
            };
            slot.seal(&buy).expect("seal");
            // The egress commit boundary: clock, then the one publish.
            let commit_ns = monotonic_ns();
            publish_buy(egress, slot, commit_ns).expect("egress publish");
            // One BUY outstanding: read its evidence before the next sample.
            let mut expected = usize::MAX;
            let mut seen = 0usize;
            let evidence_deadline = Instant::now() + Duration::from_secs(10);
            while seen < expected {
                assert!(
                    Instant::now() < evidence_deadline,
                    "outbound evidence missing"
                );
                for index in 0..=workers {
                    let Some(guard) = evidence.try_acquire(index).expect("evidence lane") else {
                        continue;
                    };
                    let decoded = decode_handoff(&guard.record()).expect("evidence record");
                    guard.release().expect("evidence release");
                    assert_eq!(decoded.hash, buy_hash, "evidence of another BUY");
                    if decoded.status == OutboundStatus::Dispatched {
                        // One summary, one record per worker slot.
                        expected = workers + 1;
                    }
                    outbound_rows.push(OutboundRow {
                        egress_commit_ns: commit_ns,
                        evidence: decoded,
                    });
                    seen += 1;
                }
            }
        }
        assert!(poller.intents_mut().release(handle));
    }
    if let Some(open) = window_open {
        window_allocations = allocations() - open;
    }

    wait_for(&dir.join("go.done"));
    let mut csv = String::from(
        "phase,lane_id,lane_sequence,tx_hash,t_packet_decoded_ns,t_ingress_ns,\
         l1_pre_commit_ns,l1_post_commit_ns,l2_acquire_ns,queue_depth\n",
    );
    let mut per_lane: BTreeMap<u16, u64> = BTreeMap::new();
    for row in &ingress_rows {
        let seen = per_lane.entry(row.lane_id).or_insert(0);
        let phase = if *seen < warmup {
            "warmup"
        } else {
            "qualification"
        };
        *seen += 1;
        let _ = writeln!(
            csv,
            "{phase},{},{},{},{},{},{},{},{},{}",
            row.lane_id,
            row.sequence,
            hex(&row.hash),
            row.packet_decoded_ns,
            row.ingress_ns,
            row.l1_pre_ns,
            row.l1_post_ns,
            row.l2_ns,
            row.queue_depth
        );
    }
    fs::write(dir.join("raw_ingress.csv"), csv).unwrap();

    let mut csv = String::from(
        "tx_hash,egress_sequence,egress_commit_ns,origin,worker,status,selected,queued,\
         coordinator_observed_ns,third_ns,event_ns,peer,evidence_lane,evidence_sequence\n",
    );
    for row in &outbound_rows {
        let e = &row.evidence;
        let worker = e.worker.map_or(-1, i32::from);
        let _ = writeln!(
            csv,
            "{},{},{},{:?},{worker},{},{},{},{},{},{},{},{},{}",
            hex(&e.hash),
            e.egress_sequence,
            row.egress_commit_ns,
            e.origin,
            e.status.name(),
            e.selected,
            e.queued,
            e.observed_ns,
            e.third_ns,
            e.event_ns,
            hex(&e.peer),
            e.lane_id,
            e.lane_sequence
        );
    }
    fs::write(dir.join("raw_outbound.csv"), csv).unwrap();

    let stats = poller.stats();
    let mut summary = String::new();
    let _ = writeln!(summary, "ingress_samples={}", ingress_rows.len());
    let _ = writeln!(summary, "outbound_evidence_rows={}", outbound_rows.len());
    let _ = writeln!(summary, "chain_records_serviced={chain_records}");
    let _ = writeln!(
        summary,
        "hot0_allocations_in_qualification_window={window_allocations}"
    );
    let _ = writeln!(summary, "hot0_polls={}", stats.polls);
    let _ = writeln!(summary, "hot0_idle_polls={}", stats.idle_polls);
    let _ = writeln!(summary, "hot0_peer_records={}", stats.peer_records);
    let _ = writeln!(summary, "hot0_admitted={}", stats.admitted);
    let _ = writeln!(summary, "hot0_rejected={}", stats.rejected);
    let _ = writeln!(summary, "hot0_duplicates={}", stats.duplicates);
    let _ = writeln!(summary, "hot0_invalid_records={}", stats.invalid_records);
    let _ = writeln!(summary, "hot0_lane_faults={}", stats.lane_faults);
    let _ = writeln!(summary, "hot0_chain_gaps={}", stats.chain_gaps);
    let dropped =
        stats.dropped_pool_exhausted + stats.dropped_launch_too_large + stats.dropped_unstable_slot;
    let _ = writeln!(summary, "hot0_dropped={dropped}");
    let _ = writeln!(summary, "hot0_dedup_evictions={}", poller.dedup_evictions());
    let _ = writeln!(summary, "clock=CLOCK_MONOTONIC");
    fs::write(dir.join("rust_summary.txt"), summary).unwrap();
    fs::write(dir.join("rust.done"), b"ok\n").unwrap();
    println!("C19_RUST_CONSUMER_OK samples={}", ingress_rows.len());
}
