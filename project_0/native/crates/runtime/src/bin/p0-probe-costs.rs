//! FIX-129: read-only RPC fee/simulation evidence. No send method. DEC-17f (P8-CTRL-03d): its
//! three messages are signed by the wallet under one permit of the owner's — WALLET and
//! ECON_SIGN ENABLED in the running owner's epoch, the operator's confirmation (`tool_permit`).
use darkbot_runner::orchestration::tool_permit::{self, Tool, ToolUse};
use darkbot_solana::{
    submit::{rpc::RpcSubmitter, SubmissionAdapter},
    tx::{self, BuildVersion, Resources, TxLimits, TxPlan},
};
use serde_json::json;
use solana_client::{rpc_config::RpcSimulateTransactionConfig, rpc_request::RpcRequest};
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Signer,
};
use std::{env, error::Error, str::FromStr};
fn main() -> Result<(), Box<dyn Error>> {
    // DEC-18: the registry's RPC provider, its credential applied by the transport.
    let adapter = RpcSubmitter::for_candidates(
        darkbot_runner::orchestration::provider_registry::activate_tool()?,
        false,
    )?;
    let payer = tx::load_keypair_json(&env::var("P0_KEYPAIR_PATH")?)?;
    if payer.pubkey() != Pubkey::from_str(&env::var("P0_EXPECTED_PUBKEY")?)? {
        return Err("signer identity mismatch".into());
    }
    let hash = adapter
        .client()
        .get_latest_blockhash()
        .map_err(|_| "blockhash RPC failed")?;
    let mut built = vec![];
    for (name, fail, price) in [
        ("success_no_priority", false, 0),
        ("success_requested_priority", false, 2000),
        ("execution_revert_requested_priority", true, 2000),
    ] {
        // Canonical SystemInstruction::Transfer discriminant (2), u64 amount.
        let mut data = 2u32.to_le_bytes().to_vec();
        data.extend_from_slice(&(if fail { u64::MAX } else { 0u64 }).to_le_bytes());
        let plan = TxPlan {
            version: BuildVersion::V0,
            payer: payer.pubkey(),
            instructions: vec![Instruction {
                program_id: Pubkey::default(),
                accounts: vec![
                    AccountMeta::new(payer.pubkey(), true),
                    AccountMeta::new(payer.pubkey(), false),
                ],
                data,
            }],
            lookup_tables: vec![].into(),
            resources: Resources {
                compute_unit_limit: 1_400_000,
                priority_micro_lamports_per_cu: price,
                loaded_accounts_data_size: Some(4 * 1024 * 1024),
            },
        };
        let tx = tx::build(&plan, hash, &TxLimits::default())?.tx;
        built.push((name, price, plan, tx));
    }
    // DEC-17f: one permit, one confirmation, for exactly these three messages.
    let messages: Vec<Vec<u8>> = built.iter().map(|(.., t)| t.message.serialize()).collect();
    let project = std::path::PathBuf::from(env::var("P0_PROJECT_DIR")?);
    let mut permit = tool_permit::issue(
        &project,
        darkbot_solana::runtime_config::active(),
        &ToolUse {
            tool: Tool::ProbeCosts,
            signers: &[payer.pubkey()],
            messages: &messages.iter().map(Vec::as_slice).collect::<Vec<_>>(),
            send: false,
            cost_lamports: 0,
        },
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
        &tool_permit::operator_confirmation(&project, Tool::ProbeCosts),
    )?;
    let mut scenarios = vec![];
    for ((name, price, plan, mut tx), message) in built.into_iter().zip(&messages) {
        tx.signatures = permit.sign(message, &[&payer])?;
        // N2: one probe call, bounded like the provider's own per-call limit.
        let quote = adapter.fee_for_message(
            &tx,
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        );
        let sim = adapter
            .client()
            .simulate_transaction_with_config(
                &tx,
                RpcSimulateTransactionConfig {
                    sig_verify: true,
                    commitment: Some(CommitmentConfig::confirmed()),
                    ..Default::default()
                },
            )
            .map_err(|_| "simulation RPC failed")?;
        scenarios.push(json!({"case":name,"fee_quote":quote.as_ref().ok(),
            "fee_quote_error":quote.as_ref().err().map(|e| format!("{e:?}")),"requested_cu_limit":plan.resources.compute_unit_limit,
            "cu_price_micro_lamports":price,"priority_lamports_from_requested_limit":plan.resources.priority_total_lamports(),
            "simulation":sim}));
    }
    let known =
        "2AAPtxnp6RBsGNikHuEGZJSN6jpFEcFajXV28b97Sbs7gwn6XDnQ775xHkfc3A7xRwXVvEvjgpgwWSyZdcB8n1t2";
    let landed: serde_json::Value = adapter.client().send(RpcRequest::GetTransaction,
        json!([known,{"encoding":"json","commitment":"finalized","maxSupportedTransactionVersion":1}]))
        .map_err(|_| "historical transaction RPC failed")?;
    // Read-only third-party sample: validates meta.fee for a real execution
    // failure, but does NOT establish its RPC/Jito path or our paid cost.
    let recent: serde_json::Value = adapter.client().send(RpcRequest::GetSignaturesForAddress,
        json!(["MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA", {"limit":100,"commitment":"finalized"}]))
        .map_err(|_| "historical signature RPC failed")?;
    let mut landed_revert = serde_json::Value::Null;
    if let Some(sig) = recent
        .as_array()
        .and_then(|xs| {
            xs.iter()
                .find(|x| x.get("err").is_some_and(|e| !e.is_null()))
        })
        .and_then(|x| x.get("signature"))
        .and_then(|x| x.as_str())
    {
        let transaction: serde_json::Value = adapter.client().send(RpcRequest::GetTransaction,
            json!([sig,{"encoding":"json","commitment":"finalized","maxSupportedTransactionVersion":1}]))
            .map_err(|_| "historical revert RPC failed")?;
        landed_revert =
            json!({"signature":sig,"transaction":transaction,"submission_path":null,"ours":false});
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"probe":"FIX-129 exact message fees","transactions_sent":0,
        "scenarios":scenarios,"landed_ata_signature":known,"landed_ata_transaction":landed,
        "third_party_landed_revert":landed_revert,"jito_lifecycle_evidence":null,"claims":"Simulated revert is not a landed revert; prior ATA fee/rent applies only to that transaction."})
        )?
    );
    Ok(())
}
