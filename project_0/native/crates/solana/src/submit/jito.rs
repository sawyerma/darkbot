//! S3-04: Jito-Adapter (JSON-RPC, kostenlos; Tip nur bei Landing — VERIFY in P5).
//!
//! Endpunkte (VERIFY gegen aktuelle Jito-Doku):
//!   POST {block_engine}/api/v1/transactions  sendTransaction [tx_b64, {"encoding":"base64"}]
//!   POST {block_engine}/api/v1/bundles       sendBundle [[tx_b64,...], {"encoding":"base64"}]
//!   POST {block_engine}/api/v1/getTipAccounts getTipAccounts []
//!   POST {block_engine}/api/v1/getBundleStatuses [[bundle_id]]
//! Tip = SystemProgram-Transfer an einen Tip-Account innerhalb der (letzten) TX.
//! Bundle max. 5 TXs, atomar, sequenziell im selben Slot. Bundle-ID ≠ landed.
//!
//! Status: UNTESTED / VERIFY.

use super::{SubmissionAdapter, SubmissionPath, SubmitAck, SubmitError};
use serde_json::{json, Value};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_sdk::transaction::VersionedTransaction;
use std::str::FromStr;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BundleStatus {
    NotFound,
    Pending,
    Landed {
        slot: u64,
        confirmation: Option<String>,
    },
    Failed {
        slot: Option<u64>,
        error: String,
    },
}

pub struct JitoAdapter {
    http: reqwest::blocking::Client,
    block_engine: String,
    /// [FIX-32] Getrennter RPC für `simulateBundle` (Jito-Solana-RPC-Methode); None → Bundle fail closed.
    bundle_sim_rpc: Option<String>,
    /// Für Single-TX über sendTransaction; Bundles über `submit_bundle`.
    path: SubmissionPath,
}

