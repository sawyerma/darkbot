//! FE-01 one-class offline freeze (PHASENPLAN §13): the pinned identities of one closed
//! observation scope (`fe_qualification::collect_scope`) for exactly one funding class —
//! Program/ProgramData, source commit, group, FeeState, every bank of the targets with its oracle
//! setup, observation keys and dependencies (mint, oracles, integration accounts), the build TX
//! version and the class's own pins — as one typed record (`FE01_FREEZE`). A later observation is
//! compared field by field (`drift`). An unknown layout or oracle, a missing dependency, an
//! unpinned class requirement or any drift blocks. Offline it is never FE-01 PASS: current
//! Mainnet deployment/state parity needs an authorized read (NOT_RUN) and the source build parity
//! stays unproven.
use super::fe_qualification::{sha, typed, CollectedScope, SOURCE_PIN, TRANSACTION_VERSION_PIN};
use crate::analytics::payload::{Fe01Freeze, FrozenBank};
use darkbot_project0::funding::FundingClass;
use darkbot_project0::health::{crate_impl::bank_observation_keys, dependency_scan};
use darkbot_project0::protocol;
use marginfi_type_crate::types::{Bank, MarginfiAccount};
use solana_sdk::pubkey::Pubkey;
use std::collections::BTreeSet;

/// The freeze's own journal in an FE-01 output directory.
pub const FE01_FREEZE_JOURNAL: &str = "fe01_freeze.p0j";

/// The class's pins beyond the observed scope: the token and ATA programs every plan uses and,
/// for ZERO_PREFUND_SAME_TX, the allowlisted swap programs of the pinned instruction table and
/// the group's published protocol lookup tables (its v0 transaction needs them).
fn class_pins(
    class: FundingClass,
    program: &Pubkey,
    group: &Pubkey,
) -> Result<Vec<Pubkey>, String> {
    let mut pins: BTreeSet<Pubkey> = [
        protocol::TOKEN_PROGRAM_ID,
        protocol::TOKEN_2022_PROGRAM_ID,
        protocol::ASSOCIATED_TOKEN_PROGRAM_ID,
    ]
    .iter()
    .map(|s| s.parse().map_err(|_| "FE_FREEZE_PIN_INVALID".to_string()))
    .collect::<Result<_, _>>()?;
    if class == FundingClass::ZeroPrefundSameTx {
        let table = darkbot_project0::receivership::builders::instruction_table(program)?;
        if table.swap_programs.is_empty() {
            return Err("FE_FREEZE_SWAP_PROGRAMS_UNPINNED".into());
        }
        let luts = crate::execution::lookup_tables::published_keys(group);
        if luts.is_empty() {
            return Err("FE_FREEZE_PROTOCOL_LUTS_UNPINNED".into());
        }
        pins.extend(table.swap_programs);
        pins.extend(luts);
    }
    Ok(pins.into_iter().collect())
}

