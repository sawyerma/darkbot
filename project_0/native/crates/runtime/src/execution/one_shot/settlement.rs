//! C3/P8-05: finalized settlement of the one durable FE-04 attempt, in the same wallet journal
//! (no second financial store). Receipts come from the durable finalized observations of every
//! bound signature (`reconcile_observed`) or a strict non-landing proof (`reconcile_not_landed`);
//! the realized i128 net PnL is booked exactly once as a journal transition (`Phase::Settled`).
//! An incomplete bundle, an observation with unknown reasons, a missing proof or any delta without
//! a conservative value (inventory whose exit is not proven) stays UNKNOWN: holds and HALT remain.
use super::{err, Intent, Journal, Phase, Receipt, Transition};
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use std::collections::{BTreeMap, BTreeSet};

/// The booked outcome of one attempt (USD micro where named so; lamports and base units else).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settlement {
    /// Some own transaction landed (false: a proven non-landing, nothing was paid).
    pub landed: bool,
    /// Every own transaction landed and succeeded.
    pub landed_success: bool,
    /// Network fees of all own transactions; already inside the SOL delta.
    pub fee_lamports: u64,
    /// Wallet deltas summed over all own transactions; SOL includes fees, tip and rent.
    pub wallet_deltas: BTreeMap<String, i128>,
    /// The conservative USD-micro value of every non-zero delta.
    pub valuation: BTreeMap<String, i128>,
    pub realized_net_usd_micro: i128,
}

/// C12: a strict non-landing proof in the domain of blockhash validity, block height: a finalized
/// block height beyond the signed blockhash's `lastValidBlockHeight` was observed first, then
/// every bound signature was absent from a history that reaches back to the intent's state slot.
/// (A slot bound proves nothing: skipped slots let a later slot carry a lower block height.)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NonLandingProof {
    pub finalized_block_height: u64,
    /// The oldest slot the queried node's history covers (`minimumLedgerSlot`).
    pub history_from_slot: u64,
    pub absent_signatures: BTreeSet<String>,
}

/// C12: acquires the proof through the reconciliation RPC — the finalized block height first,
/// then the history floor, then the bound signatures with history search. An unknown expiry (an
/// intent before C12), an unexpired blockhash, a history starting after the intent, any status
/// (landed or pending: it must be observed instead) or any failed call is no proof.
pub fn non_landing_proof(rpc: &RpcClient, intent: &Intent) -> Result<NonLandingProof, String> {
    let expiry = intent
        .last_valid_block_height
        .ok_or_else(|| err("NON_LANDING_EXPIRY_UNKNOWN"))?;
    let height = rpc
        .get_block_height_with_commitment(CommitmentConfig::finalized())
        .map_err(|_| err("NON_LANDING_HEIGHT_UNAVAILABLE"))?;
    if height <= expiry {
        return Err(err("NON_LANDING_BLOCKHASH_NOT_EXPIRED"));
    }
    let floor = rpc
        .minimum_ledger_slot()
        .map_err(|_| err("NON_LANDING_HISTORY_UNAVAILABLE"))?;
    if floor > intent.state_slot {
        return Err(err("NON_LANDING_HISTORY_INCOMPLETE"));
    }
    let signatures = intent
        .signatures
        .iter()
        .map(|s| s.parse().map_err(|_| err("SIGNATURE_BINDING_INVALID")))
        .collect::<Result<Vec<solana_sdk::signature::Signature>, _>>()?;
    let statuses = rpc
        .get_signature_statuses_with_history(&signatures)
        .map_err(|_| err("NON_LANDING_STATUS_UNAVAILABLE"))?
        .value;
    if statuses.len() != signatures.len() {
        return Err(err("NON_LANDING_STATUS_UNAVAILABLE"));
    }
    if statuses.iter().any(Option::is_some) {
        return Err(err("NON_LANDING_SIGNATURE_SEEN"));
    }
    Ok(NonLandingProof {
        finalized_block_height: height,
        history_from_slot: floor,
        absent_signatures: intent.signatures.iter().cloned().collect(),
    })
}

