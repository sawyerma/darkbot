use super::*;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

const CONFIG: &str = include_str!("../../../../config/ingress.public.env");

fn sources(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bsc-strategy-c-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
    dir
}

const MAX_OPEN: &str = "BSC_STRATEGY_C_MAX_OPEN";
const EXIT: &str = "BSC_STRATEGY_C_CREATOR_SELL_EXIT";

/// A set key takes its line's place, a key the file does not set is
/// appended, and the runtime's parser reads both as set.
#[test]
fn an_overlay_is_the_configuration_with_the_settings_in_place() {
    let without_exit: String = CONFIG
        .lines()
        .filter(|l| !l.starts_with(EXIT))
        .map(|l| format!("{l}\n"))
        .collect();
    let s = Settings {
        generation: 2,
        request_id: Some("r2".into()),
        values: vec![(MAX_OPEN, "3".into()), (EXIT, "DISABLED".into())],
    };
    // A configuration from before the key: ENABLED, what every run before it did.
    assert!(
        RuntimeConfig::parse(&without_exit)
            .unwrap()
            .strategy
            .engine
            .creator_sell_exit
    );
    let text = overlay(&without_exit, &s.values);
    assert_eq!(text.lines().filter(|l| l.starts_with(MAX_OPEN)).count(), 1);
    assert!(text.ends_with("BSC_STRATEGY_C_CREATOR_SELL_EXIT=DISABLED\n"));
    let c = effective(&without_exit, &s).unwrap();
    assert_eq!(c.strategy.engine.max_open, 3);
    assert!(!c.strategy.engine.creator_sell_exit);
    // The file's own value where nothing is set.
    let file = RuntimeConfig::parse(CONFIG).unwrap();
    assert!(file.strategy.engine.creator_sell_exit);
    assert_eq!(
        c.strategy.engine.observation_ms,
        file.strategy.engine.observation_ms
    );
    let values = effective_values(CONFIG, &s);
    assert_eq!(values.len(), KEYS.len());
    assert!(values.contains(&(MAX_OPEN, "3".into())));
    assert!(values.contains(&(EXIT, "DISABLED".into())));
}

/// One generation per applied request, its receipt idempotent; another
/// payload under the ID is refused; a stale generation is refused.
#[test]
fn a_setting_is_a_compare_and_swap_with_one_receipt_per_request() {
    let dir = sources("cas");
    assert_eq!(current(&dir).unwrap(), Settings::default());
    let o = Owner::open(&dir).unwrap();
    let r = o.set("r1", MAX_OPEN, "3", 0, CONFIG, 1).unwrap();
    assert_eq!((r.outcome, r.generation), ("APPLIED", 1));
    assert_eq!(o.set("r1", MAX_OPEN, "3", 0, CONFIG, 2).unwrap(), r);
    assert!(o.set("r1", MAX_OPEN, "4", 0, CONFIG, 2).is_err());
    let stale = o.set("r2", MAX_OPEN, "4", 0, CONFIG, 3).unwrap();
    assert_eq!((stale.outcome, stale.generation), ("REJECTED_STALE", 1));
    let ahead = o.set("r3", MAX_OPEN, "4", 7, CONFIG, 3).unwrap();
    assert_eq!(ahead.outcome, "REJECTED_STALE");
    let r4 = o.set("r4", EXIT, "DISABLED", 1, CONFIG, 4).unwrap();
    assert_eq!((r4.outcome, r4.generation), ("APPLIED", 2));
    let now = current(&dir).unwrap();
    assert_eq!(now.generation, 2);
    assert_eq!(now.request_id.as_deref(), Some("r4"));
    assert_eq!(
        now.values,
        vec![(MAX_OPEN, "3".to_string()), (EXIT, "DISABLED".to_string())]
    );
    // A later value of the same key replaces it.
    o.set("r5", MAX_OPEN, "4", 2, CONFIG, 5).unwrap();
    assert_eq!(current(&dir).unwrap().value(MAX_OPEN), Some("4"));
}

