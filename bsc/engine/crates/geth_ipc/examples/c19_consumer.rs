//! C1.9 native latency consumer.
//!
//! Run by `bsc/tools/c19_native_latency.sh`. It consumes the
//! integrated C1.5-C1.8 candidate exactly as HOT0 does (`Hot0Poller`:
//! borrowed guard, Chain lane first, fair multi-lane polling, one launch copy
//! into an `IntentSlot`), publishes one prepared signed BUY per admitted
//! launch on the egress lane, and reads geth's outbound evidence.
//!
//! Each BUY goes out exactly as in production: through
//! `EgressProducerV2::publish_buy` against the `EgressWrite` of a consumed
//! intent permit (E6-2). The harness prepares its intents with the
//! test-support constructor `IntentLane::dispatched_for_tests` before the
//! measured loop; that constructor skips the economic binding check, so these
//! BUYs prove transport latency only, never an economic admission.
//!
//! It records raw boundaries only, as fixed-width binary tables
//! (`raw_ingress.dbtab`, `raw_outbound.dbtab`, DBTAB001) and a summary record
//! (`rust_summary.dbrec`, DBREC001) that binds both tables; the
//! `c19_report` tool of `darkbot-bsc-qualification` derives every statistic:
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
//! Any failure is one `C19_RUST_CONSUMER_FAIL` line on stderr and exit 1.

use alloy_primitives::{Address, B256};
use darkbot_bsc_geth_ipc::hot0::{
    Hot0Config, Hot0Decision, Hot0Outcome, Hot0Poller, IntentBuy, monotonic_ns,
};
use darkbot_bsc_geth_ipc::ingress::{C14_FLAP_SCOPE, decode_tx_target};
use darkbot_bsc_geth_ipc::outbound::{
    EgressProducerV2, EvidenceOrigin, HandoffEvidence, OutboundStatus, chain_lane, decode_handoff,
    evidence_lane,
};
use darkbot_bsc_geth_ipc::protocol_v2::{
    BorrowedRecordV2, DirectionV2, LaneClassV2, LaneConfigV2, LaneSetV2, arena_path_v2,
};
use darkbot_bsc_record::schema::c1_qualification::{c19_rust_summary as summary, table};
use darkbot_bsc_record::{Draft, Value, observed_now, sha256};
use darkbot_bsc_trading::intent::{AdapterId, AdapterSet, ArmedFence, Dispatch, IntentLane};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::ExitCode;
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
    fn open(path: &Path) -> Result<Self, String> {
        let what = |e: std::io::Error| format!("arena {}: {e}", path.display());
        let file = File::options()
            .read(true)
            .write(true)
            .open(path)
            .map_err(what)?;
        let len = file.metadata().map_err(what)?.len();
        let len = usize::try_from(len).map_err(|_| "arena larger than memory".to_string())?;
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
        if raw == libc::MAP_FAILED {
            return Err(format!(
                "mmap {}: {}",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }
        let ptr = NonNull::new(raw.cast::<u8>()).ok_or("mmap returned null")?;
        Ok(Self {
            _file: file,
            ptr,
            len,
        })
    }

    /// The aligned word at `offset`; `None` outside the mapping.
    fn u64_at(&self, offset: usize) -> Option<u64> {
        if offset.checked_add(8)? > self.len || !offset.is_multiple_of(8) {
            return None;
        }
        let cell = unsafe { &*self.ptr.as_ptr().add(offset).cast::<AtomicU64>() };
        Some(cell.load(Ordering::Acquire))
    }

    fn bytes(&self, offset: usize, len: usize) -> Result<&[u8], String> {
        match offset.checked_add(len) {
            Some(end) if end <= self.len => {
                Ok(unsafe { std::slice::from_raw_parts(self.ptr.as_ptr().add(offset), len) })
            }
            _ => Err(format!("arena read at {offset}+{len} outside the mapping")),
        }
    }

    /// Lane id -> lane offset, from the arena directory.
    fn lane_offsets(&self) -> Result<BTreeMap<u16, usize>, String> {
        let mut count = [0u8; 4];
        count.copy_from_slice(self.bytes(SH_LANE_COUNT, 4)?);
        let mut out = BTreeMap::new();
        for entry in 0..u32::from_le_bytes(count) as usize {
            let base = SUPER_HEADER_SIZE + entry * DIRECTORY_ENTRY_SIZE;
            let mut id = [0u8; 2];
            let mut offset = [0u8; 8];
            id.copy_from_slice(self.bytes(base + DE_LANE_ID, 2)?);
            offset.copy_from_slice(self.bytes(base + DE_LANE_OFFSET, 8)?);
            let lane_offset = usize::try_from(u64::from_le_bytes(offset))
                .map_err(|_| "lane offset beyond the address space".to_string())?;
            out.insert(u16::from_le_bytes(id), lane_offset);
        }
        Ok(out)
    }
}

impl Drop for RawMap {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr.as_ptr().cast(), self.len);
        }
    }
}

