//! B7 profile: health and protocol sizing of one liquidatable account with `n` asset and `n`
//! liability Fixed-oracle banks (n×n pairs) — time p50/p95/p99 and heap allocations (count, bytes)
//! per call. Own test binary (counting allocator). Run explicitly:
//! `cargo test --release -p darkbot-project0 --all-features --test b7_profile -- --ignored --nocapture`.
//! Prints measurements; asserts only that every call succeeds.
#![cfg(feature = "crate_health")]

use darkbot_project0::health::crate_impl::CrateHealth;
use darkbot_project0::health::{AccountSource, HealthEngine};
use darkbot_project0::protocol;
use darkbot_project0::sizing::{ProtocolSizer, DEFAULT_HEADROOM_BPS};
use marginfi_type_crate::types::{
    Bank, BankOperationalState, FeeState, MarginfiAccount, MarginfiGroup, OracleSetup,
    WrappedI80F48,
};
use solana_sdk::pubkey::Pubkey;
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

static ALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

struct Counting;
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(layout.size() as u64, Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static COUNTING: Counting = Counting;

struct Map(HashMap<Pubkey, (Pubkey, Vec<u8>)>);
impl AccountSource for Map {
    fn account(&self, k: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
        self.0.get(k).map(|(o, d)| (*o, 1, d.as_slice()))
    }
}

/// `v / 2^shift` as I80F48 (48 fractional bits).
fn fixed(v: i128, shift: u32) -> WrappedI80F48 {
    WrappedI80F48 {
        value: ((v << 48) >> shift).to_le_bytes(),
    }
}

/// `n` assets (1000 each, weight 0.5) and `n` liabilities (600 each, weight 1), all 1 USD.
fn account(n: usize) -> (Map, Pubkey, Pubkey, Pubkey) {
    let program: Pubkey = protocol::PROGRAM_ID_MAINNET.parse().unwrap();
    let (group, account) = (Pubkey::new_unique(), Pubkey::new_unique());
    let wrap = |disc: [u8; 8], bytes: &[u8]| [disc.as_slice(), bytes].concat();
    let mut m = HashMap::new();
    let mut banks: Vec<(Pubkey, bool)> =
        (0..2 * n).map(|i| (Pubkey::new_unique(), i < n)).collect();
    // Pinned Marginfi keeps the balances sorted by bank key, descending.
    banks.sort_by(|a, b| b.0.cmp(&a.0));
    let mut a: MarginfiAccount = bytemuck::Zeroable::zeroed();
    a.group = group;
    for (i, (key, is_asset)) in banks.iter().enumerate() {
        let mut b: Bank = bytemuck::Zeroable::zeroed();
        b.group = group;
        b.mint = Pubkey::new_unique();
        b.mint_decimals = 6;
        b.asset_share_value = fixed(1, 0);
        b.liability_share_value = fixed(1, 0);
        let weight = if *is_asset { fixed(1, 1) } else { fixed(1, 0) };
        b.config.asset_weight_init = weight;
        b.config.asset_weight_maint = weight;
        b.config.liability_weight_init = fixed(1, 0);
        b.config.liability_weight_maint = fixed(1, 0);
        b.config.operational_state = BankOperationalState::Operational;
        b.config.oracle_setup = OracleSetup::Fixed;
        b.config.fixed_price = fixed(1, 0);
        m.insert(
            *key,
            (program, wrap(protocol::DISC_BANK, bytemuck::bytes_of(&b))),
        );
        let bal = &mut a.lending_account.balances[i];
        bal.active = 1;
        bal.bank_pk = *key;
        if *is_asset {
            bal.asset_shares = fixed(1_000_000_000, 0);
        } else {
            bal.liability_shares = fixed(600_000_000, 0);
        }
    }
    let mut fee: FeeState = bytemuck::Zeroable::zeroed();
    fee.liquidation_max_fee = fixed(1, 3); // 12.5 %
    m.insert(
        protocol::fee_state_pda(&program),
        (
            program,
            wrap(protocol::DISC_FEE_STATE, bytemuck::bytes_of(&fee)),
        ),
    );
    m.insert(
        group,
        (
            program,
            wrap(
                protocol::DISC_MARGINFI_GROUP,
                bytemuck::bytes_of(&<MarginfiGroup as bytemuck::Zeroable>::zeroed()),
            ),
        ),
    );
    m.insert(
        account,
        (
            program,
            wrap(protocol::DISC_MARGINFI_ACCOUNT, bytemuck::bytes_of(&a)),
        ),
    );
    (Map(m), program, group, account)
}

fn measure(label: &str, run: &dyn Fn()) {
    const N: usize = 2_000;
    for _ in 0..100 {
        run();
    }
    let (a0, b0) = (ALLOCS.load(Relaxed), BYTES.load(Relaxed));
    let mut ns: Vec<u128> = (0..N)
        .map(|_| {
            let t = std::time::Instant::now();
            run();
            t.elapsed().as_nanos()
        })
        .collect();
    let (allocs, bytes) = (ALLOCS.load(Relaxed) - a0, BYTES.load(Relaxed) - b0);
    ns.sort_unstable();
    println!(
        "B7_PROFILE {label} n={N} p50_ns={} p95_ns={} p99_ns={} allocs_per_call={} bytes_per_call={}",
        ns[N / 2],
        ns[N * 95 / 100],
        ns[N * 99 / 100],
        allocs / N as u64,
        bytes / N as u64
    );
}

#[test]
#[ignore]
fn b7_health_and_sizing_profile() {
    for n in [1usize, 4, 8] {
        let (state, program, group, acct) = account(n);
        let health = CrateHealth {
            state: &state,
            group,
        };
        let sizer = ProtocolSizer {
            state: &state,
            program,
            group,
            headroom_bps: DEFAULT_HEADROOM_BPS,
        };
        let h = health.health(&acct, 1_000, 10).unwrap();
        assert!(h.liquidatable, "fixture must be liquidatable (n={n})");
        let pairs = sizer.sizes(&acct, 1_000, 10).unwrap().len();
        measure(&format!("health banks={}", 2 * n), &|| {
            std::hint::black_box(health.health(&acct, 1_000, 10).unwrap());
        });
        measure(&format!("sizing banks={} sizings={pairs}", 2 * n), &|| {
            std::hint::black_box(sizer.sizes(&acct, 1_000, 10).unwrap());
        });
    }
}