/// FE-04/C3: the conservative value of a signed settlement delta from a low and a high value of
/// a native amount: a gain at its lower bound, a loss at its upper bound (negative). Unknown (no
/// value, an amount beyond `u64`) is `None`: the settlement stays UNKNOWN.
pub fn conservative_value(
    low: &dyn Fn(&str, u64) -> Option<u64>,
    high: &dyn Fn(&str, u64) -> Option<u64>,
    asset: &str,
    delta: i128,
) -> Option<i128> {
    let amount = u64::try_from(delta.unsigned_abs()).ok()?;
    if delta >= 0 {
        low(asset, amount).map(i128::from)
    } else {
        high(asset, amount).map(|v| -i128::from(v))
    }
}

/// The outcome of one settlement step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettleStep {
    /// Booked once (also when it already was): holds are released by the finalized phase.
    Settled(Settlement),
    /// Still UNKNOWN: every hold stays; the reasons of this step.
    Unknown(Vec<String>),
}

impl Journal {
    /// Finalizes from the durable observations: every bound signature (prerequisites,
    /// receivership, tip) observed finalized, without unknown reasons and with its token deltas.
    /// Anything less keeps the attempt UNKNOWN (partial bundle, missing archive, 429).
    pub fn reconcile_observed(&mut self) -> Result<(), String> {
        let saved = self.saved.as_ref().ok_or_else(|| err("NO_ATTEMPT"))?;
        if saved.phase != Phase::HandoffUnknown {
            return Err(err("NOT_UNKNOWN"));
        }
        let mut receipts = Vec::with_capacity(saved.intent.signatures.len());
        for signature in &saved.intent.signatures {
            let o = saved
                .observations
                .get(signature)
                .ok_or_else(|| err("SETTLEMENT_SIGNATURE_UNOBSERVED_HOLD_RETAINED"))?;
            if !o.unknown_reasons.is_empty() {
                return Err(err("SETTLEMENT_OBSERVATION_UNKNOWN_HOLD_RETAINED"));
            }
            let mut deltas = o
                .wallet_token_deltas
                .clone()
                .ok_or_else(|| err("SETTLEMENT_TOKEN_DELTAS_UNKNOWN_HOLD_RETAINED"))?;
            deltas.insert("SOL".into(), o.wallet_sol_delta_lamports);
            receipts.push(Receipt {
                signature: signature.clone(),
                finalized: true,
                success: o.success,
                fee_lamports: Some(o.fee_lamports),
                wallet_deltas: Some(deltas),
                exit_complete: true,
            });
        }
        self.reconcile(receipts)
    }

    /// Finalizes a proven non-landing: nothing landed, nothing was paid.
    pub fn reconcile_not_landed(&mut self, proof: &NonLandingProof) -> Result<(), String> {
        let mut s = self.saved.clone().ok_or_else(|| err("NO_ATTEMPT"))?;
        if s.phase != Phase::HandoffUnknown {
            return Err(err("NOT_UNKNOWN"));
        }
        let bound: BTreeSet<String> = s.intent.signatures.iter().cloned().collect();
        // C12: an intent without its blockhash expiry (schema 2) can never be proven absent.
        if s.intent
            .last_valid_block_height
            .is_none_or(|expiry| proof.finalized_block_height <= expiry)
            || proof.history_from_slot > s.intent.state_slot
            || proof.absent_signatures != bound
            || !s.observations.is_empty()
        {
            return Err(err("NON_LANDING_NOT_PROVEN_HOLD_RETAINED"));
        }
        s.phase = Phase::HaltFinalized;
        s.finalized = Some(Vec::new());
        self.persist(s, Transition::NotLanded)
    }

    /// FE-04/C3: one settlement step of the attempt. Every bound signature observed finalized
    /// without unknown reasons ⇒ reconcile and book (`value`); nothing observed ⇒ a strict
    /// non-landing proof finalizes it (booked with nothing paid); anything partial or unproven
    /// stays UNKNOWN with its reasons (holds retained). Idempotent across steps and restarts.
    pub fn settle_step(
        &mut self,
        rpc: &RpcClient,
        value: &dyn Fn(&str, i128) -> Option<i128>,
    ) -> Result<SettleStep, String> {
        let phase = self
            .saved
            .as_ref()
            .ok_or_else(|| err("NO_ATTEMPT"))?
            .phase
            .clone();
        match phase {
            Phase::Settled(done) => return Ok(SettleStep::Settled(done)),
            Phase::HaltFinalized => return self.settle(value).map(SettleStep::Settled),
            Phase::Reserved => return Ok(SettleStep::Unknown(vec![err("NOT_HANDED_OFF")])),
            Phase::HandoffUnknown => {}
        }
        let mut reasons = self.observe_finalized(rpc)?;
        let saved = self.saved.as_ref().ok_or_else(|| err("NO_ATTEMPT"))?;
        if reasons.is_empty() && saved.observations.len() == saved.intent.signatures.len() {
            self.reconcile_observed()?;
            return self.settle(value).map(SettleStep::Settled);
        }
        if saved.observations.is_empty() {
            match non_landing_proof(rpc, &saved.intent) {
                Ok(proof) => {
                    self.reconcile_not_landed(&proof)?;
                    return self.settle(value).map(SettleStep::Settled);
                }
                Err(reason) => reasons.push(reason),
            }
        }
        Ok(SettleStep::Unknown(reasons))
    }

