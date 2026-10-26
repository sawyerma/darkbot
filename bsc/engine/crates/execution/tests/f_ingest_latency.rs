//! Phase F contract §8, the local part: the production ingest path's
//! latency per event on real bytes, after a warm-up, in release:
//!
//! - `recognize_other`: the venue decision for a transaction that is no
//!   launch (the common case: one envelope decode, both decoders refuse);
//! - `recognize_flap_launch` / `recognize_four_launch`: the decision for a
//!   Flap launch (C1 scope and the production decode) and an OpenFour one;
//! - `four_pending_token`: the Four decoder's pre-execution token
//!   derivation (CREATE2 of the frozen preset template);
//! - `ingress_first_sighting`: `IngressCore::observe` of a new full launch
//!   (dedup window, venue decision, the first-sighting decision);
//! - `recorder_enqueue`: the runtime journal's enqueue of that sighting
//!   (byte budget, bounded queue; the writer drains on its own thread).
//!
//! Every sample is one event between two clock reads; the clock's own
//! floor is reported. Quantiles by nearest rank, p99.99 from 10 000
//! samples, and the counts at or above 0.5 / 1 / 3 ms (a reproducible
//! local path above 3 ms blocks production). With
//! `BSC_F_LATENCY_EVIDENCE_DIR` set the statistics are written there as one
//! binary record. The geth hook stages need a synced node: not measured.
#![cfg(feature = "runtime")]

use darkbot_bsc_execution::ingress::{IngressCore, Observation, four_launch_token, launch_venue};
use darkbot_bsc_execution::runtime_journal::{
    JournalSetup, RuntimeJournal, first_sighting_payload,
};
use darkbot_bsc_execution::sources::SourceId;
use darkbot_bsc_journal::RecordKind;
use darkbot_bsc_record::Draft;
use darkbot_bsc_record::schema::bsc_capture::ingest_latency as il;
use darkbot_bsc_trading::venues::fourmeme::deployment::bsc_mainnet_g01_129842711;
use std::fs;
use std::hint::black_box;
use std::path::Path;
use std::time::Instant;

const N: usize = 100_000;
const WARMUP: usize = 1_000;
const MAX_CALLDATA: usize = 16_384;
const PORTAL: [u8; 20] = [
    0xe2, 0xce, 0x6a, 0xb8, 0x08, 0x74, 0xfa, 0x9f, 0xa2, 0xaa, 0xe6, 0x5d, 0x27, 0x7d, 0xd6, 0xb8,
    0xe6, 0x5c, 0x9d, 0xe0,
];

fn legacy(to: [u8; 20], data: &[u8]) -> Vec<u8> {
    use darkbot_bsc_tx_encoding::rlp::{encode_bytes, encode_list, encode_u64};
    encode_list(&[
        encode_u64(1),
        encode_u64(1_000_000_000),
        encode_u64(21_000),
        encode_bytes(&to),
        encode_u64(0),
        encode_bytes(data),
        encode_u64(147),
        encode_bytes(&[1u8; 32]),
        encode_bytes(&[2u8; 32]),
    ])
}

fn four_fixture(prefix: &str) -> Vec<u8> {
    let calls = include_str!("../../trading/fixtures/fourmeme/freeze_20261026_calls.txt");
    let row: Vec<&str> = calls
        .lines()
        .find(|l| l.starts_with(prefix))
        .unwrap()
        .split_whitespace()
        .collect();
    alloy_primitives::hex::decode(row[10]).unwrap()
}

/// `WARMUP` unmeasured calls, then `N` timed ones; `f` gets the call index.
fn measure(mut f: impl FnMut(usize)) -> Vec<u64> {
    for i in 0..WARMUP {
        f(i);
    }
    (WARMUP..WARMUP + N)
        .map(|i| {
            let t = Instant::now();
            f(i);
            t.elapsed().as_nanos() as u64
        })
        .collect()
}

fn stage(name: &str, values: &[u64]) -> (Draft, String) {
    let mut v = values.to_vec();
    v.sort_unstable();
    let n = v.len();
    let rank = |p: f64| v[(((n as f64) * p).ceil() as usize).clamp(1, n) - 1];
    let mean = (v.iter().map(|x| u128::from(*x)).sum::<u128>() / n as u128) as u64;
    let at_least = |ns: u64| v.iter().filter(|x| **x >= ns).count() as u64;
    let mut d = Draft::new(&il::stage::SCHEMA);
    d.set(il::stage::NAME, name)
        .set(il::stage::SAMPLES, n as u64)
        .set(il::stage::MIN_NS, v[0])
        .set(il::stage::MEAN_NS, mean)
        .set(il::stage::P50_NS, rank(0.50))
        .set(il::stage::P95_NS, rank(0.95))
        .set(il::stage::P99_NS, rank(0.99))
        .set(il::stage::MAX_NS, v[n - 1])
        .set(il::stage::GE_500US, at_least(500_000))
        .set(il::stage::GE_1MS, at_least(1_000_000))
        .set(il::stage::GE_3MS, at_least(3_000_000));
    if n >= 1_000 {
        d.set(il::stage::P99_9_NS, rank(0.999));
    }
    if n >= 10_000 {
        d.set(il::stage::P99_99_NS, rank(0.9999));
    }
    let line = format!(
        "{name}: n={n} min={} mean={mean} p50={} p95={} p99={} p99.9={} p99.99={} max={} ns; >=0.5ms {} >=1ms {} >=3ms {}",
        v[0],
        rank(0.50),
        rank(0.95),
        rank(0.99),
        rank(0.999),
        rank(0.9999),
        v[n - 1],
        at_least(500_000),
        at_least(1_000_000),
        at_least(3_000_000)
    );
    (d, line)
}

