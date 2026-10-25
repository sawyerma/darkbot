//! Pinned-source instruction inventory and deployment continuity qualification.
//! The reference hash is the ProgramData baseline recorded by the qualified 0.1.11
//! Shadow upgrade watch (c0e1_4h_20261008T222326Z/runtime.log). This is continuity
//! evidence, NOT a reproducible-build bytecode/source-equivalence attestation.
use super::*;
use darkbot_project0::protocol;
const REFERENCE_HASH: &str = protocol::PROGRAM_DATA_REFERENCE_HASH;
const PROGRAM: &str = protocol::PROGRAM_ID_MAINNET;
const LOADER: &str = protocol::BPF_LOADER_UPGRADEABLE_ID;
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Deployment {
    pub context_slot: u64,
    pub hash: String,
    pub program_data: String,
}
pub(super) fn verified(
    before: Option<&Deployment>,
    after: Option<&Deployment>,
    end: Option<u64>,
) -> bool {
    before.zip(after).zip(end).is_some_and(|((a, b), end)| {
        a.hash == REFERENCE_HASH
            && b.hash == REFERENCE_HASH
            && a.program_data == b.program_data
            && b.context_slot >= end
            && b.context_slot >= a.context_slot
    })
}
pub(super) fn inspect(url: Option<&str>, program: &Pubkey) -> Result<Deployment, &'static str> {
    if program.to_string() != PROGRAM {
        return Err("PROGRAM_NOT_AUDITED");
    }
    let url = url.ok_or("DEPLOYMENT_RPC_MISSING")?;
    let loader = Pubkey::from_str(LOADER).map_err(|_| "LOADER_ID")?;
    let address = protocol::program_data_address(program);
    // D01: request bound, overall deadline and attempts from the typed limits.
    let limits = &darkbot_solana::runtime_config::active().limits;
    let request = Duration::from_millis(limits.deployment_request_ms);
    let client = darkbot_solana::provider::blocking_http()
        .timeout(request)
        .build()
        .map_err(|_| "DEPLOYMENT_CLIENT")?;
    let mut response = Err("DEPLOYMENT_RPC");
    let deadline = Instant::now() + Duration::from_millis(limits.deployment_deadline_ms);
    for attempt in 0..limits.rpc_attempts {
        let permit = darkbot_solana::provider::shared()
            .acquire(darkbot_solana::provider::Workload::Deployment, deadline)
            .map_err(|e| {
                if e == "RPC_RATE_LIMIT" {
                    "DEPLOYMENT_RPC_RATE_LIMIT"
                } else {
                    "DEPLOYMENT_RPC_BUDGET"
                }
            })?;
        response = (|| {
            let resp=client.post(url).timeout(deadline.saturating_duration_since(Instant::now()).min(request))
                .json(&json!({"jsonrpc":"2.0","id":1,"method":"getAccountInfo","params":[address.to_string(),{"encoding":"base64","commitment":"finalized"}]}))
                .send().map_err(|_| "DEPLOYMENT_RPC_TRANSPORT")?;
            darkbot_solana::provider::observe_response(&resp, "getAccountInfo").map_err(|e| {
                if e == "RPC_RATE_LIMIT" {
                    "DEPLOYMENT_RPC_RATE_LIMIT"
                } else {
                    "DEPLOYMENT_RPC_HTTP"
                }
            })?;
            resp.json::<Value>().map_err(|_| "DEPLOYMENT_RPC_JSON")
        })();
        drop(permit);
        if response.is_ok() || Instant::now() >= deadline {
            break;
        }
        if attempt + 1 < limits.rpc_attempts {
            // Exponential backoff from `rpc_backoff_ms`, capped by `rpc_backoff_max_ms`.
            let backoff = limits
                .rpc_backoff_ms
                .saturating_mul(1u64 << attempt.min(16))
                .min(limits.rpc_backoff_max_ms);
            std::thread::sleep(
                Duration::from_millis(backoff)
                    .min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }
    let response = response?;
    if response.get("error").is_some() {
        return Err("DEPLOYMENT_RPC_REMOTE_ERROR");
    }
    let v = &response["result"]["value"];
    if v["owner"] != LOADER || v["executable"] != false {
        return Err("DEPLOYMENT_ACCOUNT_OWNER");
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(v["data"][0].as_str().ok_or("DEPLOYMENT_DATA")?)
        .map_err(|_| "DEPLOYMENT_BASE64")?;
    let deployed = protocol::decode_program_data(&address, &loader, false, &bytes)
        .map_err(|_| "DEPLOYMENT_LAYOUT")?;
    Ok(Deployment {
        context_slot: response["result"]["context"]["slot"]
            .as_u64()
            .ok_or("DEPLOYMENT_CONTEXT_SLOT")?,
        hash: deployed.hash.to_string(),
        program_data: address.to_string(),
    })
}
// Exhaustive exported instruction namespace of the exact Cargo pin. Unknown
// successful instructions in scanned blocks cannot silently become non-liquidations.
pub(super) const INSTRUCTIONS: &[&str] = &[
    "marginfi_group_initialize",
    "marginfi_group_configure",
    "lending_pool_add_bank",
    "lending_pool_add_bank_with_seed",
    "lending_pool_clone_bank",
    "lending_pool_add_bank_permissionless",
    "lending_pool_backfill_bank_is_t22_flag",
    "lending_pool_backfill_staked_bank_validator_vote_account",
    "disable_staked_oracles",
    "enable_staked_oracle_onramp",
    "lending_pool_configure_bank",
    "lending_pool_configure_bank_interest_only",
    "lending_pool_configure_bank_limits_only",
    "lending_pool_force_tokenless_repay_complete",
    "lending_pool_clear_circuit_breaker",
    "lending_pool_configure_bank_oracle",
    "lending_pool_configure_bank_oracle_scope",
    "lending_pool_set_oracle_price",
    "lending_pool_init_same_asset_emode_registry",
    "lending_pool_set_bank_same_asset_emode_eligibility",
    "lending_pool_configure_bank_emode",
    "lending_pool_clone_emode",
    "lending_pool_emissions_deposit",
    "lending_pool_handle_bankruptcy",
    "super_admin_withdraw",
    "super_admin_deposit",
    "marginfi_account_initialize",
    "marginfi_account_init_liq_record",
    "marginfi_account_close_liq_record",
    "marginfi_account_initialize_pda",
    "marginfi_account_place_order",
    "marginfi_account_close_order",
    "marginfi_account_keeper_close_order",
    "marginfi_account_set_keeper_close_flags",
    "marginfi_account_start_execute_order",
    "marginfi_account_end_execute_order",
    "lending_account_deposit",
    "lending_account_repay",
    "lending_account_withdraw",
    "lending_account_borrow",
    "lending_account_close_balance",
    "lending_account_liquidate",
    "lending_account_start_flashloan",
    "lending_account_end_flashloan",
    "marginfi_account_update_emissions_destination_account",
    "lending_pool_accrue_bank_interest",
    "lending_pool_resize_group_account",
    "resize_global_fee_state",
    "lending_pool_collect_bank_fees",
    "lending_pool_withdraw_fees",
    "lending_pool_withdraw_fees_permissionless",
    "lending_pool_update_fees_destination_account",
    "lending_pool_withdraw_insurance",
    "lending_pool_close_bank",
    "transfer_to_new_account",
    "transfer_to_new_account_pda",
    "marginfi_account_set_freeze",
    "marginfi_account_close",
    "admin_close_account",
    "lending_account_pulse_health",
    "sync_indexer_flags",
    "lending_pool_pulse_bank_price_cache",
    "init_global_fee_state",
    "edit_global_fee_state",
    "propagate_fee_state",
    "config_group_fee",
    "init_staked_settings",
    "edit_staked_settings",
    "propagate_staked_settings",
    "start_liquidation",
    "end_liquidation",
    "start_deleverage",
    "end_deleverage",
    "panic_pause",
    "panic_unpause",
    "panic_unpause_permissionless",
    "init_bank_metadata",
    "write_bank_metadata",
    "write_bank_metadata_pre_init",
    "configure_deleverage_withdrawal_limit",
    "update_deleverage_withdrawals",
    "configure_bank_rate_limits",
    "configure_group_rate_limits",
    "update_group_rate_limiter",
    "purge_deleverage_balance",
    "kamino_init_obligation",
    "kamino_deposit",
    "kamino_withdraw",
    "lending_pool_add_bank_kamino",
    "kamino_harvest_reward",
    "lending_pool_add_bank_drift",
    "drift_init_user",
    "drift_deposit",
    "drift_withdraw",
    "drift_harvest_reward",
    "drift_claim_bad_debt",
    "lending_pool_add_bank_solend",
    "solend_init_obligation",
    "solend_deposit",
    "solend_withdraw",
    "lending_pool_add_bank_juplend",
    "juplend_init_position",
    "juplend_deposit",
    "juplend_withdraw",
];
pub(super) fn instruction_known(d: &[u8]) -> bool {
    use std::sync::OnceLock;
    static DISCRIMINATORS: OnceLock<HashSet<[u8; 8]>> = OnceLock::new();
    DISCRIMINATORS
        .get_or_init(|| {
            INSTRUCTIONS
                .iter()
                .map(|n| {
                    solana_sdk::hash::hash(format!("global:{n}").as_bytes()).to_bytes()[..8]
                        .try_into()
                        .unwrap()
                })
                .collect()
        })
        .contains(d)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upgrade_or_missing_proof_never_qualifies() {
        let a = Deployment {
            context_slot: 1,
            hash: REFERENCE_HASH.into(),
            program_data: "p".into(),
        };
        let mut b = a.clone();
        b.context_slot = 10;
        assert!(verified(Some(&a), Some(&b), Some(10)));
        assert!(!verified(None, Some(&b), Some(10)));
        assert!(!verified(Some(&a), Some(&b), Some(11)));
        b.hash = "changed".into();
        assert!(!verified(Some(&a), Some(&b), Some(10)));
    }
    #[test]
    fn instruction_namespace_is_unique_and_unknown_fails_closed() {
        let hashes: HashSet<_> = INSTRUCTIONS
            .iter()
            .map(|n| {
                solana_sdk::hash::hash(format!("global:{n}").as_bytes()).to_bytes()[..8].to_vec()
            })
            .collect();
        assert_eq!(hashes.len(), INSTRUCTIONS.len());
        assert!(!instruction_known(&[0; 8]));
        for n in [
            "lending_account_liquidate",
            "end_liquidation",
            "end_deleverage",
            "lending_pool_handle_bankruptcy",
            "purge_deleverage_balance",
            "drift_claim_bad_debt",
        ] {
            assert!(instruction_known(
                &solana_sdk::hash::hash(format!("global:{n}").as_bytes()).to_bytes()[..8]
            ));
        }
    }
}
