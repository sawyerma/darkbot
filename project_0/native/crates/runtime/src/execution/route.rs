//! [FIX-65] Jupiter Swap API V2 Router adapter.
//!
//! Uses GET /swap/v2/build and returns raw instructions + ALT contents. The transaction remains
//! ours: blockhash, CU limit, signing, simulation and submission all stay in DarkBot.
//! Keyless Jupiter access is supported; an API key is optional.

use crate::execution::builder::p0::{PrewarmedAta, Route, RouteProvider};
use base64::Engine as _;
use serde::Deserialize;
use solana_message::AddressLookupTableAccount;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;
use spl_associated_token_account::{
    get_associated_token_address_with_program_id,
    instruction::create_associated_token_account_idempotent,
};
use std::collections::HashMap;
use std::str::FromStr;
use std::time::{Duration, Instant};

/// Upper bound of one Jupiter request (also when the candidate deadline is further away).
const REQUEST_TIMEOUT: Duration = Duration::from_millis(1500);

const DEFAULT_BASE: &str = "https://api.jup.ag/swap/v2";
const JUP_PROGRAM: &str = darkbot_project0::protocol::JUP_SWAP_PROGRAM_ID;

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
                .timeout(REQUEST_TIMEOUT)
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

    /// Tests point the provider at a loopback endpoint.
    #[cfg(test)]
    pub(crate) fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    fn exact_in(
        &self,
        from_mint: &Pubkey,
        to_mint: &Pubkey,
        amount: u64,
        prewarmed_atas: &[PrewarmedAta],
        max_accounts: u8,
        deadline: Instant,
    ) -> Result<Option<Route>, String> {
        // N2: no request after the candidate deadline; one in flight ends at it.
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("jupiter /build: DEADLINE (not started)".into());
        }
        if amount == 0 {
            return Err("jupiter exact-in rejected locally: amount=0".into());
        }
        if from_mint == to_mint {
            return Err(format!(
                "jupiter exact-in rejected locally: identical mint {}",
                from_mint
            ));
        }
        let url = format!("{}/build", self.base_url.trim_end_matches('/'));
        let mut req = self
            .client
            .get(url)
            .timeout(remaining.min(REQUEST_TIMEOUT))
            .query(&[
                ("inputMint", from_mint.to_string()),
                ("outputMint", to_mint.to_string()),
                ("amount", amount.to_string()),
                ("taker", self.taker.to_string()),
                ("slippageBps", self.slippage_bps.to_string()),
                (
                    "maxAccounts",
                    max_accounts.min(self.max_accounts).max(1).to_string(),
                ),
                ("wrapAndUnwrapSol", "false".to_string()),
                ("forJitoBundle", self.for_jito_bundle.to_string()),
            ]);
        if self.fast {
            req = req.query(&[("mode", "fast")]);
        }
        if let Some(key) = self.api_key.as_deref() {
            req = req.header("x-api-key", key);
        }
        // [MT0] latency + outcome per request (measurement only).
        let mt0_start = std::time::Instant::now();
        let response = match req.send() {
            Ok(r) => r,
            Err(e) => {
                let outcome = if e.is_timeout() {
                    darkbot_common::mt0::RemoteOutcome::Timeout
                } else {
                    darkbot_common::mt0::RemoteOutcome::Error
                };
                darkbot_common::mt0::MT0.record_remote(
                    darkbot_common::mt0::RemoteClass::Jupiter,
                    mt0_start.elapsed(),
                    outcome,
                );
                // B13: the transport error is classified; its text may carry the URL.
                let class = if Instant::now() >= deadline {
                    "DEADLINE"
                } else if e.is_timeout() {
                    "TIMEOUT"
                } else if e.is_connect() {
                    "CONNECT"
                } else {
                    "REQUEST"
                };
                return Err(format!("jupiter /build transport: {class}"));
            }
        };
        let mt0_status = response.status().as_u16();
        // 404 is Jupiter's regular "no route" answer, not a transport failure.
        let mt0_outcome = if mt0_status == 404 {
            darkbot_common::mt0::RemoteOutcome::Ok
        } else {
            darkbot_common::mt0::RemoteOutcome::from_http_status(mt0_status)
        };
        darkbot_common::mt0::MT0.record_remote(
            darkbot_common::mt0::RemoteClass::Jupiter,
            mt0_start.elapsed(),
            mt0_outcome,
        );
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err("jupiter /build: no route (HTTP 404)".into());
        }
        if !response.status().is_success() {
            let status = response.status();
            let mut body = Vec::new();
            let _ = std::io::Read::read_to_end(
                &mut std::io::Read::take(response, ERROR_BODY_LIMIT),
                &mut body,
            );
            return Err(jupiter_error(status, &body));
        }
        let build: BuildResponse = response
            .json()
            .map_err(|e| format!("jupiter /build json: {e}"))?;
        if build.swap_mode != "ExactIn" {
            return Err(format!("jupiter unexpected swapMode {}", build.swap_mode));
        }
        let (in_amount, quoted_out, minimum_executable_out) = parse_build_amounts(
            amount,
            &build.in_amount,
            &build.out_amount,
            build.other_amount_threshold.as_ref(),
        )?;

        let mut instructions = Vec::new();
        for raw in &build.setup_instructions {
            let ix = to_instruction(raw)?;
            if is_verified_redundant_prewarmed_ata_setup(&ix, &self.taker, prewarmed_atas)? {
                continue;
            }
            instructions.push(ix);
        }
        instructions.push(to_instruction(&build.swap_instruction)?);
        if let Some(ix) = &build.cleanup_instruction {
            instructions.push(to_instruction(ix)?);
        }
        for ix in &build.other_instructions {
            instructions.push(to_instruction(ix)?);
        }

        let jup = Pubkey::from_str(JUP_PROGRAM).map_err(|e| e.to_string())?;
        if instructions.iter().any(|ix| ix.program_id != jup) {
            let mut bad: Vec<String> = instructions
                .iter()
                .filter(|ix| ix.program_id != jup)
                .map(|ix| format!("{}(data_len={})", ix.program_id, ix.data.len()))
                .collect();
            bad.sort();
            bad.dedup();
            return Err(format!(
                "jupiter route incompatible with receivership top-level validator after prewarm filtering: {}",
                bad.join(",")
            ));
        }

        let lookup_tables = transform_alts(build.addresses_by_lookup_table_address)?;
        Ok(Some(Route {
            instructions,
            lookup_tables,
            swap_program: jup,
            amount_in: in_amount,
            quoted_out,
            minimum_executable_out,
        }))
    }
}

