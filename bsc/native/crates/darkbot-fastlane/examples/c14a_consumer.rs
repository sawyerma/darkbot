use darkbot_fastlane::v2::{
    CLOCK_ID_MONOTONIC, ConsumerV2, DirectionV2, LaneClassV2, LaneConfigV2, arena_path_v2,
};
use darkbot_fastlane::{HASH_LEN, RecordKind};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::ptr::NonNull;
use std::slice;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const SUPER_HEADER_SIZE: usize = 256;
const DIRECTORY_ENTRY_SIZE: usize = 128;
const LANE_HEADER_SIZE: usize = 384;
const SH_LANE_COUNT: usize = 24;
const DE_LANE_ID: usize = 0;
const DE_LANE_OFFSET: usize = 16;
const LH_WRITE_SEQUENCE: usize = 0;
const LH_READ_SEQUENCE: usize = 128;
const SLOT_COMMITTED: usize = 0;
const PROBE_PRE_COMMIT: usize = 160;
const PROBE_POST_COMMIT: usize = 168;
const PROBE_MAGIC_OFFSET: usize = 176;
const PROBE_MAGIC: u64 = 0x4331_3441_5052_4f42;

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
                libc::PROT_READ | libc::PROT_WRITE,
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

    fn bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    fn atomic_u64(&self, offset: usize) -> &AtomicU64 {
        assert_eq!(offset % 8, 0);
        unsafe { &*self.ptr.as_ptr().add(offset).cast::<AtomicU64>() }
    }
}

impl Drop for RawMap {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr.as_ptr().cast(), self.len);
        }
    }
}

struct Lane {
    id: u16,
    offset: usize,
    consumer: ConsumerV2,
}

#[derive(Clone, Copy)]
struct Sample {
    hash: [u8; HASH_LEN],
    lane_id: u16,
    sequence: u64,
    decoded_ns: u64,
    ingress_ns: u64,
    l1_pre_commit_ns: u64,
    l1_post_commit_ns: u64,
    l2_acquire_ns: u64,
    queue_depth: u64,
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn lane_offset(map: &RawMap, lane_id: u16) -> usize {
    let bytes = map.bytes();
    let count = usize::try_from(u32_at(bytes, SH_LANE_COUNT)).unwrap();
    for index in 0..count {
        let entry = SUPER_HEADER_SIZE + index * DIRECTORY_ENTRY_SIZE;
        if u16_at(bytes, entry + DE_LANE_ID) == lane_id {
            return usize::try_from(u64_at(bytes, entry + DE_LANE_OFFSET)).unwrap();
        }
    }
    panic!("lane {lane_id} not found");
}

fn monotonic_ns() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    assert_eq!(rc, 0, "CLOCK_MONOTONIC failed");
    assert!(ts.tv_sec >= 0 && ts.tv_nsec >= 0);
    u64::try_from(ts.tv_sec).unwrap() * 1_000_000_000 + u64::try_from(ts.tv_nsec).unwrap()
}

fn manifest(path: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    const LUT: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(LUT[(byte >> 4) as usize] as char);
        out.push(LUT[(byte & 0x0f) as usize] as char);
    }
    out
}

