//! ZERO_PREFUND_SAME_TX fixture.
//!
//! Uses official marginfi builders, a captured Jupiter `swap-instructions` response,
//! real v0 lookup tables, and the isolated v1 capability adapter. The fixture itself
//! is deterministic/offline once route JSON + LUT accounts + state snapshot are supplied.

#[cfg(feature = "crate_health")]
pub mod run {
    use base64::Engine;
    use darkbot_project0::health::crate_impl::CrateHealth;
    use darkbot_project0::health::{AccountSource, HealthEngine};
    use darkbot_project0::protocol;
    use darkbot_project0::receivership::{builders, ReceivershipParams};
    use darkbot_solana::tx::{self, BuildVersion, Resources, TxLimits, TxPlan};
    use marginfi_type_crate::types::Bank;
    use serde_json::Value;
    use solana_client::rpc_client::RpcClient;
    use solana_message::AddressLookupTableAccount /* [FIX-13] */;
    use solana_sdk::hash::Hash;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;
    use std::str::FromStr;

    const LOOKUP_TABLE_META_SIZE: usize = 56;

    #[derive(Clone, Debug)]
    pub struct JupiterRoute {
        /// Route-only instructions. Compute-budget instructions are intentionally omitted;
        /// DarkBot owns resource configuration so v0/v1 compare the same economics.
        pub instructions: Vec<Instruction>,
        pub lookup_table_addresses: Vec<Pubkey>,
    }

    fn parse_ix(ix: &Value) -> Result<Instruction, String> {
        let program_id = Pubkey::from_str(ix["programId"].as_str().ok_or("programId")?)
            .map_err(|e| e.to_string())?;
        let accounts = ix["accounts"]
            .as_array()
            .ok_or("accounts")?
            .iter()
            .map(|a| {
                let pk = Pubkey::from_str(a["pubkey"].as_str().ok_or("account pubkey")?)
                    .map_err(|e| e.to_string())?;
                Ok(AccountMeta {
                    pubkey: pk,
                    is_signer: a["isSigner"].as_bool().unwrap_or(false),
                    is_writable: a["isWritable"].as_bool().unwrap_or(false),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let data = base64::engine::general_purpose::STANDARD
            .decode(ix["data"].as_str().ok_or("data")?)
            .map_err(|e| e.to_string())?;
        Ok(Instruction { program_id, accounts, data })
    }

    fn push_optional(out: &mut Vec<Instruction>, v: &Value, field: &str) -> Result<(), String> {
        let ix = &v[field];
        if !ix.is_null() {
            out.push(parse_ix(ix)?);
        }
        Ok(())
    }

    fn push_array(out: &mut Vec<Instruction>, v: &Value, field: &str) -> Result<(), String> {
        if let Some(arr) = v[field].as_array() {
            for ix in arr {
                out.push(parse_ix(ix)?);
            }
        }
        Ok(())
    }

    /// Parses every route-relevant instruction from Jupiter's swap-instructions response.
    /// `computeBudgetInstructions` are deliberately ignored because DarkBot injects/encodes
    /// resources itself. `tokenLedgerInstruction`, setup/other/swap/cleanup are preserved.
    pub fn route_from_jupiter_json(v: &Value) -> Result<JupiterRoute, String> {
        let mut instructions = Vec::new();
        push_optional(&mut instructions, v, "tokenLedgerInstruction")?;
        push_array(&mut instructions, v, "setupInstructions")?;
        push_array(&mut instructions, v, "otherInstructions")?;
        instructions.push(parse_ix(&v["swapInstruction"])?);
        push_optional(&mut instructions, v, "cleanupInstruction")?;

        let lookup_table_addresses = v["addressLookupTableAddresses"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|x| {
                        Pubkey::from_str(x.as_str().ok_or("lookup table address")?)
                            .map_err(|e| e.to_string())
                    })
                    .collect::<Result<Vec<_>, String>>()
            })
            .transpose()?
            .unwrap_or_default();

        Ok(JupiterRoute { instructions, lookup_table_addresses })
    }

    /// Loads actual ALT contents for a fair v0 measurement. The lookup-table account wire
    /// format is fixed: 56-byte metadata followed by packed 32-byte addresses.
    pub fn load_lookup_tables(
        rpc: &RpcClient,
        keys: &[Pubkey],
    ) -> Result<Vec<AddressLookupTableAccount>, String> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let accounts = rpc.get_multiple_accounts(keys).map_err(|e| e.to_string())?;
        keys.iter()
            .copied()
            .zip(accounts)
            .map(|(key, account)| {
                let account = account.ok_or_else(|| format!("lookup table missing: {key}"))?;
                if account.data.len() < LOOKUP_TABLE_META_SIZE {
                    return Err(format!("lookup table {key} too short: {}", account.data.len()));
                }
                let raw = &account.data[LOOKUP_TABLE_META_SIZE..];
                if raw.len() % 32 != 0 {
                    return Err(format!("lookup table {key} address payload not multiple of 32"));
                }
                let mut addresses = Vec::with_capacity(raw.len() / 32);
                for chunk in raw.chunks_exact(32) {
                    let bytes: [u8; 32] = chunk.try_into().map_err(|_| "bad lookup address")?;
                    addresses.push(Pubkey::new_from_array(bytes));
                }
                Ok(AddressLookupTableAccount { key, addresses })
            })
            .collect()
    }

    /// Route instructions themselves must be Project-0 allowlisted. ComputeBudget is not
    /// present here; it is generated before the receivership by the v0 builder / v1 config.
    pub fn route_allowlisted(ixs: &[Instruction]) -> bool {
        let allowed = protocol::receivership_allowed_programs();
        ixs.iter().all(|ix| allowed.contains(&ix.program_id))
    }

    pub struct FixtureInput<'a> {
        pub program: Pubkey,
        pub group: Pubkey,
        pub account: Pubkey,
        pub liquidator: Pubkey,
        pub asset_bank: Pubkey,
        pub liab_bank: Pubkey,
        pub liquidator_asset_ata: Pubkey,
        pub liquidator_liab_ata: Pubkey,
        pub global_fee_wallet: Pubkey,
        pub route: JupiterRoute,
        pub route_lookup_tables: Vec<AddressLookupTableAccount>,
        pub init_record: bool,
        pub state: &'a dyn AccountSource,
    }

