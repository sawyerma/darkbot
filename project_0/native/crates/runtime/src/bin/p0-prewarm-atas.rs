//! Prewarms the wallet's associated token accounts. A dry run reads, builds the creating
//! transaction and simulates it unsigned (its fee and each account's rent measured). DEC-17f
//! (P8-CTRL-03d): `--send` signs and sends it under the owner's own permits — WALLET, ECON_SIGN
//! and ECON_SEND ENABLED in the running owner's epoch, the operator's confirmation, its cost
//! within `prewarm_max_lamports` (`tool_permit`); there is no other way to send.
//! DEC-17b B4: `--for <liquidator>` prewarms the ATAs of a configured liquidator without its own
//! fees, created and paid (rent and fee) by this keypair — a configured fee payer alone; the
//! liquidator signs nothing.
use darkbot_project0::protocol;
use darkbot_runner::orchestration::tool_permit::{self, Tool, ToolUse};
use darkbot_solana::tx;
use solana_client::rpc_config::{
    RpcSimulateTransactionAccountsConfig, RpcSimulateTransactionConfig,
};
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{message::Message, pubkey::Pubkey, signature::Signer, transaction::Transaction};
use spl_associated_token_account::{
    get_associated_token_address_with_program_id,
    instruction::create_associated_token_account_idempotent,
};
use std::{env, error::Error, str::FromStr};

fn need(name: &str) -> Result<String, Box<dyn Error>> {
    env::var(name)
        .map_err(|_| format!("missing required env: {name}").into())
        .and_then(|v| {
            if v.trim().is_empty() {
                Err(format!("missing required env: {name}").into())
            } else {
                Ok(v)
            }
        })
}