fn manifest(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let pairs = text.lines().filter_map(|line| line.split_once('='));
    Ok(pairs.map(|(k, v)| (k.to_owned(), v.to_owned())).collect())
}

fn text<'m>(map: &'m BTreeMap<String, String>, key: &str) -> Result<&'m str, String> {
    map.get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("manifest has no {key}"))
}

fn number(map: &BTreeMap<String, String>, key: &str) -> Result<u64, String> {
    let value = text(map, key)?;
    value
        .parse()
        .map_err(|_| format!("manifest {key}={value:?} is not a number"))
}

fn size(map: &BTreeMap<String, String>, key: &str) -> Result<usize, String> {
    usize::try_from(number(map, key)?).map_err(|_| format!("manifest {key} is too large"))
}

fn unhex(text: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return Err(format!("{text:?} is not hex"));
    }
    let step = (0..text.len()).step_by(2);
    step.map(|i| {
        u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| format!("{text:?} is not hex"))
    })
    .collect()
}

fn wait_for(path: &Path) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(120);
    while !path.exists() {
        if Instant::now() >= deadline {
            return Err(format!("timed out waiting for {}", path.display()));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(())
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

fn main() -> ExitCode {
    match run() {
        Ok(samples) => {
            println!("C19_RUST_CONSUMER_OK samples={samples}");
            ExitCode::SUCCESS
        }
        Err(problem) => {
            eprintln!("C19_RUST_CONSUMER_FAIL {problem}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<usize, String> {
    let dir = std::env::var_os("DARKBOT_C19_DIR").ok_or("DARKBOT_C19_DIR is not set")?;
    let dir = Path::new(&dir);
    let m = manifest(&dir.join("manifest.txt"))?;
    let generation = number(&m, "generation")?;
    let warmup = number(&m, "warmup_per_peer")?;
    let samples = number(&m, "samples_per_peer")?;
    let workers = size(&m, "outbound_workers")?;
    let capacity = size(&m, "peer_lane_capacity")?;
    let stride = size(&m, "peer_lane_slot_stride")?;
    let lane_ids: Vec<u16> = text(&m, "lane_ids")?
        .split(',')
        .map(|id| id.parse().map_err(|_| format!("lane id {id:?}")))
        .collect::<Result<_, String>>()?;
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
    let per_peer = warmup
        .checked_add(samples)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or("warmup + samples overflows")?;
    let total = per_peer
        .checked_mul(peers.len())
        .ok_or("the sample count overflows")?;

    // Prepared signed BUYs: `hash raw` in hex, one per admitted launch.
    let buys_path = dir.join("buys.txt");
    let buys: Vec<(Vec<u8>, Vec<u8>)> = fs::read_to_string(&buys_path)
        .map_err(|e| format!("{}: {e}", buys_path.display()))?
        .lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(hash, raw)| Ok((unhex(hash)?, unhex(raw)?)))
        .collect::<Result<_, String>>()?;
    if workers > 0 && buys.len() < total {
        return Err(format!("{} prepared BUYs for {total} samples", buys.len()));
    }
    if buys.iter().any(|(hash, _)| hash.len() != 32) {
        return Err("a prepared BUY hash is not 32 bytes".into());
    }

    let path = arena_path_v2(dir, generation);
    let raw = RawMap::open(&path)?;
    let offsets = raw.lane_offsets()?;
    let config = Hot0Config {
        intent_slots: 8,
        dedup_sets: 4096,
    };
    let chain = Some(chain_lane());
    let mut poller = Hot0Poller::open(&path, generation, &peers, chain, config)
        .map_err(|e| format!("HOT0 poller: {e:?}"))?;
    // With outbound workers the egress and evidence lanes must open.
    let mut egress = match workers {
        0 => None,
        _ => Some(
            EgressProducerV2::open(&path, generation).map_err(|e| format!("egress lane: {e:?}"))?,
        ),
    };
    // One dispatched intent per prepared BUY, allocated before the loop.
    let adapter = AdapterId::new(0).ok_or("adapter 0 is refused")?;
    let permits = AdapterSet::new(&[adapter]).ok_or("adapter set is refused")?;
    let mut intents: Vec<(IntentLane, Dispatch)> = buys
        .iter()
        .enumerate()
        .map(|(index, (hash, signed))| {
            let fence = ArmedFence::for_tests(97, Address::repeat_byte(0x11), index as u64);
            let token = Address::with_last_byte((index % 251) as u8 + 1);
            IntentLane::dispatched_for_tests(fence, token, signed, B256::from_slice(hash), permits)
                .map_err(|e| format!("dispatched intent {index}: {e:?}"))
        })
        .collect::<Result<_, String>>()?;
    let evidence_lanes: Vec<LaneConfigV2> = (0..=workers).map(evidence_lane).collect();
    let mut evidence = match workers {
        0 => None,
        _ => Some(
            LaneSetV2::open(&path, generation, &evidence_lanes)
                .map_err(|e| format!("evidence lanes: {e:?}"))?,
        ),
    };

    // Everything the measured loop writes is allocated before it starts.
    let mut ingress_rows: Vec<IngressRow> = Vec::with_capacity(total);
    let mut outbound_rows: Vec<OutboundRow> = Vec::with_capacity(total * (workers + 2));
    let mut chain_records = 0u64;
    let mut window_allocations = 0u64;
    let mut window_open = None;
    let warmup_total = usize::try_from(warmup)
        .ok()
        .and_then(|warmup| warmup.checked_mul(peers.len()))
        .ok_or("the warmup window overflows")?;

    write_new(&dir.join("rust.ready"), b"ok\n")?;
    let deadline = Instant::now() + Duration::from_secs(600);
    while ingress_rows.len() < total {
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out at {} of {total}; HOT0 {:?}",
                ingress_rows.len(),
                poller.stats()
            ));
        }
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
        let slot = pool.get_mut(handle).ok_or("admitted intent is not live")?;
        let (lane_id, _epoch, sequence) = slot.launch_provenance();
        let Some(&lane_offset) = offsets.get(&lane_id) else {
            return Err(format!("lane {lane_id} is not in the arena directory"));
        };
        let slot_offset = lane_offset + LANE_HEADER_SIZE + (sequence as usize % capacity) * stride;
        let probe = |offset: usize| {
            raw.u64_at(slot_offset + offset)
                .ok_or_else(|| format!("L1 probe of lane {lane_id} outside the arena"))
        };
        // The probes are written just after the commit; wait for the marker.
        let probe_deadline = Instant::now() + Duration::from_millis(200);
        while probe(PROBE_MAGIC_OFFSET)? != PROBE_MAGIC {
            if Instant::now() >= probe_deadline {
                return Err(format!(
                    "L1 probe missing on lane {lane_id} sequence {sequence}"
                ));
            }
            std::hint::spin_loop();
        }
        let write = raw
            .u64_at(lane_offset + LH_WRITE_SEQUENCE)
            .ok_or("lane header outside the arena")?;
        ingress_rows.push(IngressRow {
            lane_id,
            sequence,
            hash: *slot.launch_hash(),
            packet_decoded_ns: slot.packet_decoded_ns(),
            ingress_ns: slot.ingress_ns(),
            l1_pre_ns: probe(PROBE_PRE_COMMIT)?,
            l1_post_ns: probe(PROBE_POST_COMMIT)?,
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
            // HOT0 storage only; the slot is no egress authority.
            slot.seal(&buy).map_err(|e| format!("seal: {e:?}"))?;
            let (lane, dispatch) = &mut intents[ingress_rows.len() - 1];
            let write = lane
                .start_write(dispatch.ticket, adapter, dispatch.tx_hash)
                .map_err(|e| format!("egress permit: {e:?}"))?;
            // The egress commit boundary: clock, then the one publish.
            let commit_ns = monotonic_ns();
            egress
                .publish_buy(write, commit_ns)
                .map_err(|e| format!("egress publish: {e:?}"))?;
            // One BUY outstanding: read its evidence before the next sample.
            let mut expected = usize::MAX;
            let mut seen = 0usize;
            let evidence_deadline = Instant::now() + Duration::from_secs(10);
            while seen < expected {
                if Instant::now() >= evidence_deadline {
                    return Err(format!(
                        "outbound evidence missing for BUY {}",
                        buy.intent_id
                    ));
                }
                for index in 0..=workers {
                    let acquired = evidence
                        .try_acquire(index)
                        .map_err(|e| format!("evidence lane {index}: {e:?}"))?;
                    let Some(guard) = acquired else {
                        continue;
                    };
                    let decoded = decode_handoff(&guard.record())
                        .map_err(|e| format!("evidence record: {e:?}"))?;
                    guard
                        .release()
                        .map_err(|e| format!("evidence release: {e:?}"))?;
                    if decoded.hash != buy_hash {
                        return Err(format!("evidence of another BUY at {}", buy.intent_id));
                    }
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
        if !poller.intents_mut().release(handle) {
            return Err("intent release refused".into());
        }
    }
    if let Some(open) = window_open {
        window_allocations = allocations() - open;
    }

    wait_for(&dir.join("go.done"))?;
    let mut rows = Vec::with_capacity(ingress_rows.len() * table::IngressSample::WIDTH);
    let mut per_lane: BTreeMap<u16, u64> = BTreeMap::new();
    for row in &ingress_rows {
        let seen = per_lane.entry(row.lane_id).or_insert(0);
        let qualification = *seen >= warmup;
        *seen += 1;
        table::IngressSample {
            qualification,
            lane_id: row.lane_id,
            lane_sequence: row.sequence,
            tx_hash: row.hash,
            t_packet_decoded_ns: row.packet_decoded_ns,
            t_ingress_ns: row.ingress_ns,
            l1_pre_commit_ns: row.l1_pre_ns,
            l1_post_commit_ns: row.l1_post_ns,
            l2_acquire_ns: row.l2_ns,
            queue_depth: row.queue_depth,
        }
        .write(&mut rows);
    }
    let ingress_table = table::encode(&table::C19_INGRESS, &rows).map_err(|e| e.0)?;
    write_new(&dir.join("raw_ingress.dbtab"), &ingress_table)?;

    let mut rows = Vec::with_capacity(outbound_rows.len() * table::OutboundEvidence::WIDTH);
    for row in &outbound_rows {
        let e = &row.evidence;
        table::OutboundEvidence {
            tx_hash: e.hash,
            peer: e.peer,
            egress_sequence: e.egress_sequence,
            egress_commit_ns: row.egress_commit_ns,
            coordinator_observed_ns: e.observed_ns,
            third_ns: e.third_ns,
            event_ns: e.event_ns,
            evidence_lane_sequence: e.lane_sequence,
            evidence_lane_id: e.lane_id,
            from_worker: e.origin == EvidenceOrigin::Worker,
            status: e.status as u8,
            worker: e.worker,
            selected: e.selected,
            queued: e.queued,
        }
        .write(&mut rows);
    }
    let outbound_table = table::encode(&table::C19_OUTBOUND, &rows).map_err(|e| e.0)?;
    write_new(&dir.join("raw_outbound.dbtab"), &outbound_table)?;

    let stats = poller.stats();
    let dropped =
        stats.dropped_pool_exhausted + stats.dropped_launch_too_large + stats.dropped_unstable_slot;
    let mut record = Draft::new(&summary::SCHEMA);
    record
        .set(summary::INGRESS_SAMPLES, ingress_rows.len() as u64)
        .set(summary::OUTBOUND_EVIDENCE_ROWS, outbound_rows.len() as u64)
        .set(summary::PROVENANCE_TOOL, "geth_ipc example c19_consumer")
        .set(summary::PROVENANCE_OBSERVED_UNIX_MS, observed_now()?)
        .set(summary::CHAIN_RECORDS_SERVICED, chain_records)
        .set(summary::HOT0_ALLOCATIONS_IN_WINDOW, window_allocations)
        .set(summary::HOT0_POLLS, stats.polls)
        .set(summary::HOT0_IDLE_POLLS, stats.idle_polls)
        .set(summary::HOT0_PEER_RECORDS, stats.peer_records)
        .set(summary::HOT0_ADMITTED, stats.admitted)
        .set(summary::HOT0_REJECTED, stats.rejected)
        .set(summary::HOT0_DUPLICATES, stats.duplicates)
        .set(summary::HOT0_INVALID_RECORDS, stats.invalid_records)
        .set(summary::HOT0_LANE_FAULTS, stats.lane_faults)
        .set(summary::HOT0_CHAIN_GAPS, stats.chain_gaps)
        .set(summary::HOT0_DROPPED, dropped)
        .set(summary::HOT0_DEDUP_EVICTIONS, poller.dedup_evictions())
        .set(summary::CLOCK, "CLOCK_MONOTONIC")
        .set(
            summary::RAW_INGRESS_SHA256,
            Value::B256(sha256(&ingress_table)),
        )
        .set(
            summary::RAW_OUTBOUND_SHA256,
            Value::B256(sha256(&outbound_table)),
        );
    let bytes = record.encode().map_err(|e| e.0)?;
    write_new(&dir.join("rust_summary.dbrec"), &bytes)?;
    write_new(&dir.join("rust.done"), b"ok\n")?;
    Ok(ingress_rows.len())
}
