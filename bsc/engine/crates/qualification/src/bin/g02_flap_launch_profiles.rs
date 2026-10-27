//! G0.2 (Gate-0 test plan §G0.2): the launch profiles of foreign Flap
//! launches as the read-only runtime journaled them. Every first sighting
//! (`RawSignedTx`) addressed to the Portal with `newTokenV6` is decoded by
//! the production decoder; each launch prints its economic fields and the
//! first field the production pending path (`classify_launch`) refuses,
//! then the profiles are counted. The identity fields (name, symbol, URI,
//! beneficiary) are counted apart from the economic ones. Read-only: no
//! RPC, no signature, nothing is sent.
//!
//! ```text
//! g02_flap_launch_profiles <journal dir> [<journal dir> ...]
//! ```
//!
//! A journal dir is a run directory (`<started_ms>/` with segments) or a
//! directory of runs. Only launches some source delivered in full are
//! here; launches no source saw are the census's `missed`.

use alloy_primitives::{Address, B256};
use darkbot_bsc_geth_ipc::ingress::decode_tx_target;
use darkbot_bsc_journal::RecordKind;
use darkbot_bsc_journal::storage::run::RunReader;
use darkbot_bsc_trading::venues::flap::abi::{
    LaunchParamsV6, NEW_TOKEN_V6_SELECTOR, decode_new_token_v6,
};
use darkbot_bsc_trading::venues::flap::deployment::{
    ObservedDeployment, ValidatedDeployment, bsc_mainnet_v5_27_0,
};
use darkbot_bsc_trading::venues::flap::profile::classify_launch;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The journal's payload bound (the runtime's largest message plus framing).
const MAX_PAYLOAD: usize = 16 << 20;
/// The decoder's calldata bound (the runtime's `BSC_RUNTIME_MAX_LAUNCH_CALLDATA` range top).
const MAX_CALLDATA: usize = 1 << 20;

/// The economic fields of one launch, in the order the pending path checks them.
fn economic(l: &LaunchParamsV6<'_>) -> String {
    format!(
        "version={} dex_thresh={} migrator={} quote={} quote_amt={} permit={} extension={} dex={} lp_fee={} buy_tax={} sell_tax={} tax_duration={} anti_farmer={} mkt={} deflation={} dividend={} lp={} min_share={} dividend_token={} commission={}",
        l.token_version,
        l.dex_thresh,
        l.migrator_type,
        if l.quote_token == Address::ZERO {
            "native".to_string()
        } else {
            format!("{}", l.quote_token)
        },
        if l.quote_amt.is_zero() { "0" } else { ">0" },
        if l.permit.is_empty() { "none" } else { "set" },
        if l.extension_id == B256::ZERO && l.extension_data.is_empty() {
            "none"
        } else {
            "set"
        },
        l.dex_id,
        l.lp_fee_profile,
        l.buy_tax,
        l.sell_tax,
        l.tax_duration,
        l.anti_farmer_duration,
        l.mkt_bps,
        l.deflation_bps,
        l.dividend_bps,
        l.lp_bps,
        if l.minimum_share_balance.is_zero() {
            "0"
        } else {
            ">0"
        },
        if l.dividend_token == Address::ZERO {
            "none"
        } else {
            "set"
        },
        if l.commission_receiver == Address::ZERO {
            "none"
        } else {
            "set"
        },
    )
}