impl JitoAdapter {
    pub fn new(
        block_engine_url: String,
        bundle_sim_rpc: Option<String>,
        timeout: Duration,
    ) -> Result<Self, SubmitError> {
        let http = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| SubmitError::Transport(super::rpc::safe_http_error(&e).into()))?;
        Ok(JitoAdapter {
            http,
            block_engine: block_engine_url.trim_end_matches('/').to_string(),
            bundle_sim_rpc: bundle_sim_rpc.map(|u| u.trim_end_matches('/').to_string()),
            path: SubmissionPath::JitoSingle,
        })
    }

    fn call(&self, route: &str, method: &str, params: Value) -> Result<Value, SubmitError> {
        let body = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params});
        let resp = self
            .http
            .post(format!("{}{}", self.block_engine, route))
            .json(&body)
            .send()
            .map_err(|e| SubmitError::Transport(super::rpc::safe_http_error(&e).into()))?;
        let resp = resp
            .error_for_status()
            .map_err(|e| SubmitError::Transport(super::rpc::safe_http_error(&e).into()))?;
        let v: Value = resp
            .json()
            .map_err(|e| SubmitError::Transport(super::rpc::safe_http_error(&e).into()))?;
        if let Some(err) = v.get("error") {
            return Err(SubmitError::Rejected(super::rpc::safe_remote_error(err)));
        }
        v.get("result")
            .cloned()
            .ok_or_else(|| SubmitError::Transport("no result".into()))
    }

    pub fn tip_accounts(&self) -> Result<Vec<Pubkey>, SubmitError> {
        let res = self.call("/api/v1/getTipAccounts", "getTipAccounts", json!([]))?;
        let arr = res
            .as_array()
            .ok_or_else(|| SubmitError::Transport("tip accounts not array".into()))?;
        arr.iter()
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| SubmitError::Transport("tip account not string".into()))
                    .and_then(|s| {
                        Pubkey::from_str(s).map_err(|e| SubmitError::Transport(e.to_string()))
                    })
            })
            .collect()
    }

    /// Atomares Bundle (≤ 5 TXs). Rückgabe = Bundle-ID (kein Landing-Beweis).
    pub fn submit_bundle(
        &self,
        txs: &[VersionedTransaction],
        t_handoff_ns: u64,
    ) -> Result<SubmitAck, SubmitError> {
        if txs.is_empty() || txs.len() > 5 {
            return Err(SubmitError::Rejected(format!(
                "bundle size {} not in 1..=5",
                txs.len()
            )));
        }
        // L2/9e: every transaction must carry its (non-default) fee-payer signature before any
        // network handoff; the sentinel is taken from this complete list, never by blind index.
        let sigs: Vec<Signature> = txs
            .iter()
            .filter_map(|t| t.signatures.first().copied())
            .filter(|s| *s != Signature::default())
            .collect();
        if sigs.len() != txs.len() {
            return Err(SubmitError::Rejected("BUNDLE_SIGNATURE_MISSING".into()));
        }
        // [FIX-18] Sentinel = Receivership-TX (vorletzte, wenn Tip zuletzt), alle Signaturen mitführen.
        let sentinel = sigs[sigs.len().saturating_sub(2)];
        let encoded: Result<Vec<String>, _> = txs.iter().map(super::encode_base64).collect();
        let encoded = encoded?;
        let total_len: usize = txs
            .iter()
            .filter_map(|t| super::serialize_tx(t).ok())
            .map(|b| b.len())
            .sum();
        let res = self.call(
            "/api/v1/bundles",
            "sendBundle",
            json!([encoded, {"encoding":"base64"}]),
        )?;
        let bundle_id = res.as_str().map(|s| s.to_string());
        Ok(SubmitAck {
            path: SubmissionPath::JitoBundle,
            signature: sentinel,
            signatures: sigs,
            submission_id: bundle_id,
            t_handoff_ns,
            serialized_len: total_len,
        })
    }

    /// [FIX-33] Inflight-Status (schneller Pending/Failed-Hinweis; on-chain bleibt authoritative). VERIFY Format.
    pub fn inflight_status(&self, bundle_id: &str) -> Result<BundleStatus, SubmitError> {
        let res = self.call(
            "/api/v1/getInflightBundleStatuses",
            "getInflightBundleStatuses",
            json!([[bundle_id]]),
        )?;
        let entry = res.pointer("/value/0").cloned().unwrap_or(Value::Null);
        Ok(match entry.get("status").and_then(|s| s.as_str()) {
            // 9e: a landing without its slot is not yet a landing (never slot 0).
            Some("Landed") => match entry.get("landed_slot").and_then(|s| s.as_u64()) {
                Some(slot) => BundleStatus::Landed {
                    slot,
                    confirmation: None,
                },
                None => BundleStatus::Pending,
            },
            Some("Failed") => BundleStatus::Failed {
                slot: None,
                error: "inflight: Failed".into(),
            },
            Some("Pending") => BundleStatus::Pending,
            _ => BundleStatus::NotFound,
        })
    }

    /// Bundle-Status normalisiert. VERIFY Antwortformat: result.value[0] = {bundle_id, transactions, slot, confirmation_status, err}.
    /// Bundle-ID ≠ landed: nur `confirmation_status` + fehlender `err` zählt als gelandet.
    pub fn bundle_status(&self, bundle_id: &str) -> Result<BundleStatus, SubmitError> {
        let res = self.call(
            "/api/v1/getBundleStatuses",
            "getBundleStatuses",
            json!([[bundle_id]]),
        )?;
        let entry = res.pointer("/value/0").cloned().unwrap_or(Value::Null);
        if entry.is_null() {
            return Ok(BundleStatus::NotFound);
        }
        let err = entry
            .get("err")
            .filter(|e| !e.is_null() && *e != &json!({"Ok": null}));
        let slot = entry.get("slot").and_then(|s| s.as_u64());
        let conf = entry
            .get("confirmation_status")
            .and_then(|c| c.as_str())
            .filter(|c| matches!(*c, "processed" | "confirmed" | "finalized"))
            .map(str::to_owned);
        match (err, slot) {
            (Some(e), slot) => Ok(BundleStatus::Failed {
                slot,
                error: super::rpc::safe_remote_error(e),
            }),
            (None, Some(slot)) => Ok(BundleStatus::Landed {
                slot,
                confirmation: conf,
            }),
            (None, None) => Ok(BundleStatus::Pending),
        }
    }
}

impl SubmissionAdapter for JitoAdapter {
    fn path(&self) -> SubmissionPath {
        self.path
    }

    fn supports_bundle(&self) -> bool {
        true
    } // [FIX-14]

    /// [FIX-4] Bundle über den Block-Engine-Pfad (Tip-TX muss enthalten sein).
    fn submit_bundle(
        &self,
        txs: &[VersionedTransaction],
        t_handoff_ns: u64,
    ) -> Result<SubmitAck, SubmitError> {
        JitoAdapter::submit_bundle(self, txs, t_handoff_ns)
    }

