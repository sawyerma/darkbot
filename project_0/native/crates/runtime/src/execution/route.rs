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
/// The native mint (wrapped SOL).
pub use crate::execution::one_shot::exit::WSOL_MINT;

/// FE-02/E2: Jupiter v6 swap instructions pinned from the public IDL `jup-ag/jupiter-cpi` commit
/// `12bc5f67` (`idl.json` SHA-256 `764ea6d7…5c50`); parity with the deployed program is FE-01
/// (NOT_RUN). Anchor discriminators = sha256("global:<name>")[..8]. Only the ExactIn routes are
/// accepted; any other discriminator (token ledger, exact out, newer instructions) fails closed.
const JUP_ROUTE: [u8; 8] = [229, 23, 203, 151, 122, 227, 173, 42];
const JUP_SHARED_ACCOUNTS_ROUTE: [u8; 8] = [193, 32, 155, 51, 65, 214, 156, 129];
const JUP_ROUTE_WITH_TOKEN_LEDGER: [u8; 8] = [150, 86, 71, 116, 167, 93, 14, 104];
const JUP_SHARED_ROUTE_WITH_TOKEN_LEDGER: [u8; 8] = [230, 121, 143, 80, 119, 159, 106, 170];
const JUP_SHARED_EXACT_OUT_ROUTE: [u8; 8] = [176, 209, 105, 168, 154, 125, 69, 62];

/// What the signed swap instruction itself enforces (FE-02: the repay floor from its bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SwapTerms {
    pub(crate) in_amount: u64,
    pub(crate) quoted_out: u64,
    pub(crate) slippage_bps: u16,
    /// `quoted_out − ceil(quoted_out · slippage / 10⁴)`: the lowest output any rounding of the
    /// program's slippage check accepts.
    pub(crate) minimum_out: u64,
}

