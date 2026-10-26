//! Phase G (Economics gate §16): the latency of the local deterministic
//! quote on this machine, over the real curve states of the Portal parity
//! set (`fixtures/flap/quote_parity_g02_v3.txt`), bound to the economics
//! snapshot of the tree. Offline; run from the repository root, release
//! build (a debug build measures nothing and writes no evidence).
//!
//! ```text
//! g_quote_latency [--samples 100000] [--warmup 1000] [--out-root bsc/native/evidence/g]
//! ```
//!
//! Not a T_PENDING_SEEN -> T_SOCKET_HANDOFF claim: the quote alone.
#![forbid(unsafe_code)]

use alloy_primitives::U256;
use darkbot_bsc_qualification::c1_common::{Args, git_head};
use darkbot_bsc_qualification::economics_snapshot::current;
use darkbot_bsc_record::schema::bsc_capture::ingest_latency::stage as st;
use darkbot_bsc_record::schema::bsc_economics::quote_latency as ql;
use darkbot_bsc_record::{Draft, Value};
use darkbot_bsc_trading::economics::flap::native_curve20::{
    FRESH_LAUNCH_HOLDING_CAP, HoldingCap, NativeBuyInputs, NativeSellInputs, PricedToken,
    QUALIFIED_MAINNET_INPUT_WEI, WITNESSED_MAINNET_TERMS, profile_terms, quote_native_buy,
    quote_native_sell,
};
use darkbot_bsc_trading::economics::slippage::min_output;
use std::hint::black_box;
use std::path::Path;
use std::process::ExitCode;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[path = "g01_common/util.rs"]
#[allow(dead_code)]
mod util;

/// Every tunable bound and its default (each overridable by its flag).
mod defaults {
    pub const SAMPLES: usize = 100_000;
    pub const WARMUP: usize = 1_000;
    pub const OUT_ROOT: &str = "bsc/native/evidence/g";
    /// The policy slippage of the admission's minimum output, in bps.
    pub const SLIPPAGE_BPS: u16 = 100;
}

