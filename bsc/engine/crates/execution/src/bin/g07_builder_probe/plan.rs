//! The probe plan: throwaway accounts, one pair of non-executable
//! transactions per case, the eleven cases and the canonical plan text whose
//! SHA-256 the user approves. Everything here is deterministic (RFC 6979
//! signatures over fixed inputs), and the plan text carries, per case, a
//! worked example computed through the same window, expiry, body and framing
//! code a live run uses, so the hash binds that code's behavior too.

use super::*;

pub(crate) const PROBE_SCHEMA: &str = "darkbot.g07.builder-probe.v4";
/// The G0.7 candidate's signed gas price (the documented builder floor).
pub(crate) const PROBE_GAS_PRICE_WEI: u64 = 50_000_000;
/// The price controls: far below any builder floor.
pub(crate) const UNDERPRICED_GAS_PRICE_WEI: u64 = 1;
/// The nonce of every probe transaction. No account ever reaches it, so the
/// transactions can never execute, whoever might fund the throwaway accounts.
pub(crate) const PROBE_NONCE: u64 = 1 << 40;
const _: () = assert!(
    PROBE_NONCE >= 1 << 32,
    "the probe nonce must stay unreachable"
);
pub(crate) const PROBE_GAS_LIMIT: u64 = 21_000;
/// Minimum pause between two probe requests (the wait for a fresh head comes
/// on top).
pub(crate) const PROBE_SPACING: Duration = Duration::from_secs(2);
/// New heads a case waits for until one passes the G0.7 freshness gates.
pub(crate) const PROBE_HEAD_ATTEMPTS: u32 = 3;

/// The G0.7 attempt-1 wallets (DRY evidence): the probe must never use them.
pub(crate) const G07_WALLETS: [&str; 2] = [
    "0x3e7b487ae5e1c49ab1cf6222bdb5cf016e86aacd",
    "0x9c8a0db41914beee52dd49790c4f087e252743de",
];

/// Throwaway key `n`: derived from a public seed, so it protects nothing and
/// holds nothing; the live run checks balance 0 and pending nonce 0.
pub(crate) fn throwaway_key(n: u8) -> SigningKey {
    let seed = keccak256(format!("darkbot-g07-builder-probe-v1/throwaway/{n}"));
    SigningKey::from_slice(seed.as_slice()).expect("keccak seed is a valid secp256k1 key")
}

/// The two throwaway keys of case `case_index` (0-based): every case has
/// its own senders, so no sender-based builder rule links two cases.
pub(crate) fn case_keys(case_index: usize) -> [SigningKey; 2] {
    let first = u8::try_from(2 * case_index).expect("fewer than 128 cases");
    [throwaway_key(first), throwaway_key(first + 1)]
}

/// All throwaway accounts of the plan, case by case.
pub(crate) fn throwaway_accounts() -> Vec<Address> {
    (0..CASES.len())
        .flat_map(|n| case_keys(n).map(|key| signer_address(&key)))
        .collect()
}

pub(crate) struct ProbeTx {
    pub(crate) from: Address,
    pub(crate) nonce: u64,
    pub(crate) gas_price: u64,
    pub(crate) signed: SignedLegacy,
}

impl ProbeTx {
    pub(crate) fn raw_hex(&self) -> String {
        format!("0x{}", hex::encode(&self.signed.raw))
    }
}