/// FE-02/E2: decodes the ExactIn route instruction. Its arguments end in the fixed-size
/// `in_amount u64, quoted_out_amount u64, slippage_bps u16, platform_fee_bps u8` (Borsh is
/// sequential, so this tail is exact without parsing the route plan). The route moves the taker's
/// own `from_mint` account into its own `to_mint` account; a platform fee is refused.
pub(crate) fn swap_terms(
    ix: &Instruction,
    taker: &Pubkey,
    from_mint: &Pubkey,
    to_mint: &Pubkey,
) -> Result<SwapTerms, String> {
    let jup = Pubkey::from_str(JUP_PROGRAM).map_err(|e| e.to_string())?;
    if ix.program_id != jup {
        return Err("SWAP_PROGRAM_MISMATCH".into());
    }
    let d = &ix.data;
    let disc: [u8; 8] = d
        .get(..8)
        .and_then(|s| s.try_into().ok())
        .ok_or("SWAP_DATA_SHORT")?;
    // (bytes before the route plan, authority, source, destination, source mint, dest. mint)
    let (prefix, authority, source, destination, source_mint, destination_mint) = match disc {
        JUP_ROUTE => (8, 1, 2, 3, None, 5),
        JUP_SHARED_ACCOUNTS_ROUTE => (9, 2, 3, 6, Some(7), 8),
        JUP_ROUTE_WITH_TOKEN_LEDGER
        | JUP_SHARED_ROUTE_WITH_TOKEN_LEDGER
        | JUP_SHARED_EXACT_OUT_ROUTE => return Err("SWAP_INSTRUCTION_NOT_EXACT_IN".into()),
        other => {
            return Err(format!(
                "SWAP_INSTRUCTION_UNPINNED: {}",
                other.iter().map(|b| format!("{b:02x}")).collect::<String>()
            ))
        }
    };
    if d.len() < prefix + 4 + 19 {
        return Err("SWAP_DATA_SHORT".into());
    }
    let t = &d[d.len() - 19..];
    let u64_at = |i: usize| u64::from_le_bytes(t[i..i + 8].try_into().expect("8 bytes"));
    let (in_amount, quoted_out) = (u64_at(0), u64_at(8));
    let slippage_bps = u16::from_le_bytes([t[16], t[17]]);
    if t[18] != 0 {
        return Err(format!("SWAP_PLATFORM_FEE_NONZERO: {} bps", t[18]));
    }
    if slippage_bps > 10_000 || quoted_out == 0 {
        return Err(format!(
            "SWAP_TERMS_INVALID: slippage_bps={slippage_bps} quoted_out={quoted_out}"
        ));
    }
    let cut = (u128::from(quoted_out) * u128::from(slippage_bps)).div_ceil(10_000);
    let minimum_out = quoted_out - u64::try_from(cut).map_err(|_| "SWAP_TERMS_INVALID")?;
    let key = |i: usize| {
        ix.accounts
            .get(i)
            .ok_or_else(|| "SWAP_ACCOUNTS_SHORT".to_string())
    };
    let token = Pubkey::from_str(darkbot_project0::protocol::TOKEN_PROGRAM_ID)
        .map_err(|e| e.to_string())?;
    let token_2022 = Pubkey::from_str(darkbot_project0::protocol::TOKEN_2022_PROGRAM_ID)
        .map_err(|e| e.to_string())?;
    let own_ata = |addr: &Pubkey, mint: &Pubkey| {
        [token, token_2022]
            .iter()
            .any(|p| get_associated_token_address_with_program_id(taker, mint, p) == *addr)
    };
    let a = key(authority)?;
    if a.pubkey != *taker || !a.is_signer {
        return Err("SWAP_AUTHORITY_NOT_TAKER".into());
    }
    if !own_ata(&key(source)?.pubkey, from_mint) {
        return Err("SWAP_SOURCE_NOT_OWN_ATA".into());
    }
    if !own_ata(&key(destination)?.pubkey, to_mint) {
        return Err("SWAP_DESTINATION_NOT_OWN_ATA".into());
    }
    if disc == JUP_ROUTE {
        // The optional final destination: absent (program id) or the taker's own account.
        let out = key(4)?.pubkey;
        if out != jup && !own_ata(&out, to_mint) {
            return Err("SWAP_DESTINATION_NOT_OWN_ATA".into());
        }
    }
    if source_mint.is_some_and(|i| key(i).map(|k| k.pubkey) != Ok(*from_mint))
        || key(destination_mint)?.pubkey != *to_mint
    {
        return Err("SWAP_MINT_MISMATCH".into());
    }
    Ok(SwapTerms {
        in_amount,
        quoted_out,
        slippage_bps,
        minimum_out,
    })
}

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
            client: darkbot_solana::provider::blocking_http()
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

    /// One `/build` request: local refusals, the N2 deadline, MT0 measurement, the classified
    /// transport/status errors and the parsed response.
    fn request_build(
        &self,
        from_mint: &Pubkey,
        to_mint: &Pubkey,
        amount: u64,
        max_accounts: u8,
        wrap_and_unwrap_sol: bool,
        deadline: Instant,
    ) -> Result<BuildResponse, String> {
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
                ("wrapAndUnwrapSol", wrap_and_unwrap_sol.to_string()),
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
        response
            .json()
            .map_err(|e| format!("jupiter /build json: {e}"))
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
        let build =
            self.request_build(from_mint, to_mint, amount, max_accounts, false, deadline)?;
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
        // FE-02/E2: the floor the signed swap instruction enforces, from its own bytes; the
        // provider's `otherAmountThreshold` may only be lower (never an overstatement).
        let swap = to_instruction(&build.swap_instruction)?;
        let terms = swap_terms(&swap, &self.taker, from_mint, to_mint)?;
        if terms.in_amount != in_amount || terms.quoted_out != quoted_out {
            return Err(format!(
                "SWAP_TERMS_MISMATCH: instruction in={} out={} response in={in_amount} out={quoted_out}",
                terms.in_amount, terms.quoted_out
            ));
        }
        let minimum_executable_out = match minimum_executable_out {
            Some(json) if json > terms.minimum_out => {
                return Err(format!(
                    "SWAP_FLOOR_MISMATCH: response threshold {json} > instruction floor {}",
                    terms.minimum_out
                ))
            }
            Some(json) => Some(json),
            None => Some(terms.minimum_out),
        };
        instructions.push(swap);
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
            enforced_slippage_bps: Some(terms.slippage_bps),
        }))
    }
}

