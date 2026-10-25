//! The binary acceptance contract of G0.7 attempt 1 (DBREC001 schema
//! `darkbot.g07.acceptance-contract.v3`). Its reviewed values live here as
//! source; `g07_acceptance_contract` writes them to `V3_RELATIVE_PATH`, and
//! the guard test checks that the committed file is exactly these bytes. It
//! replaces the JSON v2 proposal and the JSON v2 draft, which stay legacy
//! only.
//!
//! The approval state is never inferred: the committed file is the
//! `Proposed` variant (`approved=false`, `execution_ready=false`). The
//! `Approved` variant differs only in those fields, its status and note; it
//! is written only for the exact SHA-256 the user approved, and even then it
//! arms nothing until a dedicated arming commit compiles that SHA-256 into
//! the guard.

use darkbot_bsc_record::schema::g07::contract;
use darkbot_bsc_record::schema::{IMPORT_LEGACY_SHA256, decode_any, g06a, g07};
use darkbot_bsc_record::{
    Draft, Record, Schema, Value, decode, hex, sha256, u256_from_decimal, u256_to_decimal,
};
use std::path::Path;

pub const ATTEMPT_ID: &str = "g07-mainnet-attempt-1-dry-20261015T222241Z";
pub const LAUNCHER: &str = "0x3e7B487Ae5e1c49aB1cF6222BDb5cf016e86aacd";
pub const BUYER: &str = "0x9c8a0Db41914beEE52Dd49790c4F087E252743DE";
pub const FROZEN_DIR: &str = "bsc/native/evidence/g06a/g07_binary_import_20261016";
pub const REQUAL_STATUS: &str = "requalification_status.dbrec";
pub const REQUAL_CHECKS: &[&str] = &["R0_UPGRADE", "DRY0_PREFLIGHT", "DRY1_BYTES_AND_SIGNERS", "DRY2_SIMULATION", "DRY3_GAS_ECONOMICS_BUDGET", "DRY4_EXPIRY_NONCES_ZERO_EGRESS"];
/// The check and record a requalification adds for a v5 contract (C1/R1).
pub const REQUAL_LAUNCH_PATH_CHECK: &str = "DRY5_LAUNCH_PATH";
pub const REQUAL_LAUNCH_PATH: &str = "launch_path.dbrec";
/// The check a secret-salt SELF_B run adds (`g06a_requal_check --candidate`).
pub const REQUAL_B_CANDIDATE_CHECK: &str = "DRY6_B_CANDIDATE";

/// What a requalification run must hold: exactly the v4 checks and records,
/// those plus the launch path (v5), or those plus the SELF_B candidate
/// record at its path (v5 of B).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape<'a> {
    V4,
    V5,
    B(&'a str),
}

/// No aliases across the approval boundary: paths are repository-relative,
/// normalized and every existing component must be non-symlinked.
fn evidence_path(root: &Path, relative: &str) -> Result<(), String> {
    use std::path::Component;
    let path = Path::new(relative);
    if relative.is_empty() || relative.split('/').any(|p| p.is_empty() || p == "." || p == "..") {
        return Err("non-canonical evidence path".into());
    }
    let mut current = root.to_path_buf();
    for part in path.components() {
        let Component::Normal(part) = part else { return Err("evidence path must be relative".into()); };
        current.push(part);
        let meta = std::fs::symlink_metadata(&current).map_err(|e| format!("evidence path: {e}"))?;
        if meta.file_type().is_symlink() { return Err("symlinked evidence path".into()); }
    }
    Ok(())
}

fn requalification_pass(
    root: &Path,
    dir: &str,
    policy: &str,
    shape: Shape<'_>,
) -> Result<(), String> {
    let launch_path = shape != Shape::V4;
    let b = match shape {
        Shape::B(candidate) => Some(candidate),
        _ => None,
    };
    use g06a::requalification as r;
    use std::collections::{BTreeMap, BTreeSet};
    let e = |x: darkbot_bsc_record::RecordError| x.0;
    for path in [dir, FROZEN_DIR, policy].into_iter().chain(b) {
        evidence_path(root, path)?;
    }
    let status = format!("{dir}/{REQUAL_STATUS}");
    evidence_path(root, &status)?;
    let bytes = std::fs::read(root.join(&status)).map_err(|e| e.to_string())?;
    let record = decode(&r::SCHEMA, &bytes).map_err(e)?;
    let canonical_root = root.canonicalize().map_err(|e| e.to_string())?;
    // Verifier records produced by the shell use canonical absolute paths.
    // Accept only that exact spelling or the normalized repository-relative one.
    let same_path = |recorded: &str, relative: &str| recorded == relative || Path::new(recorded) == canonical_root.join(relative);
    if record.text(r::VERDICT).map_err(e)? != "PASS"
        || record.text(darkbot_bsc_record::schema::PROVENANCE_TOOL).map_err(e)? != "g06a_requal_check"
        || !same_path(record.text(r::REQUAL_DIR).map_err(e)?, dir)
        || !same_path(record.text(r::FROZEN_DIR).map_err(e)?, FROZEN_DIR) {
        return Err("requalification verdict/provenance/directory mismatch".into());
    }
    let mut checks = BTreeSet::new();
    for check in record.nested_all(r::CHECKS).map_err(e)? {
        let name = check.text(r::check::NAME).map_err(e)?.to_owned();
        let known = REQUAL_CHECKS.contains(&name.as_str())
            || (launch_path && name == REQUAL_LAUNCH_PATH_CHECK)
            || (b.is_some() && name == REQUAL_B_CANDIDATE_CHECK);
        if !known || !checks.insert(name) || check.text(r::check::STATUS).map_err(e)? != "PASS" {
            return Err("requalification check missing, duplicated or not PASS".into());
        }
    }
    if checks.len() != REQUAL_CHECKS.len() + usize::from(launch_path) + usize::from(b.is_some()) {
        return Err("incomplete requalification checks".into());
    }
    let mut records = BTreeMap::new();
    for row in record.nested_all(r::RECORDS).map_err(e)? {
        let raw_name = row.text(r::bound::NAME).map_err(e)?;
        let name = if same_path(raw_name, policy) {
            policy
        } else if let Some(candidate) = b.filter(|c| same_path(raw_name, c)) {
            candidate
        } else {
            raw_name
        };
        if records.insert(name.to_owned(), row.b256(r::bound::SHA256).map_err(e)?).is_some() {
            return Err("duplicate requalification record".into());
        }
    }
    let extra = [launch_path.then_some(REQUAL_LAUNCH_PATH), b]
        .into_iter()
        .flatten();
    for name in [
        REQUAL_PREFLIGHT,
        "native_quote.dbrec",
        REQUAL_ECONOMICS,
        REQUAL_LOCAL_SIMULATION,
        REQUAL_DRY,
        REQUAL_PORTAL_UPGRADE,
        policy,
    ]
    .into_iter()
    .chain(extra)
    {
        let path = if name == policy || Some(name) == b {
            name.to_owned()
        } else {
            format!("{dir}/{name}")
        };
        evidence_path(root, &path)?;
        let bytes = std::fs::read(root.join(path)).map_err(|e| e.to_string())?;
        if records.remove(name) != Some(sha256(&bytes)) { return Err(format!("requalification record binding mismatch: {name}")); }
    }
    if !records.is_empty() { return Err("unexpected requalification records".into()); }
    Ok(())
}

/// Checks every individual safety flag, not just the aggregate.
pub fn require_dry_pass(dry: &Record) -> Result<(), String> {
    use g06a::dry as d;
    for tag in [d::DRY_1, d::DRY_2, d::DRY_3, d::DRY_4, d::DRY_ALL, d::ZERO_EGRESS_STRUCTURAL] {
        if dry.bool(tag).map_err(|e| e.0)? != true { return Err(format!("DRY flag {tag} is not PASS")); }
    }
    Ok(())
}

/// The binary input records G0.7 attempt 1 reads (the offline legacy import).
pub const DRY_RECORD: &str = "bsc/native/evidence/g06a/g07_binary_import_20261016/dry_result.dbrec";
pub const PREFLIGHT_RECORD: &str =
    "bsc/native/evidence/g06a/g07_binary_import_20261016/preflight.dbrec";
pub const BUILDER_POLICY_RECORD: &str =
    "bsc/native/evidence/g06a/g07_binary_import_20261016/builder_policy.dbrec";
pub const WITNESS_SOURCE_RECORD: &str =
    "bsc/native/evidence/g06a/g07_binary_import_20261016/producer_witness_source.dbrec";

/// One bound input: SHA-256 of the record file and the legacy SHA-256 the
/// record carries from its import.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputBinding {
    pub record_sha256: [u8; 32],
    pub legacy_sha256: [u8; 32],
}

/// Reads a bound input record under `root` and returns its binding.
pub fn binding(root: &Path, path: &str, schema: &'static Schema) -> Result<InputBinding, String> {
    let bytes = std::fs::read(root.join(path)).map_err(|e| format!("{path}: {e}"))?;
    let record = decode(schema, &bytes).map_err(|e| format!("{path}: {e}"))?;
    Ok(InputBinding {
        record_sha256: darkbot_bsc_record::sha256(&bytes),
        legacy_sha256: record.b256(IMPORT_LEGACY_SHA256).map_err(|e| e.0)?,
    })
}

