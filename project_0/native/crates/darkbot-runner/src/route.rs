//! [FIX-65] Jupiter Swap API V2 Router adapter.
//!
//! Uses GET /swap/v2/build and returns raw instructions + ALT contents. The transaction remains
//! ours: blockhash, CU limit, signing, simulation and submission all stay in DarkBot.
//! Keyless Jupiter access is supported; an API key is optional.

use crate::builder::p0::{Route, RouteProvider};
use base64::Engine as _;
use serde::Deserialize;
use solana_message::AddressLookupTableAccount;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::str::FromStr;

const DEFAULT_BASE: &str = "https://api.jup.ag/swap/v2";
const JUP_PROGRAM: &str = darkbot_project0::protocol::JUP_SWAP_PROGRAM_ID;
const ATA_PROGRAM: &str = darkbot_project0::protocol::ASSOCIATED_TOKEN_PROGRAM_ID;

#[derive(Clone)]
pub struct JupiterV2RouteProvider {
    client: reqwest::blocking::Client,
    base_url: String,
    taker: Pubkey,
    api_key: Option<String>,
    slippage_bps: u64,
    max_accounts: u8,
    fast: bool,
    for_jito_bundle: bool,
}

impl JupiterV2RouteProvider {
    pub fn new(
        taker: Pubkey,
        api_key: Option<String>,
        slippage_bps: u64,
        max_accounts: u8,
        fast: bool,
        for_jito_bundle: bool,
    ) -> Result<Self, String> {
        if max_accounts == 0 || max_accounts > 64 {
            return Err("Jupiter max_accounts must be 1..=64".into());
        }
        Ok(Self {
            client: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_millis(1500))
                .build()
                .map_err(|e| format!("jupiter client: {e}"))?,
            base_url: DEFAULT_BASE.into(),
            taker,
            api_key,
            slippage_bps,
            max_accounts,
            fast,
            for_jito_bundle,
        })
    }

    #[cfg(test)]
    fn with_base_url(mut self, base: String) -> Self {
        self.base_url = base;
        self
    }

    fn exact_in(&self, from_mint: &Pubkey, to_mint: &Pubkey, amount: u64) -> Result<Option<Route>, String> {
        if amount == 0 || from_mint == to_mint {
            return Ok(None);
        }
        let url = format!("{}/build", self.base_url.trim_end_matches('/'));
        let mut req = self.client.get(url).query(&[
            ("inputMint", from_mint.to_string()),
            ("outputMint", to_mint.to_string()),
            ("amount", amount.to_string()),
            ("taker", self.taker.to_string()),
            ("slippageBps", self.slippage_bps.to_string()),
            ("maxAccounts", self.max_accounts.to_string()),
            ("wrapAndUnwrapSol", "false".to_string()),
            ("forJitoBundle", self.for_jito_bundle.to_string()),
        ]);
        if self.fast {
            req = req.query(&[("mode", "fast")]);
        }
        if let Some(key) = self.api_key.as_deref() {
            req = req.header("x-api-key", key);
        }
        let response = req.send().map_err(|e| format!("jupiter /build transport: {e}"))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            if body.contains("NO_ROUTES_FOUND") || body.contains("COULD_NOT_FIND_ANY_ROUTE") || body.contains("TOKEN_NOT_TRADABLE") {
                return Ok(None);
            }
            return Err(format!("jupiter /build HTTP {status}: {body}"));
        }
        let build: BuildResponse = response.json().map_err(|e| format!("jupiter /build json: {e}"))?;
        if build.swap_mode != "ExactIn" {
            return Err(format!("jupiter unexpected swapMode {}", build.swap_mode));
        }
        let in_amount = build.in_amount.parse::<u64>().map_err(|e| format!("jupiter inAmount: {e}"))?;
        let out_amount = build.out_amount.parse::<u64>().map_err(|e| format!("jupiter outAmount: {e}"))?;
        if in_amount != amount || out_amount == 0 {
            return Ok(None);
        }

        let mut instructions = Vec::new();
        for ix in &build.setup_instructions {
            instructions.push(to_instruction(ix)?);
        }
        instructions.push(to_instruction(&build.swap_instruction)?);
        if let Some(ix) = &build.cleanup_instruction {
            instructions.push(to_instruction(ix)?);
        }
        for ix in &build.other_instructions {
            instructions.push(to_instruction(ix)?);
        }

        let jup = Pubkey::from_str(JUP_PROGRAM).map_err(|e| e.to_string())?;
        let ata = Pubkey::from_str(ATA_PROGRAM).map_err(|e| e.to_string())?;
        if instructions.iter().any(|ix| ix.program_id != jup && ix.program_id != ata) {
            // Marginfi receivership validates top-level programs; never smuggle a Token/System
            // cleanup/setup instruction through a route.
            return Ok(None);
        }

        let lookup_tables = transform_alts(build.addresses_by_lookup_table_address)?;
        Ok(Some(Route {
            instructions,
            lookup_tables,
            swap_program: jup,
            amount_in: in_amount,
            amount_out: out_amount,
        }))
    }
}