#[test]
#[ignore = "Phase F ingest latency measurement; run in release"]
fn f_ingest_latency() {
    let four = bsc_mainnet_g01_129842711();
    let flap_launch = legacy(
        PORTAL,
        &alloy_primitives::hex::decode(
            include_str!("../../trading/fixtures/flap/launch_v6_native_split_tax_pending.hex")
                .trim(),
        )
        .unwrap(),
    );
    let open_launch = four_fixture("OPENFOUR LAUNCH");
    let other = four_fixture("OPENFOUR BUY");
    // The inputs are what the code says they are.
    assert_eq!(launch_venue(&four, &other, MAX_CALLDATA), None);
    assert!(launch_venue(&four, &flap_launch, MAX_CALLDATA).is_some());
    assert!(four_launch_token(&four, &open_launch, MAX_CALLDATA).is_some());

    let clock = measure(|_| {});
    let mut stages: Vec<(&str, Vec<u64>)> = vec![
        (
            "recognize_other",
            measure(|_| {
                black_box(launch_venue(&four, black_box(&other), MAX_CALLDATA));
            }),
        ),
        (
            "recognize_flap_launch",
            measure(|_| {
                black_box(launch_venue(&four, black_box(&flap_launch), MAX_CALLDATA));
            }),
        ),
        (
            "recognize_four_launch",
            measure(|_| {
                black_box(launch_venue(&four, black_box(&open_launch), MAX_CALLDATA));
            }),
        ),
        (
            "four_pending_token",
            measure(|_| {
                black_box(four_launch_token(&four, black_box(&open_launch), MAX_CALLDATA));
            }),
        ),
    ];
    // A new hash per sighting: every one is a first full launch sighting.
    let mut core = IngressCore::new(WARMUP + N, MAX_CALLDATA);
    let hash = |i: usize| {
        let mut h = [0u8; 32];
        h[..8].copy_from_slice(&(i as u64).to_le_bytes());
        h
    };
    stages.push((
        "ingress_first_sighting",
        measure(|i| {
            black_box(core.observe(
                SourceId::PublicNode,
                Observation::Full {
                    hash: hash(i),
                    raw: &flap_launch,
                },
                i as u64,
                None,
            ));
        }),
    ));
    // The runtime journal with its bounds of the repository configuration.
    let dir = std::env::temp_dir().join(format!("bsc-f-latency-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let payload = first_sighting_payload(SourceId::PublicNode, &[7; 32], &flap_launch);
    let mut journal = RuntimeJournal::open(
        &dir,
        1,
        1,
        JournalSetup {
            budget_bytes: 64 << 20,
            ..JournalSetup::whole(65_536, MAX_CALLDATA + 64)
        },
    )
    .unwrap();
    stages.push((
        "recorder_enqueue",
        measure(|i| {
            black_box(journal.record(RecordKind::RawSignedTx, i as u64, black_box(&payload)));
        }),
    ));
    let lost = journal.lost();
    journal.finish().unwrap();
    fs::remove_dir_all(&dir).unwrap();

    let clock_p50 = {
        let mut c = clock.clone();
        c.sort_unstable();
        c[c.len() / 2]
    };
    println!(
        "Phase F local ingest latency, {N} samples per stage after {WARMUP} warm-up, this machine (clock floor p50 {clock_p50} ns; recorder lost {lost}):"
    );
    let mut d = Draft::new(&il::SCHEMA);
    for (name, values) in &stages {
        let (s, line) = stage(name, values);
        println!("{line}");
        d.push(il::STAGES, s.into_nested());
    }
    if let Some(out) = std::env::var_os("BSC_F_LATENCY_EVIDENCE_DIR") {
        let host = format!(
            "{} {} {}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::process::Command::new("sysctl")
                .args(["-n", "machdep.cpu.brand_string"])
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|| "cpu unknown".into())
        );
        let observed = darkbot_bsc_record::observed_now().unwrap();
        d.set(il::HOST, host.as_str())
            .set(
                il::BUILD,
                if cfg!(debug_assertions) {
                    "debug"
                } else {
                    "release"
                },
            )
            .set(il::PROVENANCE_TOOL, "f_ingest_latency")
            .set(il::PROVENANCE_OBSERVED_UNIX_MS, observed)
            .set(il::WARMUP, WARMUP as u64)
            .set(il::CLOCK_OVERHEAD_P50_NS, clock_p50)
            .set(il::RECORDER_LOST, lost);
        if let Ok(o) = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
        {
            d.set(il::GIT_HEAD, String::from_utf8_lossy(&o.stdout).trim());
        }
        for n in [
            "LOCAL_PART: the geth hook stages (peer decode, native event, ring, Rust receive) need a synced node and are not measured",
            "DEV_HOST: this host, not the dedicated Linux target hardware",
            "SYNTHETIC_ORDER: the same bytes in a loop, not a live mempool mix",
        ] {
            d.push(il::NON_CLAIMS, n);
        }
        let out = Path::new(&out);
        fs::create_dir_all(out).unwrap();
        let path = out.join(format!("f_ingest_latency_{observed}.dbrec"));
        fs::write(&path, d.encode().unwrap()).unwrap();
        println!("evidence: {}", path.display());
    }
}