/// The four bindings of attempt 1, read from the committed import.
pub fn attempt1_bindings(root: &Path) -> Result<[InputBinding; 4], String> {
    Ok([
        binding(root, DRY_RECORD, &g06a::dry::SCHEMA)?,
        binding(root, PREFLIGHT_RECORD, &g06a::preflight::SCHEMA)?,
        binding(root, BUILDER_POLICY_RECORD, &g06a::builder_policy::SCHEMA)?,
        binding(root, WITNESS_SOURCE_RECORD, &g07::witness_source::SCHEMA)?,
    ])
}

fn tag(name: &str) -> Result<u16, String> {
    contract::SCHEMA
        .fields
        .iter()
        .find(|f| f.name == name)
        .map(|f| f.tag)
        .ok_or_else(|| format!("acceptance contract v3 has no field {name}"))
}

fn hex_literal<const N: usize>(text: &str) -> Result<[u8; N], String> {
    let digits = text.strip_prefix("0x").unwrap_or(text);
    let mut out = [0u8; N];
    if digits.len() != N * 2 {
        return Err(format!("bad hex literal {text}"));
    }
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = digits
            .get(i * 2..i * 2 + 2)
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
            .ok_or_else(|| format!("bad hex literal {text}"))?;
    }
    Ok(out)
}

fn address(text: &str) -> Result<Value, String> {
    hex_literal::<20>(text).map(Value::Address)
}

fn wei(text: &str) -> Result<Value, String> {
    u256_from_decimal(text)
        .map(Value::U256)
        .ok_or_else(|| format!("bad decimal literal {text}"))
}

fn criterion(id: &str, pass: &str) -> Value {
    let mut c = Draft::new(&contract::CRITERION);
    c.set(1, id).set(2, pass);
    c.into_nested()
}

/// Whether the contract bytes carry the user's real-money authorization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Authorization {
    /// A proposal: no authorization exists; the guard refuses it.
    Proposed,
    /// The variant the user approves by its exact SHA-256.
    Approved,
}

/// The exact bytes of the attempt-1 contract for these bindings
/// (dry, preflight, builder policy, producer-witness source).
pub fn attempt1_v3(
    bindings: &[InputBinding; 4],
    authorization: Authorization,
) -> Result<Vec<u8>, String> {
    let mut d = Draft::new(&contract::SCHEMA);
    let [dry, preflight, policy, witness] = bindings;
    d.set(contract::DRY_RECORD_SHA256, Value::B256(dry.record_sha256))
        .set(
            contract::PREFLIGHT_RECORD_SHA256,
            Value::B256(preflight.record_sha256),
        )
        .set(
            contract::BUILDER_POLICY_RECORD_SHA256,
            Value::B256(policy.record_sha256),
        )
        .set(
            contract::WITNESS_SOURCE_RECORD_SHA256,
            Value::B256(witness.record_sha256),
        )
        .set(contract::DRY_LEGACY_SHA256, Value::B256(dry.legacy_sha256))
        .set(
            contract::PREFLIGHT_LEGACY_SHA256,
            Value::B256(preflight.legacy_sha256),
        )
        .set(
            contract::BUILDER_POLICY_LEGACY_SHA256,
            Value::B256(policy.legacy_sha256),
        )
        .set(
            contract::WITNESS_SOURCE_LEGACY_SHA256,
            Value::B256(witness.legacy_sha256),
        )
        .set(contract::DRY_RECORD_PATH, DRY_RECORD)
        .set(contract::PREFLIGHT_RECORD_PATH, PREFLIGHT_RECORD)
        .set(contract::BUILDER_POLICY_RECORD_PATH, BUILDER_POLICY_RECORD)
        .set(contract::WITNESS_SOURCE_RECORD_PATH, WITNESS_SOURCE_RECORD);
    d.set(
        tag("authorization.evidence_paths.builder_requirements_refresh")?,
        "bsc/native/evidence/g06a/builder_requirements_refresh_20261016T013943Z.json (legacy JSON evidence, informational)",
    );
    d.set(
        tag("authorization.evidence_paths.builder_requirements_refresh_sha256")?,
        Value::B256(hex_literal(
            "ef4b72ffd26c07850fda15a8193a744abbd5e33015623a4c8d0818af740287b6",
        )?),
    );
    d.set(
        tag("authorization.evidence_paths.note")?,
        "The guard binds the SHA-256 of the four binary input records and the legacy SHA-256 each record carries from the audited offline import (PARITY.md beside the records); G0.7 takes the record paths at runtime by environment variable and checks both bindings.",
    );
    common(&mut d, authorization == Authorization::Approved)?;
    d.encode().map_err(|e| e.0)
}