impl RouteProvider for JupiterV2RouteProvider {
    fn quote_exact_in(
        &self,
        from_mint: &Pubkey,
        to_mint: &Pubkey,
        amount_in: u64,
        prewarmed_atas: &[PrewarmedAta],
        deadline: Instant,
    ) -> Result<Option<Route>, String> {
        self.exact_in(
            from_mint,
            to_mint,
            amount_in,
            prewarmed_atas,
            self.max_accounts,
            deadline,
        )
    }

    fn max_accounts(&self) -> u8 {
        self.max_accounts
    }

    fn quote_exact_in_with_budget(
        &self,
        from_mint: &Pubkey,
        to_mint: &Pubkey,
        amount_in: u64,
        prewarmed_atas: &[PrewarmedAta],
        max_accounts: u8,
        deadline: Instant,
    ) -> Result<Option<Route>, String> {
        self.exact_in(
            from_mint,
            to_mint,
            amount_in,
            prewarmed_atas,
            max_accounts,
            deadline,
        )
    }

    fn quote_exact_out(
        &self,
        _from_mint: &Pubkey,
        _to_mint: &Pubkey,
        _amount_out: u64,
        _deadline: Instant,
    ) -> Result<Option<Route>, String> {
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
    /// [P1-C] ExactIn: minimum output enforced on-chain (slippage floor). Accepted as JSON
    /// string or number; absent = unknown (fail closed downstream).
    #[serde(default)]
    other_amount_threshold: Option<serde_json::Value>,
    swap_mode: String,
    setup_instructions: Vec<ApiInstruction>,
    swap_instruction: ApiInstruction,
    cleanup_instruction: Option<ApiInstruction>,
    #[serde(default)]
    other_instructions: Vec<ApiInstruction>,
    addresses_by_lookup_table_address: Option<HashMap<String, Vec<String>>>,
}

/// [P1-C] Parse and validate `/build` amounts for ExactIn.
/// `outAmount` is the quote. `otherAmountThreshold` is the minimum output the swap instruction
/// enforces on-chain. No local `quote - slippage` arithmetic: the provider floor is the truth.
fn parse_build_amounts(
    requested_in: u64,
    in_amount: &str,
    out_amount: &str,
    other_amount_threshold: Option<&serde_json::Value>,
) -> Result<(u64, u64, Option<u64>), String> {
    let in_amount = in_amount
        .parse::<u64>()
        .map_err(|e| format!("jupiter inAmount: {e}"))?;
    let quoted_out = out_amount
        .parse::<u64>()
        .map_err(|e| format!("jupiter outAmount: {e}"))?;
    if in_amount != requested_in || quoted_out == 0 {
        return Err(format!(
            "jupiter /build invalid amounts: requested_in={} response_in={} response_out={}",
            requested_in, in_amount, quoted_out
        ));
    }
    let floor = match other_amount_threshold {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(raw)) => Some(
            raw.parse::<u64>()
                .map_err(|e| format!("jupiter otherAmountThreshold malformed: {e}"))?,
        ),
        Some(serde_json::Value::Number(n)) => Some(
            n.as_u64()
                .ok_or_else(|| format!("jupiter otherAmountThreshold malformed: {n}"))?,
        ),
        Some(other) => {
            return Err(format!(
                "jupiter otherAmountThreshold malformed type: {other}"
            ));
        }
    };
    if let Some(floor) = floor {
        if floor == 0 {
            return Err("jupiter otherAmountThreshold=0: no executable floor".into());
        }
        if floor > quoted_out {
            return Err(format!(
                "jupiter otherAmountThreshold {} > outAmount {}: schema invalid",
                floor, quoted_out
            ));
        }
    }
    Ok((in_amount, quoted_out, floor))
}