    #[derive(Debug, serde::Serialize)]
    pub struct FixtureMeasure {
        pub funding: &'static str,
        pub version: String,
        pub instructions: usize,
        pub locked_accounts: usize,
        pub lookup_tables: usize,
        pub serialized_bytes: Option<usize>,
        pub build_error: Option<String>,
        pub route_allowlisted: bool,
    }

    fn bank_fields(
        state: &dyn AccountSource,
        bank: &Pubkey,
    ) -> Result<(Pubkey, Pubkey, Pubkey), String> {
        let (_, _, data) = state.account(bank).ok_or("bank not in state")?;
        let need = 8 + std::mem::size_of::<Bank>();
        if data.len() < need {
            return Err(format!("bank account too short: {} < {}", data.len(), need));
        }
        let b: &Bank = bytemuck::try_from_bytes(&data[8..need]).map_err(|e| format!("{e:?}"))?;
        let (mint_owner, _, _) = state.account(&b.mint).ok_or("mint not in state")?;
        Ok((b.liquidity_vault, b.mint, mint_owner))
    }

    /// Builds identical business instructions for v0 and v1. v0 gets real Jupiter LUTs;
    /// v1 gets no LUTs and is measured by the official modular v1 message adapter.
    pub fn build_and_measure(
        inp: &FixtureInput<'_>,
        limits: &TxLimits,
    ) -> Result<(Vec<Instruction>, Vec<FixtureMeasure>), String> {
        let health = CrateHealth { state: inp.state, group: inp.group };
        let obs = health.observation_accounts(&inp.account).map_err(|e| e.to_string())?;
        let (asset_vault, _asset_mint, asset_tp) = bank_fields(inp.state, &inp.asset_bank)?;
        let (liab_vault, _liab_mint, liab_tp) = bank_fields(inp.state, &inp.liab_bank)?;
        let withdraw = builders::withdraw(
            &inp.program, &inp.group, &inp.account, &inp.liquidator, &inp.asset_bank,
            &inp.liquidator_asset_ata, &asset_vault, &asset_tp, 0, Some(true), &obs,
        );
        let repay = builders::repay(
            &inp.program, &inp.group, &inp.account, &inp.liquidator, &inp.liab_bank,
            &inp.liquidator_liab_ata, &liab_vault, &liab_tp, 0, Some(true),
        );
        let ixs = builders::build_receivership(ReceivershipParams {
            program: &inp.program,
            group: &inp.group,
            marginfi_account: &inp.account,
            liquidator: &inp.liquidator,
            global_fee_wallet: &inp.global_fee_wallet,
            observation: &obs,
            init_record: inp.init_record,
            pre_refreshes: vec![],
            post_start_setup: vec![],
            withdraws: vec![withdraw],
            swaps: inp.route.instructions.clone(),
            repays: vec![repay],
        });
        let allow = route_allowlisted(&ixs); // [FIX-11] komplette finale Ix-Liste, nicht nur die Route
        let resources = Resources {
            compute_unit_limit: 1_400_000,
            priority_micro_lamports_per_cu: 0,
            loaded_accounts_data_size: Some(64 * 1024 * 1024),
        };
        let mut out = Vec::new();

        let v0 = TxPlan {
            version: BuildVersion::V0,
            payer: inp.liquidator,
            instructions: ixs.clone(),
            lookup_tables: inp.route_lookup_tables.clone(),
            resources,
        };
        match tx::build(&v0, Hash::default(), limits) {
            Ok(b) => out.push(FixtureMeasure {
                funding: "ZERO_PREFUND_SAME_TX",
                version: "V0".into(),
                instructions: tx::effective_instruction_count(&v0),
                locked_accounts: b.locked_accounts,
                lookup_tables: v0.lookup_tables.len(),
                serialized_bytes: Some(b.serialized_len),
                build_error: None,
                route_allowlisted: allow,
            }),
            Err(e) => out.push(FixtureMeasure {
                funding: "ZERO_PREFUND_SAME_TX",
                version: "V0".into(),
                instructions: tx::effective_instruction_count(&v0),
                locked_accounts: tx::effective_locked_account_count(&v0),
                lookup_tables: v0.lookup_tables.len(),
                serialized_bytes: None,
                build_error: Some(e.to_string()),
                route_allowlisted: allow,
            }),
        }

        let v1 = TxPlan {
            version: BuildVersion::V1,
            payer: inp.liquidator,
            instructions: ixs.clone(),
            lookup_tables: vec![],
            resources,
        };
        match tx::measure_v1(&v1, Hash::default(), limits) {
            Ok(m) => out.push(FixtureMeasure {
                funding: "ZERO_PREFUND_SAME_TX",
                version: "V1".into(),
                instructions: m.instructions,
                locked_accounts: m.locked_accounts,
                lookup_tables: 0,
                serialized_bytes: Some(m.serialized_tx_bytes),
                build_error: None,
                route_allowlisted: allow,
            }),
            Err(e) => out.push(FixtureMeasure {
                funding: "ZERO_PREFUND_SAME_TX",
                version: "V1".into(),
                instructions: ixs.len(),
                locked_accounts: tx::locked_account_count(&inp.liquidator, &ixs),
                lookup_tables: 0,
                serialized_bytes: None,
                build_error: Some(e.to_string()),
                route_allowlisted: allow,
            }),
        }

        Ok((ixs, out))
    }
}