const PARITY: &str = "bsc/engine/crates/trading/fixtures/flap/quote_parity_g02_v3.txt";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("g_quote_latency: {e}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse(std::env::args().skip(1));
    args.only(&["samples", "warmup", "out-root", "slippage-bps"])?;
    if cfg!(debug_assertions) {
        return Err("a debug build measures nothing: build with --release".into());
    }
    let samples: usize = util::number(&args, "samples", defaults::SAMPLES)?;
    let warmup: usize = util::number(&args, "warmup", defaults::WARMUP)?;
    let slippage: u16 = util::number(&args, "slippage-bps", defaults::SLIPPAGE_BPS)?;
    if samples == 0 {
        return Err("--samples must be positive".into());
    }
    let root = Path::new(".");
    let text = std::fs::read_to_string(root.join(PARITY)).map_err(|e| format!("{PARITY}: {e}"))?;
    // The parity set's states in the production scope (its SUPPORTED
    // rows): the profile's terms with each sample's taxes and threshold.
    let token = |buy: u16, sell: u16, threshold: U256| {
        let mut terms = profile_terms(buy, sell);
        terms.dex_supply_threshold = threshold;
        PricedToken::new(&terms).map_err(|e| format!("{e:?}"))
    };
    let (mut buys, mut sells) = (Vec::new(), Vec::new());
    for line in text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
    {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() != 19 || f[18] != "SUPPORTED" {
            continue;
        }
        let n = |i: usize| U256::from_str_radix(f[i], 10).map_err(|e| format!("{line}: {e}"));
        let tax = |i: usize| f[i].parse::<u16>().map_err(|e| format!("{line}: {e}"));
        match f[7] {
            "BUY" => buys.push(NativeBuyInputs {
                token: token(tax(5)?, tax(6)?, n(17)?)?,
                reserve_before: n(3)?,
                circulating_before: n(4)?,
                input_amount: n(8)?,
                protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
                tax_on_bonding_curve_enabled: true,
                caller_fee_exempt: false,
                holding_cap: HoldingCap::None,
            }),
            "SELL" => sells.push(NativeSellInputs {
                token: token(tax(5)?, tax(6)?, n(17)?)?,
                reserve_before: n(3)?,
                circulating_before: n(4)?,
                input_amount: n(8)?,
                protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_sell_fee_bps,
                tax_on_bonding_curve_enabled: true,
                caller_fee_exempt: false,
            }),
            _ => {}
        }
    }
    if buys.is_empty() || sells.is_empty() {
        return Err(format!("{PARITY}: no supported BUY or SELL state"));
    }
    let fresh = NativeBuyInputs {
        token: token(100, 100, profile_terms(100, 100).dex_supply_threshold)?,
        reserve_before: U256::ZERO,
        circulating_before: U256::ZERO,
        input_amount: U256::from(QUALIFIED_MAINNET_INPUT_WEI),
        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
        tax_on_bonding_curve_enabled: WITNESSED_MAINNET_TERMS.tax_on_bonding_curve_enabled,
        caller_fee_exempt: WITNESSED_MAINNET_TERMS.caller_fee_exempt,
        holding_cap: FRESH_LAUNCH_HOLDING_CAP,
    };
    let measure = |f: &mut dyn FnMut(usize)| -> Vec<u64> {
        for i in 0..warmup {
            f(i);
        }
        (warmup..warmup + samples)
            .map(|i| {
                let t = Instant::now();
                f(i);
                t.elapsed().as_nanos() as u64
            })
            .collect()
    };
    let clock = measure(&mut |_| {
        black_box(Instant::now());
    });
    let stages = [
        (
            "flap_buy_quote",
            measure(&mut |i| {
                let _ = black_box(quote_native_buy(black_box(&buys[i % buys.len()])));
            }),
        ),
        (
            "flap_sell_quote",
            measure(&mut |i| {
                let _ = black_box(quote_native_sell(black_box(&sells[i % sells.len()])));
            }),
        ),
        (
            "flap_fresh_buy_quote_and_min_output",
            measure(&mut |_| {
                let q = quote_native_buy(black_box(&fresh)).map(|q| q.output_amount);
                let _ = black_box(q.map(|o| min_output(o, slippage)));
            }),
        ),
    ];
    let snapshot = current(root)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    let observed = u64::try_from(now.as_millis()).map_err(|e| e.to_string())?;
    let mut d = Draft::new(&ql::SCHEMA);
    d.set(
        ql::HOST,
        format!(
            "{} {} cpus={}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::thread::available_parallelism().map_or(0, |n| n.get())
        ),
    )
    .set(ql::BUILD, "release")
    .set(ql::PROVENANCE_TOOL, "g_quote_latency")
    .set(ql::PROVENANCE_OBSERVED_UNIX_MS, observed)
    .set_opt(ql::GIT_HEAD, git_head(root))
    .set(ql::WARMUP, warmup as u64)
    .set(ql::CLOCK_OVERHEAD_P50_NS, percentile(&clock, 0.50))
    .set(ql::SNAPSHOT_SHA256, Value::B256(snapshot))
    .push(
        ql::NON_CLAIMS,
        "the local quote alone on this machine: no T_PENDING_SEEN -> T_SOCKET_HANDOFF, no network, no EV",
    );
    println!(
        "Phase G local quote latency, {samples} samples per stage after {warmup} warm-up (clock floor p50 {} ns):",
        percentile(&clock, 0.50)
    );
    for (name, values) in &stages {
        let (s, line) = stage(name, values);
        println!("{line}");
        d.push(ql::STAGES, s.into_nested());
    }
    let bytes = d.encode().map_err(|e| format!("quote latency: {e}"))?;
    let dir = Path::new(args.opt("out-root")?.unwrap_or(defaults::OUT_ROOT));
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let out = dir.join(format!("quote_latency_{}.dbrec", util::utc_stamp(observed)));
    std::fs::write(&out, bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("-> {}", out.display());
    Ok(())
}

/// Nearest rank of `p` in `values` (unsorted).
fn percentile(values: &[u64], p: f64) -> u64 {
    let mut v = values.to_vec();
    v.sort_unstable();
    let n = v.len();
    v[(((n as f64) * p).ceil() as usize).clamp(1, n) - 1]
}

fn stage(name: &str, values: &[u64]) -> (Draft, String) {
    let mut v = values.to_vec();
    v.sort_unstable();
    let n = v.len();
    let rank = |p: f64| v[(((n as f64) * p).ceil() as usize).clamp(1, n) - 1];
    let mean = (v.iter().map(|x| u128::from(*x)).sum::<u128>() / n as u128) as u64;
    let at_least = |ns: u64| v.iter().filter(|x| **x >= ns).count() as u64;
    let mut d = Draft::new(&st::SCHEMA);
    d.set(st::NAME, name)
        .set(st::SAMPLES, n as u64)
        .set(st::MIN_NS, v[0])
        .set(st::MEAN_NS, mean)
        .set(st::P50_NS, rank(0.50))
        .set(st::P95_NS, rank(0.95))
        .set(st::P99_NS, rank(0.99))
        .set(st::MAX_NS, v[n - 1])
        .set(st::GE_500US, at_least(500_000))
        .set(st::GE_1MS, at_least(1_000_000))
        .set(st::GE_3MS, at_least(3_000_000));
    if n >= 1_000 {
        d.set(st::P99_9_NS, rank(0.999));
    }
    if n >= 10_000 {
        d.set(st::P99_99_NS, rank(0.9999));
    }
    let line = format!(
        "{name}: n={n} min={} mean={mean} p50={} p95={} p99={} max={} ns",
        v[0],
        rank(0.50),
        rank(0.95),
        rank(0.99),
        v[n - 1]
    );
    (d, line)
}