impl RouteProvider for JupiterV2RouteProvider {
    fn quote_exact_in(&self, from_mint: &Pubkey, to_mint: &Pubkey, amount_in: u64) -> Result<Option<Route>, String> {
        self.exact_in(from_mint, to_mint, amount_in)
    }

    fn quote_exact_out(&self, _from_mint: &Pubkey, _to_mint: &Pubkey, _amount_out: u64) -> Result<Option<Route>, String> {
        // Swap V2 /build docs currently specify amount as input amount and do not document an
        // ExactOut request parameter. JIT remains fail-closed until an exact-output route is proven.
        Ok(None)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiAccount {
    pubkey: String,
    is_signer: bool,
    is_writable: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiInstruction {
    program_id: String,
    accounts: Vec<ApiAccount>,
    data: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildResponse {
    in_amount: String,
    out_amount: String,
    swap_mode: String,
    setup_instructions: Vec<ApiInstruction>,
    swap_instruction: ApiInstruction,
    cleanup_instruction: Option<ApiInstruction>,
    #[serde(default)]
    other_instructions: Vec<ApiInstruction>,
    addresses_by_lookup_table_address: Option<HashMap<String, Vec<String>>>,
}

fn to_instruction(ix: &ApiInstruction) -> Result<Instruction, String> {
    let program_id = Pubkey::from_str(&ix.program_id).map_err(|e| format!("jupiter program id: {e}"))?;
    let accounts = ix.accounts.iter().map(|a| {
        let pk = Pubkey::from_str(&a.pubkey).map_err(|e| format!("jupiter account pubkey: {e}"))?;
        Ok(if a.is_writable {
            AccountMeta::new(pk, a.is_signer)
        } else {
            AccountMeta::new_readonly(pk, a.is_signer)
        })
    }).collect::<Result<Vec<_>, String>>()?;
    let data = base64::engine::general_purpose::STANDARD
        .decode(&ix.data)
        .map_err(|e| format!("jupiter instruction data: {e}"))?;
    Ok(Instruction { program_id, accounts, data })
}

fn transform_alts(raw: Option<HashMap<String, Vec<String>>>) -> Result<Vec<AddressLookupTableAccount>, String> {
    let Some(raw) = raw else { return Ok(Vec::new()) };
    let mut out = Vec::with_capacity(raw.len());
    for (key, addresses) in raw {
        let key = Pubkey::from_str(&key).map_err(|e| format!("jupiter ALT key: {e}"))?;
        let addresses = addresses.into_iter()
            .map(|a| Pubkey::from_str(&a).map_err(|e| format!("jupiter ALT address: {e}")))
            .collect::<Result<Vec<_>, _>>()?;
        out.push(AddressLookupTableAccount { key, addresses });
    }
    out.sort_unstable_by_key(|a| a.key.to_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_instruction_and_alt_without_rpc() {
        let jup = JUP_PROGRAM;
        let a = Pubkey::new_unique();
        let ix = ApiInstruction {
            program_id: jup.into(),
            accounts: vec![ApiAccount { pubkey: a.to_string(), is_signer: false, is_writable: true }],
            data: base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3]),
        };
        let out = to_instruction(&ix).unwrap();
        assert_eq!(out.accounts[0].pubkey, a);
        assert_eq!(out.data, vec![1, 2, 3]);

        let key = Pubkey::new_unique();
        let addr = Pubkey::new_unique();
        let mut raw = HashMap::new();
        raw.insert(key.to_string(), vec![addr.to_string()]);
        let alts = transform_alts(Some(raw)).unwrap();
        assert_eq!(alts[0].key, key);
        assert_eq!(alts[0].addresses, vec![addr]);
    }
}