/// A crash between a generation and its receipt: the same request reads
/// APPLIED from the generation that names it, current or kept.
#[test]
fn a_lost_receipt_is_recovered_from_its_generation() {
    let dir = sources("recover");
    let o = Owner::open(&dir).unwrap();
    o.set("r1", MAX_OPEN, "3", 0, CONFIG, 1).unwrap();
    let receipt = dir.join(DIR).join("receipts/r1.dbrec");
    std::fs::remove_file(&receipt).unwrap();
    let r = o.set("r1", MAX_OPEN, "3", 0, CONFIG, 2).unwrap();
    assert_eq!((r.outcome, r.generation), ("APPLIED", 1));
    o.set("r2", MAX_OPEN, "4", 1, CONFIG, 3).unwrap();
    std::fs::remove_file(&receipt).unwrap();
    let r = o.set("r1", MAX_OPEN, "3", 0, CONFIG, 4).unwrap();
    assert_eq!((r.outcome, r.generation), ("APPLIED", 1));
    // Another payload under the ID of a kept generation is refused.
    std::fs::remove_file(&receipt).unwrap();
    assert!(o.set("r1", MAX_OPEN, "5", 0, CONFIG, 5).is_err());
    // V8-01 review M1: under another CAS too (the ID's intent is kept):
    // never a second generation of one request.
    assert!(o.set("r1", MAX_OPEN, "3", 2, CONFIG, 6).is_err());
    assert_eq!(current(&dir).unwrap().generation, 2);
}

/// The runtime configuration's parser decides: a value out of its bounds
/// or against a cross-check is REJECTED_INVALID with its reason, and
/// nothing is stored; a key outside the scope or a value no line can hold
/// is refused without a receipt.
#[test]
fn the_runtime_configuration_refuses_what_it_would_not_start_with() {
    let dir = sources("invalid");
    let o = Owner::open(&dir).unwrap();
    let r = o.set("r1", MAX_OPEN, "0", 0, CONFIG, 1).unwrap();
    assert_eq!((r.outcome, r.generation), ("REJECTED_INVALID", 0));
    assert!(r.reason.as_deref().unwrap().contains(MAX_OPEN), "{r:?}");
    // The stop multiple at or above 1x (the strategy's own check).
    let r = o
        .set("r2", "BSC_STRATEGY_C_STOP_NET_MULTIPLE", "1", 0, CONFIG, 2)
        .unwrap();
    assert_eq!(r.outcome, "REJECTED_INVALID");
    let r = o.set("r3", EXIT, "MAYBE", 0, CONFIG, 3).unwrap();
    assert_eq!(r.outcome, "REJECTED_INVALID");
    assert_eq!(current(&dir).unwrap(), Settings::default());
    assert!(
        o.set("r4", "BSC_STRATEGY_C_GAS_PRICE_WEI", "1", 0, CONFIG, 4)
            .is_err()
    );
    assert!(o.set("r5", MAX_OPEN, "3 4", 0, CONFIG, 4).is_err());
    // Nothing is a value; this key does not take it.
    let r = o.set("r6", MAX_OPEN, "", 0, CONFIG, 4).unwrap();
    assert_eq!(r.outcome, "REJECTED_INVALID");
    assert!(o.set("../r7", MAX_OPEN, "3", 0, CONFIG, 4).is_err());
    // A configuration file the runtime refuses takes no setting, and its
    // lines never reach the refusal.
    let e = o.set("r9", MAX_OPEN, "3", 0, "NOT_A_LINE", 4).unwrap_err();
    assert!(!e.contains("NOT_A_LINE"), "{e}");
    // Review LOW 2: a value beyond the records' bound is refused before any
    // receipt; a refusal's reason is cut to the receipt's bound.
    let long = "0".repeat(max_value_len() + 1);
    assert!(o.set("r10", MAX_OPEN, &long, 0, CONFIG, 4).is_err());
    let r = o
        .set("r11", MAX_OPEN, &"0".repeat(max_value_len()), 0, CONFIG, 4)
        .unwrap();
    assert_eq!(r.outcome, "REJECTED_INVALID");
    assert!(
        r.reason.as_ref().unwrap().len() <= max_value_len(),
        "{}",
        r.reason.unwrap().len()
    );
    // A refusal is the request's receipt: the same request reads it again.
    let again = o.set("r1", MAX_OPEN, "0", 0, CONFIG, 5).unwrap();
    assert_eq!(again.outcome, "REJECTED_INVALID");
    let r = o.set("r8", MAX_OPEN, "3", 0, CONFIG, 6).unwrap();
    assert_eq!((r.outcome, r.generation), ("APPLIED", 1));
}