/// DEC-17b B4: `liquidator` may have its ATAs paid by `payer` — among the configured signers the
/// liquidator is a LIQUIDATOR without its own FEE_PAYER role and the payer a FEE_PAYER alone.
fn paid_owner(signers: &[String], payer: &Pubkey, liquidator: &Pubkey) -> Result<(), String> {
    use darkbot_solana::runtime_config::{roles, signer_entry};
    let roles_of = |key: &Pubkey| -> Result<Option<Vec<&'static str>>, String> {
        for entry in signers {
            let (k, r) = signer_entry(entry)?;
            if k == *key {
                return Ok(Some(r));
            }
        }
        Ok(None)
    };
    if roles_of(liquidator)?.as_deref() != Some(&[roles::LIQUIDATOR][..])
        || roles_of(payer)?.as_deref() != Some(&[roles::FEE_PAYER][..])
    {
        return Err(format!(
            "PREWARM_FOR_NOT_PAID: {liquidator} is no configured LIQUIDATOR paid by the configured FEE_PAYER {payer} (DEC-17b B4)"
        ));
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("p0-prewarm-atas: {e}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    // DEC-18: the registry's RPC provider, its credential applied by the transport.
    let rpc_url = darkbot_runner::orchestration::provider_registry::activate_tool()?;
    let keypair_path = need("P0_KEYPAIR_PATH")?;
    let expected_pubkey = Pubkey::from_str(&need("P0_EXPECTED_PUBKEY")?)?;
    let payer = tx::load_keypair_json(&keypair_path)?;
    let payer_pk = payer.pubkey();

    if payer_pk != expected_pubkey {
        return Err(format!(
            "signer identity mismatch: derived={} expected={}",
            payer_pk, expected_pubkey
        )
        .into());
    }

    let mut args: Vec<String> = env::args().skip(1).collect();
    let send = args.first().is_some_and(|a| a == "--send");
    if send {
        args.remove(0);
    }
    // DEC-17b B4: the wallet whose ATAs are created (the payer's own, or a liquidator it pays).
    let owner_pk = if args.first().is_some_and(|a| a == "--for") {
        args.remove(0);
        if args.is_empty() {
            return Err("usage: --for <liquidator pubkey>".into());
        }
        let liquidator = Pubkey::from_str(&args.remove(0))?;
        paid_owner(
            &darkbot_solana::runtime_config::active().signers,
            &payer_pk,
            &liquidator,
        )?;
        liquidator
    } else {
        payer_pk
    };
    if args.is_empty() || !args.len().is_multiple_of(2) {
        return Err(
            "usage: p0-prewarm-atas [--send] [--for <liquidator>] <mint> <token_program> [<mint> <token_program> ...]"
                .into(),
        );
    }

    let token_v1 = Pubkey::from_str(protocol::TOKEN_PROGRAM_ID)?;
    let token_2022 = Pubkey::from_str(protocol::TOKEN_2022_PROGRAM_ID)?;
    // F-01/C07: through the process provider budget like every other client (no plain client).
    let rpc = darkbot_solana::provider::rpc_client(
        rpc_url,
        std::time::Duration::from_millis(
            darkbot_solana::runtime_config::active()
                .limits
                .rpc_timeout_ms,
        ),
        CommitmentConfig::finalized(),
        darkbot_solana::provider::Workload::Bootstrap,
    );
    let balance = rpc.get_balance(&payer_pk)?;

    let mut missing = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for pair in args.chunks_exact(2) {
        let mint = Pubkey::from_str(&pair[0])?;
        let token_program = Pubkey::from_str(&pair[1])?;
        if token_program != token_v1 && token_program != token_2022 {
            return Err(format!(
                "unsupported token program for mint {}: {}",
                mint, token_program
            )
            .into());
        }

        let ata = get_associated_token_address_with_program_id(&owner_pk, &mint, &token_program);
        if !seen.insert(ata) {
            continue;
        }

        let existing = rpc
            .get_account_with_commitment(&ata, CommitmentConfig::confirmed())?
            .value;

        match existing {
            Some(account) => {
                if account.owner != token_program {
                    return Err(format!(
                        "ATA owner mismatch: ata={} owner={} expected={}",
                        ata, account.owner, token_program
                    )
                    .into());
                }
                println!(
                    "PREWARM_PRESENT ata={} mint={} token_program={} lamports={}",
                    ata, mint, token_program, account.lamports
                );
            }
            None => {
                println!(
                    "PREWARM_MISSING ata={} mint={} token_program={}",
                    ata, mint, token_program
                );
                missing.push((mint, token_program, ata));
            }
        }
    }

    println!(
        "PREWARM_BALANCE payer={} owner={} lamports={} missing={}",
        payer_pk,
        owner_pk,
        balance,
        missing.len()
    );

    if missing.is_empty() {
        println!("PREWARM_READY all requested ATAs already exist");
        return Ok(());
    }

    let instructions = missing
        .iter()
        .map(|(mint, token_program, _)| {
            create_associated_token_account_idempotent(&payer_pk, &owner_pk, mint, token_program)
        })
        .collect::<Vec<_>>();

    let blockhash = rpc.get_latest_blockhash()?;
    // Unsigned: no real signature before the permit (DEC-17f).
    let mut transaction = Transaction::new_unsigned(Message::new_with_blockhash(
        &instructions,
        Some(&payer_pk),
        &blockhash,
    ));

    // [FIX-122] Measure the actual post-create rent for each ATA, including Token-2022
    // extensions, rather than guessing an account size.
    let fee_lamports = rpc.get_fee_for_message(&transaction.message)?;
    let simulation = rpc.simulate_transaction_with_config(
        &transaction,
        RpcSimulateTransactionConfig {
            sig_verify: false,
            replace_recent_blockhash: false,
            commitment: Some(CommitmentConfig::confirmed()),
            accounts: Some(RpcSimulateTransactionAccountsConfig {
                encoding: None,
                addresses: missing.iter().map(|(_, _, ata)| ata.to_string()).collect(),
            }),
            ..RpcSimulateTransactionConfig::default()
        },
    )?;
    if let Some(err) = simulation.value.err {
        return Err(format!(
            "prewarm simulation failed: {:?} logs={:?}",
            err, simulation.value.logs
        )
        .into());
    }
    println!(
        "PREWARM_SIM_OK units_consumed={:?}",
        simulation.value.units_consumed
    );

    let post_accounts = simulation
        .value
        .accounts
        .ok_or("prewarm simulation omitted post accounts")?;
    if post_accounts.len() != missing.len() {
        return Err("prewarm simulation account count mismatch".into());
    }
    let mut rent_lamports = 0u64;
    for ((mint, token_program, ata), ui) in missing.iter().zip(post_accounts) {
        let account = ui
            .ok_or("prewarm simulation account missing")?
            .decode::<solana_sdk::account::Account>()
            .ok_or("prewarm account decode")?;
        if account.owner != *token_program
            || account.data.len() < 64
            || account.data[..32] != mint.to_bytes()
            || account.data[32..64] != owner_pk.to_bytes()
        {
            return Err("prewarm simulated ATA identity mismatch".into());
        }
        rent_lamports = rent_lamports
            .checked_add(account.lamports)
            .ok_or("rent overflow")?;
        println!(
            "PREWARM_RENT ata={} data_len={} lamports={}",
            ata,
            account.data.len(),
            account.lamports
        );
    }
    let total = rent_lamports
        .checked_add(fee_lamports)
        .ok_or("cost overflow")?;
    let remaining = balance
        .checked_sub(total)
        .ok_or("insufficient prewarm balance")?;
    println!(
        "PREWARM_COST rent_lamports={} fee_lamports={} total_lamports={} remaining_lamports={}",
        rent_lamports, fee_lamports, total, remaining
    );

    #[cfg(feature = "crate_health")]
    {
        let program = Pubkey::from_str(&need("P0_PROGRAM_ID")?)?;
        let state = rpc.get_account(&protocol::fee_state_pda(&program))?;
        if state.owner != program {
            return Err("invalid protocol FeeState".into());
        }
        // B2: strict pinned layout, never a prefix decode.
        let fee = protocol::decode_fee_state(&state.data)
            .map_err(|e| format!("invalid protocol FeeState: {e}"))?;
        println!("PREWARM_PROTOCOL_RESERVE flat_fee_lamports={} remaining_after_prewarm={} covers_flat_fee={}",
            fee.liquidation_flat_sol_fee, remaining, remaining >= fee.liquidation_flat_sol_fee as u64);
    }

    if !send {
        println!("PREWARM_DRY_RUN simulation passed; no transaction sent; --send creates the missing ATAs under the owner's permits");
        return Ok(());
    }

    // DEC-17f: the permit of exactly this message and its cost — the owner's switches, the
    // operator's confirmation, the cost within `prewarm_max_lamports`.
    let project = std::path::PathBuf::from(need("P0_PROJECT_DIR")?);
    let message = transaction.message_data();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let mut permit = tool_permit::issue(
        &project,
        darkbot_solana::runtime_config::active(),
        &ToolUse {
            tool: Tool::PrewarmAtas,
            signers: &[payer_pk],
            messages: &[&message],
            send: true,
            cost_lamports: total,
        },
        now,
        &tool_permit::operator_confirmation(&project, Tool::PrewarmAtas),
    )?;
    transaction.signatures = permit.sign(&message, &[&payer])?;
    // The exact signed message verifies locally — nothing signed leaves before the send's own
    // check (review L4).
    transaction.verify()?;
    permit.admit_send()?;
    let signature = rpc.send_and_confirm_transaction(&transaction)?;
    println!("PREWARM_SENT signature={}", signature);

    for (_, _, ata) in &missing {
        let account = rpc
            .get_account_with_commitment(ata, CommitmentConfig::confirmed())?
            .value
            .ok_or_else(|| format!("created ATA not visible at confirmed commitment: {ata}"))?;
        println!(
            "PREWARM_CONFIRMED ata={} owner={} lamports={}",
            ata, account.owner, account.lamports
        );
    }

    println!("PREWARM_READY created={}", missing.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DEC-17b B4: `--for` only for a configured liquidator without its own fees, paid by a
    /// configured fee payer alone.
    #[test]
    fn dec17b_b4_prewarm_for_a_liquidator_its_fee_payer_pays() {
        let (l, p, own, t) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let signers = vec![
            format!("{l}:LIQUIDATOR"),
            format!("{p}:FEE_PAYER"),
            format!("{own}:LIQUIDATOR+FEE_PAYER"),
            format!("{t}:TREASURY"),
        ];
        assert_eq!(paid_owner(&signers, &p, &l), Ok(()));
        for (payer, liquidator) in [
            (&own, &l),
            (&p, &own),
            (&p, &t),
            (&l, &p),
            (&p, &Pubkey::new_unique()),
        ] {
            assert!(paid_owner(&signers, payer, liquidator)
                .unwrap_err()
                .starts_with("PREWARM_FOR_NOT_PAID"));
        }
    }
}