/// Every field of the attempt-1 contract outside its evidence bindings, as
/// reviewed for v3; v4 overrides what refers to the superseded DRY.
fn common(d: &mut Draft, approved: bool) -> Result<(), String> {
    d.set(
        tag("status")?,
        if approved {
            "APPROVED_SINGLE_ATTEMPT_G07_ATTEMPT_1"
        } else {
            "PROPOSED_NOT_AUTHORIZED"
        },
    );
    d.set(tag("supersedes")?, "bsc/config/g01_mainnet_acceptance_proposal_v2.json and docs/G07_ATTEMPT1_ACCEPTANCE_V2_DRAFT.json (JSON, legacy only; v1 historical)");
    d.set(tag("purpose")?, "Controlled Mainnet self-launch ordered-bundle qualification with two independent wallets and parallel builder fan-out");
    d.set(tag("authorization.approved")?, approved);
    d.set(tag("authorization.execution_ready")?, approved);
    d.set(
        tag("authorization.approval_required_for")?,
        "Any value-bearing Mainnet bundle submission",
    );
    d.set(
        tag("authorization.note")?,
        if approved {
            "Final single-attempt authorization for G0.7 attempt g07-mainnet-attempt-1-dry-20261015T222241Z. Effective only after the user explicitly approves these exact bytes and a dedicated arming commit sets FROZEN_APPROVED_V3_SHA256 to their SHA-256 while this file sits at bsc/config/g07_attempt1_acceptance_v3.dbrec. The JSON v1/v2 contracts and the JSON v2 draft remain historical."
        } else {
            "PROPOSAL ONLY: no real-money authorization exists for G0.7 attempt g07-mainnet-attempt-1-dry-20261015T222241Z; the guard refuses this contract (approved=false, execution_ready=false). The approved variant differs only in status, note, approved and execution_ready; it is written only for the exact SHA-256 the user approves, and a dedicated arming commit must then compile that SHA-256 as FROZEN_APPROVED_V3_SHA256."
        },
    );
    d.set(
        tag("authorization.launcher_address")?,
        address(LAUNCHER)?,
    );
    d.set(
        tag("authorization.buyer_address")?,
        address(BUYER)?,
    );
    d.set(tag("authorization.execution_authority")?, "Only this binary v3 contract may authorize the self-launch Mainnet send path; the JSON v1/v2 contracts and the JSON v2 draft are forbidden as runtime authority");
    d.set(tag("authorization.approval_scope")?, "Exactly one economic G0.7 attempt (attempt_number 1) of the exact DRY-qualified [launch_A, buy_B] bytes; no retry; no second attempt under this approval; a disarm commit (FROZEN back to UNARMED) is mandatory immediately after the release regardless of outcome");
    d.set(
        tag("authorization.attempt_id")?,
        ATTEMPT_ID,
    );
    d.set(tag("scope.chain_id")?, 56u64);
    d.set(tag("scope.venue")?, "Flap");
    d.set(tag("scope.launch")?, "newTokenV6");
    d.set(tag("scope.tokenVersion")?, 6u64);
    d.set(
        tag("scope.portal_expected_from_current_evidence")?,
        address("0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0")?,
    );
    d.set(tag("scope.portal_rule")?, "Reverify current Mainnet deployment/version/runtime code immediately before arming; mismatch is STOP");
    d.set(tag("scope.launch_mode")?, "controlled native-quote self-launch matching the intended harness, not the historical DOGE fixture");
    d.set(
        tag("scope.quoteToken")?,
        address("0x0000000000000000000000000000000000000000")?,
    );
    d.set(tag("scope.quoteAmt")?, wei("0")?);
    d.set(
        tag("scope.buy")?,
        "Direct Portal swapExactInput with native input and nonzero locally-derived minOutput",
    );
    d.set(tag("scope.historical_g03_authority")?, "Historical DOGE/0.15-BNB G0.3 fixture is NOT authority for this native-quote self-launch; native mode requires its own dry semantic validation");
    d.set(
        tag("wallet_separation.launcher")?,
        address(LAUNCHER)?,
    );
    d.set(
        tag("wallet_separation.buyer")?,
        address(BUYER)?,
    );
    d.set(tag("wallet_separation.must_be_distinct_addresses")?, true);
    d.set(
        tag("wallet_separation.reason")?,
        "Launch/BUY ordering must be proven by bundle semantics, not by same-sender nonce ordering",
    );
    d.set(
        tag("wallet_separation.secret_storage")?,
        "No private key or secret is written to evidence",
    );
    d.set(tag("attempts.maximum")?, 1u64);
    d.set(tag("attempts.economic_attempt_definition")?, "One self-launch plus one intended BUY. Parallel submission of byte-identical [launch_A,buy_B] bundle copies to multiple builders counts as ONE economic attempt.");
    d.push(tag("attempts.builder_fanout")?, "48Club/Puissant");
    d.push(tag("attempts.builder_fanout")?, "BlockRazor");
    d.set(tag("attempts.parallel")?, true);
    d.set(tag("attempts.no_public_launch_broadcast")?, true);
    d.set(tag("attempts.no_automatic_retry")?, true);
    d.set(
        tag("attempts.no_parallel_fallback_buy_bundle_in_stage_1")?,
        true,
    );
    d.set(
        tag("attempts.selected_attempts_remain_in_denominator")?,
        true,
    );
    d.push(tag("bundle_bytes.tx_order")?, "raw_signed_launch_A");
    d.push(tag("bundle_bytes.tx_order")?, "raw_signed_buy_B");
    d.set(
        tag("bundle_bytes.identical_raw_transactions_across_builders")?,
        true,
    );
    d.set(tag("bundle_bytes.reverting_transactions_allowed")?, false);
    d.set(tag("bundle_bytes.max_block_window")?, "At most send_head+2 unless a stricter provider-specific bound is available; never use a long default such as +100");
    d.set(
        tag("bundle_bytes.expiry_mismatch")?,
        "STOP if equivalent tight expiry cannot be expressed for every armed builder",
    );
    d.set(tag("bundle_bytes.fortyeightclub.noMerge")?, true);
    d.set(tag("bundle_bytes.fortyeightclub.positionFirst")?, true);
    d.set(
        tag("bundle_bytes.fortyeightclub.rationale")?,
        "N+1 qualification must not accept late-arriving bundle placement at the block tail.",
    );
    d.set(tag("bundle_bytes.send_head_rule")?, "send_head is the next block G0.7 observes, polled over a pre-warmed keep-alive RPC TLS connection after the builder TLS prewarm, immediately before the release (not the preflight head). maxBlockNumber = send_head + 2 for both builders: this is the head+2 bound. 48Club additionally gets maxTimestamp (Unix seconds) = floor((send_head milliTimestamp + 900 ms) / 1000) + 1, milliTimestamp per BEP-520: the second after head+2's nominal second, so a 'maxTimestamp must exceed the current block time / now' check cannot refuse the bundle while head+2 is open. A seconds value cannot express head+2 exactly; maxTimestamp alone would admit blocks stamped up to that second (about head+6 at the nominal cadence), so for 48Club the head+2 bound rests on maxBlockNumber being enforced (documented as an inclusive parameter and checked by 48Club at submission per the builder probe 2026-10-16; building-time enforcement unqualified, pre-arm check 6). G0.7 stops before any bundle write if send_head is below the preflight head, has the preflight height with another hash, is older than 450 ms by the local clock or, independent of the clock, at least 450 ms staler than the endpoint's freshest read, if the local clock lags the head timestamp by more than 150 ms, if no new block arrives within 3 s, if the builder connections idled longer than 5 s, if a builder closed its prewarmed connection or sent unsolicited data, or if the journal does not hold the exact request bodies within 50 ms.");
    d.set(
        tag("economics_validation.min_output.must_be_positive")?,
        true,
    );
    d.set(
        tag("economics_validation.min_output.source")?,
        "Exact controlled post-launch state from the chosen native launch parameters",
    );
    d.set(tag("economics_validation.min_output.independent_cross_check")?, "Local/offline independent sequential execution of exact launch_A then buy_B. Before authorization, exact valid Mainnet bytes MUST NOT be sent to external builder simulation endpoints.");
    d.set(
        tag("economics_validation.min_output.slippage_max_bps")?,
        100u64,
    );
    d.set(
        tag("economics_validation.min_output.historical_min_output_reuse_forbidden")?,
        true,
    );
    d.set(tag("economics_validation.gas_floor.policy")?, "Before signing, determine the current acceptance floor/ranking rules for every armed builder. Use the strictest requirement for the identical transaction bytes.");
    d.set(tag("economics_validation.gas_floor.blockrazor_current_documented_reference")?, "0.05 gwei (50000000 wei), refreshed read-only 2026-10-16 against official BlockRazor docs (GitHub blobs unchanged since 2026-10-04): per-transaction validator minimum and non-mempool bundle average with builder tip in the numerator; both allow equality. See authorization.evidence_paths.builder_requirements_refresh.");
    d.set(tag("economics_validation.gas_floor.fortyeightclub")?, "Do not reuse a stale universal 1-gwei assumption. Verify the current Puissant endpoint/validator acceptance and priority rule immediately before arming. Read-only refresh 2026-10-16: web3.48.club (auction-transaction-feed, backrun context; publication date unknown) states a 0.05 gwei bundle-average requirement; applicability to non-backrun bundles is unverified; the legacy GitBook page still shows 1 gwei (appears outdated). maxBlockNumber is an integer; the web3 sendBundle page also asks for a maxTimestamp expiry: G0.7 sends maxTimestamp (Unix seconds) = floor((send_head milliTimestamp + 900 ms) / 1000) + 1 next to maxBlockNumber = send_head + 2 (see bundle_bytes.send_head_rule). Builder probe 2026-10-16: 48Club parses the exact G0.7 request and checks maxBlockNumber and maxTimestamp at submission, but showed no gas-price check at submission, so acceptance of a 0.05 gwei / tip-0 bundle for inclusion stays unverified. Auction Feed/backrunTarget requirements remain a separate scope.");
    d.set(
        tag("economics_validation.gas_floor.max_signed_gas_price_wei")?,
        wei("50000000")?,
    );
    d.set(tag("economics_validation.gas_floor.freshness_rule")?, "Re-run the read-only builder refresh no more than 24 hours before the arming commit; STOP if any explicit floor exceeds 50000000 wei or any rule changed.");
    d.set(tag("economics_validation.gas_floor.bound_policy_superseded_fields")?, "In the bound addendum bsc/config/g06a_builder_floor_addendum_2026-10-11.json, fortyeightclub.explicit_minimum_documented=false and the BlockRazor rule text are superseded by the 2026-10-16 refresh (same 50000000 wei value); the addendum stays bound because its strictest floor and stage-1 policy are unchanged and the DRY binds it.");
    d.set(tag("economics_validation.bid_policy.stage_1")?, "Bid through the signed transaction gas-price policy only; no separate automatic builder transfer/tip");
    d.set(
        tag("economics_validation.bid_policy.max_direct_builder_tip_wei")?,
        wei("0")?,
    );
    d.set(
        tag("economics_validation.bid_policy.policy_change_requires_new_contract_version")?,
        true,
    );
    d.set(
        tag("budget_wei.max_buy_value_per_attempt")?,
        wei("1000000000000")?,
    );
    d.set(
        tag("budget_wei.max_launch_call_value_per_attempt")?,
        wei("0")?,
    );
    d.set(tag("budget_wei.max_launch_gas_limit")?, 6603270u64);
    d.set(tag("budget_wei.max_buy_gas_limit")?, 3000000u64);
    d.set(tag("budget_wei.max_gas_price_wei")?, wei("50000000")?);
    d.set(
        tag("budget_wei.max_gas_cost_per_attempt")?,
        wei("480163500000000")?,
    );
    d.set(
        tag("budget_wei.max_total_per_attempt")?,
        wei("481163500000000")?,
    );
    d.set(
        tag("budget_wei.absolute_max_total_all_attempts")?,
        wei("481163500000000")?,
    );
    d.set(tag("budget_wei.note")?, "Final single-attempt budget for the exact DRY candidate: signed gas price 0.05 gwei, launch gas limit 6,603,270, BUY gas limit 3,000,000, launch value 0, BUY value 0.000001 BNB, builder tip 0. Worst-case gas reserve 0.0004801635 BNB; total worst-case reserve 0.0004811635 BNB (launcher 0.0003301635, buyer 0.000151). The launch gas cap is the measured Mainnet eth_estimateGas 5,502,725 with a 120% reserve = 6,603,270 (re-measured at block 127865760). Expected actual cost from the local simulation (gas used 5,251,225 + 334,654 at 0.05 gwei plus BUY value) is 0.00028029395 BNB; the ceilings are the worst-case reserve, not the expected charge. The guard reads budget_wei.absolute_max_total_all_attempts (the v2 key absolute_max_total_all_four_attempts); with attempts.maximum 1 it is the absolute single-attempt ceiling. Never increase these ceilings at runtime.");
    d.push(tag("pre_arm_checks")?, "Reverify current Flap Mainnet Portal/version/runtime code and selected native launch semantics");
    d.push(tag("pre_arm_checks")?, "Validate exact native-mode launch->BUY transition independently; historical DOGE G0.3 fixture is insufficient");
    d.push(
        tag("pre_arm_checks")?,
        "Resolve and freeze distinct launcher_A and buyer_B addresses and both pending nonces",
    );
    d.push(tag("pre_arm_checks")?, "Verify both wallets have sufficient balance under the proposed reserved worst-case budget without writing secrets to evidence");
    d.push(tag("pre_arm_checks")?, "Compute positive minOutput locally from the exact self-launch parameters and cross-check with independent simulation");
    d.push(tag("pre_arm_checks")?, "Validate exact signed raw launch and BUY transactions decode/recover to the intended sender, nonce, chain, portal, value, gas and calldata");
    d.push(tag("pre_arm_checks")?, "Validate builder method/format/expiry semantics only with deliberately non-executable fixtures; exact valid Mainnet bytes remain local until authorization");
    d.push(tag("pre_arm_checks")?, "Determine current builder gas-floor/ranking requirements; signed bytes must satisfy the strictest armed builder");
    d.push(
        tag("pre_arm_checks")?,
        "Verify neither transaction is broadcast to the public mempool during dry qualification",
    );
    d.push(
        tag("pre_arm_checks")?,
        "Explicit user approval of this v3 scope and numeric absolute budget ceiling",
    );
    d.push(tag("pre_arm_checks")?, "Validate 48Club noMerge=true and positionFirst=true semantics using deliberately non-executable fixtures before arming.");
    d.set(tag("success.acquisition")?, "BUY receipt success plus positive intended-token balance delta at buyer_B with attributable economic debit/cost");
    d.set(tag("success.ordering")?, "Launch and BUY are in the same canonical block; BUY transaction index equals launch transaction index + 1; zero successful external BUYs between");
    d.set(tag("success.builder_evidence")?, "Record builder submission hash/status where exposed and identify block builder/validator where reliably attributable");
    d.set(tag("success.timing")?, "N+1 is ordering evidence only. T_buy-T0 remains UNKNOWN unless an original producer witness conforming to producer_execution_witness_v1 exists; re-execution traces never qualify. For attempt 1 the producer witness source is not available (source_ready=false), so the timing gate cannot pass for this attempt.");
    d.set(tag("retention_observability.record_send_head")?, true);
    d.set(tag("retention_observability.record_inclusion_block")?, true);
    d.set(tag("retention_observability.record_builder")?, true);
    d.set(tag("retention_observability.record_validator")?, true);
    d.set(tag("retention_observability.record_bundle_status")?, true);
    d.push(
        tag("retention_observability.on_expiry_without_inclusion")?,
        "Verify launcher_A nonce unchanged",
    );
    d.push(
        tag("retention_observability.on_expiry_without_inclusion")?,
        "Verify buyer_B nonce unchanged",
    );
    d.push(
        tag("retention_observability.on_expiry_without_inclusion")?,
        "Verify neither transaction appeared canonically or in the public mempool where observable",
    );
    d.set(tag("retention_observability.interpretation")?, "Inclusion one or two blocks after submission is evidence relevant to bundle carry/re-simulation, but provider retention semantics must not be assumed solely from one successful inclusion.");
    d.set(
        tag("retention_observability.pre_arm_bounded_capture_required")?,
        true,
    );
    d.set(
        tag("retention_observability.producer_witness_contract_required_before_execution")?,
        true,
    );
    d.set(tag("retention_observability.g07_local_attempt_state")?, "G0.7 runs only from the main checkout and creates bsc/.local/g07/<attempt_id>/ (0700, git-ignored, no symlinks) with ATTEMPT_LATCH and capture.journal (0600) before the guard; a second start of the same attempt_id is refused while that directory exists. The journal holds broadcastable signed bytes: never commit or share it before both nonces are consumed on-chain. A run that provably wrote no bundle byte (guard refusal or pre-egress stop, journal AUTHORIZATION_REFUSED or PRE_EGRESS_ABORT) is not an economic attempt: G0.7 renames the directory to <attempt_id>.refused-<ms> or .pre-egress-<ms> (journal kept) and removes its own empty evidence file, so the same attempt can be started again by the user's explicit decision. Right after the release G0.7 writes RELEASED into the directory; from then on the latch is never released and the attempt is consumed whatever the outcome. DARKBOT_G07_EVIDENCE_PATH is mandatory, must not exist and must lie in a private directory (real directory, no symlink on the way, mode 0700, owned by the user, e.g. bsc/.local/g07/); the evidence file is 0600 and only ever replaced atomically (complete fsynced private sibling, rename, directory sync), never truncated in place: provisional right after the release, again after the send threads, final after the receipt window; each write is first checked against the fields G0.8 reads. G0.7 never marks the canonical [T0, T0+15 s] window as reconciled; that is G0.8's job.");
    d.set(tag("thresholds.PASS")?, "Controlled self-launch attempt acquires the token and produces same-block adjacent [launch_A@N,buy_B@N+1] ordering with zero successful external BUYs between, no public leakage, no budget/scope violation and complete evidence");
    d.set(tag("thresholds.ADAPT")?, "Bundle valid but not adjacent/same-block, builder rejected/outbid/expired, dry simulation divergence, or incomplete attribution without a safety violation");
    d.set(tag("thresholds.STOP")?, "Any scope/version mismatch, non-distinct wallets, zero/unverified minOutput, fee/floor uncertainty at arming, budget violation, nonce conflict, unexpected public broadcast, first revert, unresolved sent transaction, unexpected duplicate economic execution, or inability to prove expiration/non-execution safely");
    d.set(
        tag("coverage.foreign_launch_coverage_requirement")?,
        "NOT_APPLICABLE_TO_SELF_LAUNCH_STAGE_1",
    );
    d.set(
        tag("coverage.note")?,
        "Coverage returns in the subsequent foreign-launch ingress qualification stage",
    );
    d.set(tag("send_guard.fail_closed")?, true);
    d.push(tag("send_guard.required_checks")?, "Runtime file path is exactly bsc/config/g07_attempt1_acceptance_v3.dbrec (relative to the repository root)");
    d.push(tag("send_guard.required_checks")?, "SHA-256 of the exact v3 record bytes equals the hash compiled/frozen into the armed harness");
    d.push(tag("send_guard.required_checks")?, "The file decodes fail-closed as DBREC001 record type 1, schema version 3 (darkbot.g07.acceptance-contract.v3)");
    d.push(
        tag("send_guard.required_checks")?,
        "authorization.approved == true",
    );
    d.push(
        tag("send_guard.required_checks")?,
        "authorization.execution_ready == true",
    );
    d.push(
        tag("send_guard.required_checks")?,
        "authorization.launcher_address equals recovered launcher_A address",
    );
    d.push(
        tag("send_guard.required_checks")?,
        "authorization.buyer_address equals recovered buyer_B address",
    );
    d.push(tag("send_guard.required_checks")?, "launcher_A != buyer_B");
    d.push(tag("send_guard.required_checks")?, "The contract does not hold its own hash (no guard check): the hash binding is external via FROZEN_APPROVED_V3_SHA256 in the arming commit, because a file cannot contain its own SHA-256");
    d.push(
        tag("send_guard.required_checks")?,
        "spent_budget_wei + reserved_worst_case_attempt_wei <= absolute approved total budget",
    );
    d.push(
        tag("send_guard.required_checks")?,
        "attempt number == 1 (single-attempt freeze) and does not exceed attempts.maximum",
    );
    d.push(
        tag("send_guard.required_checks")?,
        "all Dry-PASS evidence predicates are true",
    );
    d.push(tag("send_guard.required_checks")?, "The four authorization.evidence_bindings record hashes equal the SHA-256 of the DRY, preflight, builder-policy and producer-witness-source records passed at runtime, and the legacy hash each record carries equals the bound legacy hash");
    d.push(
        tag("send_guard.forbidden_authorities")?,
        "g01_mainnet_acceptance_proposal_v1.json",
    );
    d.push(
        tag("send_guard.forbidden_authorities")?,
        "g01_mainnet_acceptance_proposal_v2.json and every other JSON contract or draft",
    );
    d.push(
        tag("send_guard.forbidden_authorities")?,
        "environment variable alone",
    );
    d.push(
        tag("send_guard.forbidden_authorities")?,
        "command-line flag alone",
    );
    d.set(tag("dry_pass.status")?, "PASS_BOUND_BY_EVIDENCE_BINDINGS");
    d.set(
        tag("dry_pass.valid_mainnet_bytes_may_leave_machine")?,
        false,
    );
    d.push(tag("dry_pass.criteria")?, criterion("DRY-1-BYTES-AND-SIGNERS", "Both builder payload constructors contain the exact same raw_signed_launch_A bytes followed by the exact same raw_signed_buy_B bytes; independent transaction decoding and signature recovery yield distinct frozen wallet A and wallet B addresses, correct ChainID 56, nonces, Portal, value, gas and calldata."));
    d.push(tag("dry_pass.criteria")?, criterion("DRY-2-INDEPENDENT-LOCAL-SIMULATION", "A local/offline independent sequential execution of launch_A followed by buy_B succeeds, produces the same token address and exact received token amount as the local economics calculation, and signed minOutput is >0 and <= independently simulated received amount. Historical DOGE fixture values are not reused as authority."));
    d.push(tag("dry_pass.criteria")?, criterion("DRY-3-GAS-FLOOR-AND-BUDGET", "Current acceptance/ranking gas-floor semantics for every armed builder are independently verified before signing; identical signed bytes satisfy the strictest verified requirement. Independent simulated gas for launch and BUY, multiplied by the signed gas price and combined with launch value, BUY value and any allowed builder cost, fits both the per-attempt reserve and absolute approved budget."));
    d.push(tag("dry_pass.criteria")?, criterion("DRY-4-EXPIRY-NONCES-NO-EGRESS", "Bundle expiry/maxBlock fields are within the approved tight window, launcher and buyer pending nonces match the signed transactions, and an egress guard proves zero bytes of the valid signed launch, valid signed BUY, or valid bundle payload were written to any socket/process/stdout/file intended for network transmission before authorization."));
    d.set(tag("dry_pass.provider_prequalification")?, "Builder method/format/expiry behavior before authorization may only use deliberately non-executable fixtures. Exact valid Mainnet transaction/bundle bytes must not be sent to a remote simulation endpoint before authorization.");
    d.set(tag("dry_pass.all_four_required")?, true);
    d.set(tag("dry_pass.evidence_scope_note")?, "The bound DRY (2026-10-15T22:22:41Z, head 127865760) was evaluated against the proposal sha 026b534f...; its transaction hashes bind the exact signed bytes. Its bundle projection serialized BlockRazor maxBlockNumber as a hex string while live G0.7 sends a JSON integer (the documented format), and live G0.7 adds the 48Club maxTimestamp from the head read immediately before the release, so the DRY envelope hashes do not describe the live request bodies; only the transactions are bound.");
    d.push(tag("pre_arm_check_status")?, "0 PASS: preflight drift fields in the bound DRY preflight; read-only readiness PASS 2026-10-16; re-checked by G0.7 at runtime");
    d.push(tag("pre_arm_check_status")?, "1 PASS: DRY-2 independent EVM sequence (economics_crosscheck EVM_SEQUENCE_PASS) and exact local simulation");
    d.push(tag("pre_arm_check_status")?, "2 PASS: distinct DRY wallets; pending nonces 0/0 in the bound preflight and in readiness 2026-10-16");
    d.push(tag("pre_arm_check_status")?, "3 PASS: readiness FUNDING=PASS 2026-10-16 against the exact DRY parameters (launcher 0.0003301635, buyer 0.000151 BNB)");
    d.push(tag("pre_arm_check_status")?, "4 PASS: native quote equals the independent EVM output (176693134169499757962); minOutput positive");
    d.push(
        tag("pre_arm_check_status")?,
        "5 PASS: DRY-1 bytes and signers",
    );
    d.push(tag("pre_arm_check_status")?, "6 PASS AT SUBMISSION, residuals UNQUALIFIED: builder probe 2026-10-16 (non-executable bundles; evidence bsc/native/evidence/g06a/builder_probe_20261016T183411Z/probe.evidence.json, SHA-256 5636c463d6247b90d17d9bc977a6e50aae499434a17d241f16f05b87ba95cc7f; plan 77957205fd844dea51d4eb97a4b12c345978952fafc91c95cefec75d28f9abf5): both builders parse the exact G0.7 method and format (48Club: integer maxBlockNumber, maxTimestamp, noMerge, positionFirst; BlockRazor: integer maxBlockNumber with Authorization) and reached transaction validation (nonce too high); both check expiry at submission before the transactions (48Club: maxBlockNumber >= current block and maxTimestamp >= current block timestamp; BlockRazor: maxBlockNumber > current block) and accepted the G0.7 values; BlockRazor refuses gas prices below exactly 0.05 gwei at submission and the G0.7 price passes; 48Club shows no gas-price check at submission (a 1-wei control reached the same nonce check). UNQUALIFIED and requiring explicit user acceptance before approval: 48Club acceptance of 0.05 gwei for inclusion, building-time enforcement of maxBlockNumber next to maxTimestamp, acceptance of a valid bundle and inclusion");
    d.push(tag("pre_arm_check_status")?, "7 PASS (zero margin): builder refresh 2026-10-16; every refreshable explicit floor is 50000000 wei = signed gas price; the builder probe 2026-10-16 confirmed BlockRazor's submission floor of exactly 0.05 gwei");
    d.push(
        tag("pre_arm_check_status")?,
        "8 PASS: DRY-4 zero egress; no signed bytes left the machine",
    );
    d.push(
        tag("pre_arm_check_status")?,
        "9 PENDING: explicit user approval of these exact bytes, scope and budget",
    );
    d.push(tag("pre_arm_check_status")?, "10 PARTIAL: builder probe 2026-10-16 (non-executable bundles; evidence bsc/native/evidence/g06a/builder_probe_20261016T183411Z/probe.evidence.json, SHA-256 5636c463d6247b90d17d9bc977a6e50aae499434a17d241f16f05b87ba95cc7f; plan 77957205fd844dea51d4eb97a4b12c345978952fafc91c95cefec75d28f9abf5): 48Club parses noMerge and positionFirst as typed bool fields (string values refused with -32602 naming SendBundleArgs.noMerge / SendBundleArgs.positionFirst); their effect at building time (no merging, top of block) is UNQUALIFIED and requires explicit user acceptance before approval");
    Ok(())
}

