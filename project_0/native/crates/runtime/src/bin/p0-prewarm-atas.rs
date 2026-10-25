use darkbot_project0::protocol;
use darkbot_solana::tx;
use solana_client::rpc_config::{
    RpcSimulateTransactionAccountsConfig, RpcSimulateTransactionConfig,
};
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{pubkey::Pubkey, signature::Signer, transaction::Transaction};
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

fn main() {
    if let Err(e) = run() {
        eprintln!("p0-prewarm-atas: {e}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let rpc_url = need("P0_RPC_URL")?;
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

    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || !args.len().is_multiple_of(2) {
        return Err(
            "usage: p0-prewarm-atas <mint> <token_program> [<mint> <token_program> ...]".into(),
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

        let ata = get_associated_token_address_with_program_id(&payer_pk, &mint, &token_program);
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
        "PREWARM_BALANCE payer={} lamports={} missing={}",
        payer_pk,
        balance,
        missing.len()
    );

    if missing.is_empty() {
        println!("PREWARM_READY all requested ATAs already exist");
        return Ok(());
    }

    let send = env::var("P0_PREWARM_SEND").ok().as_deref() == Some("YES");

    let instructions = missing
        .iter()
        .map(|(mint, token_program, _)| {
            create_associated_token_account_idempotent(&payer_pk, &payer_pk, mint, token_program)
        })
        .collect::<Vec<_>>();

    let blockhash = rpc.get_latest_blockhash()?;
    let transaction =
        Transaction::new_signed_with_payer(&instructions, Some(&payer_pk), &[&payer], blockhash);

    // [FIX-122] Measure the actual post-create rent for each ATA, including Token-2022
    // extensions, rather than guessing an account size. Verify the exact signed message.
    let fee_lamports = rpc.get_fee_for_message(&transaction.message)?;
    let simulation = rpc.simulate_transaction_with_config(
        &transaction,
        RpcSimulateTransactionConfig {
            sig_verify: true,
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
            || account.data[32..64] != payer_pk.to_bytes()
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
        println!("PREWARM_DRY_RUN simulation passed; no transaction sent; set P0_PREWARM_SEND=YES to create missing ATAs");
        return Ok(());
    }

    // A real prerequisite send requires the explicitly approved total-cost ceiling.
    let approved_max = need("P0_PREWARM_MAX_LAMPORTS")?.parse::<u64>()?;
    if total > approved_max {
        return Err(format!(
            "prewarm cost {} exceeds approved ceiling {}",
            total, approved_max
        )
        .into());
    }
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