/// FE-04 exit: the instructions of a native-SOL exit of `mint` by `taker`, exactly — an optional
/// idempotent creation of the taker's WSOL ATA, the Jupiter v6 ExactIn swap from the taker's
/// `mint` ATA into it, the SPL Token close of that ATA to the taker (the unwrap) — and the swap's
/// signed terms. Anything else is refused.
pub(crate) fn exit_route_terms(
    instructions: &[Instruction],
    taker: &Pubkey,
    mint: &Pubkey,
) -> Result<SwapTerms, String> {
    let wsol = Pubkey::from_str(WSOL_MINT).map_err(|e| e.to_string())?;
    let token = Pubkey::from_str(darkbot_project0::protocol::TOKEN_PROGRAM_ID)
        .map_err(|e| e.to_string())?;
    let wsol_ata = get_associated_token_address_with_program_id(taker, &wsol, &token);
    let create = create_associated_token_account_idempotent(taker, taker, &wsol, &token);
    let close = Instruction {
        program_id: token,
        accounts: vec![
            AccountMeta::new(wsol_ata, false),
            AccountMeta::new(*taker, false),
            AccountMeta::new_readonly(*taker, true),
        ],
        // SPL Token `CloseAccount`.
        data: vec![9],
    };
    let rest = match instructions.first() {
        Some(ix) if *ix == create => &instructions[1..],
        _ => instructions,
    };
    match rest {
        [swap, unwrap] if *unwrap == close => swap_terms(swap, taker, mint, &wsol),
        [_] | [_, _] => Err("EXIT_ROUTE_UNWRAP_MISSING".into()),
        _ => Err("EXIT_ROUTE_SHAPE: create? swap close".into()),
    }
}