/// The record files of a requalification DRY run directory
/// (`tools/g06a_final_dry.sh`).
pub const REQUAL_DRY: &str = "dry_result.dbrec";
pub const REQUAL_PREFLIGHT: &str = "preflight.dbrec";
pub const REQUAL_LOCAL_SIMULATION: &str = "local_simulation.dbrec";
pub const REQUAL_ECONOMICS: &str = "economics_crosscheck.dbrec";
pub const REQUAL_PORTAL_UPGRADE: &str = "portal_upgrade.dbrec";

/// One input of the v4 contract: its repository path, the SHA-256 of the
/// record file and, for an audited import, the legacy SHA-256 it carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct V4Input {
    pub path: String,
    pub record_sha256: [u8; 32],
    pub legacy_sha256: Option<[u8; 32]>,
}

/// What the v4 contract binds and quotes, read from the requalification
/// records.
#[derive(Clone, Debug)]
pub struct V4Evidence {
    pub dry: V4Input,
    pub preflight: V4Input,
    pub builder_policy: V4Input,
    pub witness_source: V4Input,
    pub local_simulation: V4Input,
    pub economics: V4Input,
    pub portal_upgrade: V4Input,
    /// The superseded v3 proposal.
    pub supersedes_sha256: [u8; 32],
    head: u64,
    portal_version: String,
    portal_implementation: [u8; 20],
    nonces: (u64, u64),
    funding_pass: bool,
    required_wei: ([u8; 32], [u8; 32]),
    expected_output: [u8; 32],
    tx_hashes: ([u8; 32], [u8; 32]),
    builder_observed: String,
    builder_floor: [u8; 32],
}

