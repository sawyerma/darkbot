//! Explicit read-only provider capability probe; never calls sendTransaction. DEC-17f
//! (P8-CTRL-03d): its messages are signed by the wallet under one permit of the owner's — WALLET
//! and ECON_SIGN ENABLED in the running owner's epoch, the operator's confirmation, their fee
//! exposure within `probe_max_lamports` (`tool_permit`), under the operational configuration
//! (the operator key's hash, the limits).
use base64::Engine;
use darkbot_runner::orchestration::tool_permit::{self, Tool, ToolUse};
use darkbot_solana::tx::{self, BuildVersion, Resources, TxLimits, TxPlan};
use solana_client::rpc_request::RpcRequest;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Signer,
};
use std::{env, error::Error, str::FromStr};

fn main() -> Result<(), Box<dyn Error>> {
    // The operational configuration (the operator key's hash, the limits) — review M2.
    darkbot_solana::runtime_config::Resolved::parse(&[], &|k| env::var(k).ok())?.activate()?;
    // F-01/C07: through the process provider budget like every other client (no plain client).
    // DEC-18: a local node only (a remote provider is the registry's; its measurement is
    // `p0ctl provider measure`, C-11).
    let local = darkbot_solana::providers::BoundProvider::local(
        darkbot_solana::providers::ProviderKind::Rpc,
        &env::var("P0_RPC_URL")?,
    )?;
    let rpc = darkbot_solana::provider::rpc_client(
        local.endpoint,
        std::time::Duration::from_millis(
            darkbot_solana::runtime_config::active()
                .limits
                .rpc_timeout_ms,
        ),
        CommitmentConfig::confirmed(),
        darkbot_solana::provider::Workload::Simulation,
    );
    let payer = tx::load_keypair_json(&env::var("P0_KEYPAIR_PATH")?)?;
    let expected = Pubkey::from_str(&env::var("P0_EXPECTED_PUBKEY")?)?;
    if payer.pubkey() != expected {
        return Err("signer identity mismatch".into());
    }
    let hash = rpc
        .get_latest_blockhash()
        .map_err(|_| "blockhash RPC failed")?;
    let plan_of = |large: bool| -> Result<TxPlan, Box<dyn Error>> {
        // Canonical SystemInstruction::Transfer discriminant (2), amount 0, to itself.
        let mut data = 2u32.to_le_bytes().to_vec();
        data.extend_from_slice(&0u64.to_le_bytes());
        let mut instructions = vec![Instruction {
            program_id: Pubkey::default(),
            accounts: vec![
                AccountMeta::new(payer.pubkey(), true),
                AccountMeta::new(payer.pubkey(), false),
            ],
            data,
        }];
        if large {
            instructions.push(Instruction::new_with_bytes(
                Pubkey::from_str("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr")?,
                &vec![b'x'; 1200],
                vec![],
            ));
        }
        Ok(TxPlan {
            version: BuildVersion::V1,
            payer: payer.pubkey(),
            instructions,
            lookup_tables: vec![].into(),
            resources: Resources {
                compute_unit_limit: 1_400_000,
                priority_micro_lamports_per_cu: 0,
                loaded_accounts_data_size: Some(4 * 1024 * 1024),
            },
        })
    };
    let (small, over) = (plan_of(false)?, plan_of(true)?);
    let mut control = small.clone();
    control.version = BuildVersion::V0;
    let mut control = tx::build(&control, hash, &TxLimits::default())?;
    // Their fee exposure, quoted before any signature: the control's fee (one signature, no
    // priority fee) is each message's — the v1 ones carry the same signature and no priority.
    let solana_sdk::message::VersionedMessage::V0(quoted) = &control.tx.message else {
        return Err("TOOL_COST_UNKNOWN: the control is no v0 message".into());
    };
    let fee = rpc
        .get_fee_for_message(quoted)
        .map_err(|_| "TOOL_COST_UNKNOWN: the fee quote failed")?;
    // DEC-17f: one permit, one confirmation, for exactly these three messages.
    let messages = [
        tx::v1_message_bytes(&small, hash, &TxLimits::default())?,
        tx::v1_message_bytes(&over, hash, &TxLimits::default())?,
        control.tx.message.serialize(),
    ];
    let exposure = fee
        .checked_mul(messages.len() as u64)
        .ok_or("TOOL_COST_UNKNOWN")?;
    let project = std::path::PathBuf::from(env::var("P0_PROJECT_DIR")?);
    let mut permit = tool_permit::issue(
        &project,
        darkbot_solana::runtime_config::active(),
        &ToolUse {
            tool: Tool::ProbeV1,
            signers: &[payer.pubkey()],
            messages: &messages.iter().map(Vec::as_slice).collect::<Vec<_>>(),
            send: false,
            cost_lamports: exposure,
        },
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
        &tool_permit::operator_confirmation(&project, Tool::ProbeV1),
    )?;
    let mut results = Vec::new();
    for (large, plan) in [(false, &small), (true, &over)] {
        let wire = tx::signed_v1_wire_with(plan, hash, &TxLimits::default(), &mut |m| {
            permit.sign(m, &[&payer])
        })?;
        let response: Result<serde_json::Value, _> = rpc.send(
            RpcRequest::SimulateTransaction,
            serde_json::json!([base64::engine::general_purpose::STANDARD.encode(&wire), {
                "encoding":"base64", "sigVerify":true, "replaceRecentBlockhash":false,
                "commitment":"confirmed"
            }]),
        );
        // Do not include transport URLs/API keys in evidence.
        let result = match response {
            Ok(value) => serde_json::json!({"response": value}),
            Err(e) => match e.kind() {
                solana_client::client_error::ClientErrorKind::RpcError(e) => {
                    serde_json::json!({"rpc_error":e.to_string()})
                }
                _ => serde_json::json!({"transport_or_decode_error":true}),
            },
        };
        results.push(
            serde_json::json!({"version":"V1", "case":if large {"over_legacy_limit"} else {"small"},
            "wire_bytes":wire.len(),"sig_verify":true,"result":result}),
        );
        if !large {
            control.tx.signatures = permit.sign(&messages[2], &[&payer])?;
            let result = rpc
                .simulate_transaction_with_config(
                    &control.tx,
                    solana_client::rpc_config::RpcSimulateTransactionConfig {
                        sig_verify: true,
                        commitment: Some(CommitmentConfig::confirmed()),
                        ..Default::default()
                    },
                )
                .map_err(|_| "v0 control RPC failed")?;
            results.push(serde_json::json!({"version":"V0","case":"control","response":result}));
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"probe":"v1_canonical_wire","transactions_sent":0,"payer":payer.pubkey().to_string(),"results":results})
        )?
    );
    Ok(())
}