impl JupiterV2RouteProvider {
    /// FE-04 exit: the native-SOL sale of `amount` of `mint` — a Jupiter v6 ExactIn route into the
    /// taker's own WSOL ATA, unwrapped by closing that ATA to the taker in the same transaction.
    /// Exactly: an idempotent creation of that ATA (optional), the swap (its floor from the signed
    /// bytes, as for every route), the close; any other instruction, a missing or foreign close,
    /// another destination or an amount other than `amount` is refused.
    pub fn exit_to_sol(
        &self,
        mint: &Pubkey,
        amount: u64,
        deadline: Instant,
    ) -> Result<Route, String> {
        let wsol = Pubkey::from_str(WSOL_MINT).map_err(|e| e.to_string())?;
        let build = self.request_build(mint, &wsol, amount, self.max_accounts, true, deadline)?;
        if build.swap_mode != "ExactIn" {
            return Err(format!("jupiter unexpected swapMode {}", build.swap_mode));
        }
        let (in_amount, quoted_out, threshold) = parse_build_amounts(
            amount,
            &build.in_amount,
            &build.out_amount,
            build.other_amount_threshold.as_ref(),
        )?;
        if !build.other_instructions.is_empty() {
            return Err("EXIT_ROUTE_OTHER_INSTRUCTIONS".into());
        }
        let mut instructions = build
            .setup_instructions
            .iter()
            .map(to_instruction)
            .collect::<Result<Vec<_>, _>>()?;
        instructions.push(to_instruction(&build.swap_instruction)?);
        if let Some(ix) = &build.cleanup_instruction {
            instructions.push(to_instruction(ix)?);
        }
        let terms = exit_route_terms(&instructions, &self.taker, mint)?;
        if terms.in_amount != in_amount || terms.quoted_out != quoted_out || in_amount != amount {
            return Err(format!(
                "SWAP_TERMS_MISMATCH: instruction in={} out={} response in={in_amount} out={quoted_out}",
                terms.in_amount, terms.quoted_out
            ));
        }
        if threshold.is_some_and(|t| t > terms.minimum_out) {
            return Err("SWAP_FLOOR_MISMATCH".into());
        }
        Ok(Route {
            instructions,
            lookup_tables: transform_alts(build.addresses_by_lookup_table_address)?,
            swap_program: Pubkey::from_str(JUP_PROGRAM).map_err(|e| e.to_string())?,
            amount_in: in_amount,
            quoted_out,
            minimum_executable_out: Some(terms.minimum_out),
            enforced_slippage_bps: Some(terms.slippage_bps),
        })
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
    /// [P1-C] ExactIn: minimum output enforced on-chain (slippage floor); absent or null =
    /// unknown (fail closed downstream).
    #[serde(default)]
    other_amount_threshold: Option<WireAmount>,
    swap_mode: String,
    setup_instructions: Vec<ApiInstruction>,
    swap_instruction: ApiInstruction,
    cleanup_instruction: Option<ApiInstruction>,
    #[serde(default)]
    other_instructions: Vec<ApiInstruction>,
    addresses_by_lookup_table_address: Option<HashMap<String, Vec<String>>>,
}

/// [JFI-FIX-07] An atomic amount as this wire carries it: a decimal string or an unsigned
/// integer. Any other JSON type (negative, fractional, array, object) fails the response decode
/// at the wire edge; no generic JSON value reaches the router.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
enum WireAmount {
    Text(String),
    Integer(u64),
}

/// [P1-C] Parse and validate `/build` amounts for ExactIn.
/// `outAmount` is the quote. `otherAmountThreshold` is the minimum output the swap instruction
/// enforces on-chain. No local `quote - slippage` arithmetic: the provider floor is the truth.
fn parse_build_amounts(
    requested_in: u64,
    in_amount: &str,
    out_amount: &str,
    other_amount_threshold: Option<&WireAmount>,
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
        None => None,
        Some(WireAmount::Text(raw)) => Some(
            raw.parse::<u64>()
                .map_err(|e| format!("jupiter otherAmountThreshold malformed: {e}"))?,
        ),
        Some(WireAmount::Integer(n)) => Some(*n),
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
        let text = |s: &str| Some(WireAmount::Text(s.into()));
        // quote > floor
        assert_eq!(
            parse_build_amounts(10, "10", "1000", text("950").as_ref()),
            Ok((10, 1000, Some(950)))
        );
        // quote == floor
        assert_eq!(
            parse_build_amounts(10, "10", "1000", text("1000").as_ref()),
            Ok((10, 1000, Some(1000)))
        );
        // numeric threshold accepted
        assert_eq!(
            parse_build_amounts(10, "10", "1000", Some(&WireAmount::Integer(900))),
            Ok((10, 1000, Some(900)))
        );
        // missing floor -> unknown, never the quote
        assert_eq!(
            parse_build_amounts(10, "10", "1000", None),
            Ok((10, 1000, None))
        );
        // JFI-FIX-07: the wire decode itself: string or integer; null = absent; any other JSON
        // type fails the response (fail closed before the router).
        let wire = |v: serde_json::Value| serde_json::from_value::<Option<WireAmount>>(v);
        assert_eq!(wire(json!("950")).unwrap(), text("950"));
        assert_eq!(wire(json!(900)).unwrap(), Some(WireAmount::Integer(900)));
        assert_eq!(wire(json!(null)).unwrap(), None);
        for bad in [json!(-5), json!(1.5), json!(["1"]), json!({"v": 1})] {
            assert!(wire(bad.clone()).is_err(), "{bad}");
        }
        // malformed floor text -> fail closed
        assert!(parse_build_amounts(10, "10", "1000", text("abc").as_ref()).is_err());
        assert!(parse_build_amounts(10, "10", "1000", text("").as_ref()).is_err());
        assert!(parse_build_amounts(10, "10", "1000", text("-5").as_ref()).is_err());
        // floor > quote -> schema invalid -> fail closed
        assert!(parse_build_amounts(10, "10", "1000", text("1001").as_ref()).is_err());
        // zero floor -> no protection -> fail closed
        assert!(parse_build_amounts(10, "10", "1000", text("0").as_ref()).is_err());
        // input mismatch still rejected
        assert!(parse_build_amounts(11, "10", "1000", text("950").as_ref()).is_err());
    }

    /// A synthetic (not recorded) Jupiter ExactIn route instruction in the pinned layout.
    fn swap_ix(
        disc: [u8; 8],
        taker: &Pubkey,
        from: &Pubkey,
        to: &Pubkey,
        terms: (u64, u64, u16, u8),
    ) -> Instruction {
        let jup = Pubkey::from_str(JUP_PROGRAM).unwrap();
        let token = Pubkey::from_str(darkbot_project0::protocol::TOKEN_PROGRAM_ID).unwrap();
        let ata = |m: &Pubkey| get_associated_token_address_with_program_id(taker, m, &token);
        let mut data = disc.to_vec();
        let shared = disc == JUP_SHARED_ACCOUNTS_ROUTE;
        if shared {
            data.push(3);
        }
        // A one-step route plan (opaque to the decoder): length 1, a step body.
        data.extend(1u32.to_le_bytes());
        data.extend([7, 100, 0, 1]);
        data.extend(terms.0.to_le_bytes());
        data.extend(terms.1.to_le_bytes());
        data.extend(terms.2.to_le_bytes());
        data.push(terms.3);
        let ro = AccountMeta::new_readonly;
        let accounts = if shared {
            vec![
                ro(token, false),
                ro(Pubkey::new_unique(), false),
                ro(*taker, true),
                AccountMeta::new(ata(from), false),
                AccountMeta::new(Pubkey::new_unique(), false),
                AccountMeta::new(Pubkey::new_unique(), false),
                AccountMeta::new(ata(to), false),
                ro(*from, false),
                ro(*to, false),
                ro(jup, false),
                ro(jup, false),
                ro(Pubkey::new_unique(), false),
                ro(jup, false),
            ]
        } else {
            vec![
                ro(token, false),
                ro(*taker, true),
                AccountMeta::new(ata(from), false),
                AccountMeta::new(ata(to), false),
                ro(jup, false),
                ro(*to, false),
                ro(jup, false),
                ro(Pubkey::new_unique(), false),
                ro(jup, false),
            ]
        };
        Instruction {
            program_id: jup,
            accounts,
            data,
        }
    }

