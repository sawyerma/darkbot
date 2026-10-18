//! Explicit read-only provider capability probe; never calls sendTransaction.
use base64::Engine;
use darkbot_solana::tx::{self, BuildVersion, Resources, TxLimits, TxPlan};
use solana_client::{rpc_client::RpcClient, rpc_request::RpcRequest};
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey, signature::Signer};
use std::{env, error::Error, str::FromStr};

fn main() -> Result<(), Box<dyn Error>> {
    let rpc =
        RpcClient::new_with_commitment(env::var("P0_RPC_URL")?, CommitmentConfig::confirmed());
    let payer = tx::load_keypair_json(&env::var("P0_KEYPAIR_PATH")?)?;
    let expected = Pubkey::from_str(&env::var("P0_EXPECTED_PUBKEY")?)?;
    if payer.pubkey() != expected {
        return Err("signer identity mismatch".into());
    }
    let mut results = Vec::new();
    for large in [false, true] {
        let hash = rpc
            .get_latest_blockhash()
            .map_err(|_| "blockhash RPC failed")?;
        let mut instructions = vec![solana_system_interface::instruction::transfer(
            &payer.pubkey(),
            &payer.pubkey(),
            0,
        )];
        if large {
            instructions.push(Instruction::new_with_bytes(
                Pubkey::from_str("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr")?,
                &vec![b'x'; 1200],
                vec![],
            ));
        }
        let plan = TxPlan {
            version: BuildVersion::V1,
            payer: payer.pubkey(),
            instructions,
            lookup_tables: vec![],
            resources: Resources {
                compute_unit_limit: 1_400_000,
                priority_micro_lamports_per_cu: 0,
                loaded_accounts_data_size: Some(4 * 1024 * 1024),
            },
        };
        let wire = tx::signed_v1_wire(&plan, hash, &TxLimits::default(), &[&payer])?;
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
            let mut control = plan.clone();
            control.version = BuildVersion::V0;
            let mut built = tx::build(&control, hash, &TxLimits::default())?;
            tx::sign(&mut built, &[&payer])?;
            let result = rpc
                .simulate_transaction_with_config(
                    &built.tx,
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