/// The journal record reads back as written, and nothing else reads as one.
#[test]
fn the_journal_record_is_exact() {
    let s = Settings {
        generation: 9,
        request_id: None,
        values: vec![
            (
                "BSC_STRATEGY_C_FILTERS",
                "buy_count:3:,unique_buyers:2:".into(),
            ),
            (MAX_OPEN, "3".into()),
        ],
    };
    let p = s.journal();
    assert_eq!(p[0], JOURNAL_TAG);
    assert_eq!(Settings::of_journal(&p), Some(s.clone()));
    let empty = Settings::default();
    assert_eq!(Settings::of_journal(&empty.journal()), Some(empty));
    let mut long = p.clone();
    long.push(0);
    assert_eq!(Settings::of_journal(&long), None);
    assert_eq!(Settings::of_journal(&p[..p.len() - 1]), None);
    let mut tag = p.clone();
    tag[0] = b'U';
    assert_eq!(Settings::of_journal(&tag), None);
    // Out of order, or a key twice.
    let swapped = Settings {
        values: s.values.iter().rev().cloned().collect(),
        ..s.clone()
    };
    assert_eq!(Settings::of_journal(&swapped.journal()), None);
    let mut unknown = p.clone();
    unknown[10] = KEYS.len() as u8;
    assert_eq!(Settings::of_journal(&unknown), None);
}

/// A store another user can change counts nothing (BSC3-003).
#[test]
fn an_exposed_store_is_refused() {
    let dir = sources("exposed");
    let o = Owner::open(&dir).unwrap();
    o.set("r1", MAX_OPEN, "3", 0, CONFIG, 1).unwrap();
    let store = dir.join(DIR);
    std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(current(&dir).is_err());
    assert!(Owner::open(&dir).is_err());
    std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(current(&dir).unwrap().value(MAX_OPEN), Some("3"));
}

const TARGET: &str = "BSC_STRATEGY_C_TARGET_NET_MULTIPLE";
const STOP: &str = "BSC_STRATEGY_C_STOP_NET_MULTIPLE";
const WINDOW: &str = "BSC_STRATEGY_C_FIRST_BUYER_WINDOW";

/// BSC-V8-04/05: "save all" is one generation with one receipt for every
/// key it sets, the same CAS and request rules as a single setting; the
/// whole set is validated together (all or none); the first-buyer window
/// is a settable key within the parser's bounds.
#[test]
fn save_all_is_one_validated_generation_with_one_receipt() {
    let dir = sources("set-all");
    let o = Owner::open(&dir).unwrap();
    let all = [(MAX_OPEN, "3"), (EXIT, "DISABLED"), (WINDOW, "7")];
    let r = o.set_all("a1", &all, 0, CONFIG, 1).unwrap();
    assert_eq!((r.outcome, r.generation), ("APPLIED", 1));
    assert_eq!(
        r.line(),
        "outcome=APPLIED keys=3 generation=1 request_id=a1"
    );
    let now = current(&dir).unwrap();
    assert_eq!((now.generation, now.values.len()), (1, 3));
    assert_eq!(now.value(WINDOW), Some("7"));
    // The same request again: its receipt (any order of the pairs); other
    // values under its ID: refused.
    let reordered = [(WINDOW, "7"), (MAX_OPEN, "3"), (EXIT, "DISABLED")];
    assert_eq!(o.set_all("a1", &reordered, 0, CONFIG, 2).unwrap(), r);
    assert!(o.set_all("a1", &[(MAX_OPEN, "4")], 0, CONFIG, 2).is_err());
    // Validated together: one value against another refuses all of them.
    let crossed = [(TARGET, "1.5"), (STOP, "2")];
    let bad = o.set_all("a2", &crossed, 1, CONFIG, 3).unwrap();
    assert_eq!((bad.outcome, bad.generation), ("REJECTED_INVALID", 1));
    assert!(bad.reason.is_some());
    assert_eq!(current(&dir).unwrap().generation, 1);
    // The window within the parser's bounds only.
    let out = o.set_all("a3", &[(WINDOW, "0")], 1, CONFIG, 4).unwrap();
    assert_eq!(out.outcome, "REJECTED_INVALID");
    // Stale, a key twice, no key, a key outside the scope.
    let stale = o.set_all("a4", &[(MAX_OPEN, "5")], 0, CONFIG, 5).unwrap();
    assert_eq!(stale.outcome, "REJECTED_STALE");
    assert!(
        o.set_all("a5", &[(MAX_OPEN, "4"), (MAX_OPEN, "5")], 1, CONFIG, 6)
            .is_err()
    );
    assert!(o.set_all("a6", &[], 1, CONFIG, 6).is_err());
    assert!(
        o.set_all("a7", &[("BSC_STRATEGY_C_GAS_PRICE_WEI", "1")], 1, CONFIG, 6)
            .is_err()
    );
    // A lost receipt of "save all" is recovered from its generation, also
    // after later ones; a subset or another expected generation under its
    // ID is another request (review).
    let receipt = dir.join(DIR).join("receipts/a1.dbrec");
    std::fs::remove_file(&receipt).unwrap();
    let again = o.set_all("a1", &all, 0, CONFIG, 7).unwrap();
    assert_eq!((again.outcome, again.generation), ("APPLIED", 1));
    let later = o.set_all("a8", &[(MAX_OPEN, "4")], 1, CONFIG, 8).unwrap();
    assert_eq!((later.outcome, later.generation), ("APPLIED", 2));
    std::fs::remove_file(&receipt).unwrap();
    let again = o.set_all("a1", &all, 0, CONFIG, 9).unwrap();
    assert_eq!((again.outcome, again.generation), ("APPLIED", 1));
    std::fs::remove_file(&receipt).unwrap();
    assert!(o.set_all("a1", &all[..2], 0, CONFIG, 10).is_err());
    // V8-01 review M1: another CAS under its ID is another request (its
    // intent is kept): refused, never APPLIED from generation 1.
    assert!(o.set_all("a1", &all, 1, CONFIG, 10).is_err());
}