    /// FE-02/E2: the floor and slippage come from the signed swap instruction's own bytes —
    /// `quoted − ceil(quoted·slippage/10⁴)` — for both pinned ExactIn layouts; any other
    /// instruction, a platform fee, a foreign authority/source/destination or a mint mismatch
    /// fails closed with its code.
    #[test]
    fn fe02_swap_terms_come_from_the_signed_instruction() {
        let (taker, from, to) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let terms = (500, 1_000_003, 50, 0);
        let want = SwapTerms {
            in_amount: 500,
            quoted_out: 1_000_003,
            slippage_bps: 50,
            minimum_out: 995_002,
        };
        for disc in [JUP_ROUTE, JUP_SHARED_ACCOUNTS_ROUTE] {
            let ix = swap_ix(disc, &taker, &from, &to, terms);
            assert_eq!(swap_terms(&ix, &taker, &from, &to), Ok(want));
            let err = |ix: &Instruction| swap_terms(ix, &taker, &from, &to).unwrap_err();
            let mut fee = swap_ix(disc, &taker, &from, &to, (500, 1_000_003, 50, 10));
            assert!(err(&fee).starts_with("SWAP_PLATFORM_FEE_NONZERO"));
            fee.data.truncate(12);
            assert_eq!(err(&fee), "SWAP_DATA_SHORT");
            let other = Pubkey::new_unique();
            assert_eq!(
                swap_terms(&ix, &other, &from, &to).unwrap_err(),
                "SWAP_AUTHORITY_NOT_TAKER"
            );
            assert_eq!(
                swap_terms(&ix, &taker, &other, &to).unwrap_err(),
                "SWAP_SOURCE_NOT_OWN_ATA"
            );
            assert_eq!(
                swap_terms(&ix, &taker, &from, &other).unwrap_err(),
                "SWAP_DESTINATION_NOT_OWN_ATA"
            );
            let mut unsigned = ix.clone();
            unsigned
                .accounts
                .iter_mut()
                .for_each(|a| a.is_signer = false);
            assert_eq!(err(&unsigned), "SWAP_AUTHORITY_NOT_TAKER");
            let mut foreign = ix.clone();
            foreign.program_id = Pubkey::new_unique();
            assert_eq!(err(&foreign), "SWAP_PROGRAM_MISMATCH");
            let mint_at = if disc == JUP_ROUTE { 5 } else { 8 };
            let mut mint = ix.clone();
            mint.accounts[mint_at].pubkey = other;
            assert_eq!(err(&mint), "SWAP_MINT_MISMATCH");
        }
        let mut redirected = swap_ix(JUP_ROUTE, &taker, &from, &to, terms);
        redirected.accounts[4].pubkey = Pubkey::new_unique();
        assert_eq!(
            swap_terms(&redirected, &taker, &from, &to).unwrap_err(),
            "SWAP_DESTINATION_NOT_OWN_ATA",
            "the optional final destination is ours or absent"
        );
        for disc in [
            JUP_ROUTE_WITH_TOKEN_LEDGER,
            JUP_SHARED_ROUTE_WITH_TOKEN_LEDGER,
            JUP_SHARED_EXACT_OUT_ROUTE,
        ] {
            let ix = swap_ix(disc, &taker, &from, &to, terms);
            assert_eq!(
                swap_terms(&ix, &taker, &from, &to).unwrap_err(),
                "SWAP_INSTRUCTION_NOT_EXACT_IN"
            );
        }
        let unknown = swap_ix([9; 8], &taker, &from, &to, terms);
        assert_eq!(
            swap_terms(&unknown, &taker, &from, &to).unwrap_err(),
            "SWAP_INSTRUCTION_UNPINNED: 0909090909090909"
        );
        // The pinned discriminators are the Anchor hashes of the IDL names.
        for (name, disc) in [
            ("route", JUP_ROUTE),
            ("shared_accounts_route", JUP_SHARED_ACCOUNTS_ROUTE),
            ("route_with_token_ledger", JUP_ROUTE_WITH_TOKEN_LEDGER),
            (
                "shared_accounts_route_with_token_ledger",
                JUP_SHARED_ROUTE_WITH_TOKEN_LEDGER,
            ),
            (
                "shared_accounts_exact_out_route",
                JUP_SHARED_EXACT_OUT_ROUTE,
            ),
        ] {
            let h = solana_sdk::hash::hash(format!("global:{name}").as_bytes());
            assert_eq!(h.to_bytes()[..8], disc, "{name}");
        }
    }