    /// Books the realized net PnL of a finalized attempt exactly once. `value(asset, delta)` is
    /// the conservative USD-micro value of a signed delta (`None`: not valued, e.g. inventory
    /// without a proven exit); a booked settlement is returned unchanged, never booked twice.
    pub fn settle(
        &mut self,
        value: &dyn Fn(&str, i128) -> Option<i128>,
    ) -> Result<Settlement, String> {
        let s = self.saved.clone().ok_or_else(|| err("NO_ATTEMPT"))?;
        let receipts = match &s.phase {
            Phase::Settled(done) => return Ok(done.clone()),
            Phase::HaltFinalized => s.finalized.clone().ok_or_else(|| err("NOT_FINALIZED"))?,
            _ => return Err(err("NOT_FINALIZED_HOLD_RETAINED")),
        };
        let settlement = compute(&receipts, value)?;
        let mut next = s;
        next.phase = Phase::Settled(settlement.clone());
        self.persist(next, Transition::Settle)?;
        Ok(settlement)
    }
}

fn compute(
    receipts: &[Receipt],
    value: &dyn Fn(&str, i128) -> Option<i128>,
) -> Result<Settlement, String> {
    let overflow = || err("SETTLEMENT_OVERFLOW");
    let mut deltas: BTreeMap<String, i128> = BTreeMap::new();
    let mut fee = 0u64;
    for r in receipts {
        fee = fee
            .checked_add(
                r.fee_lamports
                    .ok_or_else(|| err("SETTLEMENT_FEE_UNKNOWN"))?,
            )
            .ok_or_else(overflow)?;
        for (asset, d) in r
            .wallet_deltas
            .as_ref()
            .ok_or_else(|| err("SETTLEMENT_DELTAS_UNKNOWN"))?
        {
            let total = deltas.entry(asset.clone()).or_default();
            *total = total.checked_add(*d).ok_or_else(overflow)?;
        }
    }
    let mut valuation = BTreeMap::new();
    let mut net: i128 = 0;
    for (asset, d) in deltas.iter().filter(|(_, d)| **d != 0) {
        let v = value(asset, *d).ok_or_else(|| format!("SETTLEMENT_VALUE_UNKNOWN:{asset}"))?;
        if v.signum() * d.signum() < 0 {
            return Err(format!("SETTLEMENT_VALUE_SIGN:{asset}"));
        }
        net = net.checked_add(v).ok_or_else(overflow)?;
        valuation.insert(asset.clone(), v);
    }
    Ok(Settlement {
        landed: !receipts.is_empty(),
        landed_success: !receipts.is_empty() && receipts.iter().all(|r| r.success),
        fee_lamports: fee,
        wallet_deltas: deltas,
        valuation,
        realized_net_usd_micro: net,
    })
}

#[cfg(test)]
mod tests {
    use super::super::receipts::FinalizedObservation;
    use super::super::tests::{path_of, setup};
    use super::super::*;
    use super::{non_landing_proof, NonLandingProof, SettleStep, Settlement};
    use serde_json::{json, Value};
    use std::fs;
    use std::path::PathBuf;

    /// The journal after reserve and handoff of `n` bound signatures.
    fn handed_off(n: usize) -> (PathBuf, Journal, Intent) {
        let (path, a, mut i, mut c) = setup();
        i.signatures = (0..n)
            .map(|_| solana_sdk::signature::Signature::new_unique().to_string())
            .collect();
        c.bundle_simulated = n > 1;
        let mut j = Journal::open(&path, &a.wallet).unwrap();
        j.reserve(a, i.clone(), &c).unwrap();
        j.handoff(&i.signed_wire_sha256, &c, || Ok(())).unwrap();
        (path, j, i)
    }

