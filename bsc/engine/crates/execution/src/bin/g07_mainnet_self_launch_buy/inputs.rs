//! Inputs of the controlled attempt: environment, frozen JSON evidence, the
//! producer-witness source contract and the builder gas-floor policy.

use super::*;

pub(crate) fn req_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

pub(crate) fn env_u64(name: &str) -> u64 {
    req_env(name)
        .parse::<u64>()
        .unwrap_or_else(|_| panic!("{name} must be decimal u64"))
}

pub(crate) fn env_u128(name: &str) -> u128 {
    req_env(name)
        .parse::<u128>()
        .unwrap_or_else(|_| panic!("{name} must be decimal u128"))
}

pub(crate) fn norm(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

pub(crate) fn key_from_env(name: &str) -> SigningKey {
    let raw = req_env(name);
    let bytes = hex::decode(raw.trim().trim_start_matches("0x"))
        .unwrap_or_else(|_| panic!("{name} is not valid hex"));
    assert_eq!(bytes.len(), 32, "{name} must be exactly 32 bytes");
    SigningKey::from_slice(&bytes)
        .unwrap_or_else(|_| panic!("{name} is not a valid secp256k1 private key"))
}

pub(crate) fn signer_address(key: &SigningKey) -> Address {
    let point = key.verifying_key().to_encoded_point(false);
    let h = keccak256(&point.as_bytes()[1..]);
    Address::from_slice(&h.as_slice()[12..])
}

pub(crate) fn load_json(path: &str) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}")))
        .unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

pub(crate) fn req_str<'a>(v: &'a Value, ptr: &str) -> &'a str {
    v.pointer(ptr)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing/bad string at {ptr}"))
}

pub(crate) fn req_bool(v: &Value, ptr: &str) -> bool {
    v.pointer(ptr)
        .and_then(Value::as_bool)
        .unwrap_or_else(|| panic!("missing/bad bool at {ptr}"))
}

pub(crate) fn req_u64(v: &Value, ptr: &str) -> u64 {
    v.pointer(ptr)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("missing/bad u64 at {ptr}"))
}

pub(crate) fn req_u64_string(v: &Value, ptr: &str) -> u64 {
    req_str(v, ptr)
        .parse::<u64>()
        .unwrap_or_else(|_| panic!("bad decimal u64 at {ptr}"))
}

pub(crate) fn req_u128_string(v: &Value, ptr: &str) -> u128 {
    req_str(v, ptr)
        .parse::<u128>()
        .unwrap_or_else(|_| panic!("bad decimal u128 at {ptr}"))
}

pub(crate) fn validate_producer_witness_source(path: &str, attempt_id: &str) -> (Value, String) {
    let bytes = fs::read(path)
        .unwrap_or_else(|e| panic!("read producer witness source contract {path}: {e}"));
    let value: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("parse producer witness source contract {path}: {e}"));
    assert_eq!(
        req_str(&value, "/schema"),
        "darkbot.producer-witness-source.v1",
        "wrong producer witness source contract schema"
    );
    assert_eq!(
        req_str(&value, "/attempt_id"),
        attempt_id,
        "producer witness source contract belongs to another attempt"
    );
    let _source_ready = req_bool(&value, "/source_ready");
    assert!(req_bool(
        &value,
        "/original_successful_producer_execution_only"
    ));
    assert_eq!(
        req_str(&value, "/required_witness_schema"),
        PRODUCER_WITNESS_SCHEMA_PATH
    );
    assert!(req_bool(&value, "/delivery_may_be_asynchronous"));
    assert!(req_bool(&value, "/canonical_identity_binding_required"));
    assert!(req_bool(&value, "/clock_and_uncertainty_metadata_required"));
    assert!(
        !req_str(&value, "/source_description").trim().is_empty(),
        "producer witness source description is empty"
    );
    (value, sha256_hex(&bytes))
}

pub(crate) fn validate_builder_policy(path: &str, signed_gas_price: u64) -> (Value, String) {
    let bytes =
        fs::read(path).unwrap_or_else(|e| panic!("read builder policy evidence {path}: {e}"));
    let value: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("parse builder policy evidence {path}: {e}"));
    assert_eq!(
        req_str(&value, "/schema"),
        "darkbot.g06a.builder-floor-evidence-addendum.v2"
    );
    assert!(req_bool(&value, "/dry3_ready"));
    assert!(req_bool(&value, "/blockrazor/verified"));
    assert!(req_bool(&value, "/fortyeightclub/verified"));
    let strict_floor = req_u64_string(&value, "/strictest_explicit_verified_floor_wei");
    let stage1_policy = req_u64_string(&value, "/stage1_signed_gas_price_policy_wei");
    assert!(
        signed_gas_price >= strict_floor,
        "signed gas price is below the frozen builder floor evidence"
    );
    assert_eq!(
        signed_gas_price, stage1_policy,
        "signed gas price differs from the frozen Stage-1 builder policy"
    );
    (value, sha256_hex(&bytes))
}
