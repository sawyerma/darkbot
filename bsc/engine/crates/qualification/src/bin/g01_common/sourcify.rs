//! Read-only Sourcify v2 lookups shared by the G0.1 freeze binaries: the
//! answer is kept as opaque evidence bytes next to the facts decoded from
//! it, and a deployment transaction Sourcify names is checked on chain.

use super::bytecode::addr;
use super::edge::{self, Endpoint, SourcifyFacts};
use alloy_primitives::Address;
use darkbot_bsc_execution::rpc_wire::Limits;

#[derive(Clone, Debug)]
pub struct SourcifyObs {
    pub address: Address,
    pub response: Option<Vec<u8>>,
    pub facts: Result<SourcifyFacts, String>,
    pub deployment_receipt: String,
}

impl SourcifyObs {
    pub fn verified(&self) -> bool {
        matches!(&self.facts, Ok(f) if f.status != "NO_MATCH")
    }
}

/// One lookup of `address` on chain 56; `limits` bound the GET, `max_bytes`
/// the kept answer (`--sourcify-max-bytes`).
pub fn lookup_sourcify(
    rpc: &Endpoint,
    limits: Limits,
    max_bytes: u64,
    base: &str,
    address: Address,
) -> SourcifyObs {
    let url = edge::sourcify_url(base, edge::BSC_CHAIN_ID, address);
    let limits = Limits {
        compressed: false,
        ..limits
    };
    match edge::get(&url, limits, max_bytes) {
        Err(e) => SourcifyObs {
            address,
            response: None,
            facts: Err(e),
            deployment_receipt: "NOT_CHECKED".into(),
        },
        Ok((_, raw)) if raw.len() as u64 > max_bytes => SourcifyObs {
            address,
            response: None,
            facts: Err(format!(
                "answer of {} bytes exceeds --sourcify-max-bytes {}",
                raw.len(),
                max_bytes
            )),
            deployment_receipt: "NOT_CHECKED".into(),
        },
        Ok((status, raw)) => {
            // Sourcify answers an unverified contract with 404 and a body
            // whose `match` is null.
            let facts = match status {
                200 | 404 => edge::sourcify_facts(&raw),
                other => Err(format!("HTTP {other}")),
            };
            let deployment_receipt = match facts.as_ref().ok().and_then(|f| f.deployment_tx) {
                None => "NOT_CHECKED".into(),
                Some(tx) => match rpc.receipt(tx) {
                    Ok(r) if r.contract_address == Some(address) && r.status == 1 => {
                        format!("CONTRACT_ADDRESS_MATCH block {}", r.block_number)
                    }
                    Ok(r) => format!(
                        "CONTRACT_ADDRESS_MISMATCH: receipt contractAddress {:?}, status {}",
                        r.contract_address.map(addr),
                        r.status
                    ),
                    Err(e) => format!("READ_FAILED: {e}"),
                },
            };
            SourcifyObs {
                address,
                response: Some(raw),
                facts,
                deployment_receipt,
            }
        }
    }
}