fn to_instruction(ix: &ApiInstruction) -> Result<Instruction, String> {
    let program_id =
        Pubkey::from_str(&ix.program_id).map_err(|e| format!("jupiter program id: {e}"))?;
    let accounts = ix
        .accounts
        .iter()
        .map(|a| {
            let pk =
                Pubkey::from_str(&a.pubkey).map_err(|e| format!("jupiter account pubkey: {e}"))?;
            Ok(if a.is_writable {
                AccountMeta::new(pk, a.is_signer)
            } else {
                AccountMeta::new_readonly(pk, a.is_signer)
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let data = base64::engine::general_purpose::STANDARD
        .decode(&ix.data)
        .map_err(|e| format!("jupiter instruction data: {e}"))?;
    Ok(Instruction {
        program_id,
        accounts,
        data,
    })
}

/// [FIX-106] Remove only an exact canonical idempotent ATA-create emitted in Jupiter's
/// setup section when that canonical ATA has already been proven present in reconciled state.
/// Any other Associated Token instruction remains fail-closed because Marginfi rejects it as
/// a top-level receivership instruction.
fn is_verified_redundant_prewarmed_ata_setup(
    ix: &Instruction,
    taker: &Pubkey,
    prewarmed_atas: &[PrewarmedAta],
) -> Result<bool, String> {
    let ata_program = Pubkey::from_str(darkbot_project0::protocol::ASSOCIATED_TOKEN_PROGRAM_ID)
        .map_err(|e| e.to_string())?;
    if ix.program_id != ata_program {
        return Ok(false);
    }

    for known in prewarmed_atas {
        let derived =
            get_associated_token_address_with_program_id(taker, &known.mint, &known.token_program);
        if derived != known.address {
            return Err(format!(
                "prewarmed ATA identity mismatch: recorded={} derived={} mint={} token_program={}",
                known.address, derived, known.mint, known.token_program
            ));
        }

        let expected = create_associated_token_account_idempotent(
            taker,
            taker,
            &known.mint,
            &known.token_program,
        );
        let same_accounts = ix.accounts.len() == expected.accounts.len()
            && ix
                .accounts
                .iter()
                .zip(expected.accounts.iter())
                .all(|(a, b)| {
                    a.pubkey == b.pubkey
                        && a.is_signer == b.is_signer
                        && a.is_writable == b.is_writable
                });

        if ix.program_id == expected.program_id && ix.data == expected.data && same_accounts {
            return Ok(true);
        }
    }

    Err(format!(
        "jupiter ATA setup not safely removable: data_len={} accounts={}",
        ix.data.len(),
        ix.accounts.len()
    ))
}

fn transform_alts(
    raw: Option<HashMap<String, Vec<String>>>,
) -> Result<Vec<AddressLookupTableAccount>, String> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(raw.len());
    for (key, addresses) in raw {
        let key = Pubkey::from_str(&key).map_err(|e| format!("jupiter ALT key: {e}"))?;
        let addresses = addresses
            .into_iter()
            .map(|a| Pubkey::from_str(&a).map_err(|e| format!("jupiter ALT address: {e}")))
            .collect::<Result<Vec<_>, _>>()?;
        out.push(AddressLookupTableAccount { key, addresses });
    }
    out.sort_unstable_by_key(|a| a.key.to_bytes());
    Ok(out)
}

/// Bytes of a Jupiter error body that are inspected (B13): never read unbounded.
const ERROR_BODY_LIMIT: u64 = 4096;
/// B13: only the HTTP status or an allowlisted no-route code reaches the error; body free text,
/// URLs and keys never do.
fn jupiter_error(status: reqwest::StatusCode, body: &[u8]) -> String {
    const NO_ROUTE: [&str; 3] = [
        "NO_ROUTES_FOUND",
        "COULD_NOT_FIND_ANY_ROUTE",
        "TOKEN_NOT_TRADABLE",
    ];
    let text = String::from_utf8_lossy(body);
    match NO_ROUTE.iter().find(|code| text.contains(*code)) {
        Some(code) => format!("jupiter /build: no executable route: {code}"),
        None => format!("jupiter /build HTTP {}", status.as_u16()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jupiter_error_keeps_only_status_or_allowlisted_code() {
        let leak = "SECRET https://user:key@jup.invalid/swap?api-key=SECRET";
        let big = format!("{}{leak}", "x".repeat(1 << 20));
        let status = reqwest::StatusCode::INTERNAL_SERVER_ERROR;
        assert_eq!(
            jupiter_error(status, big.as_bytes()),
            "jupiter /build HTTP 500"
        );
        assert_eq!(
            jupiter_error(status, leak.as_bytes()),
            "jupiter /build HTTP 500"
        );
        let no_route = format!("{{\"errorCode\":\"NO_ROUTES_FOUND\",\"message\":\"{leak}\"}}");
        assert_eq!(
            jupiter_error(reqwest::StatusCode::BAD_REQUEST, no_route.as_bytes()),
            "jupiter /build: no executable route: NO_ROUTES_FOUND"
        );
    }

    #[test]
    fn strips_only_exact_verified_prewarmed_ata_setup() {
        let taker = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let token_program = Pubkey::from_str(darkbot_project0::protocol::TOKEN_PROGRAM_ID).unwrap();
        let address = get_associated_token_address_with_program_id(&taker, &mint, &token_program);
        let known = PrewarmedAta {
            address,
            mint,
            token_program,
        };
        let ix = create_associated_token_account_idempotent(&taker, &taker, &mint, &token_program);

        assert_eq!(
            is_verified_redundant_prewarmed_ata_setup(&ix, &taker, &[known]),
            Ok(true)
        );

        let other_taker = Pubkey::new_unique();
        assert!(is_verified_redundant_prewarmed_ata_setup(&ix, &other_taker, &[known]).is_err());
    }

    #[test]
    fn p1c_parses_quote_and_executable_floor_separately() {
        use serde_json::json;
        // quote > floor
        assert_eq!(
            parse_build_amounts(10, "10", "1000", Some(&json!("950"))),
            Ok((10, 1000, Some(950)))
        );
        // quote == floor
        assert_eq!(
            parse_build_amounts(10, "10", "1000", Some(&json!("1000"))),
            Ok((10, 1000, Some(1000)))
        );
        // numeric threshold accepted
        assert_eq!(
            parse_build_amounts(10, "10", "1000", Some(&json!(900))),
            Ok((10, 1000, Some(900)))
        );
        // missing / null floor -> unknown, never the quote
        assert_eq!(
            parse_build_amounts(10, "10", "1000", None),
            Ok((10, 1000, None))
        );
        assert_eq!(
            parse_build_amounts(10, "10", "1000", Some(&json!(null))),
            Ok((10, 1000, None))
        );
        // malformed floor -> fail closed
        assert!(parse_build_amounts(10, "10", "1000", Some(&json!("abc"))).is_err());
        assert!(parse_build_amounts(10, "10", "1000", Some(&json!(""))).is_err());
        assert!(parse_build_amounts(10, "10", "1000", Some(&json!(-5))).is_err());
        assert!(parse_build_amounts(10, "10", "1000", Some(&json!(["1"]))).is_err());
        // floor > quote -> schema invalid -> fail closed
        assert!(parse_build_amounts(10, "10", "1000", Some(&json!("1001"))).is_err());
        // zero floor -> no protection -> fail closed
        assert!(parse_build_amounts(10, "10", "1000", Some(&json!("0"))).is_err());
        // input mismatch still rejected
        assert!(parse_build_amounts(11, "10", "1000", Some(&json!("950"))).is_err());
    }

    #[test]
    fn parses_instruction_and_alt_without_rpc() {
        let jup = JUP_PROGRAM;
        let a = Pubkey::new_unique();
        let ix = ApiInstruction {
            program_id: jup.into(),
            accounts: vec![ApiAccount {
                pubkey: a.to_string(),
                is_signer: false,
                is_writable: true,
            }],
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