/// BSC-V8-04: the strings the gateway's "save all" maps a form to are
/// what the owner and the runtime parser take: filter rules with signed
/// and open bounds, no rules at all, the enum tokens, a multiple.
#[test]
fn the_gateways_save_all_strings_are_the_natives() {
    let dir = sources("set-all-strings");
    let o = Owner::open(&dir).unwrap();
    let form = [
        ("BSC_STRATEGY_C_OBSERVATION_MS", "3000"),
        (
            "BSC_STRATEGY_C_FILTERS",
            "buy_count:2:,unique_buyers:2:40,price_change:-0.2:",
        ),
        ("BSC_STRATEGY_C_INPUT_WEI", "10000000000000000"),
        (TARGET, "1.50"),
        ("BSC_STRATEGY_C_MAX_OPEN", "2"),
        ("BSC_STRATEGY_C_CREATOR_SOLD_BEFORE_FILL", "TX_LANDS"),
        (EXIT, "DISABLED"),
        (WINDOW, "7"),
    ];
    let r = o.set_all("g1", &form, 0, CONFIG, 1).unwrap();
    assert_eq!(r.outcome, "APPLIED", "{:?}", r.reason);
    let none = o
        .set_all("g2", &[("BSC_STRATEGY_C_FILTERS", "")], 1, CONFIG, 2)
        .unwrap();
    assert_eq!(none.outcome, "APPLIED", "{:?}", none.reason);
    let c = effective(CONFIG, &current(&dir).unwrap()).unwrap();
    assert!(c.strategy.engine.rules.is_empty());
}

/// A single setting's v1 receipt of the frontend package (06.11.) still
/// reads, as the same receipt.
#[test]
fn a_v1_receipt_still_reads() {
    use darkbot_bsc_record::schema::bsc_runtime::strategy_setting_receipt as sr;
    let mut d = darkbot_bsc_record::Draft::new(&sr::V1);
    d.set(sr::REQUEST_ID, "r1")
        .set(sr::ID, MAX_OPEN)
        .set(sr::VALUE, "3")
        .set(sr::EXPECTED_GENERATION, 0u64)
        .set(sr::OUTCOME, "APPLIED")
        .set(sr::GENERATION, 1u64)
        .set(sr::OBSERVED_UNIX_MS, 1u64);
    let r = Receipt::decode(&d.encode().unwrap()).unwrap();
    assert_eq!(r.values, vec![(MAX_OPEN, "3".to_string())]);
    assert_eq!(
        r.line(),
        "outcome=APPLIED id=BSC_STRATEGY_C_MAX_OPEN value=3 generation=1 request_id=r1"
    );
}

/// BSC-V8: the canonical text of a settings basis is `KEY=value\n` in the
/// keys' order, whatever order the values come in; its SHA-256 follows it.
#[test]
fn the_canonical_text_is_in_key_order() {
    let a = [(WINDOW, "7".to_string()), (MAX_OPEN, "3".to_string())];
    let b = [(MAX_OPEN, "3".to_string()), (WINDOW, "7".to_string())];
    assert_eq!(
        canonical_text(&a),
        "BSC_STRATEGY_C_MAX_OPEN=3\nBSC_STRATEGY_C_FIRST_BUYER_WINDOW=7\n"
    );
    assert_eq!(config_sha256(&a), config_sha256(&b));
    assert_ne!(config_sha256(&a), config_sha256(&a[..1]));
}