    /// A loopback `/build` that answers every request with `body` (raw JSON, as Jupiter does).
    fn serve_build(body: serde_json::Value) -> String {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let body = body.to_string();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) <= 2 {
                        break;
                    }
                }
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .as_bytes(),
                );
            }
        });
        url
    }

    /// FE-02/E2 through the real adapter (loopback `/build`, synthetic response): the route's
    /// floor is the instruction floor, or the provider threshold only when lower; a threshold
    /// above the instruction floor and instruction amounts that differ from the response fail
    /// closed; the enforced slippage is carried for the safety checks.
    #[test]
    fn fe02_build_floor_is_bound_to_the_swap_bytes() {
        let (taker, from, to) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let api = |ix: &Instruction| ApiInstruction {
            program_id: ix.program_id.to_string(),
            accounts: ix
                .accounts
                .iter()
                .map(|a| ApiAccount {
                    pubkey: a.pubkey.to_string(),
                    is_signer: a.is_signer,
                    is_writable: a.is_writable,
                })
                .collect(),
            data: base64::engine::general_purpose::STANDARD.encode(&ix.data),
        };
        let quote = |threshold: Option<&str>, ix: &Instruction| {
            let a = api(ix);
            let mut body = serde_json::json!({
                "inAmount": "500", "outAmount": "1000003", "swapMode": "ExactIn",
                "setupInstructions": [], "cleanupInstruction": null, "otherInstructions": [],
                "addressesByLookupTableAddress": null,
                "swapInstruction": {"programId": a.program_id, "data": a.data,
                    "accounts": a.accounts.iter().map(|m| serde_json::json!({
                        "pubkey": m.pubkey, "isSigner": m.is_signer, "isWritable": m.is_writable}))
                        .collect::<Vec<_>>()},
            });
            if let Some(t) = threshold {
                body["otherAmountThreshold"] = t.into();
            }
            let provider = JupiterV2RouteProvider::new(taker, None, 50, 64, false, false)
                .unwrap()
                .with_base_url(serve_build(body));
            provider.exact_in(
                &from,
                &to,
                500,
                &[],
                64,
                Instant::now() + Duration::from_secs(5),
            )
        };
        let ix = swap_ix(JUP_ROUTE, &taker, &from, &to, (500, 1_000_003, 50, 0));
        let route = quote(None, &ix).unwrap().unwrap();
        assert_eq!(
            route.minimum_executable_out,
            Some(995_002),
            "from the bytes"
        );
        assert_eq!(route.enforced_slippage_bps, Some(50));
        let route = quote(Some("990000"), &ix).unwrap().unwrap();
        assert_eq!(
            route.minimum_executable_out,
            Some(990_000),
            "a lower threshold"
        );
        assert!(quote(Some("995003"), &ix)
            .unwrap_err()
            .starts_with("SWAP_FLOOR_MISMATCH"));
        let lying = swap_ix(JUP_ROUTE, &taker, &from, &to, (500, 2_000_000, 50, 0));
        assert!(quote(Some("995002"), &lying)
            .unwrap_err()
            .starts_with("SWAP_TERMS_MISMATCH"));
        let fake = Instruction {
            data: vec![1],
            ..ix.clone()
        };
        assert_eq!(quote(None, &fake).unwrap_err(), "SWAP_DATA_SHORT");
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