    fn observe(
        j: &mut Journal,
        signature: &str,
        success: bool,
        sol: i128,
        mint: Option<(&str, i128)>,
    ) {
        let mut s = j.saved.clone().unwrap();
        s.observations.insert(
            signature.into(),
            FinalizedObservation {
                signature: signature.into(),
                slot: 500,
                success,
                fee_lamports: 5_000,
                wallet_sol_delta_lamports: sol,
                wallet_token_deltas: Some(
                    mint.map(|(m, d)| (m.to_string(), d)).into_iter().collect(),
                ),
                unknown_reasons: vec![],
            },
        );
        j.persist(s, Transition::Observe).unwrap();
    }

    /// USD micro: 1 lamport = 0.1, 1 token base unit = 1 (positive and negative alike here).
    fn value(asset: &str, d: i128) -> Option<i128> {
        match asset {
            "SOL" => Some(d / 10),
            "MINT" => Some(d),
            _ => None,
        }
    }

    /// C3: a landed success books its realized net from the finalized deltas of every own
    /// transaction once — durable across a restart, never twice; holds are released only then.
    #[test]
    fn landed_success_books_the_realized_net_once() {
        let (path, mut j, i) = handed_off(2);
        let (s1, s2) = (&i.signatures[0], &i.signatures[1]);
        observe(&mut j, s1, true, -5_000, Some(("MINT", -1_000)));
        observe(&mut j, s2, true, -15_000, Some(("MINT", 1_600)));
        assert_eq!(j.settle(&value).unwrap_err(), "NOT_FINALIZED_HOLD_RETAINED");
        j.reconcile_observed().unwrap();
        let s = j.settle(&value).unwrap();
        assert!(s.landed && s.landed_success);
        assert_eq!(s.fee_lamports, 10_000);
        assert_eq!(s.wallet_deltas["SOL"], -20_000);
        assert_eq!(s.wallet_deltas["MINT"], 600);
        assert_eq!(s.realized_net_usd_micro, 600 - 2_000);
        assert!(j.active_hold().is_none() && j.holds().is_empty());
        // Idempotent: the same settlement, no second booking; after a restart as well.
        assert_eq!(j.settle(&|_, _| None).unwrap(), s);
        drop(j);
        let mut reopened = Journal::open(&path, &i.wallet).unwrap();
        assert_eq!(reopened.settle(&|_, _| None).unwrap(), s);
        assert!(
            reopened.reconcile_observed().is_err(),
            "no second finalization"
        );
        fs::remove_dir_all(path).unwrap();
    }

    /// C3: a landed failure books the paid fees as a realized loss.
    #[test]
    fn landed_failure_books_the_paid_fee_as_loss() {
        let (path, mut j, i) = handed_off(1);
        observe(&mut j, &i.signatures[0], false, -5_000, Some(("MINT", 0)));
        j.reconcile_observed().unwrap();
        let s = j.settle(&value).unwrap();
        assert!(s.landed && !s.landed_success);
        assert_eq!(s.realized_net_usd_micro, -500);
        fs::remove_dir_all(path).unwrap();
    }

    /// C3: UNKNOWN stays UNKNOWN — a partially observed bundle, an observation with unknown
    /// reasons or an unvalued delta (inventory without a proven exit) never settles; the hold
    /// and the attempt stay.
    #[test]
    fn incomplete_evidence_never_settles_and_keeps_the_hold() {
        let (path, mut j, i) = handed_off(2);
        let (s1, s2) = (&i.signatures[0], &i.signatures[1]);
        observe(&mut j, s1, true, -5_000, Some(("MINT", 10)));
        assert_eq!(
            j.reconcile_observed().unwrap_err(),
            "SETTLEMENT_SIGNATURE_UNOBSERVED_HOLD_RETAINED",
            "partial bundle"
        );
        let mut s = j.saved.clone().unwrap();
        s.observations.get_mut(s1).unwrap().unknown_reasons = vec!["ARCHIVE_429".into()];
        j.persist(s, Transition::Observe).unwrap();
        observe(&mut j, s2, true, -5_000, Some(("MINT", 10)));
        assert_eq!(
            j.reconcile_observed().unwrap_err(),
            "SETTLEMENT_OBSERVATION_UNKNOWN_HOLD_RETAINED"
        );
        assert!(j.active_hold().is_some(), "UNKNOWN keeps the hold");
        observe(&mut j, s1, true, -5_000, Some(("OTHER", 10)));
        j.reconcile_observed().unwrap();
        assert_eq!(
            j.settle(&value).unwrap_err(),
            "SETTLEMENT_VALUE_UNKNOWN:OTHER",
            "inventory without a proven exit"
        );
        assert_eq!(j.saved.as_ref().unwrap().phase, Phase::HaltFinalized);
        let wrong_sign = |_: &str, d: i128| Some(-d);
        assert!(j
            .settle(&wrong_sign)
            .unwrap_err()
            .starts_with("SETTLEMENT_VALUE_SIGN"));
        fs::remove_dir_all(path).unwrap();
    }