/// Freezes a closed scope for `class` (ZERO_PREFUND_SAME_TX or NO_SWAP_INVENTORY only).
pub fn freeze(
    scope: &CollectedScope,
    class: FundingClass,
    historical: bool,
) -> Result<Fe01Freeze, String> {
    super::fe_replay::first_class(class.as_str())?;
    let mut banks = BTreeSet::new();
    for target in &scope.targets {
        let account: MarginfiAccount = typed(
            &scope.state,
            target,
            &scope.program,
            protocol::DISC_MARGINFI_ACCOUNT,
        )?;
        banks.extend(
            account
                .lending_account
                .balances
                .iter()
                .filter(|b| b.is_active())
                .map(|b| b.bank_pk),
        );
    }
    if banks.is_empty() {
        return Err("FE_FREEZE_NO_ACTIVE_BANKS".into());
    }
    let present = |k: &Pubkey, reason: &str| -> Result<(), String> {
        match scope.state.0.get(k) {
            Some(a) if !a.data.is_empty() => Ok(()),
            _ => Err(format!("{reason}:{k}")),
        }
    };
    let mut frozen = Vec::with_capacity(banks.len());
    for bk in banks {
        let bank: Bank = typed(&scope.state, &bk, &scope.program, protocol::DISC_BANK)?;
        // `bank_observation_keys` drops default keys: the setup's named oracle is checked here,
        // a bank without a price (setup None, or no oracle key) never freezes.
        let named = {
            use marginfi_type_crate::types::OracleSetup as O;
            let keys = &bank.config.oracle_keys;
            match bank.config.oracle_setup {
                O::None => false,
                O::Fixed => true,
                O::FixedKamino | O::FixedDrift | O::FixedJuplend => keys[1] != Pubkey::default(),
                _ => keys[0] != Pubkey::default(),
            }
        };
        if !named {
            return Err(format!("FE_FREEZE_ORACLE_MISSING:{bk}"));
        }
        let oracle_keys =
            bank_observation_keys(&bank).map_err(|e| format!("FE_ORACLE_UNSUPPORTED:{e}"))?;
        for k in &oracle_keys {
            present(k, "FE_FREEZE_ORACLE_MISSING")?;
        }
        let scan = dependency_scan(&scope.state, &scope.program, &[bk]);
        if let Some((key, gap)) = scan.gaps.first() {
            return Err(format!("FE_FREEZE_DEPENDENCY_GAP:{key}:{gap:?}"));
        }
        for k in &scan.keys {
            present(k, "FE_FREEZE_DEPENDENCY_MISSING")?;
        }
        present(&bank.liquidity_vault, "FE_FREEZE_VAULT_MISSING")?;
        frozen.push(FrozenBank {
            bank: bk,
            mint: bank.mint,
            mint_decimals: bank.mint_decimals,
            liquidity_vault: bank.liquidity_vault,
            oracle_setup: bank.config.oracle_setup as u8,
            oracle_keys,
            dependencies: scan.keys,
            config_sha256: sha(bytemuck::bytes_of(&bank.config)),
        });
    }
    Ok(Fe01Freeze {
        funding_class: class.as_str().into(),
        source_commit: SOURCE_PIN.into(),
        program: scope.program,
        program_data: scope.pd,
        program_data_sha256: scope.deployment.1.clone(),
        deployment_slot: scope.deployment.0,
        group: scope.group,
        fee_state: protocol::fee_state_pda(&scope.program),
        transaction_version: TRANSACTION_VERSION_PIN.into(),
        class_pins: class_pins(class, &scope.program, &scope.group)?,
        targets: scope.targets.clone(),
        banks: frozen,
        observed_slot: scope.clock.slot,
        historical,
    })
}