fn runs(dir: &Path) -> Vec<PathBuf> {
    if dir.join("segment_000000.dbjnl").exists()
        || std::fs::read_dir(dir).is_ok_and(|mut d| {
            d.any(|e| e.is_ok_and(|e| e.file_name().to_string_lossy().starts_with("segment_")))
        })
    {
        return vec![dir.to_path_buf()];
    }
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

fn main() -> ExitCode {
    let dirs: Vec<String> = std::env::args().skip(1).collect();
    if dirs.is_empty() {
        eprintln!("usage: g02_flap_launch_profiles <journal dir> [<journal dir> ...]");
        return ExitCode::from(2);
    }
    let frozen = bsc_mainnet_v5_27_0();
    let portal = frozen.portal;
    // The frozen deployment as the runtime validates it (its own observed
    // identity): the classification of a launch, not of the live Portal.
    let deployment = frozen
        .validate(
            &ObservedDeployment {
                chain_id: frozen.chain_id,
                portal_version: frozen.portal_version,
                portal_runtime_sha256: frozen.portal_runtime_sha256,
                portal_implementation: frozen.portal_implementation,
                portal_implementation_runtime_sha256: frozen.portal_implementation_runtime_sha256,
                tax_v3_implementation_runtime_sha256: frozen.tax_v3_implementation_runtime_sha256,
            },
            1,
        )
        .expect("the frozen deployment validates against itself");
    let mut seen = HashSet::new();
    let (mut launches, mut undecodable, mut identity_only) = (0u64, 0u64, 0u64);
    let mut profiles: BTreeMap<String, u64> = BTreeMap::new();
    let mut refusals: BTreeMap<String, u64> = BTreeMap::new();
    for dir in &dirs {
        for run in runs(Path::new(dir)) {
            let mut reader = match RunReader::open(&run, MAX_PAYLOAD) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{}: {e}", run.display());
                    continue;
                }
            };
            loop {
                let r = match reader.next_record() {
                    Ok(Some(r)) => r,
                    Ok(None) => break,
                    // A segment still written ends in a partial frame.
                    Err(e) => {
                        eprintln!("{}: stops at {e}", run.display());
                        break;
                    }
                };
                if r.kind != RecordKind::RawSignedTx || r.payload.len() < 34 {
                    continue;
                }
                let (hash, raw) = (&r.payload[1..33], &r.payload[33..]);
                let Ok(target) = decode_tx_target(raw) else {
                    continue;
                };
                if target.to.map(Address::from) != Some(portal)
                    || !target.data.starts_with(&NEW_TOKEN_V6_SELECTOR)
                    || !seen.insert(hash.to_vec())
                {
                    continue;
                }
                launches += 1;
                let l = match decode_new_token_v6(target.data, MAX_CALLDATA) {
                    Ok(l) => l,
                    Err(e) => {
                        undecodable += 1;
                        println!("0x{} UNDECODABLE {e:?}", hex::encode(hash));
                        continue;
                    }
                };
                let profile = economic(&l);
                let refusal = refusal(&deployment, hash, &l);
                if refusal == "Beneficiary" || refusal == "Metadata" {
                    identity_only += 1;
                }
                println!("0x{} first refusal {refusal}: {profile}", hex::encode(hash));
                *profiles.entry(profile).or_default() += 1;
                *refusals.entry(refusal).or_default() += 1;
            }
        }
    }
    println!(
        "\nnewTokenV6 launches sighted {launches}, undecodable {undecodable}; refused only for an identity field (beneficiary, name/symbol/URI) {identity_only}"
    );
    println!("\nfirst refusal of the pending path (classify_launch order):");
    for (k, n) in &refusals {
        println!("  {n:>6} {k}");
    }
    println!("\neconomic profiles (identity fields excluded):");
    let mut by_count: Vec<_> = profiles.into_iter().collect();
    by_count.sort_by(|a, b| b.1.cmp(&a.1));
    for (p, n) in by_count {
        println!("  {n:>6} {p}");
    }
    ExitCode::SUCCESS
}

/// The first field the production pending path (`classify_launch`, at the
/// frozen v5.27.0 deployment with its witnessed economics) refuses.
fn refusal(deployment: &ValidatedDeployment, hash: &[u8], l: &LaunchParamsV6<'_>) -> String {
    match classify_launch(deployment, B256::from_slice(hash), l) {
        Ok(_) => "NONE".into(),
        Err(e) => format!("{e:?}"),
    }
}
