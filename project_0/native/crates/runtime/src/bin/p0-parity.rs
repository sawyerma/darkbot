//! Ticket 3 A4 offline differential (read-only): prints one canonical line per historical raw
//! source from the Rust decode, for comparison with the pinned Python verifier on the same bytes.
//!
//! `p0-parity tx FILE...`    `getTransaction` results (e.g. docs/evidence/AUDIT_20261005/*.json)
//! `p0-parity rows FILE...`  historical market `transactions.jsonl`: `full_slot_reconciliation`
//!                           block entries and `logsSubscribe` notifications (raw provider bytes)
use darkbot_runner::market_observer::parity::{logs_line, transaction_line};
use solana_sdk::pubkey::Pubkey;
use std::io::BufRead;
use std::str::FromStr;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let program = Pubkey::from_str(darkbot_project0::protocol::PROGRAM_ID_MAINNET).unwrap();
    let result = (|| -> Result<(), String> {
        match args.split_first() {
            Some((mode, files)) if mode == "tx" => {
                for f in files {
                    let raw: serde_json::Value =
                        serde_json::from_slice(&std::fs::read(f).map_err(|e| e.to_string())?)
                            .map_err(|e| format!("{f}: {e}"))?;
                    match darkbot_solana::chain::decode_transaction(&raw) {
                        Ok(tx) => println!("{}", transaction_line(&tx, &program)),
                        Err(e) => println!("DECODE_ERROR {f} {}", e.0),
                    }
                }
                Ok(())
            }
            Some((mode, files)) if mode == "rows" => {
                for f in files {
                    let file = std::fs::File::open(f).map_err(|e| e.to_string())?;
                    for line in std::io::BufReader::new(file).lines() {
                        let line = line.map_err(|e| e.to_string())?;
                        // A killed historical capture can end in a partial line: reported, kept as is.
                        let Ok(row) = serde_json::from_str::<serde_json::Value>(&line) else {
                            println!("TRUNCATED_LINE {f}");
                            continue;
                        };
                        let slot = row["slot"].as_u64().unwrap_or(0);
                        match row["source"].as_str() {
                            Some("full_slot_reconciliation") => {
                                let index = row["transaction_index"].as_u64();
                                match darkbot_solana::chain::decode_entry(
                                    &row["raw"],
                                    slot,
                                    index,
                                    None,
                                ) {
                                    Ok(tx) => println!("{}", transaction_line(&tx, &program)),
                                    Err(e) => println!("DECODE_ERROR {} {}", row["signature"], e.0),
                                }
                            }
                            Some("logsSubscribe") => {
                                match darkbot_solana::chain::decode_logs_notification(&row["raw"]) {
                                    Ok(l) => println!("{}", logs_line(&l, &program)),
                                    Err(e) => println!("DECODE_ERROR {} {}", row["signature"], e.0),
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Ok(())
            }
            _ => Err("USAGE: p0-parity tx FILE... | rows FILE...".into()),
        }
    })();
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(2);
    }
}