    /// C3: a receipt that is not finalized (a reorg can still remove it) or a duplicated receipt
    /// event never finalizes; the attempt stays UNKNOWN with its hold.
    #[test]
    fn unfinalized_or_duplicated_receipts_never_finalize() {
        let (path, mut j, i) = handed_off(1);
        let receipt = |finalized| Receipt {
            signature: i.signatures[0].clone(),
            finalized,
            success: true,
            fee_lamports: Some(5_000),
            wallet_deltas: Some([("SOL".to_string(), -5_000)].into_iter().collect()),
            exit_complete: true,
        };
        let held = "RECONCILIATION_INCOMPLETE_HOLD_RETAINED";
        assert_eq!(
            j.reconcile(vec![receipt(false)]).unwrap_err(),
            held,
            "confirmed, not finalized"
        );
        assert_eq!(
            j.reconcile(vec![receipt(true), receipt(true)]).unwrap_err(),
            held,
            "duplicated receipt event"
        );
        assert!(j.active_hold().is_some_and(|h| h.unknown_outcome));
        assert_eq!(j.settle(&value).unwrap_err(), "NOT_FINALIZED_HOLD_RETAINED");
        fs::remove_dir_all(path).unwrap();
    }

    /// C3/C12: a non-landing needs a finalized block height past the signed blockhash's
    /// `lastValidBlockHeight`, a history reaching back to the intent's slot and every bound
    /// signature absent; then it settles at zero (nothing paid). Anything less keeps the hold.
    #[test]
    fn not_landed_settles_only_with_a_strict_proof() {
        let (path, mut j, i) = handed_off(2);
        let expiry = i.last_valid_block_height.unwrap();
        let proof = |height, floor, sigs: &[String]| NonLandingProof {
            finalized_block_height: height,
            history_from_slot: floor,
            absent_signatures: sigs.iter().cloned().collect(),
        };
        let reject = "NON_LANDING_NOT_PROVEN_HOLD_RETAINED";
        let all = &i.signatures[..];
        for bad in [
            proof(expiry, 0, all),
            proof(expiry + 1, 0, &all[..1]),
            proof(expiry + 1, i.state_slot + 1, all),
        ] {
            assert_eq!(j.reconcile_not_landed(&bad).unwrap_err(), reject);
        }
        assert!(j.active_hold().is_some());
        // A schema-2 intent (no recorded expiry) is never proven absent.
        let mut old = j.saved.clone().unwrap();
        old.intent.last_valid_block_height = None;
        let saved = j.saved.replace(old);
        assert_eq!(
            j.reconcile_not_landed(&proof(u64::MAX, 0, all))
                .unwrap_err(),
            reject
        );
        j.saved = saved;
        j.reconcile_not_landed(&proof(expiry + 1, i.state_slot, all))
            .unwrap();
        let s = j.settle(&value).unwrap();
        assert_eq!(
            (s.landed, s.realized_net_usd_micro, s.fee_lamports),
            (false, 0, 0)
        );
        assert!(j.active_hold().is_none());
        // A landed signature can never be proven absent.
        let (path2, mut k, i2) = handed_off(1);
        observe(&mut k, &i2.signatures[0], true, -5_000, Some(("MINT", 1)));
        assert_eq!(
            k.reconcile_not_landed(&proof(u64::MAX, 0, &i2.signatures))
                .unwrap_err(),
            reject
        );
        fs::remove_dir_all(path).unwrap();
        fs::remove_dir_all(path2).unwrap();
    }

