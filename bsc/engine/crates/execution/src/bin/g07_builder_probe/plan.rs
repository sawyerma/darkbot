//! The probe plan: throwaway accounts, the two non-executable transactions,
//! the seven cases and the canonical plan text whose SHA-256 the user
//! approves. Everything here is deterministic (RFC 6979 signatures over fixed
//! inputs), so the same binary always presents the same plan.

use super::*;

pub(crate) const PROBE_SCHEMA: &str = "darkbot.g07.builder-probe.v1";
/// The G0.7 candidate's signed gas price (the documented builder floor).
pub(crate) const PROBE_GAS_PRICE_WEI: u64 = 50_000_000;
/// A nonce no account ever reaches: the probe transactions can never execute,
/// whoever might fund the throwaway accounts.
pub(crate) const PROBE_NONCE: u64 = 1 << 40;
pub(crate) const PROBE_GAS_LIMIT: u64 = 21_000;
/// Pause between two probe requests.
pub(crate) const PROBE_SPACING: Duration = Duration::from_secs(2);

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

pub(crate) struct ProbeTx {
    pub(crate) from: Address,
    pub(crate) signed: SignedLegacy,
}

impl ProbeTx {
    pub(crate) fn raw_hex(&self) -> String {
        format!("0x{}", hex::encode(&self.signed.raw))
    }
}