/// The transaction pair of case `case_index` (0-based): shaped like the G0.7
/// pair (two distinct senders, chain 56), signed by the G0.7 signer, each a
/// zero-value self-transfer at an unreachable nonce from the case's own
/// throwaway accounts, so no two cases share a sender, transaction or bundle.
pub(crate) fn case_txs(case_index: usize, gas_price: u64) -> [ProbeTx; 2] {
    let nonce = PROBE_NONCE;
    case_keys(case_index).map(|key| {
        let from = signer_address(&key);
        let signed = sign_legacy(
            &key,
            nonce,
            gas_price,
            PROBE_GAS_LIMIT,
            from,
            U256::ZERO,
            &[],
        );
        ProbeTx {
            from,
            nonce,
            gas_price,
            signed,
        }
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Builder {
    Club48,
    BlockRazor,
}

impl Builder {
    pub(crate) fn host(self) -> &'static str {
        match self {
            Builder::Club48 => CLUB48_HOST,
            Builder::BlockRazor => BLOCKRAZOR_HOST,
        }
    }
}

/// The expiry values of a case, as symbolic expressions over the send head.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Expiry {
    /// maxBlockNumber = HEAD+2, maxTimestamp = MAXTS (exactly G0.7).
    G07,
    /// maxBlockNumber = HEAD+2, no maxTimestamp field.
    NoTimestamp,
    /// maxBlockNumber = HEAD+2, maxTimestamp = MAXTS-1 (without the +1).
    TimestampMinusOne,
    /// maxBlockNumber = HEAD-1 (already mined), maxTimestamp = MAXTS.
    PastBlock,
    /// maxBlockNumber = HEAD+2, maxTimestamp = HEAD_S-2 (already past).
    PastTimestamp,
}

pub(crate) struct Case {
    pub(crate) id: &'static str,
    pub(crate) builder: Builder,
    pub(crate) expiry: Expiry,
    pub(crate) gas_price: u64,
    /// A 48Club boolean field sent as the string "true" instead of `true`.
    pub(crate) string_field: Option<&'static str>,
    pub(crate) question: &'static str,
}

pub(crate) const CASES: [Case; 11] = [
    Case {
        id: "48club-g07-shape",
        builder: Builder::Club48,
        expiry: Expiry::G07,
        gas_price: PROBE_GAS_PRICE_WEI,
        string_field: None,
        question: "Does 48Club parse the exact G0.7 request (method, fields, integer maxBlockNumber, maxTimestamp, noMerge, positionFirst)?",
    },
    Case {
        id: "48club-no-maxtimestamp",
        builder: Builder::Club48,
        expiry: Expiry::NoTimestamp,
        gas_price: PROBE_GAS_PRICE_WEI,
        string_field: None,
        question: "Does 48Club require maxTimestamp (web3 docs: at least one valid expiry with maxTimestamp)?",
    },
    Case {
        id: "48club-maxtimestamp-minus-one",
        builder: Builder::Club48,
        expiry: Expiry::TimestampMinusOne,
        gas_price: PROBE_GAS_PRICE_WEI,
        string_field: None,
        question: "Does 48Club refuse maxTimestamp without the +1 second (answer depends on the head's millisecond phase)?",
    },
    Case {
        id: "48club-past-block",
        builder: Builder::Club48,
        expiry: Expiry::PastBlock,
        gas_price: PROBE_GAS_PRICE_WEI,
        string_field: None,
        question: "Does 48Club check maxBlockNumber at submission (expired block refused)?",
    },
    Case {
        id: "48club-past-timestamp",
        builder: Builder::Club48,
        expiry: Expiry::PastTimestamp,
        gas_price: PROBE_GAS_PRICE_WEI,
        string_field: None,
        question: "Does 48Club check maxTimestamp at submission (past second refused)?",
    },
    Case {
        id: "48club-underpriced-control",
        builder: Builder::Club48,
        expiry: Expiry::G07,
        gas_price: UNDERPRICED_GAS_PRICE_WEI,
        string_field: None,
        question: "Does 48Club check the gas price at submission (1 wei refused while 0.05 gwei is not)?",
    },
    Case {
        id: "48club-nomerge-type-control",
        builder: Builder::Club48,
        expiry: Expiry::G07,
        gas_price: PROBE_GAS_PRICE_WEI,
        string_field: Some("noMerge"),
        question: "Does 48Club parse noMerge as a typed field (a string value refused)?",
    },
    Case {
        id: "48club-positionfirst-type-control",
        builder: Builder::Club48,
        expiry: Expiry::G07,
        gas_price: PROBE_GAS_PRICE_WEI,
        string_field: Some("positionFirst"),
        question: "Does 48Club parse positionFirst as a typed field (a string value refused)?",
    },
    Case {
        id: "blockrazor-g07-shape",
        builder: Builder::BlockRazor,
        expiry: Expiry::G07,
        gas_price: PROBE_GAS_PRICE_WEI,
        string_field: None,
        question: "Does BlockRazor parse the exact G0.7 request (method, fields, integer maxBlockNumber, Authorization)?",
    },
    Case {
        id: "blockrazor-past-block",
        builder: Builder::BlockRazor,
        expiry: Expiry::PastBlock,
        gas_price: PROBE_GAS_PRICE_WEI,
        string_field: None,
        question: "Does BlockRazor check maxBlockNumber at submission (documented -38000 below the current block)?",
    },
    Case {
        id: "blockrazor-underpriced-control",
        builder: Builder::BlockRazor,
        expiry: Expiry::G07,
        gas_price: UNDERPRICED_GAS_PRICE_WEI,
        string_field: None,
        question: "Does BlockRazor check the gas price at submission (1 wei refused while 0.05 gwei is not)?",
    },
];

/// maxBlockNumber and the maxTimestamp actually sent (BlockRazor: none) of
/// a case for a send head.
pub(crate) fn case_expiry(
    case: &Case,
    head: &ChainHead,
    window: &BundleWindow,
) -> (u64, Option<u64>) {
    let head_s = head.milli_timestamp / 1000;
    let (max_block, max_timestamp) = match case.expiry {
        Expiry::G07 => (window.max_block, Some(window.max_timestamp)),
        Expiry::NoTimestamp => (window.max_block, None),
        Expiry::TimestampMinusOne => (window.max_block, Some(window.max_timestamp - 1)),
        Expiry::PastBlock => (head.number - 1, Some(window.max_timestamp)),
        Expiry::PastTimestamp => (window.max_block, Some(head_s - 2)),
    };
    match case.builder {
        Builder::Club48 => (max_block, max_timestamp),
        Builder::BlockRazor => (max_block, None),
    }
}

fn expiry_expressions(case: &Case) -> (&'static str, Option<&'static str>) {
    let (block, timestamp) = match case.expiry {
        Expiry::G07 => ("HEAD+2", Some("MAXTS")),
        Expiry::NoTimestamp => ("HEAD+2", None),
        Expiry::TimestampMinusOne => ("HEAD+2", Some("MAXTS-1")),
        Expiry::PastBlock => ("HEAD-1", Some("MAXTS")),
        Expiry::PastTimestamp => ("HEAD+2", Some("HEAD_S-2")),
    };
    match case.builder {
        Builder::Club48 => (block, timestamp),
        Builder::BlockRazor => (block, None),
    }
}

/// The exact body of a case: the G0.7 body builder of that builder, with only
/// the case's change applied (expiry values; for two 48Club cases the
/// maxTimestamp field removed or one boolean sent as a string).
pub(crate) fn case_body(
    case: &Case,
    txs: &[ProbeTx; 2],
    max_block: u64,
    max_timestamp: Option<u64>,
) -> String {
    let raw = [txs[0].raw_hex(), txs[1].raw_hex()];
    let raws = [raw[0].as_str(), raw[1].as_str()];
    let request_id = txs[1].signed.hash.to_string();
    if case.builder == Builder::BlockRazor {
        return blockrazor_bundle_body(&request_id, raws, max_block);
    }
    let body = club48_bundle_body(&request_id, raws, max_block, max_timestamp.unwrap_or(0));
    if max_timestamp.is_some() && case.string_field.is_none() {
        return body;
    }
    // Apply only fixed, known wire fragments; no general JSON serializer.
    let mut body = body;
    if max_timestamp.is_none() {
        body = body.replacen("\"maxTimestamp\":0,", "", 1);
    }
    if let Some(field) = case.string_field {
        assert!(matches!(field, "noMerge" | "positionFirst"));
        body = body.replacen(
            &format!("\"{field}\":true"),
            &format!("\"{field}\":\"true\""),
            1,
        );
    }
    body
}

const BLOCK_SENTINEL: u64 = 7_777_777_771;
const TIMESTAMP_SENTINEL: u64 = 7_777_777_772;

/// The body of a case with its head-dependent values as expressions.
pub(crate) fn case_template(case: &Case, txs: &[ProbeTx; 2]) -> String {
    let (block_expr, ts_expr) = expiry_expressions(case);
    let body = case_body(
        case,
        txs,
        BLOCK_SENTINEL,
        ts_expr.map(|_| TIMESTAMP_SENTINEL),
    );
    let body = body.replace(
        &format!("\"maxBlockNumber\":{BLOCK_SENTINEL}"),
        &format!("\"maxBlockNumber\":{{{block_expr}}}"),
    );
    match ts_expr {
        Some(expr) => body.replace(
            &format!("\"maxTimestamp\":{TIMESTAMP_SENTINEL}"),
            &format!("\"maxTimestamp\":{{{expr}}}"),
        ),
        None => body,
    }
}

/// The synthetic head of the worked examples.
const EXAMPLE_HEAD: u64 = 1_000;
const EXAMPLE_HEAD_MS: u64 = 5_000_200;
const EXAMPLE_LOCAL_MS: u64 = 5_000_300;

/// Everything one case sends for a send head: its transactions, expiry
/// values, body, host and the full HTTP request (Authorization only for
/// BlockRazor). The live run and the worked examples both build requests
/// only here.
pub(crate) struct CaseRequest {
    pub(crate) txs: [ProbeTx; 2],
    pub(crate) max_block: u64,
    pub(crate) max_timestamp: Option<u64>,
    pub(crate) body: String,
    pub(crate) host: &'static str,
    pub(crate) request: Vec<u8>,
}

pub(crate) fn case_request(
    case_index: usize,
    case: &Case,
    head: &ChainHead,
    window: &BundleWindow,
    authorization: &str,
) -> CaseRequest {
    let txs = case_txs(case_index, case.gas_price);
    let (max_block, max_timestamp) = case_expiry(case, head, window);
    let body = case_body(case, &txs, max_block, max_timestamp);
    let host = case.builder.host();
    let auth = (case.builder == Builder::BlockRazor).then_some(authorization);
    let request = http_request(host, &body, auth);
    CaseRequest {
        txs,
        max_block,
        max_timestamp,
        body,
        host,
        request,
    }
}

/// A case worked through the live code path for the synthetic head (token
/// "REDACTED"): the request it would send and that request's SHA-256.
pub(crate) fn worked_example(case_index: usize, case: &Case) -> (CaseRequest, String) {
    let head = ChainHead {
        number: EXAMPLE_HEAD,
        hash: format!("0x{}", "ab".repeat(32)),
        milli_timestamp: EXAMPLE_HEAD_MS,
    };
    let window = bundle_window(head.number, &head.hash, &head, EXAMPLE_LOCAL_MS, 0)
        .expect("the synthetic head passes the gates");
    let request = case_request(case_index, case, &head, &window, "REDACTED");
    let sha = sha256_hex(&request.request);
    (request, sha)
}

/// The files the approval binds: the probe, the G0.7 modules it includes,
/// the library guard module that supplies its hash function, the lock file
/// (versions and checksums of every external crate) and the toolchain pin.
/// Their SHA-256s are part of the plan, so the approval binds this exact code
/// and dependency set.
const PROBE_SOURCES: [(&str, &str); 11] = [
    ("g07_builder_probe/main.rs", include_str!("main.rs")),
    ("g07_builder_probe/plan.rs", include_str!("plan.rs")),
    (
        "g07_mainnet_self_launch_buy/shared.rs",
        include_str!("../g07_mainnet_self_launch_buy/shared.rs"),
    ),
    (
        "g07_mainnet_self_launch_buy/builder_submission.rs",
        include_str!("../g07_mainnet_self_launch_buy/builder_submission.rs"),
    ),
    (
        "g07_mainnet_self_launch_buy/chain_reads.rs",
        include_str!("../g07_mainnet_self_launch_buy/chain_reads.rs"),
    ),
    (
        "g07_mainnet_self_launch_buy/inputs.rs",
        include_str!("../g07_mainnet_self_launch_buy/inputs.rs"),
    ),
    (
        "g07_mainnet_self_launch_buy/transactions.rs",
        include_str!("../g07_mainnet_self_launch_buy/transactions.rs"),
    ),
    (
        "execution/src/acceptance_guard.rs",
        include_str!("../../acceptance_guard.rs"),
    ),
    // BSC-003: the wire bodies and expiry offset the probe sends.
    (
        "execution/src/bundle_wire.rs",
        include_str!("../../bundle_wire.rs"),
    ),
    (
        "engine/Cargo.lock",
        include_str!("../../../../../Cargo.lock"),
    ),
    (
        "engine/rust-toolchain.toml",
        include_str!("../../../../../rust-toolchain.toml"),
    ),
];

/// The canonical plan text: everything a live run sends and checks. Its
/// SHA-256 is what the user approves.
pub(crate) fn plan_text() -> String {
    let mut plan = String::new();
    let mut line = |s: String| {
        plan.push_str(&s);
        plan.push('\n');
    };
    line(format!("schema: {PROBE_SCHEMA}"));
    line("purpose: G0.7 pre-arm checks 6 and 10, submission-time only: eth_sendBundle method, request format, typed noMerge/positionFirst, and whether expiry values and the gas price are checked at submission, for 48Club and BlockRazor. It cannot show building-time behavior (noMerge/positionFirst effect, maxBlockNumber enforcement next to maxTimestamp, inclusion) nor that 0.05 gwei suffices.".into());
    line(format!(
        "non_executable: every case has its own two throwaway accounts derived from public seeds (never the G0.7 wallets {} / {}); a live run first checks read-only that all {} have balance 0 and pending nonce 0, else STOP; every transaction has nonce {PROBE_NONCE} (2^40, unreachable), value 0, to = sender, gas limit {PROBE_GAS_LIMIT}, chain id {CHAIN_ID}, signed by the G0.7 signer; no two cases share a sender, transaction or bundle",
        G07_WALLETS[0],
        G07_WALLETS[1],
        throwaway_accounts().len()
    ));
    line(format!(
        "head: HEAD = a new block the G0.7 head poller sees right before the request; HEAD_S = its timestamp in seconds; MAXTS = floor((HEAD_MS + {MAX_BLOCK_OFFSET} * {BLOCK_INTERVAL_MS}) / 1000) + 1 (the G0.7 rule, maxBlockNumber offset {MAX_BLOCK_OFFSET}). The G0.7 freshness gates apply (head age <= {MAX_HEAD_AGE_MS} ms, local clock lag <= {MAX_LOCAL_CLOCK_LAG_MS} ms, monotonic staleness <= {MAX_HEAD_AGE_MS} ms); a case waits for up to {PROBE_HEAD_ATTEMPTS} new heads for one that passes, else it is skipped, not sent."
    ));
    line(format!(
        "framing: G0.7 http_request over G0.7 connect_tls; per case a new connection, opened before the wait for the head (as G0.7 prewarms) and checked live before the write; Authorization header for BlockRazor only, its value from DARKBOT_G07_BLOCKRAZOR_AUTHORIZATION and never recorded; {} requests in total, each sent at most once, in this order, at least {} s apart; response read, no retry",
        CASES.len(),
        PROBE_SPACING.as_secs()
    ));
    line("single_use: before the first request a live run latches bsc/.local/g07/builder-probe-<first 12 hex of this plan's SHA-256>/ (after its read-only checks); the same plan never runs twice".into());
    line(format!(
        "worked_example: each case below is also built for HEAD={EXAMPLE_HEAD}, HEAD_MS={EXAMPLE_HEAD_MS}, local clock {EXAMPLE_LOCAL_MS} ms by the same request function the live run uses (window, expiry, transactions, body, host, Authorization choice and framing; token replaced by REDACTED); request_sha256 is the SHA-256 of that full HTTP request"
    ));
    for (name, source) in PROBE_SOURCES {
        line(format!(
            "source: {name} sha256={}",
            sha256_hex(source.as_bytes())
        ));
    }
    for (n, case) in CASES.iter().enumerate() {
        let (example, ex_sha) = worked_example(n, case);
        let txs = &example.txs;
        line(format!(
            "case{} {}: POST https://{}/ authorization={} gas_price={} | {}",
            n + 1,
            case.id,
            case.builder.host(),
            if case.builder == Builder::BlockRazor {
                "yes"
            } else {
                "none"
            },
            case.gas_price,
            case.question
        ));
        for (k, tx) in txs.iter().enumerate() {
            line(format!(
                "  tx{k}: from={} nonce={} gas_price={} hash={} raw={}",
                tx.from.to_string().to_lowercase(),
                tx.nonce,
                tx.gas_price,
                tx.signed.hash,
                tx.raw_hex()
            ));
        }
        line(format!("  body={}", case_template(case, txs)));
        line(format!(
            "  example: maxBlockNumber={} maxTimestamp={} request_sha256={ex_sha}",
            example.max_block,
            example
                .max_timestamp
                .map_or_else(|| "none".to_string(), |t| t.to_string())
        ));
    }
    plan
}

/// Coarse class of a builder answer; the raw answer is always recorded and
/// remains the evidence. Specific phrases come before generic ones, so an
/// answer about the zero balance ("insufficient funds for gas * price") or
/// the nonce is not read as a price or expiry answer.
pub(crate) fn classify(http_status: &str, body: &str) -> &'static str {
    use serde_json::Value; // External response decoding boundary only.
    let code = http_status.split_whitespace().nth(1).unwrap_or("");
    if !code.starts_with('2') {
        return "HTTP_ERROR";
    }
    let Ok(v) = serde_json::from_str::<Value>(body) else {
        return "UNPARSEABLE";
    };
    let Some(err) = v.get("error").filter(|e| !e.is_null()) else {
        return if v.get("result").is_some_and(|r| !r.is_null()) {
            "ACCEPTED"
        } else {
            "UNPARSEABLE"
        };
    };
    let rpc_code = err.get("code").and_then(Value::as_i64);
    // The message, any data, or a bare string error.
    let message = match err {
        Value::String(text) => text.to_ascii_lowercase(),
        _ => format!(
            "{} {}",
            err.get("message").and_then(Value::as_str).unwrap_or(""),
            err.get("data").and_then(Value::as_str).unwrap_or_default()
        )
        .to_ascii_lowercase(),
    };
    let has = |keys: &[&str]| keys.iter().any(|k| message.contains(k));
    match rpc_code {
        Some(-32601) => return "METHOD_REJECTED",
        Some(-32700 | -32600 | -32602) => return "FORMAT_REJECTED",
        _ => {}
    }
    if has(&[
        "insufficient funds",
        "nonce too high",
        "nonce too low",
        "invalid nonce",
    ]) {
        return "EXECUTION_REJECTED";
    }
    if has(&[
        "underpriced",
        "price too low",
        "price is too low",
        "gas price",
        "gasprice",
        "gwei",
        "fee too low",
        "min gas price",
    ]) {
        return "PRICE_REJECTED";
    }
    if has(&["maxblock", "maxtimestamp", "expire", "outdated"]) {
        return "EXPIRY_REJECTED";
    }
    if has(&[
        "nonce",
        "balance",
        "funds",
        "simulat",
        "revert",
        "intrinsic gas",
    ]) {
        return "EXECUTION_REJECTED";
    }
    if has(&["block number", "timestamp", "current block"]) {
        return "EXPIRY_REJECTED";
    }
    if rpc_code == Some(-38000) {
        return "BUNDLE_REJECTED_UNSPECIFIED";
    }
    "OTHER_REJECTED"
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn head(number: u64, milli_timestamp: u64) -> ChainHead {
        ChainHead {
            number,
            hash: format!("0x{}", "ab".repeat(32)),
            milli_timestamp,
        }
    }

    #[test]
    fn throwaway_accounts_are_fixed_distinct_and_never_the_g07_wallets() {
        let accounts = throwaway_accounts();
        assert_eq!(accounts, throwaway_accounts());
        assert_eq!(accounts.len(), 2 * CASES.len());
        let unique: std::collections::BTreeSet<_> = accounts.iter().collect();
        assert_eq!(unique.len(), accounts.len());
        for from in accounts {
            let from = from.to_string().to_lowercase();
            assert!(!G07_WALLETS.contains(&from.as_str()), "{from}");
        }
    }

    #[test]
    fn every_case_has_its_own_non_executable_transactions() {
        let mut hashes = std::collections::BTreeSet::new();
        for (n, case) in CASES.iter().enumerate() {
            let txs = case_txs(n, case.gas_price);
            for (k, tx) in txs.iter().enumerate() {
                assert_eq!(tx.nonce, PROBE_NONCE);
                assert_eq!(tx.gas_price, case.gas_price);
                assert_eq!(tx.from, throwaway_accounts()[2 * n + k]);
                let expected = sign_legacy(
                    &throwaway_key((2 * n + k) as u8),
                    tx.nonce,
                    tx.gas_price,
                    PROBE_GAS_LIMIT,
                    tx.from,
                    U256::ZERO,
                    &[],
                );
                assert_eq!(tx.signed.raw, expected.raw);
                let header = if tx.signed.raw[0] <= 0xf7 {
                    1
                } else {
                    1 + (tx.signed.raw[0] - 0xf7) as usize
                };
                assert!(tx.signed.raw[header..].starts_with(&rlp_u64(tx.nonce)));
                assert!(hashes.insert(tx.signed.hash), "shared transaction");
            }
        }
        assert_eq!(hashes.len(), 2 * CASES.len());
    }

    #[test]
    fn the_g07_shape_cases_are_exactly_what_g07_sends() {
        let h = head(1_000, 5_000_200);
        let w = bundle_window(1_000, &h.hash, &h, 5_000_300, 0).unwrap();
        for n in [0usize, 8] {
            let case = &CASES[n];
            let txs = case_txs(n, case.gas_price);
            let raw = [txs[0].raw_hex(), txs[1].raw_hex()];
            let id = txs[1].signed.hash.to_string();
            let (b, ts) = case_expiry(case, &h, &w);
            let g07 = match case.builder {
                Builder::Club48 => {
                    club48_bundle_body(&id, [&raw[0], &raw[1]], w.max_block, w.max_timestamp)
                }
                Builder::BlockRazor => blockrazor_bundle_body(&id, [&raw[0], &raw[1]], w.max_block),
            };
            assert_eq!(case_body(case, &txs, b, ts), g07, "{}", case.id);
        }
    }

    #[test]
    fn each_variant_changes_only_what_it_names() {
        let h = head(1_000, 5_000_200);
        let w = bundle_window(1_000, &h.hash, &h, 5_000_300, 0).unwrap();
        assert_eq!((w.max_block, w.max_timestamp), (1_002, 5_002));
        let values: Vec<_> = CASES.iter().map(|c| case_expiry(c, &h, &w)).collect();
        assert_eq!(values[0], (1_002, Some(5_002)));
        assert_eq!(values[1], (1_002, None));
        assert_eq!(values[2], (1_002, Some(5_001)));
        assert_eq!(values[3], (999, Some(5_002)));
        assert_eq!(values[4], (1_002, Some(4_998)));
        assert_eq!(values[8], (1_002, None));
        assert_eq!(values[9], (999, None));
        let body = |n: usize| -> Value {
            let case = &CASES[n];
            let (b, ts) = values[n];
            serde_json::from_str(&case_body(case, &case_txs(n, case.gas_price), b, ts)).unwrap()
        };
        // The plain G0.7 48Club body over the same transactions.
        let g07_with = |n: usize| -> Value {
            let (b, _) = values[n];
            let txs = case_txs(n, CASES[n].gas_price);
            serde_json::from_str(&case_body(&CASES[0], &txs, b, Some(5_002))).unwrap()
        };
        let mut stripped = g07_with(1);
        stripped["params"][0]
            .as_object_mut()
            .unwrap()
            .remove("maxTimestamp");
        assert_eq!(body(1), stripped);
        for (n, field) in [(6usize, "noMerge"), (7, "positionFirst")] {
            let mut expected = g07_with(n);
            expected["params"][0][field] = json!("true");
            assert_eq!(body(n), expected, "{field}");
        }
        assert_eq!(body(5), g07_with(5));
        for n in 8..CASES.len() {
            let (b, ts) = values[n];
            let text = case_body(&CASES[n], &case_txs(n, CASES[n].gas_price), b, ts);
            assert!(
                !text.contains("maxTimestamp") && !text.contains("noMerge"),
                "{}",
                CASES[n].id
            );
        }
    }

    #[test]
    fn the_plan_is_deterministic_and_binds_each_case_and_its_worked_example() {
        let plan = plan_text();
        assert_eq!(plan, plan_text());
        assert_eq!(plan.matches("\ncase").count(), CASES.len());
        assert_eq!(plan.matches("  example: ").count(), CASES.len());
        assert_eq!(plan.matches("authorization=yes").count(), 3);
        // Two price controls: their case line and both transaction lines.
        assert_eq!(plan.matches("gas_price=1 ").count(), 6);
        assert_eq!(plan.matches("\nsource: ").count(), PROBE_SOURCES.len());
        assert!(plan.contains("\"maxBlockNumber\":{HEAD+2}"));
        assert!(plan.contains("\"maxTimestamp\":{MAXTS}"));
        assert!(plan.contains("\"maxBlockNumber\":{HEAD-1}"));
        assert!(plan.contains("\"maxTimestamp\":{HEAD_S-2}"));
        assert!(plan.contains("\"noMerge\":\"true\""));
        assert!(plan.contains("\"positionFirst\":\"true\""));
        assert!(!plan.contains(&BLOCK_SENTINEL.to_string()));
        assert!(!plan.contains(&TIMESTAMP_SENTINEL.to_string()));
        assert!(plan.contains("example: maxBlockNumber=1002 maxTimestamp=5002 request_sha256="));
        assert!(plan.contains("example: maxBlockNumber=999 maxTimestamp=none request_sha256="));
    }

    #[test]
    fn answers_are_classified_with_specific_phrases_first() {
        let ok = "HTTP/1.1 200 OK";
        let err = |code: i64, message: &str| {
            json!({"jsonrpc":"2.0","id":1,"error":{"code":code,"message":message}}).to_string()
        };
        let cases = [
            (
                ok,
                r#"{"jsonrpc":"2.0","id":1,"result":{"bundleHash":"0x1"}}"#.to_string(),
                "ACCEPTED",
            ),
            (
                ok,
                r#"{"jsonrpc":"2.0","id":1,"result":"0x1","error":null}"#.to_string(),
                "ACCEPTED",
            ),
            (
                ok,
                err(-32601, "the method eth_sendBundle does not exist"),
                "METHOD_REJECTED",
            ),
            (
                ok,
                err(
                    -32602,
                    "invalid argument 0: json: cannot unmarshal string into Go struct field .noMerge of type bool",
                ),
                "FORMAT_REJECTED",
            ),
            (
                ok,
                err(
                    -32000,
                    "insufficient funds for gas * price + value: balance 0, tx cost 1050000000000, overshot 1050000000000",
                ),
                "EXECUTION_REJECTED",
            ),
            (
                ok,
                err(
                    -32000,
                    "insufficient funds for gas * price + value: address 0x69a5f4187591f66c8874605692b911bb765bd1cf have 0 want 1050000000000",
                ),
                "EXECUTION_REJECTED",
            ),
            (
                ok,
                err(
                    -38000,
                    "non-reverting tx in bundle failed: nonce too high: address 0x69a5, tx: 1099511627776 state: 0",
                ),
                "EXECUTION_REJECTED",
            ),
            (
                ok,
                err(-38000, "bundle gas price is too low"),
                "PRICE_REJECTED",
            ),
            (ok, err(-32000, "transaction underpriced"), "PRICE_REJECTED"),
            (
                ok,
                err(
                    -38000,
                    "maxBlockNumber should not be smaller than current block",
                ),
                "EXPIRY_REJECTED",
            ),
            (
                ok,
                err(
                    -38000,
                    "the maxTimestamp should not be less than currentBlockTimestamp",
                ),
                "EXPIRY_REJECTED",
            ),
            (
                ok,
                err(-38000, "bundle already exist"),
                "BUNDLE_REJECTED_UNSPECIFIED",
            ),
            (ok, err(-32000, "something else"), "OTHER_REJECTED"),
            (ok, err(-32000, "nonce 7: bundle gas price too low"), "PRICE_REJECTED"),
            (ok, err(-32000, "intrinsic gas too low"), "EXECUTION_REJECTED"),
            (ok, err(-32000, "gas limit reached"), "OTHER_REJECTED"),
            (ok, r#"{"jsonrpc":"2.0","id":1,"error":"bundle gasPrice lower than required"}"#.to_string(), "PRICE_REJECTED"),
            (ok, r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"rejected","data":"insufficient funds for gas"}}"#.to_string(), "EXECUTION_REJECTED"),
            (
                "HTTP/1.1 401 Unauthorized",
                err(-32000, "invalid auth"),
                "HTTP_ERROR",
            ),
            (
                "HTTP/1.1 429 Too Many Requests",
                "slow down".to_string(),
                "HTTP_ERROR",
            ),
            (ok, "not json".to_string(), "UNPARSEABLE"),
            (ok, r#"{"jsonrpc":"2.0","id":1}"#.to_string(), "UNPARSEABLE"),
        ];
        for (status, body, class) in cases {
            assert_eq!(classify(status, &body), class, "{body}");
        }
    }
}