fn read_input(
    root: &Path,
    path: &str,
    accepted: &[&'static Schema],
) -> Result<(V4Input, Record), String> {
    let bytes = std::fs::read(root.join(path)).map_err(|e| format!("{path}: {e}"))?;
    let record = decode_any(&bytes).map_err(|e| format!("{path}: {e}"))?;
    if !accepted.contains(&record.schema()) {
        return Err(format!(
            "{path}: {} is not an accepted input here",
            record.schema().name
        ));
    }
    let legacy = if record.schema().field(IMPORT_LEGACY_SHA256).is_some() {
        record.opt_b256(IMPORT_LEGACY_SHA256).map_err(|e| e.0)?
    } else {
        None
    };
    Ok((
        V4Input {
            path: path.to_string(),
            record_sha256: sha256(&bytes),
            legacy_sha256: legacy,
        },
        record,
    ))
}

/// Reads the v4 evidence: the native records of the requalification DRY in
/// `requal_dir`, the builder policy at `builder_policy` (native refresh or
/// audited import), the imported producer-witness source and the v3
/// proposal it supersedes; every path relative to `root`.
pub fn attempt1_v4_evidence(
    root: &Path,
    requal_dir: &str,
    builder_policy: &str,
) -> Result<V4Evidence, String> {
    base_evidence(root, requal_dir, builder_policy, Shape::V4).map(|(e, _, _)| e)
}

/// The v4 evidence with the requalification's DRY and preflight records.
fn base_evidence(
    root: &Path,
    requal_dir: &str,
    builder_policy: &str,
    shape: Shape<'_>,
) -> Result<(V4Evidence, Record, Record), String> {
    use g06a::{
        builder_policy as b, dry as d, economics, local_simulation, portal_upgrade as up,
        preflight as p,
    };
    requalification_pass(root, requal_dir, builder_policy, shape)?;
    let b = matches!(shape, Shape::B(_));
    let at = |name: &str| format!("{}/{name}", requal_dir.trim_end_matches('/'));
    let e = |x: darkbot_bsc_record::RecordError| x.0;
    let (dry, dry_rec) = read_input(root, &at(REQUAL_DRY), &[&d::NATIVE])?;
    require_dry_pass(&dry_rec)?;
    let frozen_bytes = std::fs::read(root.join(DRY_RECORD)).map_err(|e| e.to_string())?;
    if hex(&sha256(&frozen_bytes)) != "c7153a7da4c6154a2a6af384c9836d7141d16fd643662d385d20d3d18ba99b98" {
        return Err("frozen DRY identity hash mismatch".into());
    }
    let frozen = decode(&d::SCHEMA, &frozen_bytes).map_err(e)?;
    for tag in [d::LAUNCHER, d::BUYER, d::DISTINCT, d::CHAIN_ID, d::PORTAL, d::GAS_PRICE_WEI, d::LAUNCH_GAS_LIMIT, d::BUY_GAS_LIMIT, d::LAUNCH_VALUE_WEI, d::BUY_VALUE_WEI, d::LAUNCH_TX_HASH, d::BUY_TX_HASH, d::TOKEN, d::LOCAL_EXPECTED_OUTPUT, d::MIN_OUTPUT] {
        // A SELF_B run has its own token and signed bytes, all else is A's.
        if b && [d::LAUNCH_TX_HASH, d::BUY_TX_HASH, d::TOKEN].contains(&tag) {
            continue;
        }
        if dry_rec.get(tag) != frozen.get(tag) {
            return Err(format!("frozen DRY identity mismatch: {tag}"));
        }
    }
    if dry_rec.address(d::LAUNCHER).map_err(e)? != hex_literal::<20>(LAUNCHER)? || dry_rec.address(d::BUYER).map_err(e)? != hex_literal::<20>(BUYER)? {
        return Err("DRY wallets differ from contract".into());
    }
    let (preflight, pre) = read_input(root, &at(REQUAL_PREFLIGHT), &[&p::SCHEMA])?;
    if preflight.legacy_sha256.is_some() {
        return Err("the requalification preflight must be a native record, not an import".into());
    }
    if pre.u64(p::CHAIN_ID).map_err(e)? != 56 || pre.address(p::PORTAL).map_err(e)? != dry_rec.address(d::PORTAL).map_err(e)? { return Err("preflight chain/Portal mismatch".into()); }
    let (builder_policy, policy) = read_input(root, builder_policy, &[&b::NATIVE, &b::SCHEMA])?;
    let (witness_source, witness) =
        read_input(root, WITNESS_SOURCE_RECORD, &[&g07::witness_source::SCHEMA])?;
    if witness.text(g07::witness_source::ATTEMPT_ID).map_err(e)? != ATTEMPT_ID {
        return Err("witness attempt_id differs from contract".into());
    }
    for (tag, expected) in [(p::LAUNCHER_PENDING_NONCE, 0), (p::BUYER_PENDING_NONCE, 0), (p::PROFILE_GAS_PRICE_WEI, 50_000_000), (p::PROFILE_LAUNCH_GAS_LIMIT, 6_603_270), (p::PROFILE_BUY_GAS_LIMIT, 3_000_000)] {
        // Gas price profile is a U256; nonce and gas-limit profiles are U64.
        let matches = if tag == p::PROFILE_GAS_PRICE_WEI { pre.u256(tag).map_err(e)? == darkbot_bsc_record::u256_from_u128(expected as u128) } else { pre.u64(tag).map_err(e)? == expected };
        if !matches { return Err(format!("preflight frozen profile mismatch: {tag}")); }
    }
    for (tag, expected) in [(p::PROFILE_LAUNCH_VALUE_WEI, 0), (p::PROFILE_BUY_VALUE_WEI, 1_000_000_000_000u128)] {
        if pre.u256(tag).map_err(e)? != darkbot_bsc_record::u256_from_u128(expected) { return Err("preflight value mismatch".into()); }
    }
    if pre.address(p::LAUNCHER_ADDRESS).map_err(e)? != dry_rec.address(d::LAUNCHER).map_err(e)? || pre.address(p::BUYER_ADDRESS).map_err(e)? != dry_rec.address(d::BUYER).map_err(e)? || pre.opt_bool(p::FUNDING_PASS).map_err(e)? != Some(true) {
        return Err("preflight wallets/funding mismatch".into());
    }
    let (local_simulation, _) = read_input(
        root,
        &at(REQUAL_LOCAL_SIMULATION),
        &[&local_simulation::NATIVE],
    )?;
    // A secret salt is only ever recorded as its commitment.
    let accepted: &[&'static Schema] = if b {
        &[&economics::PRIVATE]
    } else {
        &[&economics::NATIVE]
    };
    let (economics, _) = read_input(root, &at(REQUAL_ECONOMICS), accepted)?;
    let (portal_upgrade, upgrade) = read_input(root, &at(REQUAL_PORTAL_UPGRADE), &[&up::SCHEMA])?;
    let implementation = pre.address(p::PORTAL_IMPLEMENTATION).map_err(e)?;
    if upgrade.address(up::CURRENT_IMPLEMENTATION).map_err(e)? != implementation {
        return Err(
            "the upgrade scan does not end at the preflight's Portal implementation".into(),
        );
    }
    let v3 = std::fs::read(root.join(crate::acceptance_guard::V3_RELATIVE_PATH))
        .map_err(|x| format!("{}: {x}", crate::acceptance_guard::V3_RELATIVE_PATH))?;
    let evidence = V4Evidence {
        dry,
        preflight,
        builder_policy,
        witness_source,
        local_simulation,
        economics,
        portal_upgrade,
        supersedes_sha256: sha256(&v3),
        head: pre.u64(p::LATEST_BLOCK_NUMBER).map_err(e)?,
        portal_version: pre.text(p::PORTAL_VERSION).map_err(e)?.to_string(),
        portal_implementation: implementation,
        nonces: (
            pre.u64(p::LAUNCHER_PENDING_NONCE).map_err(e)?,
            pre.u64(p::BUYER_PENDING_NONCE).map_err(e)?,
        ),
        funding_pass: pre.opt_bool(p::FUNDING_PASS).map_err(e)? == Some(true),
        required_wei: (
            pre.u256(p::LAUNCHER_REQUIRED_WEI).map_err(e)?,
            pre.u256(p::BUYER_REQUIRED_WEI).map_err(e)?,
        ),
        expected_output: dry_rec.u256(d::LOCAL_EXPECTED_OUTPUT).map_err(e)?,
        tx_hashes: (
            dry_rec.b256(d::LAUNCH_TX_HASH).map_err(e)?,
            dry_rec.b256(d::BUY_TX_HASH).map_err(e)?,
        ),
        builder_observed: policy.text(b::OBSERVED_DATE).map_err(e)?.to_string(),
        builder_floor: policy.u256(b::STRICTEST_FLOOR_WEI).map_err(e)?,
    };
    Ok((evidence, dry_rec, pre))
}

/// Replaces item `index` of the repeated text field `name`.
fn replace_item(d: &mut Draft, name: &str, index: usize, text: &str) -> Result<(), String> {
    let field = tag(name)?;
    let mut items = d.values(field).to_vec();
    let item = items
        .get_mut(index)
        .ok_or_else(|| format!("{name} has no item {index}"))?;
    *item = Value::Text(text.to_string());
    d.clear(field);
    for value in items {
        d.push(field, value);
    }
    Ok(())
}

/// The exact bytes of the requalified attempt-1 contract (v4).
pub fn attempt1_v4(e: &V4Evidence, authorization: Authorization) -> Result<Vec<u8>, String> {
    let approved = authorization == Authorization::Approved;
    let mut d = Draft::new(&contract::V4);
    common(&mut d, approved)?;
    d.set(tag("economics_validation.gas_floor.blockrazor_current_documented_reference")?, "Active policy: authorization.evidence_bindings.builder_policy_record_sha256; see the bound native policy for the documented floor, not proof of inclusion or price enforcement");
    d.set(tag("economics_validation.gas_floor.bound_policy_superseded_fields")?, "The bound native builder policy supersedes the historical v3 addendum/refresh; inclusion, 48Club price enforcement and noMerge/positionFirst effects remain UNQUALIFIED");
    d.set(tag("budget_wei.note")?, "Frozen worst-case reserve: (6603270 + 3000000) gas * 50000000 wei + 1000000000000 wei BUY = 481163500000000 wei. Gas limits are ceilings, not measured consumption. Current measurements and funding belong to the hash-bound native local_simulation, economics and preflight records");
    let implementation = format!("0x{}", hex(&e.portal_implementation));
    let dry_dir = e
        .dry
        .path
        .rsplit_once('/')
        .map_or(e.dry.path.as_str(), |(dir, _)| dir);
    d.set(
        tag("supersedes")?,
        format!(
            "bsc/config/g07_attempt1_acceptance_v3.dbrec (proposal SHA-256 {}, bound to the Portal v5.24.0 DRY of 2026-10-15) and, as legacy only, the JSON v1/v2 contracts and the JSON v2 draft",
            hex(&e.supersedes_sha256)
        ),
    );
    d.set(
        tag("authorization.note")?,
        if approved {
            format!(
                "Final single-attempt authorization for G0.7 attempt g07-mainnet-attempt-1-dry-20261015T222241Z, requalified against Flap Portal {} (implementation {implementation}) by the DRY in {dry_dir}. Effective only after the user explicitly approves these exact bytes and a dedicated arming commit sets FROZEN_APPROVED_V4_SHA256 to their SHA-256 while this file sits at bsc/config/g07_attempt1_acceptance_v4.dbrec.",
                e.portal_version
            )
        } else {
            format!(
                "PROPOSAL ONLY: no real-money authorization exists for G0.7 attempt g07-mainnet-attempt-1-dry-20261015T222241Z (requalified against Flap Portal {}, DRY in {dry_dir}); the guard refuses this contract (approved=false, execution_ready=false). The approved variant differs only in status, note, approved and execution_ready; it is written only for the exact SHA-256 the user approves, and a dedicated arming commit must then compile that SHA-256 as FROZEN_APPROVED_V4_SHA256.",
                e.portal_version
            )
        },
    );
    d.set(
        tag("authorization.execution_authority")?,
        "Only this binary v4 contract may authorize the self-launch Mainnet send path; the superseded v3 proposal and the JSON v1/v2 contracts and drafts are forbidden as runtime authority",
    );
    d.set(
        tag("authorization.evidence_paths.note")?,
        "The guard binds the record SHA-256 of the four inputs G0.7 reads, and the legacy SHA-256 where an input is an audited import; the local simulation, the economics cross-check and the Portal upgrade scan are bound for review and readiness. G0.7 takes the record paths at runtime by environment variable.",
    );
    d.set(
        tag("dry_pass.evidence_scope_note")?,
        format!(
            "The requalification DRY in {dry_dir} at head {} against Flap Portal {} (implementation {implementation}) binds the exact signed bytes: launch 0x{}, BUY 0x{}. Its bundle projection uses G0.7's fixed wire templates; the 48Club maxTimestamp is set from the send head at the release (projected as 0), so only the transactions are bound.",
            e.head,
            e.portal_version,
            hex(&e.tx_hashes.0),
            hex(&e.tx_hashes.1)
        ),
    );
    replace_item(
        &mut d,
        "pre_arm_checks",
        9,
        "Explicit user approval of this v4 scope and numeric absolute budget ceiling",
    )?;
    replace_item(
        &mut d,
        "send_guard.required_checks",
        0,
        "Runtime file path is exactly bsc/config/g07_attempt1_acceptance_v4.dbrec (relative to the repository root)",
    )?;
    replace_item(
        &mut d,
        "send_guard.required_checks",
        1,
        "SHA-256 of the exact v4 record bytes equals the hash compiled/frozen into the armed harness",
    )?;
    replace_item(
        &mut d,
        "send_guard.required_checks",
        2,
        "The file decodes fail-closed as DBREC001 record type 25, schema version 4 (darkbot.g07.acceptance-contract.v4)",
    )?;
    replace_item(
        &mut d,
        "send_guard.required_checks",
        8,
        "The contract does not hold its own hash (no guard check): the hash binding is external via FROZEN_APPROVED_V4_SHA256 in the arming commit, because a file cannot contain its own SHA-256",
    )?;
    replace_item(
        &mut d,
        "send_guard.required_checks",
        12,
        "The four authorization.evidence_bindings record hashes equal the SHA-256 of the DRY, preflight, builder-policy and producer-witness-source records passed at runtime; where an input is an audited import, its legacy hash equals the bound legacy hash",
    )?;
    d.push(
        tag("send_guard.forbidden_authorities")?,
        "bsc/config/g07_attempt1_acceptance_v3.dbrec (superseded proposal)",
    );
    replace_item(
        &mut d,
        "pre_arm_check_status",
        0,
        &format!(
            "0 PASS: requalification preflight (head {}, Portal {}, implementation {implementation}) and the Portal upgrade scan bound by hash; G0.7 re-checks the deployment against this baseline at runtime",
            e.head, e.portal_version
        ),
    )?;
    replace_item(
        &mut d,
        "pre_arm_check_status",
        1,
        &format!(
            "1 PASS: requalification DRY-2 independent EVM sequence (EVM_SEQUENCE_PASS) and exact local simulation against Portal {}",
            e.portal_version
        ),
    )?;
    replace_item(
        &mut d,
        "pre_arm_check_status",
        2,
        &format!(
            "2 PASS: distinct DRY wallets; pending nonces {}/{} in the requalification preflight",
            e.nonces.0, e.nonces.1
        ),
    )?;
    replace_item(
        &mut d,
        "pre_arm_check_status",
        3,
        &format!(
            "3 {}: requalification preflight funding against the exact DRY parameters (launcher {} wei, buyer {} wei required)",
            if e.funding_pass { "PASS" } else { "PENDING" },
            u256_to_decimal(&e.required_wei.0),
            u256_to_decimal(&e.required_wei.1)
        ),
    )?;
    replace_item(
        &mut d,
        "pre_arm_check_status",
        4,
        &format!(
            "4 PASS: native quote equals the independent EVM output ({}); minOutput positive",
            u256_to_decimal(&e.expected_output)
        ),
    )?;
    replace_item(
        &mut d,
        "pre_arm_check_status",
        7,
        &format!(
            "7 BOUND: builder policy {} observed {}; strictest explicit floor {} wei; readiness requires an observation no older than 24 h before arming",
            e.builder_policy.path,
            e.builder_observed,
            u256_to_decimal(&e.builder_floor)
        ),
    )?;
    let opt_word = |w: Option<[u8; 32]>| w.map(Value::B256);
    d.set(
        contract::DRY_RECORD_SHA256,
        Value::B256(e.dry.record_sha256),
    )
    .set(
        contract::PREFLIGHT_RECORD_SHA256,
        Value::B256(e.preflight.record_sha256),
    )
    .set(
        contract::BUILDER_POLICY_RECORD_SHA256,
        Value::B256(e.builder_policy.record_sha256),
    )
    .set(
        contract::WITNESS_SOURCE_RECORD_SHA256,
        Value::B256(e.witness_source.record_sha256),
    )
    .set_opt(contract::DRY_LEGACY_SHA256, opt_word(e.dry.legacy_sha256))
    .set_opt(
        contract::PREFLIGHT_LEGACY_SHA256,
        opt_word(e.preflight.legacy_sha256),
    )
    .set_opt(
        contract::BUILDER_POLICY_LEGACY_SHA256,
        opt_word(e.builder_policy.legacy_sha256),
    )
    .set_opt(
        contract::WITNESS_SOURCE_LEGACY_SHA256,
        opt_word(e.witness_source.legacy_sha256),
    )
    .set(
        contract::LOCAL_SIMULATION_RECORD_SHA256,
        Value::B256(e.local_simulation.record_sha256),
    )
    .set(
        contract::ECONOMICS_RECORD_SHA256,
        Value::B256(e.economics.record_sha256),
    )
    .set(contract::DRY_RECORD_PATH, e.dry.path.as_str())
    .set(contract::PREFLIGHT_RECORD_PATH, e.preflight.path.as_str())
    .set(
        contract::BUILDER_POLICY_RECORD_PATH,
        e.builder_policy.path.as_str(),
    )
    .set(
        contract::WITNESS_SOURCE_RECORD_PATH,
        e.witness_source.path.as_str(),
    )
    .set(
        contract::LOCAL_SIMULATION_RECORD_PATH,
        e.local_simulation.path.as_str(),
    )
    .set(contract::ECONOMICS_RECORD_PATH, e.economics.path.as_str())
    .set(
        contract::SUPERSEDES_CONTRACT_SHA256,
        Value::B256(e.supersedes_sha256),
    )
    .set(
        contract::PORTAL_UPGRADE_RECORD_SHA256,
        Value::B256(e.portal_upgrade.record_sha256),
    )
    .set(
        contract::PORTAL_UPGRADE_RECORD_PATH,
        e.portal_upgrade.path.as_str(),
    );
    d.encode().map_err(|x| x.0)
}

/// What the v5 contract binds beyond v4 (v9.1 C1/R1): the launch-path
/// snapshot of the requalification run, the candidate it signs for and the
/// v4 proposal it supersedes.
#[derive(Clone, Debug)]
pub struct V5Evidence {
    pub base: V4Evidence,
    pub launch_path: V4Input,
    pub candidate_profile: String,
    pub candidate_id: [u8; 32],
    /// The superseded v4 proposal.
    pub supersedes_v4_sha256: [u8; 32],
    launch_path_entries: (usize, usize),
}

/// Reads the v5 evidence: the v4 evidence of a requalification run that also
/// passed its launch-path check, its snapshot (checked against the run's
/// preflight and of the DRY's token), the public record of the candidate at
/// `candidate_record` (its ID recomputed; of exactly that preflight's
/// deployment — Portal, version, implementation and Tax-V3 with their code
/// hashes; the candidate stays the one created earlier, v9.1 D3 — for the
/// DRY's launcher and token and, where it carries them, the DRY's signed
/// hashes) and the inactive v4 proposal it supersedes.
pub fn attempt1_v5_evidence(
    root: &Path,
    requal_dir: &str,
    builder_policy: &str,
    candidate_record: &str,
) -> Result<V5Evidence, String> {
    use darkbot_bsc_record::schema::g07_candidate::{candidate as c, candidate_id_of};
    use g06a::{dry as d, preflight as p};
    let e = |x: darkbot_bsc_record::RecordError| x.0;
    evidence_path(root, candidate_record)?;
    let candidate_bytes = std::fs::read(root.join(candidate_record))
        .map_err(|x| format!("{candidate_record}: {x}"))?;
    let candidate =
        decode(&c::SCHEMA, &candidate_bytes).map_err(|x| format!("{candidate_record}: {x}"))?;
    // SELF_B: its own run (DRY6, private cross-check); SELF_A: the frozen identity.
    let secret_salt = candidate.text(c::PROFILE).map_err(e)? == "SELF_B";
    let shape = if secret_salt {
        Shape::B(candidate_record)
    } else {
        Shape::V5
    };
    let (base, dry, pre) = base_evidence(root, requal_dir, builder_policy, shape)?;
    // BSC-003: only a DRY whose bundle projection is the compiled semantics.
    crate::bundle_wire::check_projection(&dry)?;
    let path = format!("{}/{REQUAL_LAUNCH_PATH}", requal_dir.trim_end_matches('/'));
    evidence_path(root, &path)?;
    let bytes = std::fs::read(root.join(&path)).map_err(|x| format!("{path}: {x}"))?;
    let snapshot = crate::launch_path::check(&bytes, &pre)?;
    if snapshot.token != dry.address(d::TOKEN).map_err(e)? {
        return Err("the launch path traces another token than the DRY".into());
    }
    let id = candidate.b256(c::CANDIDATE_ID).map_err(e)?;
    let profile = candidate.text(c::PROFILE).map_err(e)?.to_string();
    if candidate_id_of(&candidate).map_err(e)? != id
        || !matches!(profile.as_str(), "SELF_A" | "SELF_B")
    {
        return Err("the candidate record fails its identity".into());
    }
    let same_address = |ct: u16, r: &Record, rt: u16| -> Result<bool, String> {
        Ok(candidate.address(ct).map_err(e)? == r.address(rt).map_err(e)?)
    };
    let same_word = |ct: u16, r: &Record, rt: u16| -> Result<bool, String> {
        Ok(candidate.b256(ct).map_err(e)? == r.b256(rt).map_err(e)?)
    };
    let deployment = candidate.u64(c::CHAIN_ID).map_err(e)? == pre.u64(p::CHAIN_ID).map_err(e)?
        && same_address(c::PORTAL, &pre, p::PORTAL)?
        && candidate.text(c::PORTAL_VERSION).map_err(e)?
            == pre.text(p::PORTAL_VERSION).map_err(e)?
        && same_address(c::PORTAL_IMPLEMENTATION, &pre, p::PORTAL_IMPLEMENTATION)?
        && same_word(
            c::PORTAL_IMPLEMENTATION_CODE_SHA256,
            &pre,
            p::PORTAL_IMPLEMENTATION_CODE_SHA256,
        )?
        && same_address(c::TAX_V3_IMPLEMENTATION, &pre, p::TAX_V3_IMPL)?
        && same_word(
            c::TAX_V3_IMPLEMENTATION_CODE_SHA256,
            &pre,
            p::TAX_V3_IMPL_CODE_SHA256,
        )?;
    if !deployment {
        return Err("the candidate is not of the requalification preflight's deployment".into());
    }
    if !same_address(c::LAUNCHER, &dry, d::LAUNCHER)? || !same_address(c::TOKEN, &dry, d::TOKEN)? {
        return Err("the candidate's launcher or token differs from the DRY".into());
    }
    for (ct, dt) in [
        (c::LAUNCH_TX_HASH, d::LAUNCH_TX_HASH),
        (c::BUY_TX_HASH, d::BUY_TX_HASH),
    ] {
        if candidate
            .opt_b256(ct)
            .map_err(e)?
            .is_some_and(|h| Some(h) != dry.b256(dt).ok())
        {
            return Err("the candidate's signed hashes differ from the DRY".into());
        }
    }
    if secret_salt {
        let econ_bytes =
            std::fs::read(root.join(&base.economics.path)).map_err(|x| x.to_string())?;
        if sha256(&econ_bytes) != base.economics.record_sha256 {
            return Err("the private cross-check changed while it was read".into());
        }
        let econ = decode(&g06a::economics::PRIVATE, &econ_bytes).map_err(e)?;
        if econ.b256(g06a::economics::SALT_COMMITMENT).map_err(e)?
            != candidate.b256(c::SALT_COMMITMENT).map_err(e)?
        {
            return Err("the private cross-check is not of the candidate's salt".into());
        }
    }
    let v4 = std::fs::read(root.join(crate::acceptance_guard::V4_RELATIVE_PATH))
        .map_err(|x| format!("{}: {x}", crate::acceptance_guard::V4_RELATIVE_PATH))?;
    let superseded = decode(&contract::V4, &v4).map_err(|x| format!("v4 proposal: {x}"))?;
    if superseded.bool(contract::APPROVED).map_err(e)?
        || superseded.bool(contract::EXECUTION_READY).map_err(e)?
    {
        return Err("the superseded v4 contract is not the inactive proposal".into());
    }
    Ok(V5Evidence {
        launch_path: V4Input {
            path,
            record_sha256: sha256(&bytes),
            legacy_sha256: None,
        },
        candidate_profile: profile,
        candidate_id: id,
        supersedes_v4_sha256: sha256(&v4),
        launch_path_entries: (snapshot.code.len(), snapshot.storage.len()),
        base,
    })
}

/// The exact bytes of the attempt-1 contract v5: the v4 bytes of the same
/// evidence field for field, with the v5 authority texts, the launch-path
/// and candidate binding and the superseded v4 proposal.
pub fn attempt1_v5(e: &V5Evidence, authorization: Authorization) -> Result<Vec<u8>, String> {
    let approved = authorization == Authorization::Approved;
    let v4 = decode(&contract::V4, &attempt1_v4(&e.base, authorization)?).map_err(|x| x.0)?;
    let mut d = Draft::new(&contract::V5);
    for (field, value) in v4.fields() {
        d.push(*field, value.clone());
    }
    let implementation = format!("0x{}", hex(&e.base.portal_implementation));
    let candidate = format!("{} 0x{}", e.candidate_profile, hex(&e.candidate_id));
    d.set(
        tag("supersedes")?,
        format!(
            "bsc/config/g07_attempt1_acceptance_v4.dbrec (proposal SHA-256 {}; binds no launch-path snapshot, v9.1 C1/R1), the v3 proposal and, as legacy only, the JSON v1/v2 contracts and the JSON v2 draft",
            hex(&e.supersedes_v4_sha256)
        ),
    );
    d.set(
        tag("authorization.note")?,
        if approved {
            format!(
                "Final single-attempt authorization for G0.7 attempt {ATTEMPT_ID}, candidate {candidate}, requalified against Flap Portal {} (implementation {implementation}) with its launch-path snapshot. Effective only after the user explicitly approves these exact bytes and a dedicated arming commit sets FROZEN_APPROVED_V5_SHA256 to their SHA-256 while this file sits at bsc/config/g07_attempt1_acceptance_v5.dbrec.",
                e.base.portal_version
            )
        } else {
            format!(
                "PROPOSAL ONLY: no real-money authorization exists for G0.7 attempt {ATTEMPT_ID} (candidate {candidate}, Flap Portal {}); the guard refuses this contract (approved=false, execution_ready=false). The approved variant differs only in status, note, approved and execution_ready; it is written only for the exact SHA-256 the user approves, and a dedicated arming commit must then compile that SHA-256 as FROZEN_APPROVED_V5_SHA256.",
                e.base.portal_version
            )
        },
    );
    d.set(
        tag("authorization.execution_authority")?,
        "Only this binary v5 contract may authorize the self-launch Mainnet send path; the superseded v4 and v3 proposals and the JSON v1/v2 contracts and drafts are forbidden as runtime authority",
    );
    replace_item(
        &mut d,
        "pre_arm_checks",
        9,
        "Explicit user approval of this v5 scope, candidate and numeric absolute budget ceiling",
    )?;
    replace_item(
        &mut d,
        "send_guard.required_checks",
        0,
        "Runtime file path is exactly bsc/config/g07_attempt1_acceptance_v5.dbrec (relative to the repository root)",
    )?;
    replace_item(
        &mut d,
        "send_guard.required_checks",
        1,
        "SHA-256 of the exact v5 record bytes equals the hash compiled/frozen into the armed harness",
    )?;
    replace_item(
        &mut d,
        "send_guard.required_checks",
        2,
        "The file decodes fail-closed as DBREC001 record type 34, schema version 5 (darkbot.g07.acceptance-contract.v5)",
    )?;
    replace_item(
        &mut d,
        "send_guard.required_checks",
        8,
        "The contract does not hold its own hash (no guard check): the hash binding is external via FROZEN_APPROVED_V5_SHA256 in the arming commit, because a file cannot contain its own SHA-256",
    )?;
    d.push(
        tag("send_guard.required_checks")?,
        "The launch-path snapshot bound by authorization.evidence_bindings.launch_path_record_sha256 is of the bound preflight's block and the DRY's token, and the runner re-reads every bound code hash and storage selector at its current pinned preflight block before the latch; any drift refuses",
    );
    d.push(
        tag("send_guard.required_checks")?,
        "The selected candidate (profile and ID, verified from its salt) equals authorization.candidate",
    );
    d.push(
        tag("send_guard.forbidden_authorities")?,
        "bsc/config/g07_attempt1_acceptance_v4.dbrec (superseded proposal without launch-path binding)",
    );
    d.push(
        tag("pre_arm_check_status")?,
        format!(
            "C1/R1 BOUND: launch-path snapshot {} ({} code entries, {} storage selectors) at the preflight block",
            e.launch_path.path, e.launch_path_entries.0, e.launch_path_entries.1
        ),
    );
    d.set(
        contract::SUPERSEDES_CONTRACT_SHA256,
        Value::B256(e.supersedes_v4_sha256),
    )
    .set(
        contract::LAUNCH_PATH_RECORD_SHA256,
        Value::B256(e.launch_path.record_sha256),
    )
    .set(
        contract::LAUNCH_PATH_RECORD_PATH,
        e.launch_path.path.as_str(),
    )
    .set(contract::CANDIDATE_PROFILE, e.candidate_profile.as_str())
    .set(contract::CANDIDATE_ID, Value::B256(e.candidate_id));
    d.encode().map_err(|x| x.0)
}