/// Two transactions shaped like the G0.7 pair (two distinct senders, chain
/// 56, the G0.7 gas price), signed by the G0.7 signer, each a zero-value
/// self-transfer at an unreachable nonce.
pub(crate) fn probe_txs() -> [ProbeTx; 2] {
    [0u8, 1].map(|n| {
        let key = throwaway_key(n);
        let from = signer_address(&key);
        let signed = sign_legacy(
            &key,
            PROBE_NONCE,
            PROBE_GAS_PRICE_WEI,
            PROBE_GAS_LIMIT,
            from,
            U256::ZERO,
            &[],
        );
        ProbeTx { from, signed }
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
    pub(crate) question: &'static str,
}

pub(crate) const CASES: [Case; 7] = [
    Case {
        id: "48club-g07-shape",
        builder: Builder::Club48,
        expiry: Expiry::G07,
        question: "Does 48Club accept the exact G0.7 request (method, fields, integer maxBlockNumber, maxTimestamp, noMerge, positionFirst, 0.05 gwei)?",
    },
    Case {
        id: "48club-no-maxtimestamp",
        builder: Builder::Club48,
        expiry: Expiry::NoTimestamp,
        question: "Does 48Club require maxTimestamp (web3 docs: at least one valid expiry with maxTimestamp)?",
    },
    Case {
        id: "48club-maxtimestamp-minus-one",
        builder: Builder::Club48,
        expiry: Expiry::TimestampMinusOne,
        question: "Does 48Club accept maxTimestamp without the +1 second (the tighter variant)?",
    },
    Case {
        id: "48club-past-block",
        builder: Builder::Club48,
        expiry: Expiry::PastBlock,
        question: "Does 48Club check maxBlockNumber at submission (expired block refused)?",
    },
    Case {
        id: "48club-past-timestamp",
        builder: Builder::Club48,
        expiry: Expiry::PastTimestamp,
        question: "Does 48Club check maxTimestamp at submission (past second refused)?",
    },
    Case {
        id: "blockrazor-g07-shape",
        builder: Builder::BlockRazor,
        expiry: Expiry::G07,
        question: "Does BlockRazor accept the exact G0.7 request (method, fields, integer maxBlockNumber, Authorization, 0.05 gwei)?",
    },
    Case {
        id: "blockrazor-past-block",
        builder: Builder::BlockRazor,
        expiry: Expiry::PastBlock,
        question: "Does BlockRazor check maxBlockNumber at submission (documented -38000 below the current block)?",
    },
];

/// maxBlockNumber and maxTimestamp of `expiry` for a send head.
pub(crate) fn expiry_values(
    expiry: Expiry,
    head: &ChainHead,
    window: &BundleWindow,
) -> (u64, Option<u64>) {
    let head_s = head.milli_timestamp / 1000;
    match expiry {
        Expiry::G07 => (window.max_block, Some(window.max_timestamp)),
        Expiry::NoTimestamp => (window.max_block, None),
        Expiry::TimestampMinusOne => (window.max_block, Some(window.max_timestamp - 1)),
        Expiry::PastBlock => (head.number - 1, Some(window.max_timestamp)),
        Expiry::PastTimestamp => (window.max_block, Some(head_s - 2)),
    }
}

fn expiry_expressions(expiry: Expiry) -> (&'static str, Option<&'static str>) {
    match expiry {
        Expiry::G07 => ("HEAD+2", Some("MAXTS")),
        Expiry::NoTimestamp => ("HEAD+2", None),
        Expiry::TimestampMinusOne => ("HEAD+2", Some("MAXTS-1")),
        Expiry::PastBlock => ("HEAD-1", Some("MAXTS")),
        Expiry::PastTimestamp => ("HEAD+2", Some("HEAD_S-2")),
    }
}

/// The exact body of a case: the G0.7 body builder of that builder, with
/// only the expiry values (and, for one case, the maxTimestamp field) changed.
pub(crate) fn case_body(
    case: &Case,
    txs: &[ProbeTx; 2],
    max_block: u64,
    max_timestamp: Option<u64>,
) -> String {
    let raw = [txs[0].raw_hex(), txs[1].raw_hex()];
    let raws = [raw[0].as_str(), raw[1].as_str()];
    let request_id = txs[1].signed.hash.to_string();
    match (case.builder, max_timestamp) {
        (Builder::Club48, Some(ts)) => club48_bundle_body(&request_id, raws, max_block, ts),
        (Builder::Club48, None) => {
            // The G0.7 body with the maxTimestamp field removed, nothing else.
            let mut body: Value =
                serde_json::from_str(&club48_bundle_body(&request_id, raws, max_block, 0))
                    .expect("G0.7 body is JSON");
            if let Some(params) = body.pointer_mut("/params/0").and_then(Value::as_object_mut) {
                params.remove("maxTimestamp");
            }
            body.to_string()
        }
        (Builder::BlockRazor, _) => blockrazor_bundle_body(&request_id, raws, max_block),
    }
}

const BLOCK_SENTINEL: u64 = 7_777_777_771;
const TIMESTAMP_SENTINEL: u64 = 7_777_777_772;

/// The body of a case with its head-dependent values as expressions.
pub(crate) fn case_template(case: &Case, txs: &[ProbeTx; 2]) -> String {
    let (block_expr, ts_expr) = expiry_expressions(case.expiry);
    let ts = ts_expr.map(|_| TIMESTAMP_SENTINEL);
    let body = case_body(case, txs, BLOCK_SENTINEL, ts);
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

/// The canonical plan text: everything a live run sends and checks. Its
/// SHA-256 is what the user approves.
pub(crate) fn plan_text(txs: &[ProbeTx; 2]) -> String {
    let mut plan = String::new();
    let mut line = |s: String| {
        plan.push_str(&s);
        plan.push('\n');
    };
    line(format!("schema: {PROBE_SCHEMA}"));
    line("purpose: G0.7 pre-arm checks 6 and 10, submission-time only: eth_sendBundle method, request format and expiry acceptance of 48Club and BlockRazor. It cannot show building-time behavior (noMerge/positionFirst effect, maxBlockNumber enforcement next to maxTimestamp, inclusion).".into());
    line(format!(
        "non_executable: two throwaway accounts derived from public seeds (never the G0.7 wallets {} / {}); a live run first checks read-only that each has balance 0 and pending nonce 0, else STOP; nonce {PROBE_NONCE} (2^40, unreachable); value 0; to = sender; gas limit {PROBE_GAS_LIMIT}; gas price {PROBE_GAS_PRICE_WEI} wei (the G0.7 candidate); chain id {CHAIN_ID}; signed by the G0.7 signer",
        G07_WALLETS[0], G07_WALLETS[1]
    ));
    for (n, tx) in txs.iter().enumerate() {
        line(format!(
            "tx{n}: from={} hash={} raw={}",
            tx.from.to_string().to_lowercase(),
            tx.signed.hash,
            tx.raw_hex()
        ));
    }
    line(format!(
        "request_id: {} (the second transaction's hash, like G0.7's BUY hash)",
        txs[1].signed.hash
    ));
    line("head: HEAD = the first new block the G0.7 head poller sees right before each request; HEAD_S = its timestamp in seconds; MAXTS = floor((HEAD_MS + 900) / 1000) + 1 (the G0.7 rule). A request is skipped, not sent, if the G0.7 freshness gates fail (head age <= 450 ms, local clock lag <= 150 ms, monotonic staleness <= 450 ms).".into());
    line(format!(
        "framing: G0.7 http_request over G0.7 connect_tls; a new connection per request; Authorization header for BlockRazor only, its value from DARKBOT_G07_BLOCKRAZOR_AUTHORIZATION and never recorded; {} requests in total, each sent once, in this order, {} s apart; response read, no retry",
        CASES.len(),
        PROBE_SPACING.as_secs()
    ));
    line("single_use: a live run latches bsc/.local/g07/builder-probe-<first 12 hex of this plan's SHA-256>/ before the first request; the same plan never runs twice".into());
    for (n, case) in CASES.iter().enumerate() {
        line(format!(
            "case{} {}: POST https://{}/ authorization={} | {} | body={}",
            n + 1,
            case.id,
            case.builder.host(),
            if case.builder == Builder::BlockRazor {
                "yes"
            } else {
                "none"
            },
            case.question,
            case_template(case, txs)
        ));
    }
    plan
}

/// Coarse class of a builder answer; the raw answer is always recorded and
/// remains the evidence.
pub(crate) fn classify(http_status: &str, body: &str) -> &'static str {
    let code = http_status.split_whitespace().nth(1).unwrap_or("");
    let Ok(v) = serde_json::from_str::<Value>(body) else {
        return if code == "200" {
            "UNPARSEABLE"
        } else {
            "HTTP_ERROR"
        };
    };
    if v.get("result").is_some_and(|r| !r.is_null()) && v.get("error").is_none() {
        return "ACCEPTED";
    }
    let Some(err) = v.get("error") else {
        return if code == "200" {
            "UNPARSEABLE"
        } else {
            "HTTP_ERROR"
        };
    };
    let rpc_code = err.get("code").and_then(Value::as_i64);
    let message = err
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(rpc_code, Some(-32700 | -32600 | -32601 | -32602)) {
        return "FORMAT_REJECTED";
    }
    if [
        "maxblock",
        "maxtimestamp",
        "expire",
        "block number",
        "timestamp",
        "too low",
        "outdated",
    ]
    .iter()
    .any(|k| message.contains(k))
        || rpc_code == Some(-38000)
    {
        return "EXPIRY_REJECTED";
    }
    if ["price", "underpriced", "gwei", "fee", "tip"]
        .iter()
        .any(|k| message.contains(k))
    {
        return "PRICE_REJECTED";
    }
    if [
        "nonce",
        "insufficient",
        "balance",
        "funds",
        "simulat",
        "revert",
        "gas",
    ]
    .iter()
    .any(|k| message.contains(k))
    {
        return "EXECUTION_REJECTED";
    }
    "OTHER_REJECTED"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(number: u64, milli_timestamp: u64) -> ChainHead {
        ChainHead {
            number,
            hash: format!("0x{}", "ab".repeat(32)),
            milli_timestamp,
        }
    }

    #[test]
    fn throwaway_accounts_are_fixed_distinct_and_never_the_g07_wallets() {
        let txs = probe_txs();
        let again = probe_txs();
        assert_eq!(txs[0].signed.raw, again[0].signed.raw);
        assert_eq!(txs[1].signed.raw, again[1].signed.raw);
        assert_ne!(txs[0].from, txs[1].from);
        for tx in &txs {
            let from = tx.from.to_string().to_lowercase();
            assert!(!G07_WALLETS.contains(&from.as_str()), "{from}");
        }
    }

    #[test]
    fn probe_transactions_are_zero_value_self_transfers_at_an_unreachable_nonce() {
        for tx in probe_txs() {
            // Recompute the exact signed bytes from the declared parameters.
            let expected = sign_legacy(
                &throwaway_key(if tx.from == probe_txs()[0].from { 0 } else { 1 }),
                PROBE_NONCE,
                PROBE_GAS_PRICE_WEI,
                PROBE_GAS_LIMIT,
                tx.from,
                U256::ZERO,
                &[],
            );
            assert_eq!(tx.signed.raw, expected.raw);
            // RLP list header, then the nonce as the first field.
            let header = if tx.signed.raw[0] <= 0xf7 {
                1
            } else {
                1 + (tx.signed.raw[0] - 0xf7) as usize
            };
            assert!(tx.signed.raw[header..].starts_with(&rlp_u64(PROBE_NONCE)));
            assert_eq!(tx.signed.hash, keccak256(&tx.signed.raw));
        }
        assert!(PROBE_NONCE >= 1 << 40);
    }

    #[test]
    fn the_g07_shape_cases_are_exactly_the_g07_bodies() {
        let txs = probe_txs();
        let h = head(1_000, 5_000_200);
        let w = bundle_window(1_000, &h.hash, &h, 5_000_300, 0).unwrap();
        let raw = [txs[0].raw_hex(), txs[1].raw_hex()];
        let id = txs[1].signed.hash.to_string();
        let (b, ts) = expiry_values(Expiry::G07, &h, &w);
        assert_eq!(
            case_body(&CASES[0], &txs, b, ts),
            club48_bundle_body(&id, [&raw[0], &raw[1]], w.max_block, w.max_timestamp)
        );
        assert_eq!(
            case_body(&CASES[5], &txs, b, ts),
            blockrazor_bundle_body(&id, [&raw[0], &raw[1]], w.max_block)
        );
    }

    #[test]
    fn each_variant_changes_only_its_expiry() {
        let txs = probe_txs();
        let h = head(1_000, 5_000_200);
        let w = bundle_window(1_000, &h.hash, &h, 5_000_300, 0).unwrap();
        assert_eq!((w.max_block, w.max_timestamp), (1_002, 5_002));
        let values: Vec<_> = CASES
            .iter()
            .map(|c| expiry_values(c.expiry, &h, &w))
            .collect();
        assert_eq!(values[1], (1_002, None));
        assert_eq!(values[2], (1_002, Some(5_001)));
        assert_eq!(values[3], (999, Some(5_002)));
        assert_eq!(values[4], (1_002, Some(4_998)));
        assert_eq!(values[6], (999, Some(5_002)));
        let no_ts: Value = serde_json::from_str(&case_body(&CASES[1], &txs, 1_002, None)).unwrap();
        let g07: Value =
            serde_json::from_str(&case_body(&CASES[0], &txs, 1_002, Some(5_002))).unwrap();
        let mut stripped = g07.clone();
        stripped["params"][0]
            .as_object_mut()
            .unwrap()
            .remove("maxTimestamp");
        assert_eq!(no_ts, stripped);
        // BlockRazor bodies never carry maxTimestamp.
        assert!(!case_body(&CASES[6], &txs, 999, Some(5_002)).contains("maxTimestamp"));
    }

    #[test]
    fn the_plan_is_deterministic_and_names_every_request_once() {
        let txs = probe_txs();
        let plan = plan_text(&txs);
        assert_eq!(plan, plan_text(&probe_txs()));
        assert_eq!(plan.matches("\ncase").count(), CASES.len());
        assert_eq!(plan.matches("authorization=yes").count(), 2);
        assert!(plan.contains("\"maxBlockNumber\":{HEAD+2}"));
        assert!(plan.contains("\"maxTimestamp\":{MAXTS}"));
        assert!(plan.contains("\"maxBlockNumber\":{HEAD-1}"));
        assert!(plan.contains("\"maxTimestamp\":{HEAD_S-2}"));
        assert!(!plan.contains(&BLOCK_SENTINEL.to_string()));
        assert!(!plan.contains(&TIMESTAMP_SENTINEL.to_string()));
    }

    #[test]
    fn answers_are_classified_coarsely_and_conservatively() {
        let ok = "HTTP/1.1 200 OK";
        assert_eq!(
            classify(
                ok,
                r#"{"jsonrpc":"2.0","id":1,"result":{"bundleHash":"0x1"}}"#
            ),
            "ACCEPTED"
        );
        assert_eq!(
            classify(
                ok,
                r#"{"error":{"code":-32602,"message":"invalid params"}}"#
            ),
            "FORMAT_REJECTED"
        );
        assert_eq!(
            classify(
                ok,
                r#"{"error":{"code":-32601,"message":"method not found"}}"#
            ),
            "FORMAT_REJECTED"
        );
        assert_eq!(
            classify(ok, r#"{"error":{"code":-38000,"message":"x"}}"#),
            "EXPIRY_REJECTED"
        );
        assert_eq!(
            classify(
                ok,
                r#"{"error":{"code":-32000,"message":"maxBlockNumber too low"}}"#
            ),
            "EXPIRY_REJECTED"
        );
        assert_eq!(
            classify(
                ok,
                r#"{"error":{"code":-32000,"message":"nonce too high"}}"#
            ),
            "EXECUTION_REJECTED"
        );
        assert_eq!(
            classify(
                ok,
                r#"{"error":{"code":-32000,"message":"bundle gas price too low"}}"#
            ),
            "PRICE_REJECTED"
        );
        assert_eq!(
            classify(ok, r#"{"error":{"code":-32000,"message":"something"}}"#),
            "OTHER_REJECTED"
        );
        assert_eq!(
            classify("HTTP/1.1 403 Forbidden", "forbidden"),
            "HTTP_ERROR"
        );
        assert_eq!(classify(ok, "not json"), "UNPARSEABLE");
    }
}
