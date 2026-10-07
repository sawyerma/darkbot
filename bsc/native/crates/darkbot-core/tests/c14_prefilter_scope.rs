//! C1.4: the geth peer-ingress prefilter scope is derived only from the
//! committed venue scope: the supported launch-selector superset. Go
//! (`ingress.go`) and the Rust mirror (`darkbot-fastlane::ingress::C14_FLAP_SCOPE`)
//! carry the canonical bytes below as a golden value; this test recomputes
//! them from the venue constants this crate owns.

use darkbot_core::venues::flap::{abi, deployment};

/// `"DBC14PF1" | chain_id u64 LE | address count u16 LE | addresses
/// ascending | selector count u16 LE | (selector, role) ascending`, role 1 =
/// launch.
const C14_SCOPE_CANONICAL_HEX: &str =
    "444243313450463138000000000000000100e2ce6ab80874fa9fa2aae65d277dd6b8e65c9de001008cb5772c01";

#[test]
fn the_c14_prefilter_scope_is_the_committed_launch_scope() {
    let deployment = deployment::bsc_mainnet_v5_24_0();
    let mut canonical = Vec::new();
    canonical.extend_from_slice(b"DBC14PF1");
    canonical.extend_from_slice(&deployment.chain_id.to_le_bytes());
    canonical.extend_from_slice(&1u16.to_le_bytes());
    canonical.extend_from_slice(deployment.portal.as_slice());
    canonical.extend_from_slice(&1u16.to_le_bytes());
    canonical.extend_from_slice(&abi::NEW_TOKEN_V6_SELECTOR);
    canonical.push(1);
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut hex = String::with_capacity(canonical.len() * 2);
    for byte in &canonical {
        hex.push(char::from(DIGITS[usize::from(byte >> 4)]));
        hex.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    assert_eq!(hex, C14_SCOPE_CANONICAL_HEX);
}