    /// [FIX-15] `simulateBundle` (Jito-RPC-Methode; VERIFY Endpunkt/Antwortformat). Qualifiziert die
    /// sequenzielle Ausführung aller TXs; jeder Fehler → Failed; nicht erreichbar → Unavailable.
    fn simulate_bundle(&self, txs: &[VersionedTransaction]) -> super::SimResult {
        let encoded: Result<Vec<String>, _> = txs.iter().map(super::encode_base64).collect();
        let Ok(encoded) = encoded else {
            return super::SimResult::Unavailable("encode".into());
        };
        let Some(sim) = &self.bundle_sim_rpc else {
            return super::SimResult::Unavailable(
                "no bundle simulator configured (P0_BUNDLE_SIM_RPC_URL)".into(),
            );
        };
        let body = json!({"jsonrpc":"2.0","id":1,"method":"simulateBundle","params":[{"encodedTransactions": encoded}]});
        let call: Result<Value, String> = self
            .http
            .post(sim)
            .json(&body)
            .send()
            .map_err(|e| super::rpc::safe_http_error(&e).to_string())
            .and_then(|r| {
                r.error_for_status()
                    .map_err(|e| super::rpc::safe_http_error(&e).to_string())
            })
            .and_then(|r| {
                r.json::<Value>()
                    .map_err(|e| super::rpc::safe_http_error(&e).to_string())
            })
            .and_then(|v| {
                if let Some(err) = v.get("error") {
                    Err(super::rpc::safe_remote_error(err))
                } else {
                    v.get("result")
                        .cloned()
                        .ok_or_else(|| "no result".to_string())
                }
            });
        match call.map_err(super::SubmitError::Transport) {
            Ok(v) => {
                let summary = v.pointer("/value/summary").cloned().unwrap_or(Value::Null);
                if summary.as_str() == Some("succeeded") {
                    let cu = v
                        .pointer("/value/transactionResults")
                        .and_then(|r| r.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|t| t.get("unitsConsumed").and_then(|u| u.as_u64()))
                                .sum()
                        });
                    super::SimResult::Ok { units_consumed: cu }
                } else {
                    super::SimResult::Failed("BUNDLE_SIMULATION_FAILED".into())
                }
            }
            Err(e) => super::SimResult::Unavailable(e.to_string()),
        }
    }

    fn submit(
        &self,
        tx: &VersionedTransaction,
        t_handoff_ns: u64,
    ) -> Result<SubmitAck, SubmitError> {
        let b64 = super::encode_base64(tx)?;
        let len = super::serialize_tx(tx)
            .map(|b| b.len())
            .map_err(|e| SubmitError::Serialize(e.to_string()))?;
        let res = self.call(
            "/api/v1/transactions",
            "sendTransaction",
            json!([b64, {"encoding":"base64"}]),
        )?;
        let sig_s = res
            .as_str()
            .ok_or_else(|| SubmitError::Transport("signature not string".into()))?;
        let sig = Signature::from_str(sig_s).map_err(|e| SubmitError::Transport(e.to_string()))?;
        Ok(SubmitAck {
            path: SubmissionPath::JitoSingle,
            signature: sig,
            signatures: vec![sig],
            submission_id: None,
            t_handoff_ns,
            serialized_len: len,
        })
    }
}

#[cfg(test)]
mod secret_tests {
    use super::*;
    /// L2/9e: an unsigned or signature-less transaction is refused before any HTTP request.
    #[test]
    fn unsigned_bundle_is_rejected_without_network() {
        use solana_sdk::message::{Message, VersionedMessage};
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let url = format!("http://{}", socket.local_addr().unwrap());
        let a = JitoAdapter::new(url.clone(), Some(url), Duration::from_millis(500)).unwrap();
        let tx = |signatures: Vec<Signature>| VersionedTransaction {
            signatures,
            message: VersionedMessage::Legacy(Message::default()),
        };
        let signed = tx(vec![Signature::from([7u8; 64])]);
        for bundle in [
            vec![tx(vec![Signature::default()])],
            vec![signed.clone(), tx(vec![])],
            vec![tx(vec![]), signed.clone()],
        ] {
            let error = a.submit_bundle(&bundle, 0).unwrap_err();
            assert!(
                matches!(&error, SubmitError::Rejected(c) if c == "BUNDLE_SIGNATURE_MISSING"),
                "{error:?}"
            );
        }
        assert!(socket.accept().is_err(), "no request may reach the network");
    }
    #[test]
    fn jito_transport_and_simulation_do_not_expose_endpoint() {
        let url = "http://127.0.0.1:0/private?api-key=SECRET";
        let a = JitoAdapter::new(url.into(), Some(url.into()), Duration::from_millis(100)).unwrap();
        let error = a.tip_accounts().unwrap_err().to_string();
        assert!(!error.contains("SECRET") && !error.contains("127.0.0.1"));
        let result = a.simulate_bundle(&[]);
        assert!(matches!(result, super::super::SimResult::Unavailable(_)));
        assert!(!format!("{result:?}").contains("SECRET"));
        let remote =
            super::super::rpc::safe_remote_error(&json!({"code":-32000,"message":url,"data":url}));
        assert_eq!(remote, "RPC_RESPONSE code=-32000");
    }
}