    /// A loopback JSON-RPC endpoint answering each method with its scripted `result` (an
    /// unscripted method gets an error); the methods in call order.
    fn rpc(
        answers: Vec<(&'static str, Value)>,
    ) -> (
        solana_client::rpc_client::RpcClient,
        std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    ) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = calls.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let (answers, log) = (answers.clone(), log.clone());
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut stream = stream;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 {
                            return;
                        }
                        let mut length = 0usize;
                        loop {
                            let mut h = String::new();
                            if reader.read_line(&mut h).unwrap_or(0) == 0 {
                                return;
                            }
                            if h == "\r\n" {
                                break;
                            }
                            if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:")
                            {
                                length = v.trim().parse().unwrap_or(0);
                            }
                        }
                        let mut body = vec![0u8; length];
                        if reader.read_exact(&mut body).is_err() {
                            return;
                        }
                        let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                        let method = request["method"].as_str().unwrap_or("").to_string();
                        log.lock().unwrap().push(method.clone());
                        let answer = match answers.iter().find(|(m, _)| *m == method) {
                            Some((_, r)) => {
                                json!({"jsonrpc": "2.0", "id": request["id"], "result": r})
                            }
                            None => json!({"jsonrpc": "2.0", "id": request["id"],
                                "error": {"code": -32601, "message": "unscripted"}}),
                        }
                        .to_string();
                        let _ = stream.write_all(
                            format!(
                                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                                answer.len(),
                                answer
                            )
                            .as_bytes(),
                        );
                    }
                });
            }
        });
        (solana_client::rpc_client::RpcClient::new(url), calls)
    }

    fn statuses(status: Value) -> (&'static str, Value) {
        (
            "getSignatureStatuses",
            json!({"context": {"slot": 100}, "value": [status]}),
        )
    }

    /// C12: the proof is read in this order — the finalized block height past the expiry, then
    /// the history floor, then the statuses — so absence is read only after the expiry is final;
    /// an unexpired blockhash, a history starting after the intent, a seen signature, a failed
    /// call or an unknown expiry is no proof.
    #[test]
    fn c12_non_landing_proof_comes_from_height_history_and_statuses() {
        let (_, _, i, _) = setup();
        let rpc_of = |height: u64, floor: u64, status: Value| {
            rpc(vec![
                ("getBlockHeight", json!(height)),
                ("minimumLedgerSlot", json!(floor)),
                statuses(status),
            ])
        };
        let (client, calls) = rpc_of(61, 50, Value::Null);
        assert_eq!(
            non_landing_proof(&client, &i),
            Ok(NonLandingProof {
                finalized_block_height: 61,
                history_from_slot: 50,
                absent_signatures: i.signatures.iter().cloned().collect(),
            })
        );
        assert_eq!(
            *calls.lock().unwrap(),
            [
                "getBlockHeight",
                "minimumLedgerSlot",
                "getSignatureStatuses"
            ]
        );
        let (client, calls) = rpc_of(60, 0, Value::Null);
        assert_eq!(
            non_landing_proof(&client, &i).unwrap_err(),
            "NON_LANDING_BLOCKHASH_NOT_EXPIRED"
        );
        assert_eq!(
            calls.lock().unwrap().len(),
            1,
            "nothing read before the expiry"
        );
        let (client, _) = rpc_of(61, 51, Value::Null);
        assert_eq!(
            non_landing_proof(&client, &i).unwrap_err(),
            "NON_LANDING_HISTORY_INCOMPLETE"
        );
        let seen = json!({"slot": 55, "confirmations": null, "err": null,
            "status": {"Ok": null}, "confirmationStatus": "finalized"});
        let (client, _) = rpc_of(61, 0, seen);
        assert_eq!(
            non_landing_proof(&client, &i).unwrap_err(),
            "NON_LANDING_SIGNATURE_SEEN"
        );
        let (client, _) = rpc(vec![]);
        assert_eq!(
            non_landing_proof(&client, &i).unwrap_err(),
            "NON_LANDING_HEIGHT_UNAVAILABLE"
        );
        let mut old = i.clone();
        old.last_valid_block_height = None;
        let (client, calls) = rpc_of(u64::MAX, 0, Value::Null);
        assert_eq!(
            non_landing_proof(&client, &old).unwrap_err(),
            "NON_LANDING_EXPIRY_UNKNOWN"
        );
        assert!(calls.lock().unwrap().is_empty());
    }

    /// C3: the settlement step through the reconciliation RPC (loopback). Never landed (no
    /// finalized transaction, the blockhash expired, history complete, absent) books nothing
    /// paid and releases the hold; still waiting (blockhash valid) stays UNKNOWN with both
    /// reasons, keeps the hold and writes no frame; a finalized landing books its deltas once.
    #[test]
    fn c3_settle_step_books_landed_and_not_landed_and_keeps_unknown() {
        let (path, mut j, _) = handed_off(1);
        let (client, _) = rpc(vec![
            ("getBlockHeight", json!(61)),
            ("minimumLedgerSlot", json!(0)),
            statuses(Value::Null),
        ]);
        let none = Settlement {
            landed: false,
            landed_success: false,
            fee_lamports: 0,
            wallet_deltas: Default::default(),
            valuation: Default::default(),
            realized_net_usd_micro: 0,
        };
        assert_eq!(
            j.settle_step(&client, &value),
            Ok(SettleStep::Settled(none.clone()))
        );
        assert!(j.active_hold().is_none());
        assert_eq!(
            j.settle_step(&client, &value),
            Ok(SettleStep::Settled(none))
        );

        let (path2, mut k, _) = handed_off(1);
        let file = path_of(&k);
        let frames = |f: &std::path::Path| {
            let mut r = crate::analytics::journal::Reader::open(f).unwrap();
            std::iter::from_fn(|| r.next_frame().unwrap()).count()
        };
        let before = frames(&file);
        let (client, _) = rpc(vec![
            ("getBlockHeight", json!(60)),
            ("minimumLedgerSlot", json!(0)),
            statuses(Value::Null),
        ]);
        let SettleStep::Unknown(reasons) = k.settle_step(&client, &value).unwrap() else {
            panic!("still waiting");
        };
        assert!(
            reasons
                .iter()
                .any(|r| r.ends_with("FINALIZED_TRANSACTION_UNAVAILABLE"))
                && reasons
                    .iter()
                    .any(|r| r == "NON_LANDING_BLOCKHASH_NOT_EXPIRED"),
            "{reasons:?}"
        );
        assert!(k.active_hold().is_some_and(|h| h.unknown_outcome));
        assert_eq!(frames(&file), before, "a waiting step writes no frame");

        let (path3, mut l, i3) = handed_off(1);
        let tx = json!({"slot": 55, "blockTime": 123, "transaction": {
            "signatures": [i3.signatures[0]],
            "message": {"header": {"numRequiredSignatures": 1, "numReadonlySignedAccounts": 0,
                "numReadonlyUnsignedAccounts": 0},
                "accountKeys": [i3.wallet, solana_sdk::pubkey::Pubkey::new_unique().to_string()],
                "recentBlockhash": solana_sdk::hash::Hash::new_unique().to_string(),
                "instructions": []}},
            "meta": {"err": null, "status": {"Ok": null}, "fee": 5000,
                "preBalances": [100000, 0], "postBalances": [95000, 0],
                "preTokenBalances": [], "postTokenBalances": [],
                "loadedAddresses": {"writable": [], "readonly": []}}});
        let (client, _) = rpc(vec![("getTransaction", tx)]);
        let SettleStep::Settled(s) = l.settle_step(&client, &value).unwrap() else {
            panic!("landed");
        };
        assert!(s.landed && s.landed_success);
        assert_eq!((s.fee_lamports, s.wallet_deltas["SOL"]), (5_000, -5_000));
        assert_eq!(s.realized_net_usd_micro, -500);
        assert!(l.active_hold().is_none());
        for p in [path, path2, path3] {
            fs::remove_dir_all(p).unwrap();
        }
    }

    /// FE-04/C3: a gain is valued at its lower bound, a loss at its upper bound; an unknown
    /// price or an amount beyond `u64` stays UNKNOWN.
    #[test]
    fn settlement_values_gains_low_and_losses_high() {
        let low = |a: &str, n: u64| (a == "MINT").then_some(n * 9 / 10);
        let high = |a: &str, n: u64| (a == "MINT").then_some(n * 11 / 10);
        let v = |d| super::conservative_value(&low, &high, "MINT", d);
        assert_eq!(
            (v(1_000), v(-1_000), v(0)),
            (Some(900), Some(-1_100), Some(0))
        );
        assert_eq!(super::conservative_value(&low, &high, "OTHER", 5), None);
        assert_eq!(v(i128::from(u64::MAX) + 1), None);
    }
}
