//! FE-01 scope expansion over the existing pinned decoders, Health and Sizing.
//! No signer, swap client or submission adapter. Observation is not source parity.
use base64::Engine;
use darkbot_project0::{
    health::{
        crate_impl::{bank_observation_keys, CrateHealth},
        AccountSource, HealthEngine,
    },
    protocol,
    sizing::ProtocolSizer,
};
use darkbot_solana::runtime_config::RuntimeConfig;
use marginfi_type_crate::types::{Bank, FeeState, MarginfiAccount, MarginfiGroup};
use serde_json::{json, Value};
use solana_sdk::{account::Account, clock::Clock, pubkey::Pubkey};
use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

pub const SOURCE_PIN: &str = "33c67987a661f59c6683a65485ebb4f0ef030b77";
pub const LOADER: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
pub fn sha(bytes: &[u8]) -> String {
    solana_sdk::hash::hash(bytes)
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
/// Validate the existing measured FREE_ONLY evidence contract for every usable
/// endpoint. A bundle uses `providers: [proof, ...]`; a single proof stays valid.
/// This checks supplied evidence, not the provider's current quota availability.
pub fn validate_provider_evidence(
    config: &RuntimeConfig,
    primary: &str,
    proof: &Value,
    now: f64,
) -> Result<(), String> {
    let endpoints: BTreeSet<&str> = std::iter::once(primary)
        .chain(
            config
                .rpc_endpoints
                .iter()
                .filter(|_| config.fallback)
                .map(String::as_str),
        )
        .collect();
    for endpoint in endpoints {
        let binding = sha(endpoint.as_bytes());
        let p = if let Some(providers) = proof.get("providers").and_then(Value::as_array) {
            providers
                .iter()
                .find(|p| p["provider_binding"] == binding)
                .ok_or("FE_PROVIDER_EVIDENCE_MISSING")?
        } else {
            proof
        };
        let measured = p["measured_at_unix_s"]
            .as_f64()
            .ok_or("FE_PROVIDER_QUALIFICATION_TIME")?;
        if !now.is_finite()
            || now < 0.0
            || measured < 0.0
            || measured > now
            || now - measured > config.limits.fe_qualification_age_seconds as f64
            || p["policy"] != "FREE_ONLY"
            || p["passed"] != true
            || p["provider_binding"] != binding
            || p["evidence_sha256"]
                .as_str()
                .is_none_or(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
            || p["allowed_rps"]
                .as_f64()
                .is_none_or(|n| n < config.limits.rpc_rps.min(config.limits.bootstrap_rps) as f64)
            || p["available_calls"]
                .as_u64()
                .is_none_or(|n| n < config.limits.max_requests.min(config.limits.fe_calls))
        {
            return Err("FREE_ONLY_PROVIDER_CAPACITY_UNQUALIFIED".into());
        }
    }
    Ok(())
}
#[derive(Default)]
pub struct ObservedState(pub BTreeMap<Pubkey, Account>);
impl AccountSource for ObservedState {
    fn account(&self, key: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
        self.0
            .get(key)
            .map(|a| (a.owner, a.lamports, a.data.as_slice()))
    }
}
fn key(s: &str) -> Result<Pubkey, String> {
    Pubkey::from_str(s).map_err(|_| "FE_PUBKEY_INVALID".into())
}
fn typed<T: bytemuck::Pod>(
    state: &ObservedState,
    k: &Pubkey,
    program: &Pubkey,
    disc: [u8; 8],
) -> Result<T, String> {
    let a = state
        .0
        .get(k)
        .ok_or_else(|| format!("FE_ACCOUNT_MISSING:{k}"))?;
    if a.owner != *program
        || a.executable
        || a.data.get(..8) != Some(disc.as_slice())
        || a.data.len() != 8 + std::mem::size_of::<T>()
    {
        return Err(format!(
            "FE_OWNER_OR_EXACT_LAYOUT_MISMATCH:{k}:{}",
            a.data.len()
        ));
    }
    bytemuck::try_pod_read_unaligned(&a.data[8..]).map_err(|_| "FE_POD_LAYOUT".into())
}
/// Decode RPC encoding only; protocol layouts remain owned by the pinned Rust crates.
pub fn decode_batch(
    value: Value,
    keys: &[Pubkey],
    floor: u64,
) -> Result<(u64, ObservedState), String> {
    let slot = value["context"]["slot"]
        .as_u64()
        .filter(|s| *s > 0 && *s >= floor)
        .ok_or("FE_CONTEXT_REGRESSION")?;
    let values = value["value"]
        .as_array()
        .filter(|v| v.len() == keys.len())
        .ok_or("FE_BATCH_LENGTH")?;
    let mut state = ObservedState::default();
    for (k, v) in keys.iter().zip(values) {
        if v.is_null() {
            return Err(format!("FE_ACCOUNT_MISSING:{k}"));
        }
        if v["data"][1] != "base64" {
            return Err("FE_ACCOUNT_ENCODING".into());
        }
        let data = base64::engine::general_purpose::STANDARD
            .decode(v["data"][0].as_str().ok_or("FE_ACCOUNT_ENCODING")?)
            .map_err(|_| "FE_ACCOUNT_ENCODING")?;
        let a = Account {
            owner: key(v["owner"].as_str().ok_or("FE_OWNER")?)?,
            lamports: v["lamports"].as_u64().ok_or("FE_LAMPORTS")?,
            data,
            executable: v["executable"].as_bool().ok_or("FE_EXECUTABLE")?,
            rent_epoch: 0,
        };
        if state.0.insert(*k, a).is_some() {
            return Err("FE_DUPLICATE_ACCOUNT".into());
        }
    }
    Ok((slot, state))
}
fn program_data(state: &ObservedState, program: &Pubkey) -> Result<Pubkey, String> {
    let a = state.0.get(program).ok_or("FE_PROGRAM_MISSING")?;
    if a.owner != key(LOADER)?
        || !a.executable
        || a.data.len() != 36
        || a.data[..4] != 2u32.to_le_bytes()
    {
        return Err("FE_PROGRAM_LAYOUT".into());
    }
    let pd = Pubkey::new_from_array(a.data[4..36].try_into().unwrap());
    if pd != Pubkey::find_program_address(&[program.as_ref()], &key(LOADER)?).0 {
        return Err("FE_PROGRAMDATA_ADDRESS".into());
    }
    Ok(pd)
}
fn deployment(state: &ObservedState, pd: &Pubkey) -> Result<(u64, String), String> {
    let a = state.0.get(pd).ok_or("FE_PROGRAMDATA_MISSING")?;
    if a.owner != key(LOADER)?
        || a.executable
        || a.data.len() < 45
        || a.data[..4] != 3u32.to_le_bytes()
        || a.data[12] > 1
    {
        return Err("FE_PROGRAMDATA_LAYOUT".into());
    }
    Ok((
        u64::from_le_bytes(a.data[4..12].try_into().unwrap()),
        sha(&a.data),
    ))
}
fn dependencies(
    state: &ObservedState,
    program: &Pubkey,
    group: &Pubkey,
    targets: &[Pubkey],
) -> Result<BTreeSet<Pubkey>, String> {
    let mut banks = BTreeSet::new();
    for target in targets {
        let account: MarginfiAccount =
            typed(state, target, program, protocol::DISC_MARGINFI_ACCOUNT)?;
        if account.group != *group {
            return Err("FE_TARGET_GROUP_MISMATCH".into());
        }
        banks.extend(
            account
                .lending_account
                .balances
                .iter()
                .filter(|b| b.is_active())
                .map(|b| b.bank_pk),
        );
    }
    let mut required = banks.clone();
    for bk in banks {
        if !state.0.contains_key(&bk) {
            continue;
        }
        let bank: Bank = typed(state, &bk, program, protocol::DISC_BANK)?;
        if bank.group != *group {
            return Err("FE_BANK_GROUP_MISMATCH".into());
        }
        bank_observation_keys(&bank).map_err(|e| format!("FE_ORACLE_UNSUPPORTED:{e}"))?;
        required.extend(darkbot_project0::health::dependency_keys(
            state,
            program,
            &[bk],
        ));
        required.insert(bank.liquidity_vault);
    }
    if required.contains(&Pubkey::default()) {
        return Err("FE_ZERO_DEPENDENCY".into());
    }
    Ok(required)
}
/// Each round refreshes the complete discovered scope. New balances/banks expand
/// the next round; an inconsistent/moving snapshot never becomes qualified.
pub fn collect<F>(config: &RuntimeConfig, mut read: F) -> Result<Value, String>
where
    F: FnMut(&[Pubkey], u64) -> Result<(u64, ObservedState), String>,
{
    if config.fe_targets.is_empty()
        || config.limits.fe_batch_size == 0
        || config.limits.fe_batch_size > 100
        || config.limits.fe_snapshot_rounds == 0
    {
        return Err("FE_SCOPE_OR_BATCH_BUDGET_INVALID".into());
    }
    let mut read = |keys: &[Pubkey], floor: u64| -> Result<(u64, ObservedState), String> {
        let (slot, state) = read(keys, floor)?;
        if slot == 0 || slot < floor {
            return Err("FE_CONTEXT_REGRESSION".into());
        }
        if state.0.len() != keys.len() || keys.iter().any(|k| !state.0.contains_key(k)) {
            return Err("FE_BATCH_SCOPE_MISMATCH".into());
        }
        Ok((slot, state))
    };
    let program = key(protocol::PROGRAM_ID_MAINNET)?;
    let group = key(&config.fe_group)?;
    let targets = config
        .fe_targets
        .iter()
        .map(|s| key(s))
        .collect::<Result<Vec<_>, _>>()?;
    if group == Pubkey::default()
        || targets.contains(&Pubkey::default())
        || targets.iter().collect::<BTreeSet<_>>().len() != targets.len()
    {
        return Err("FE_TARGET_SCOPE_INVALID".into());
    }
    let (mut floor, initial) = read(&[program], 0)?;
    let pd = program_data(&initial, &program)?;
    let (s, initial_pd) = read(&[pd], floor)?;
    floor = s;
    let baseline = deployment(&initial_pd, &pd)?;
    if baseline.0 > floor {
        return Err("FE_DEPLOYMENT_FROM_FUTURE".into());
    }
    let clock_key = solana_sdk::sysvar::clock::id();
    let fee = protocol::fee_state_pda(&program);
    let mut keys: BTreeSet<_> = [program, pd, group, clock_key, fee]
        .into_iter()
        .chain(targets.iter().copied())
        .collect();
    let mut last_reason = "FE_SCOPE_NOT_CLOSED";
    for round in 1..=config.limits.fe_snapshot_rounds {
        let mut state = ObservedState::default();
        let mut slots = BTreeSet::new();
        for chunk in keys
            .iter()
            .copied()
            .collect::<Vec<_>>()
            .chunks(config.limits.fe_batch_size as usize)
        {
            let (slot, batch) = read(chunk, floor)?;
            floor = slot;
            slots.insert(slot);
            state.0.extend(batch.0);
        }
        if program_data(&state, &program)? != pd || deployment(&state, &pd)? != baseline {
            return Err("FE_PROGRAM_UPGRADE_DURING_OBSERVATION".into());
        }
        let _: MarginfiGroup = typed(&state, &group, &program, protocol::DISC_MARGINFI_GROUP)?;
        let _: FeeState = typed(&state, &fee, &program, protocol::DISC_FEE_STATE)?;
        let required = dependencies(&state, &program, &group, &targets)?;
        if !required.is_subset(&keys) {
            keys.extend(required);
            last_reason = "FE_SCOPE_NOT_CLOSED";
            continue;
        }
        if slots.len() != 1 {
            last_reason = "FE_CONTEXT_SLOTS_NOT_COHERENT";
            continue;
        }
        let clock_account = state.0.get(&clock_key).ok_or("FE_CLOCK_MISSING")?;
        if clock_account.owner != solana_sdk::sysvar::id()
            || clock_account.executable
            || clock_account.data.len() != 40
        {
            return Err("FE_CLOCK_LAYOUT".into());
        }
        let clock: Clock =
            bincode::deserialize(&clock_account.data).map_err(|_| "FE_CLOCK_LAYOUT")?;
        if clock.slot != floor || clock.unix_timestamp < 0 {
            last_reason = "FE_CLOCK_CONTEXT_MISMATCH";
            continue;
        }
        let health = CrateHealth {
            state: &state,
            group,
        };
        let sizer = ProtocolSizer {
            state: &state,
            program,
            group,
            headroom_bps: darkbot_project0::sizing::DEFAULT_HEADROOM_BPS,
        };
        let mut rows = Vec::new();
        for target in &targets {
            let h = health.health(target, clock.unix_timestamp, clock.slot);
            let eligibility =
                health.receivership_eligibility(target, clock.unix_timestamp, clock.slot);
            let mut row = json!({"account":target.to_string(),"health":"UNKNOWN","liquidatable":null,"receivership_eligibility":eligibility.as_ref().map(|_|"PASS").unwrap_or("FAIL"),"eligibility_reason":eligibility.err().map(|e|e.to_string()),"funding":"UNKNOWN","net_edge_usd_micro":null,"simulation":"NOT_RUN","collateral_exit":"UNKNOWN","source_parity":"UNKNOWN"});
            match h {
                Ok(h) => {
                    row["health"] = json!("EVALUATED_WITH_PINNED_CORE");
                    row["liquidatable"] = json!(h.liquidatable);
                    row["maintenance_health_bits"] = json!(h.maintenance_health.to_string());
                    if h.liquidatable && row["receivership_eligibility"] == "PASS" {
                        match sizer.sizes(target,clock.unix_timestamp,clock.slot){
                            Ok(sizes)=>row["protocol_sizes"]=json!(sizes.into_iter().map(|s|json!({"asset_bank":s.asset_bank.to_string(),"liability_bank":s.liab_bank.to_string(),"withdraw_native":s.withdraw_amount,"repay_native":s.repay_amount,"repay_all":s.repay_all,"flat_sol_fee_lamports":s.flat_sol_fee_lamports,"executable_value":"UNKNOWN"})).collect::<Vec<_>>()),
                            Err(e)=>row["sizing_error"]=json!(e),
                        }
                    }
                }
                Err(e) => row["health_error"] = json!(e.to_string()),
            }
            rows.push(row);
        }
        let accounts:Vec<_>=state.0.iter().map(|(key,a)|json!({"pubkey":key.to_string(),"owner":a.owner.to_string(),"lamports":a.lamports,"bytes":a.data.len(),"sha256":sha(&a.data)})).collect();
        return Ok(
            json!({"schema":"Project0/FE01FullObservation/1","status":"STATE_SCOPE_OBSERVED_NOT_QUALIFIED","scope_closed":true,"rounds":round,"slot":clock.slot,"unix_timestamp":clock.unix_timestamp,"clock":"CHAIN_SYSVAR_SAME_CONTEXT","program_data":pd.to_string(),"program_data_sha256":baseline.1,"deployment_slot":baseline.0,"source_pin":SOURCE_PIN,"source_parity":"UNKNOWN","source_parity_reason":"CURRENT_PROGRAM_BYTES_NOT_A_REPRODUCIBLE_SOURCE_BUILD_PROOF","accounts":accounts,"targets":rows,"fe01":"INCOMPLETE","fe02":"NOT_RUN","pilot_ready":false,"live_enabled":false,"submission_enabled":false}),
        );
    }
    Err(last_reason.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytemuck::Zeroable;
    #[test]
    fn provider_evidence_requires_fresh_free_capacity_for_every_fallback() {
        let mut c = RuntimeConfig::default();
        let primary = "http://primary.invalid";
        let proof = |url: &str| {
            json!({"policy":"FREE_ONLY","passed":true,
            "provider_binding":sha(url.as_bytes()),"measured_at_unix_s":1000,
            "evidence_sha256":sha(b"fixture"),"allowed_rps":5,"available_calls":3})
        };
        let p = proof(primary);
        assert!(validate_provider_evidence(&c, primary, &p, 1001.0).is_ok());
        for (field, value) in [
            ("policy", json!("PAID")),
            ("passed", json!(false)),
            ("allowed_rps", json!(0)),
            ("available_calls", json!(2)),
            ("evidence_sha256", json!("missing")),
            ("provider_binding", json!("wrong")),
            ("measured_at_unix_s", json!(1002)),
        ] {
            let mut invalid = p.clone();
            invalid[field] = value;
            assert!(
                validate_provider_evidence(&c, primary, &invalid, 1001.0).is_err(),
                "{field}"
            );
        }
        assert!(validate_provider_evidence(
            &c,
            primary,
            &p,
            1001.0 + c.limits.fe_qualification_age_seconds as f64
        )
        .is_err());
        c.rpc_endpoints = vec![primary.into(), "http://backup.invalid".into()];
        c.fallback = true;
        assert!(validate_provider_evidence(&c, primary, &p, 1001.0).is_err());
        let proofs = json!({"providers":[p,proof("http://backup.invalid")]});
        assert!(validate_provider_evidence(&c, primary, &proofs, 1001.0).is_ok());
        c.fallback = false;
        assert!(validate_provider_evidence(&c, primary, &p, 1001.0).is_ok());
    }
    fn wrapped<T: bytemuck::Pod>(v: T, disc: [u8; 8], owner: Pubkey) -> Account {
        Account {
            owner,
            lamports: 1,
            data: [disc.as_slice(), bytemuck::bytes_of(&v)].concat(),
            ..Account::default()
        }
    }
    fn fixture() -> (RuntimeConfig, BTreeMap<Pubkey, Account>) {
        let c = RuntimeConfig {
            fe_mode: "full".into(),
            ..RuntimeConfig::default()
        };
        let program = key(protocol::PROGRAM_ID_MAINNET).unwrap();
        let group = key(&c.fe_group).unwrap();
        let loader = key(LOADER).unwrap();
        let pd = Pubkey::find_program_address(&[program.as_ref()], &loader).0;
        let bank_key = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let vault = Pubkey::new_unique();
        let oracle = Pubkey::new_unique();
        let mut state = BTreeMap::new();
        state.insert(
            program,
            Account {
                owner: loader,
                executable: true,
                data: [2u32.to_le_bytes().as_slice(), pd.as_ref()].concat(),
                ..Account::default()
            },
        );
        let mut data = vec![0; 45];
        data[..4].copy_from_slice(&3u32.to_le_bytes());
        data[4..12].copy_from_slice(&80u64.to_le_bytes());
        state.insert(
            pd,
            Account {
                owner: loader,
                data,
                ..Account::default()
            },
        );
        state.insert(
            group,
            wrapped(
                MarginfiGroup::zeroed(),
                protocol::DISC_MARGINFI_GROUP,
                program,
            ),
        );
        state.insert(
            protocol::fee_state_pda(&program),
            wrapped(FeeState::zeroed(), protocol::DISC_FEE_STATE, program),
        );
        for target in &c.fe_targets {
            let mut a = MarginfiAccount::zeroed();
            a.group = group;
            a.lending_account.balances[0].active = 1;
            a.lending_account.balances[0].bank_pk = bank_key;
            state.insert(
                key(target).unwrap(),
                wrapped(a, protocol::DISC_MARGINFI_ACCOUNT, program),
            );
        }
        let mut bank = Bank::zeroed();
        bank.group = group;
        bank.mint = mint;
        bank.liquidity_vault = vault;
        bank.config.oracle_setup = marginfi_type_crate::types::OracleSetup::PythPushOracle;
        bank.config.oracle_keys[0] = oracle;
        state.insert(bank_key, wrapped(bank, protocol::DISC_BANK, program));
        for k in [mint, vault, oracle] {
            state.insert(
                k,
                Account {
                    owner: key(protocol::TOKEN_PROGRAM_ID).unwrap(),
                    lamports: 1,
                    data: vec![0; 82],
                    ..Account::default()
                },
            );
        }
        state.insert(
            solana_sdk::sysvar::clock::id(),
            Account {
                owner: solana_sdk::sysvar::id(),
                data: bincode::serialize(&Clock {
                    slot: 100,
                    unix_timestamp: 1000,
                    ..Clock::default()
                })
                .unwrap(),
                ..Account::default()
            },
        );
        (c, state)
    }
    fn batch(all: &BTreeMap<Pubkey, Account>, keys: &[Pubkey]) -> Result<ObservedState, String> {
        Ok(ObservedState(
            keys.iter()
                .map(|k| {
                    all.get(k)
                        .cloned()
                        .map(|v| (*k, v))
                        .ok_or("FE_ACCOUNT_MISSING".to_string())
                })
                .collect::<Result<_, _>>()?,
        ))
    }
    #[test]
    fn scope_expands_beyond_three_calls_using_pinned_dependencies() {
        let (c, all) = fixture();
        let mut calls = 0;
        let mut seen = BTreeSet::new();
        let report = collect(&c, |keys, floor| {
            assert!(floor <= 100);
            calls += 1;
            seen.extend(keys.iter().copied());
            Ok((100, batch(&all, keys)?))
        })
        .unwrap();
        assert!(calls > 3);
        assert_eq!(report["scope_closed"], true);
        assert_eq!(seen.len(), all.len());
        assert_eq!(report["source_parity"], "UNKNOWN");
        assert_eq!(report["fe01"], "INCOMPLETE");
        assert_eq!(report["pilot_ready"], false);
        // Zero-share balances may evaluate to zero health even with unused oracles.
        // That does not establish an executable economic candidate.
        assert_eq!(report["targets"][0]["liquidatable"], false);
        assert!(report["targets"][0]["net_edge_usd_micro"].is_null());
    }
    #[test]
    fn changed_deployment_and_missing_accounts_fail_closed() {
        let (c, all) = fixture();
        let mut calls = 0;
        let result = collect(&c, |keys, _| {
            calls += 1;
            let mut s = batch(&all, keys)?;
            if calls > 2 {
                for a in s.0.values_mut() {
                    if a.owner == key(LOADER).unwrap() && !a.executable {
                        a.data.push(0);
                    }
                }
            }
            Ok((100, s))
        });
        assert_eq!(result.unwrap_err(), "FE_PROGRAM_UPGRADE_DURING_OBSERVATION");
        let mut missing = all;
        missing.remove(&key(&c.fe_targets[0]).unwrap());
        assert!(collect(&c, |keys, _| Ok((100, batch(&missing, keys)?)))
            .unwrap_err()
            .contains("MISSING"));
    }
    #[test]
    fn mixed_contexts_and_round_budget_do_not_invent_coherent_state() {
        let (mut c, all) = fixture();
        c.limits.fe_snapshot_rounds = 1;
        assert_eq!(
            collect(&c, |keys, _| Ok((100, batch(&all, keys)?))).unwrap_err(),
            "FE_SCOPE_NOT_CLOSED"
        );
        c.limits.fe_snapshot_rounds = 5;
        c.limits.fe_batch_size = 2;
        let mut slot = 100;
        assert_eq!(
            collect(&c, |keys, _| {
                slot += 1;
                Ok((slot, batch(&all, keys)?))
            })
            .unwrap_err(),
            "FE_CONTEXT_SLOTS_NOT_COHERENT"
        );
    }
    #[test]
    fn exact_bank_layout_rejects_compatibility_padding() {
        let (c, mut all) = fixture();
        let b = all
            .values_mut()
            .find(|a| a.data.get(..8) == Some(protocol::DISC_BANK.as_slice()))
            .unwrap();
        b.data.resize(3912, 0);
        assert!(collect(&c, |keys, _| Ok((100, batch(&all, keys)?)))
            .unwrap_err()
            .contains("EXACT_LAYOUT_MISMATCH"));
    }
    #[test]
    fn wire_decoder_preserves_bytes_and_rejects_regressed_or_missing_context() {
        let k = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let raw = vec![1, 2, 3];
        let v = json!({"context":{"slot":12},"value":[{"owner":owner.to_string(),"lamports":45,"executable":false,"data":[base64::engine::general_purpose::STANDARD.encode(&raw),"base64"]}]});
        let (slot, state) = decode_batch(v.clone(), &[k], 11).unwrap();
        assert_eq!(slot, 12);
        assert_eq!(state.0[&k].data, raw);
        assert_eq!(state.0[&k].owner, owner);
        assert!(decode_batch(v, &[k], 13).is_err());
        assert!(decode_batch(json!({"value":[]}), &[], 0).is_err());
    }
    #[test]
    fn invalid_scope_and_callback_context_fail_closed_before_qualification() {
        let (c, all) = fixture();
        let mut bad = c.clone();
        bad.fe_targets.clear();
        assert!(collect(&bad, |_, _| panic!("must not read")).is_err());
        bad = c.clone();
        bad.fe_targets.push(bad.fe_targets[0].clone());
        assert!(collect(&bad, |_, _| panic!("must not read")).is_err());
        bad = c.clone();
        bad.limits.fe_batch_size = 0;
        assert!(collect(&bad, |_, _| panic!("must not read")).is_err());
        assert_eq!(
            collect(&c, |keys, _| Ok((0, batch(&all, keys)?))).unwrap_err(),
            "FE_CONTEXT_REGRESSION"
        );
        assert_eq!(
            collect(&c, |_, _| Ok((100, ObservedState::default()))).unwrap_err(),
            "FE_BATCH_SCOPE_MISMATCH"
        );
    }
    #[test]
    fn saved_full_observation_replays_identically_but_stays_historical_and_incomplete() {
        use super::super::fe_replay;
        let (mut c, all) = fixture();
        c.limits.fe_calls = 100;
        c.limits.max_requests = 100;
        let dir = std::env::temp_dir().join(format!("p0-fe-replay-{}", Pubkey::new_unique()));
        std::fs::create_dir(&dir).unwrap();
        let mut calls = 0;
        let original=collect(&c,|keys,floor| {
            calls+=1;let state=batch(&all,keys)?;
            let values:Vec<_>=keys.iter().map(|k| {let a=&state.0[k];json!({"owner":a.owner.to_string(),"lamports":a.lamports,"executable":a.executable,"data":[base64::engine::general_purpose::STANDARD.encode(&a.data),"base64"]})}).collect();
            let entry=json!({"method":"getMultipleAccounts","params":[keys.iter().map(ToString::to_string).collect::<Vec<_>>(),{"encoding":"base64","commitment":"finalized","minContextSlot":floor}],"result":{"context":{"slot":100},"value":values}});
            std::fs::write(dir.join(format!("read_{calls:06}.json")),serde_json::to_vec(&entry).unwrap()).unwrap();
            Ok((100,state))
        }).unwrap();
        let class = fe_replay::first_class("ZERO_PREFUND_SAME_TX").unwrap();
        let replayed = fe_replay::replay(&c, &dir, class).unwrap();
        for (key, value) in original.as_object().unwrap() {
            assert_eq!(&replayed[key], value, "{key}");
        }
        assert_eq!(replayed["replay"]["freshness"], "HISTORICAL_NOT_CURRENT");
        assert_eq!(replayed["pilot_ready"], false);
        c.limits.fe_calls = 1;
        assert_eq!(
            fe_replay::replay(&c, &dir, class).unwrap_err(),
            "FE_REPLAY_CALL_LIMIT"
        );
        c.limits.fe_calls = 100;
        std::fs::write(dir.join("read_999999.json"), b"{}").unwrap();
        assert_eq!(
            fe_replay::replay(&c, &dir, class).unwrap_err(),
            "FE_REPLAY_UNUSED_READS"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