/// Every identity in which `now` differs from `frozen` (stable codes, empty: no drift). The
/// observation slot and its source (historical or current) are not identities.
pub fn drift(frozen: &Fe01Freeze, now: &Fe01Freeze) -> Vec<&'static str> {
    let mut out = Vec::new();
    let mut differs = |c: bool, code| {
        if c {
            out.push(code);
        }
    };
    differs(
        frozen.funding_class != now.funding_class,
        "FE_DRIFT_FUNDING_CLASS",
    );
    differs(
        frozen.source_commit != now.source_commit,
        "FE_DRIFT_SOURCE_COMMIT",
    );
    differs(
        (
            frozen.program,
            frozen.program_data,
            &frozen.program_data_sha256,
            frozen.deployment_slot,
        ) != (
            now.program,
            now.program_data,
            &now.program_data_sha256,
            now.deployment_slot,
        ),
        "FE_DRIFT_PROGRAM_DEPLOYMENT",
    );
    differs(
        (frozen.group, frozen.fee_state) != (now.group, now.fee_state),
        "FE_DRIFT_GROUP",
    );
    differs(
        frozen.transaction_version != now.transaction_version,
        "FE_DRIFT_TRANSACTION_VERSION",
    );
    differs(frozen.class_pins != now.class_pins, "FE_DRIFT_CLASS_PINS");
    differs(frozen.targets != now.targets, "FE_DRIFT_TARGETS");
    let keys = |f: &Fe01Freeze| f.banks.iter().map(|b| b.bank).collect::<Vec<_>>();
    if keys(frozen) != keys(now) {
        differs(true, "FE_DRIFT_BANK_SET");
    } else {
        for (a, b) in frozen.banks.iter().zip(&now.banks) {
            differs(
                (a.mint, a.mint_decimals, a.liquidity_vault)
                    != (b.mint, b.mint_decimals, b.liquidity_vault),
                "FE_DRIFT_BANK_IDENTITY",
            );
            differs(
                (a.oracle_setup, &a.oracle_keys) != (b.oracle_setup, &b.oracle_keys),
                "FE_DRIFT_ORACLE",
            );
            differs(a.dependencies != b.dependencies, "FE_DRIFT_DEPENDENCIES");
            differs(a.config_sha256 != b.config_sha256, "FE_DRIFT_BANK_CONFIG");
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// The offline FE-01 verdict of a freeze: what it is and what it does not prove.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fe01Verdict {
    pub status: &'static str,
    pub gaps: Vec<&'static str>,
}

/// Never PASS offline: drift blocks; otherwise the freeze is prepared and every gap a current,
/// authorized Mainnet qualification still has to close is listed.
pub fn verdict(freeze: &Fe01Freeze, drift: &[&'static str]) -> Fe01Verdict {
    let status = if !drift.is_empty() {
        "BLOCKED_DRIFT"
    } else if freeze.historical {
        "FROZEN_FROM_HISTORICAL_OBSERVATION"
    } else {
        "FROZEN_FROM_READ_ONLY_OBSERVATION"
    };
    let mut gaps = vec![
        "CURRENT_DEPLOYMENT_REPRODUCIBLE_SOURCE_PARITY_NOT_PROVEN",
        "CURRENT_CLUSTER_TRANSACTION_VERSION_SUPPORT_NOT_OBSERVED",
        "PROTOCOL_LOOKUP_TABLE_STATE_NOT_OBSERVED",
        "COMPLETE_CURRENT_TRANSACTION_SIMULATION_NOT_RUN",
    ];
    if freeze.historical {
        gaps.insert(0, "CURRENT_MAINNET_STATE_PARITY_NOT_RUN");
    }
    Fe01Verdict { status, gaps }
}

/// Writes `freeze` as the one typed frame of a new sealed `fe01_freeze.p0j` in `dir`.
pub fn write(dir: &std::path::Path, freeze: &Fe01Freeze) -> Result<(), String> {
    use crate::analytics::journal::{source, Provenance, Writer};
    let mut w = Writer::create(&dir.join(FE01_FREEZE_JOURNAL)).map_err(|_| "FE_FREEZE_JOURNAL")?;
    let provenance = Provenance {
        source: source::FE01,
        source_seq: w.next_seq(),
        slot: Some(freeze.observed_slot),
        ..Default::default()
    };
    w.append_typed(provenance, freeze)
        .and_then(|_| w.seal(true))
        .map(drop)
        .map_err(|_| "FE_FREEZE_JOURNAL".into())
}

/// The freeze of an earlier FE-01 output directory: exactly one typed frame of a sealed,
/// complete journal, else an error (never a guessed identity).
pub fn load(dir: &std::path::Path) -> Result<Fe01Freeze, String> {
    use crate::analytics::journal::{kind, Reader, MANIFEST};
    let mut r = Reader::open(&dir.join(FE01_FREEZE_JOURNAL)).map_err(|_| "FE_FREEZE_UNREADABLE")?;
    let mut found = None;
    while let Some(frame) = r.next_frame().map_err(|_| "FE_FREEZE_UNREADABLE")? {
        match frame.kind {
            kind::FE01_FREEZE if found.is_none() => {
                found = Some(
                    frame
                        .typed::<Fe01Freeze>()
                        .map_err(|_| "FE_FREEZE_UNREADABLE")?,
                )
            }
            MANIFEST if found.is_some() => {}
            _ => return Err("FE_FREEZE_JOURNAL_ORDER".into()),
        }
    }
    if !r.sealed || !r.complete {
        return Err("FE_FREEZE_INCOMPLETE".into());
    }
    found.ok_or_else(|| "FE_FREEZE_MISSING".into())
}

/// FE-01 output step shared by observation and replay: write the freeze (if any), compare it
/// with an earlier one and summarize it for the report. `Ok(false)`: blocked (evidence kept).
pub fn record(
    dir: &std::path::Path,
    freeze: Option<Result<Fe01Freeze, String>>,
    against: Option<&std::path::Path>,
) -> Result<(serde_json::Value, bool), String> {
    use serde_json::json;
    Ok(match freeze {
        None => (
            json!({"status":"NOT_RUN","reason":"OBSERVATION_INCOMPLETE"}),
            false,
        ),
        Some(Err(reason)) => (json!({"status":"BLOCKED","reason":reason}), false),
        Some(Ok(f)) => {
            let drift = match against {
                Some(earlier) => drift(&load(earlier)?, &f),
                None => Vec::new(),
            };
            let v = verdict(&f, &drift);
            write(dir, &f)?;
            let summary = json!({"status":v.status,"funding_class":f.funding_class,"banks":f.banks.len(),"class_pins":f.class_pins.len(),"historical":f.historical,"drift":drift,"gaps":v.gaps,"journal":FE01_FREEZE_JOURNAL,"fe01":"INCOMPLETE"});
            (summary, drift.is_empty())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::fe_qualification::{self as fe, collect_scope, ObservedState};

    fn scope(
        all: &std::collections::BTreeMap<Pubkey, solana_sdk::account::Account>,
    ) -> CollectedScope {
        let (c, _) = fe::tests::fixture();
        collect_scope(
            &c,
            |keys, _| {
                Ok((
                    100,
                    ObservedState(keys.iter().map(|k| (*k, all[k].clone())).collect()),
                ))
            },
            false,
        )
        .unwrap()
    }

    fn bank_key(all: &std::collections::BTreeMap<Pubkey, solana_sdk::account::Account>) -> Pubkey {
        *all.iter()
            .find(|(_, a)| protocol::classify(&a.data) == protocol::AccountType::Bank)
            .unwrap()
            .0
    }

    fn edit_bank(
        all: &mut std::collections::BTreeMap<Pubkey, solana_sdk::account::Account>,
        f: impl FnOnce(&mut Bank),
    ) {
        let k = bank_key(all);
        let data = &mut all.get_mut(&k).unwrap().data;
        let mut bank: Bank = bytemuck::pod_read_unaligned(&data[8..]);
        f(&mut bank);
        data[8..].copy_from_slice(bytemuck::bytes_of(&bank));
    }

    /// FE-01: a closed scope freezes ZERO_PREFUND_SAME_TX with every identity pinned; the same
    /// observation shows no drift, round-trips as a typed journal frame and is never PASS.
    #[test]
    fn zero_prefund_freeze_pins_every_identity_and_is_never_pass() {
        let (_, all) = fe::tests::fixture();
        let s = scope(&all);
        let f = freeze(&s, FundingClass::ZeroPrefundSameTx, true).unwrap();
        assert_eq!(f.funding_class, "ZERO_PREFUND_SAME_TX");
        assert_eq!(f.source_commit, SOURCE_PIN);
        assert_eq!((f.program_data, f.deployment_slot), (s.pd, 80));
        assert_eq!(f.banks.len(), 1);
        let b = &f.banks[0];
        assert!(!b.oracle_keys.is_empty() && b.dependencies.contains(&b.mint));
        let jup: Pubkey = protocol::JUP_SWAP_PROGRAM_ID.parse().unwrap();
        assert!(f.class_pins.contains(&jup), "swap program pinned");
        let luts = crate::execution::lookup_tables::published_keys(&f.group);
        assert!(
            !luts.is_empty() && luts.iter().all(|k| f.class_pins.contains(k)),
            "LUTs pinned"
        );
        assert!(drift(
            &f,
            &freeze(&s, FundingClass::ZeroPrefundSameTx, false).unwrap()
        )
        .is_empty());
        // Typed binary record, no value tree.
        let dir = std::env::temp_dir().join(format!("p0-fe01-freeze-{}", Pubkey::new_unique()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join(FE01_FREEZE_JOURNAL);
        let mut w = crate::analytics::journal::Writer::create(&path).unwrap();
        w.append_typed(Default::default(), &f).unwrap();
        w.seal(true).unwrap();
        let mut r = crate::analytics::journal::Reader::open(&path).unwrap();
        let frame = r.next_frame().unwrap().unwrap();
        assert_eq!(frame.typed::<Fe01Freeze>().unwrap(), f);
        std::fs::remove_dir_all(dir).unwrap();
        let v = verdict(&f, &[]);
        assert_eq!(v.status, "FROZEN_FROM_HISTORICAL_OBSERVATION");
        assert_eq!(v.gaps[0], "CURRENT_MAINNET_STATE_PARITY_NOT_RUN");
        assert!(!v.status.contains("PASS"));
        // NO_SWAP has no swap or LUT pins; other classes are refused.
        let no_swap = freeze(&s, FundingClass::NoSwapInventory, true).unwrap();
        assert!(!no_swap.class_pins.contains(&jup));
        assert!(freeze(&s, FundingClass::JitShortfall, true).is_err());
    }

    /// FE-01: every identity drift between the frozen and a later observation is reported and
    /// blocks; the observation slot alone is no drift.
    #[test]
    fn identity_drift_blocks_with_its_reason() {
        let (_, all) = fe::tests::fixture();
        let frozen = freeze(&scope(&all), FundingClass::ZeroPrefundSameTx, true).unwrap();
        let drifted =
            |f: &dyn Fn(&mut std::collections::BTreeMap<Pubkey, solana_sdk::account::Account>)| {
                let mut all = all.clone();
                f(&mut all);
                drift(
                    &frozen,
                    &freeze(&scope(&all), FundingClass::ZeroPrefundSameTx, true).unwrap(),
                )
            };
        let swapped = Pubkey::new_unique();
        assert_eq!(
            drifted(&|a| {
                let old = a[&bank_key(a)].clone();
                let oracle: Bank = bytemuck::pod_read_unaligned(&old.data[8..]);
                let account = a[&oracle.config.oracle_keys[0]].clone();
                a.insert(swapped, account);
                edit_bank(a, |b| b.config.oracle_keys[0] = swapped);
            }),
            [
                "FE_DRIFT_BANK_CONFIG",
                "FE_DRIFT_DEPENDENCIES",
                "FE_DRIFT_ORACLE"
            ],
            "oracle swapped (config, a new dependency)"
        );
        assert_eq!(
            drifted(&|a| edit_bank(a, |b| b.config.asset_weight_maint.value[0] ^= 1)),
            ["FE_DRIFT_BANK_CONFIG"]
        );
        assert_eq!(
            drifted(&|a| edit_bank(a, |b| b.mint_decimals = 9)),
            ["FE_DRIFT_BANK_IDENTITY"]
        );
        let mut other = frozen.clone();
        other.program_data_sha256 = "00".into();
        other.class_pins.pop();
        other.observed_slot += 7;
        assert_eq!(
            drift(&frozen, &other),
            ["FE_DRIFT_CLASS_PINS", "FE_DRIFT_PROGRAM_DEPLOYMENT"]
        );
        assert_eq!(
            verdict(&frozen, &drift(&frozen, &other)).status,
            "BLOCKED_DRIFT"
        );
    }

    /// FE-01 output step: the freeze is written as its own sealed typed journal and read back;
    /// against an earlier freeze, drift blocks (evidence still written); a blocked or missing
    /// freeze never passes; an earlier directory without a freeze is an error, not a pass.
    #[test]
    fn recorded_freeze_round_trips_and_drift_against_it_blocks() {
        let (_, all) = fe::tests::fixture();
        let f = freeze(&scope(&all), FundingClass::ZeroPrefundSameTx, true).unwrap();
        let dir = |n: &str| {
            let d = std::env::temp_dir().join(format!("p0-fe01-{n}-{}", Pubkey::new_unique()));
            std::fs::create_dir(&d).unwrap();
            d
        };
        let (first, second, third) = (dir("a"), dir("b"), dir("c"));
        let (summary, ok) = record(&first, Some(Ok(f.clone())), None).unwrap();
        assert!(ok);
        assert_eq!(summary["status"], "FROZEN_FROM_HISTORICAL_OBSERVATION");
        assert_eq!(summary["fe01"], "INCOMPLETE");
        assert_eq!(load(&first).unwrap(), f);
        let mut moved = f.clone();
        moved.program_data_sha256 = "upgraded".into();
        let (summary, ok) = record(&second, Some(Ok(moved.clone())), Some(&first)).unwrap();
        assert!(!ok);
        assert_eq!(summary["status"], "BLOCKED_DRIFT");
        assert_eq!(summary["drift"][0], "FE_DRIFT_PROGRAM_DEPLOYMENT");
        assert_eq!(load(&second).unwrap(), moved, "drifted evidence is kept");
        let (summary, ok) =
            record(&third, Some(Err("FE_FREEZE_ORACLE_MISSING:x".into())), None).unwrap();
        assert_eq!((summary["status"].as_str(), ok), (Some("BLOCKED"), false));
        assert!(!record(&third, None, None).unwrap().1);
        assert_eq!(load(&third).unwrap_err(), "FE_FREEZE_UNREADABLE");
        assert!(record(&third, Some(Ok(f)), Some(&third)).is_err());
        for d in [first, second, third] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    /// FE-01: a bank without a price (oracle setup None, or no oracle key) or with an empty
    /// oracle account never freezes (fail closed with its reason).
    #[test]
    fn missing_oracle_never_freezes() {
        let (_, all) = fe::tests::fixture();
        let s = scope(&all);
        let oracle = freeze(&s, FundingClass::ZeroPrefundSameTx, true)
            .unwrap()
            .banks[0]
            .oracle_keys[0];
        let mut empty = scope(&all);
        empty.state.0.get_mut(&oracle).unwrap().data.clear();
        assert!(freeze(&empty, FundingClass::ZeroPrefundSameTx, true)
            .unwrap_err()
            .starts_with("FE_FREEZE_ORACLE_MISSING"));
        let unpriced = |f: &dyn Fn(&mut Bank)| {
            let mut edited = all.clone();
            edit_bank(&mut edited, f);
            let mut s = scope(&all);
            s.state
                .0
                .insert(bank_key(&all), edited[&bank_key(&all)].clone());
            freeze(&s, FundingClass::ZeroPrefundSameTx, true).unwrap_err()
        };
        use marginfi_type_crate::types::OracleSetup;
        assert!(unpriced(&|b| b.config.oracle_setup = OracleSetup::None)
            .starts_with("FE_FREEZE_ORACLE_MISSING"));
        assert!(unpriced(&|b| b.config.oracle_keys[0] = Pubkey::default())
            .starts_with("FE_FREEZE_ORACLE_MISSING"));
    }

    /// Verbatim accounts of the pinned marginfi source `33c67987` (`tests/fixtures`): Mainnet
    /// byte layouts, re-owned by that repository's test program.
    macro_rules! real {
        ($n:literal) => {
            (
                $n,
                include_str!(concat!(
                    "../../tests/fixtures/marginfi_33c67987/",
                    $n,
                    ".json"
                )),
            )
        };
    }
    const REAL: &[(&str, &str)] = &[
        real!("mainnet_group"),
        real!("bonk_bank"),
        real!("cloud_bank"),
        real!("pyusd_bank"),
        real!("mainnet_force_close_bank"),
        real!("mainnet_staked_backfill_bank"),
        real!("corvus"),
        real!("pre_migration_bank1"),
        real!("gappy_user3"),
        real!("pyth_sol_usd"),
        real!("sol_pyth_price_feed"),
    ];

    fn real_account(text: &str) -> (Pubkey, solana_sdk::account::Account) {
        use base64::Engine;
        let j: serde_json::Value = serde_json::from_str(text).unwrap();
        let a = &j["account"];
        let data = base64::engine::general_purpose::STANDARD
            .decode(a["data"][0].as_str().unwrap())
            .unwrap();
        let owner = a["owner"].as_str().unwrap().parse().unwrap();
        let account = solana_sdk::account::Account {
            owner,
            lamports: a["lamports"].as_u64().unwrap(),
            data,
            executable: a["executable"].as_bool().unwrap(),
            rent_epoch: 0,
        };
        (j["pubkey"].as_str().unwrap().parse().unwrap(), account)
    }

    /// A real 0.1.11 bank as `lending_pool_resize_bank_account` (0.1.12) leaves it: grown to
    /// `8 + Bank::LEN`, the new bytes zero-filled.
    fn resized_bank(mut a: solana_sdk::account::Account) -> solana_sdk::account::Account {
        a.data.resize(8 + std::mem::size_of::<Bank>(), 0);
        a
    }

    /// FE-01 against the pinned source's real accounts: each file is SHA-256 pinned; every real
    /// bank keeps its historical 0.1.11 bytes, which the pinned 0.1.12 crate refuses as that
    /// version, and decodes once resized as the program resizes it, naming its oracle; the historical real group
    /// layout is refused; a scope of the real BONK bank and real Pyth bytes at its oracle key
    /// (owner set to the Mainnet program, the only transform) freezes with the bank's own oracle;
    /// with its original foreign owner the same bank is refused (identity drift, fail closed).
    #[test]
    fn real_pinned_fixtures_freeze_and_reject_a_foreign_owner() {
        let pinned = [
            (
                "mainnet_group",
                "5acf0baa86a27198004654899a2614b673081978ad9e2a3717ca7f1867d7199a",
            ),
            (
                "bonk_bank",
                "9d500e667326eaf774abde9a824840b0aac23b1af75c15fec420641fd8104156",
            ),
            (
                "cloud_bank",
                "e3d915ea51ba07a2c79498c923192723ca2a630143e75ea68678188572251618",
            ),
            (
                "pyusd_bank",
                "829aeabd20ea8b108e5ddf19a0e6ce0c89f027040ab10d70f0f3f93b0251f85b",
            ),
            (
                "mainnet_force_close_bank",
                "2de1572b472ca63cf067a6eb872b289a2b778fb1d4a672ad314e39fedd829dcf",
            ),
            (
                "mainnet_staked_backfill_bank",
                "b71c2f4806b54a0ba4b4deb73f9a5dd8311710115b734ba957390bf0ffb6e382",
            ),
            (
                "corvus",
                "b75fd68e0eddf27c58693d050bfee713ff4013a0d2fd02706ffa2fef76cd90d7",
            ),
            (
                "pre_migration_bank1",
                "78ee53c5ca0e41dbd5d37f9ff0d6684e74ea8d893ce780e643cb68628073b456",
            ),
            (
                "gappy_user3",
                "778216f8e6e72d5a1410df5b772d849202cff9ad916a81b71879e8efc63d3b10",
            ),
            (
                "pyth_sol_usd",
                "da96843beb481b726e037d285f0c780e8dadfa64eef59d48abaaef84511b4798",
            ),
            (
                "sol_pyth_price_feed",
                "68015dbec6a58b67313b0659e3392915893e063110388f076b7cedaecd805f3a",
            ),
        ];
        let text = |n: &str| REAL.iter().find(|(name, _)| *name == n).unwrap().1;
        for (name, sha) in pinned {
            assert_eq!(
                fe::sha(text(name).as_bytes()),
                sha,
                "{name} differs from its pin"
            );
        }
        use marginfi_type_crate::types::OracleSetup as O;
        for (name, setup) in [
            ("bonk_bank", O::PythPushOracle),
            ("cloud_bank", O::SwitchboardV2),
            ("pyusd_bank", O::PythPushOracle),
            ("mainnet_force_close_bank", O::StakedWithPythPush),
            ("mainnet_staked_backfill_bank", O::StakedWithPythPush),
            ("corvus", O::SwitchboardPull),
            ("pre_migration_bank1", O::PythPushOracle),
        ] {
            let (_, a) = real_account(text(name));
            assert_eq!(
                protocol::decode_bank(&a.data).err(),
                Some(protocol::BankLayoutError::OtherVersion("0.1.11")),
                "{name}: historical layout"
            );
            let a = resized_bank(a);
            let bank = protocol::decode_bank(&a.data).unwrap();
            assert_eq!(bank.config.oracle_setup, setup, "{name}");
            assert!(!bank_observation_keys(&bank).unwrap().is_empty(), "{name}");
        }
        let (_, user) = real_account(text("gappy_user3"));
        assert_eq!(
            protocol::classify(&user.data),
            protocol::AccountType::MarginfiAccount
        );
        assert!(protocol::decode_marginfi_account(&user.data).is_ok());

        let (c, mut all) = fe::tests::fixture();
        let program: Pubkey = protocol::PROGRAM_ID_MAINNET.parse().unwrap();
        // The real Mainnet group fixture is a historical 1064-byte layout; the pinned exact
        // decoder (8 + 9248 bytes) refuses it even under the Mainnet owner: unknown layout.
        let (group_key, mut group) = real_account(text("mainnet_group"));
        assert_eq!(group_key.to_string(), c.fe_group);
        group.owner = program;
        let historical = ObservedState([(group_key, group)].into_iter().collect());
        assert!(typed::<marginfi_type_crate::types::MarginfiGroup>(
            &historical,
            &group_key,
            &program,
            protocol::DISC_MARGINFI_GROUP
        )
        .unwrap_err()
        .starts_with("FE_OWNER_OR_EXACT_LAYOUT_MISMATCH"));
        let (bonk, foreign) = real_account(text("bonk_bank"));
        let foreign = resized_bank(foreign);
        let real: Bank = bytemuck::pod_read_unaligned(&foreign.data[8..]);
        let synthetic = bank_key(&all);
        all.remove(&synthetic);
        for t in &c.fe_targets {
            let t: Pubkey = t.parse().unwrap();
            let data = &mut all.get_mut(&t).unwrap().data;
            let mut a: MarginfiAccount = bytemuck::pod_read_unaligned(&data[8..]);
            a.lending_account.balances[0].bank_pk = bonk;
            data[8..].copy_from_slice(bytemuck::bytes_of(&a));
        }
        let (_, pyth) = real_account(text("pyth_sol_usd"));
        all.insert(real.config.oracle_keys[0], pyth);
        let token = |n| solana_sdk::account::Account {
            owner: protocol::TOKEN_PROGRAM_ID.parse().unwrap(),
            lamports: 1,
            data: vec![0; n],
            ..Default::default()
        };
        all.insert(real.mint, token(82));
        all.insert(real.liquidity_vault, token(165));
        let read = |all: std::collections::BTreeMap<Pubkey, solana_sdk::account::Account>| {
            collect_scope(
                &c,
                move |keys, _| {
                    let s = keys.iter().map(|k| (*k, all[k].clone())).collect();
                    Ok((100, ObservedState(s)))
                },
                false,
            )
        };
        let mut foreign_owned = all.clone();
        foreign_owned.insert(bonk, foreign.clone());
        assert!(read(foreign_owned)
            .err()
            .unwrap()
            .starts_with("FE_OWNER_OR_EXACT_LAYOUT_MISMATCH"));
        let mut owned = foreign;
        owned.owner = program;
        all.insert(bonk, owned);
        let f = freeze(&read(all).unwrap(), FundingClass::ZeroPrefundSameTx, true).unwrap();
        let b = &f.banks[0];
        assert_eq!(
            (b.bank, b.mint, b.oracle_setup),
            (bonk, real.mint, O::PythPushOracle as u8)
        );
        assert_eq!(b.oracle_keys, [real.config.oracle_keys[0]]);
        assert!(b.dependencies.contains(&real.mint));
    }
}