fn main() {
    let dir = std::env::var("DARKBOT_C14A_DIR").expect("DARKBOT_C14A_DIR");
    let dir = Path::new(&dir);
    let values = manifest(&dir.join("manifest.txt"));
    let generation = values["generation"].parse::<u64>().unwrap();
    let peers = values["peers"].parse::<usize>().unwrap();
    let samples_per_peer = values["samples_per_peer"].parse::<usize>().unwrap();
    let capacity = values["peer_lane_capacity"].parse::<usize>().unwrap();
    let slot_stride = values["peer_lane_slot_stride"].parse::<usize>().unwrap();
    assert_eq!(
        values["clock_id"].parse::<u32>().unwrap(),
        CLOCK_ID_MONOTONIC
    );
    let lane_ids: Vec<u16> = values["lane_ids"]
        .split(',')
        .map(|value| value.parse::<u16>().unwrap())
        .collect();
    assert_eq!(lane_ids.len(), peers);

    let arena = arena_path_v2(dir, generation);
    let raw = RawMap::open(&arena);
    let mut lanes: Vec<Lane> = lane_ids
        .iter()
        .copied()
        .map(|id| {
            let config = LaneConfigV2 {
                lane_id: id,
                direction: DirectionV2::GethToRust,
                class: LaneClassV2::PeerIngress,
                flags: 0,
                capacity,
                slot_stride,
            };
            Lane {
                id,
                offset: lane_offset(&raw, id),
                consumer: ConsumerV2::open(&arena, generation, config).unwrap(),
            }
        })
        .collect();

    fs::write(dir.join("rust.ready"), b"ok\n").unwrap();

    let expected = peers * samples_per_peer;
    let mut samples = Vec::with_capacity(expected);
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut cursor = 0usize;
    let lane_count = lanes.len();

    while samples.len() < expected {
        assert!(
            Instant::now() < deadline,
            "timed out at {} / {expected}",
            samples.len()
        );
        let lane = &mut lanes[cursor];
        cursor = (cursor + 1) % lane_count;

        let read = raw
            .atomic_u64(lane.offset + LH_READ_SEQUENCE)
            .load(Ordering::Relaxed);
        let write = raw
            .atomic_u64(lane.offset + LH_WRITE_SEQUENCE)
            .load(Ordering::Acquire);
        if read == write {
            std::hint::spin_loop();
            continue;
        }

        let l2_acquire_ns = monotonic_ns();
        let queue_depth = write.wrapping_sub(read);
        let index = usize::try_from(read % u64::try_from(capacity).unwrap()).unwrap();
        let slot = lane.offset + LANE_HEADER_SIZE + index * slot_stride;
        let committed = raw
            .atomic_u64(slot + SLOT_COMMITTED)
            .load(Ordering::Acquire);
        assert_eq!(committed, read.wrapping_add(1));

        let probe_deadline = Instant::now() + Duration::from_millis(100);
        while raw
            .atomic_u64(slot + PROBE_MAGIC_OFFSET)
            .load(Ordering::Acquire)
            != PROBE_MAGIC
        {
            assert!(
                Instant::now() < probe_deadline,
                "C1.4a post-commit probe missing"
            );
            std::hint::spin_loop();
        }
        let l1_pre_commit_ns = raw
            .atomic_u64(slot + PROBE_PRE_COMMIT)
            .load(Ordering::Relaxed);
        let l1_post_commit_ns = raw
            .atomic_u64(slot + PROBE_POST_COMMIT)
            .load(Ordering::Relaxed);

        let record = lane.consumer.try_consume().unwrap().unwrap();
        assert_eq!(record.sequence, read);
        assert_eq!(record.lane_id, lane.id);
        assert_eq!(record.kind, RecordKind::EarlyTxSeen);
        assert_eq!(record.clock_id, CLOCK_ID_MONOTONIC);
        let decoded_ns = u64::from_le_bytes(record.aux[0..8].try_into().unwrap());
        let ingress_ns = record.timestamp_ns;
        assert!(decoded_ns > 0 && decoded_ns <= ingress_ns);
        assert!(ingress_ns <= l1_pre_commit_ns);
        assert!(l1_pre_commit_ns <= l1_post_commit_ns);
        assert!(l1_pre_commit_ns <= l2_acquire_ns);

        samples.push(Sample {
            hash: record.hash,
            lane_id: record.lane_id,
            sequence: record.sequence,
            decoded_ns,
            ingress_ns,
            l1_pre_commit_ns,
            l1_post_commit_ns,
            l2_acquire_ns,
            queue_depth,
        });
    }

    let mut raw_csv = File::create(dir.join("raw.csv")).unwrap();
    writeln!(
        raw_csv,
        "hash,lane_id,sequence,t_packet_decoded_ns,t_ingress_ns,\
         l1_pre_commit_ns,l1_post_commit_ns,l2_acquire_ns,queue_depth"
    )
    .unwrap();
    for sample in &samples {
        writeln!(
            raw_csv,
            "{},{},{},{},{},{},{},{},{}",
            hex(&sample.hash),
            sample.lane_id,
            sample.sequence,
            sample.decoded_ns,
            sample.ingress_ns,
            sample.l1_pre_commit_ns,
            sample.l1_post_commit_ns,
            sample.l2_acquire_ns,
            sample.queue_depth,
        )
        .unwrap();
    }

    let mut stats_csv = File::create(dir.join("lane_stats.csv")).unwrap();
    writeln!(
        stats_csv,
        "lane_id,write_sequence,read_sequence,dropped_full,oversize,\
         corrupt_records,state,quarantine_reason"
    )
    .unwrap();
    for lane in &lanes {
        let stats = lane.consumer.stats();
        writeln!(
            stats_csv,
            "{},{},{},{},{},{},{},{}",
            lane.id,
            stats.write_sequence,
            stats.read_sequence,
            stats.dropped_full,
            stats.oversize,
            stats.corrupt_records,
            stats.state,
            stats.quarantine_reason,
        )
        .unwrap();
    }

    fs::write(dir.join("rust.done"), b"ok\n").unwrap();
    println!("C14A_RUST_CONSUMER_OK samples={}", samples.len());
}
