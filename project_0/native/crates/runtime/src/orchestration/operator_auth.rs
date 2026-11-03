//! DEC-17a (P8-CTRL-02): the operator's own authentication, independent of every trading wallet
//! key. `p0ctl operator register` creates the operator key once — on macOS a P-256 key in the
//! Secure Enclave whose every use needs the operator's presence (Touch ID, otherwise the login
//! password in the system dialog). The record keeps its public key and its Secure-Enclave-wrapped
//! form (only this Mac's Secure Enclave can use it; an ad-hoc build cannot keep the key in the
//! keychain itself). The public key's SHA-256 is configured (`operator_key_sha256`, part of the
//! configuration identity). `p0ctl … --confirm` signs a request's statement with the key; the
//! owner verifies the signature against the registered key for every FE-06 grant and every ENABLE
//! of a wallet, ECON_ARM, ECON_SEND or ECON_LIVE — never for a DISABLE, REVOKE or STOP
//! ([`required`]). A software test signer (feature `operator-test-signer`) exists for the offline
//! tests only; a production build refuses its keys.
use crate::analytics::journal;
use crate::analytics::payload::{CapabilityControlRequest, Payload};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The store of the registered operator key (under a project; git-ignored, P0-026).
pub const STORE: &str = "data/operator";
const KEY: &str = "key.p0j";
/// The signer of a key made in the Secure Enclave.
pub const SECURE_ENCLAVE: &str = "SECURE_ENCLAVE";
/// The signer of a software test key (feature `operator-test-signer` only).
pub const TEST_SIGNER: &str = "TEST_SIGNER";
/// Why a request that needs the operator's authentication is refused.
pub const OPERATOR_AUTH_REQUIRED: &str = "OPERATOR_AUTH_REQUIRED";
/// The domain of every signed statement.
const DOMAIN: &[u8] = b"darkbot.p0.operator.v1\0";

/// The registered operator key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorKey {
    /// [`SECURE_ENCLAVE`] or [`TEST_SIGNER`].
    pub signer: String,
    /// The P-256 public key, SEC1 uncompressed (65 bytes).
    pub public_key: Vec<u8>,
    /// The Secure-Enclave-wrapped key (a test signer's PKCS#8).
    pub wrapped: Vec<u8>,
    pub created_unix_s: u64,
}
impl Payload for OperatorKey {
    const KIND: u16 = journal::kind::OPERATOR_KEY;
}

fn key_path(project: &Path) -> PathBuf {
    project.join(STORE).join(KEY)
}

/// SHA-256 (hex) of a public key: what `operator_key_sha256` names.
pub fn key_sha256(public_key: &[u8]) -> String {
    journal::hex(&solana_sdk::hash::hash(public_key).to_bytes())
}

/// Whether a request needs the operator's signature: an FE-06 grant, an ENABLE of a wallet,
/// ECON_ARM, ECON_SEND or ECON_LIVE. A DISABLE, a revoke or a stop never does.
pub fn required(capability: &str, requested: &str) -> bool {
    use super::capability_control as cap;
    capability == cap::FE06_GRANT
        || (requested == cap::ENABLED
            && (capability == cap::ECON_ARM
                || capability == cap::ECON_SEND
                || capability == cap::ECON_LIVE
                || capability.starts_with(cap::WALLET_PREFIX)))
}

/// The statement a request's signature binds: its sequence (the CAS generation), one-time
/// request id, owner epoch with the epoch's random nonce (an epoch number reached again is
/// another epoch), capability with its value (an FE-06 grant's value is its scope digest) and
/// request time — exactly, under the operator domain.
pub fn statement(r: &CapabilityControlRequest, epoch_nonce: &str) -> Vec<u8> {
    let fields = (
        r.seq,
        r.request_id.as_str(),
        r.owner_epoch,
        epoch_nonce,
        r.capability.as_str(),
        r.requested.as_str(),
        r.requested_unix_s,
    );
    let mut out = DOMAIN.to_vec();
    out.extend(bincode::serialize(&fields).expect("statement fields serialize"));
    out
}

/// The registered operator key of `project`.
pub fn load(project: &Path) -> Result<OperatorKey, String> {
    journal::read_typed_value(&key_path(project)).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            format!(
                "{OPERATOR_AUTH_REQUIRED}: no operator key registered (p0ctl operator register)"
            )
        }
        _ => format!("OPERATOR_KEY_UNREADABLE: {e}"),
    })
}

/// `p0ctl operator register`: the operator key, made once (a second is refused; another key is
/// another `operator_key_sha256`). `test`: a software test key — only with feature
/// `operator-test-signer`.
pub fn register(project: &Path, test: bool) -> Result<OperatorKey, String> {
    let (signer, public_key, wrapped) = if test {
        let (public, pkcs8) = test_signer::generate()?;
        (TEST_SIGNER, public, pkcs8)
    } else {
        let (public, wrapped) = secure_enclave::generate()?;
        (SECURE_ENCLAVE, public, wrapped)
    };
    let record = OperatorKey {
        signer: signer.into(),
        public_key,
        wrapped,
        created_unix_s: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    };
    std::fs::create_dir_all(project.join(STORE)).map_err(|e| format!("OPERATOR_STORE: {e}"))?;
    journal::create_typed_value(
        &key_path(project),
        journal::source::CAPABILITY_CONTROL,
        &record,
    )
    .map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => {
            "OPERATOR_KEY_REGISTERED: an operator key is registered (data/operator/key.p0j)"
                .to_string()
        }
        _ => format!("OPERATOR_STORE: {e}"),
    })?;
    Ok(record)
}

/// The operator's signature of `message` with the registered key — the Secure Enclave asks for
/// the operator's presence.
pub fn sign(project: &Path, message: &[u8]) -> Result<Vec<u8>, String> {
    let key = load(project)?;
    match key.signer.as_str() {
        SECURE_ENCLAVE => secure_enclave::sign(&key, message),
        TEST_SIGNER => test_signer::sign(&key, message),
        other => Err(format!("OPERATOR_KEY_UNREADABLE: signer {other}")),
    }
}

/// The owner's check of a request's signature: the registered key is the configured one, a
/// production build's key no test key, the signature over exactly [`statement`] valid.
pub fn verify(
    project: &Path,
    configured_sha256: &str,
    epoch_nonce: &str,
    r: &CapabilityControlRequest,
    signature: &[u8],
) -> Result<(), String> {
    if epoch_nonce.is_empty() {
        return Err(format!(
            "{OPERATOR_AUTH_REQUIRED}: the owner epoch has no nonce"
        ));
    }
    verify_message(
        project,
        configured_sha256,
        &statement(r, epoch_nonce),
        signature,
    )
}

/// The check of the operator's signature of any statement (a request's, a tool use's — each
/// under its own domain): the registered key is the configured one, a production build's key a
/// Secure Enclave key, the signature over exactly `message` valid.
pub fn verify_message(
    project: &Path,
    configured_sha256: &str,
    message: &[u8],
    signature: &[u8],
) -> Result<(), String> {
    if configured_sha256.is_empty() {
        return Err(format!(
            "{OPERATOR_AUTH_REQUIRED}: no operator key configured (operator_key_sha256)"
        ));
    }
    let key = load(project)?;
    // A production build accepts the Secure Enclave's keys only (a record's label is no proof:
    // any other signer is refused, not only one labelled a test key).
    if key.signer != SECURE_ENCLAVE && !cfg!(feature = "operator-test-signer") {
        return Err(
            "OPERATOR_TEST_SIGNER_NOT_BUILT: only a Secure Enclave key authenticates the operator of this build"
                .into(),
        );
    }
    if key_sha256(&key.public_key) != configured_sha256 {
        return Err(format!(
            "{OPERATOR_AUTH_REQUIRED}: the registered operator key is not the configured one"
        ));
    }
    if signature.is_empty() {
        return Err(format!(
            "{OPERATOR_AUTH_REQUIRED}: unsigned (confirm locally: p0ctl … --confirm)"
        ));
    }
    ring::signature::UnparsedPublicKey::new(
        &ring::signature::ECDSA_P256_SHA256_ASN1,
        &key.public_key,
    )
    .verify(message, signature)
    .map_err(|_| {
        format!("{OPERATOR_AUTH_REQUIRED}: the signature is not the operator's over this statement")
    })
}

/// The software test signer: a P-256 key whose PKCS#8 is the record's — the offline tests only.
mod test_signer {
    #[cfg(feature = "operator-test-signer")]
    pub(super) fn generate() -> Result<(Vec<u8>, Vec<u8>), String> {
        use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING as ALG};
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ALG, &rng).map_err(|_| "OPERATOR_TEST_KEY")?;
        let pair = EcdsaKeyPair::from_pkcs8(&ALG, pkcs8.as_ref(), &rng)
            .map_err(|_| "OPERATOR_TEST_KEY")?;
        Ok((pair.public_key().as_ref().to_vec(), pkcs8.as_ref().to_vec()))
    }
    #[cfg(feature = "operator-test-signer")]
    pub(super) fn sign(key: &super::OperatorKey, message: &[u8]) -> Result<Vec<u8>, String> {
        use ring::signature::{EcdsaKeyPair, ECDSA_P256_SHA256_ASN1_SIGNING as ALG};
        let rng = ring::rand::SystemRandom::new();
        let pair = EcdsaKeyPair::from_pkcs8(&ALG, &key.wrapped, &rng)
            .map_err(|_| "OPERATOR_KEY_UNREADABLE")?;
        Ok(pair
            .sign(&rng, message)
            .map_err(|_| "OPERATOR_SIGN")?
            .as_ref()
            .to_vec())
    }
    #[cfg(not(feature = "operator-test-signer"))]
    pub(super) fn generate() -> Result<(Vec<u8>, Vec<u8>), String> {
        Err("OPERATOR_TEST_SIGNER_NOT_BUILT".into())
    }
    #[cfg(not(feature = "operator-test-signer"))]
    pub(super) fn sign(_: &super::OperatorKey, _: &[u8]) -> Result<Vec<u8>, String> {
        Err("OPERATOR_TEST_SIGNER_NOT_BUILT".into())
    }
}

/// The Secure Enclave: a P-256 key with the operator's presence as access rule, kept as its
/// wrapped form (`toid`) and restored from it for each signature (spike 03.11.2026).
mod secure_enclave {
    #[cfg(target_os = "macos")]
    pub(super) fn generate() -> Result<(Vec<u8>, Vec<u8>), String> {
        use core_foundation::base::{CFType, TCFType};
        use core_foundation::data::CFData;
        use core_foundation::string::CFString;
        use security_framework::access_control::SecAccessControl;
        use security_framework::key::{GenerateKeyOptions, KeyType, SecKey, Token};
        use security_framework_sys::access_control::{
            kSecAccessControlPrivateKeyUsage, kSecAccessControlUserPresence,
        };
        let access = SecAccessControl::create_with_flags(
            kSecAccessControlPrivateKeyUsage | kSecAccessControlUserPresence,
        )
        .map_err(|e| format!("OPERATOR_SECURE_ENCLAVE: {e}"))?;
        let mut o = GenerateKeyOptions::default();
        o.set_key_type(KeyType::ec_sec_prime_random())
            .set_size_in_bits(256)
            .set_token(Token::SecureEnclave)
            .set_access_control(access);
        let key = SecKey::new(&o).map_err(|e| format!("OPERATOR_SECURE_ENCLAVE: {e}"))?;
        let public = key
            .public_key()
            .and_then(|p| p.external_representation())
            .ok_or("OPERATOR_SECURE_ENCLAVE: no public key")?
            .bytes()
            .to_vec();
        let toid = CFString::from_static_string("toid");
        let wrapped = key
            .attributes()
            .find(toid.as_CFTypeRef() as *const _)
            .map(|v| unsafe { CFType::wrap_under_get_rule(*v as _) })
            .and_then(|t| t.downcast::<CFData>())
            .ok_or("OPERATOR_SECURE_ENCLAVE: no wrapped key")?
            .bytes()
            .to_vec();
        Ok((public, wrapped))
    }
    #[cfg(target_os = "macos")]
    pub(super) fn sign(key: &super::OperatorKey, message: &[u8]) -> Result<Vec<u8>, String> {
        use core_foundation::base::TCFType;
        use core_foundation::data::CFData;
        use core_foundation::dictionary::CFDictionary;
        use core_foundation::string::CFString;
        use security_framework::key::{Algorithm, SecKey};
        use security_framework_sys::item::{kSecAttrTokenID, kSecAttrTokenIDSecureEnclave};
        let wrapped = CFData::from_buffer(&key.wrapped);
        let restored = unsafe {
            let attributes = CFDictionary::from_CFType_pairs(&[
                (
                    CFString::wrap_under_get_rule(kSecAttrTokenID),
                    CFString::wrap_under_get_rule(kSecAttrTokenIDSecureEnclave).as_CFType(),
                ),
                (CFString::from_static_string("toid"), wrapped.as_CFType()),
            ]);
            let mut error = std::ptr::null_mut();
            let k = security_framework_sys::key::SecKeyCreateWithData(
                wrapped.as_concrete_TypeRef(),
                attributes.as_concrete_TypeRef(),
                &mut error,
            );
            if k.is_null() {
                None
            } else {
                Some(SecKey::wrap_under_create_rule(k))
            }
        }
        .ok_or("OPERATOR_KEY_UNAVAILABLE: the Secure Enclave does not restore the key")?;
        let public = restored
            .public_key()
            .and_then(|p| p.external_representation())
            .map(|d| d.bytes().to_vec());
        if public.as_deref() != Some(key.public_key.as_slice()) {
            return Err("OPERATOR_KEY_UNAVAILABLE: not this Mac's operator key".into());
        }
        restored
            .create_signature(Algorithm::ECDSASignatureMessageX962SHA256, message)
            .map_err(|e| format!("OPERATOR_SIGN: {e}"))
    }
    #[cfg(not(target_os = "macos"))]
    pub(super) fn generate() -> Result<(Vec<u8>, Vec<u8>), String> {
        Err("OPERATOR_SECURE_ENCLAVE_UNAVAILABLE: the operator key needs a Mac".into())
    }
    #[cfg(not(target_os = "macos"))]
    pub(super) fn sign(_: &super::OperatorKey, _: &[u8]) -> Result<Vec<u8>, String> {
        Err("OPERATOR_SECURE_ENCLAVE_UNAVAILABLE: the operator key needs a Mac".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::capability_control as cap;

    fn request(capability: &str, requested: &str) -> CapabilityControlRequest {
        CapabilityControlRequest {
            seq: 7,
            capability: capability.into(),
            requested: requested.into(),
            requested_unix_s: 1000,
            request_id: "r".repeat(32),
            owner_epoch: 3,
            operator_signature: vec![],
        }
    }

    /// Exactly the grants and the enabling switches need the operator; a disable never does.
    #[test]
    fn the_operator_is_required_for_grants_and_enabling_only() {
        assert!(required(cap::FE06_GRANT, "d"));
        for c in [cap::ECON_ARM, cap::ECON_SEND, cap::ECON_LIVE, "WALLET:x"] {
            assert!(required(c, cap::ENABLED), "{c}");
            assert!(!required(c, cap::DISABLED), "{c}");
        }
        assert!(!required(cap::FE06_REVOKE, "d"));
        assert!(!required(cap::ECON_SIGN, cap::ENABLED));
        assert!(!required(cap::ECON_ATTEMPT, cap::ENABLED));
    }

    /// The statement binds every field of the request and the epoch's nonce.
    #[test]
    fn the_statement_binds_every_request_field() {
        let base = statement(&request(cap::FE06_GRANT, "digest"), "n");
        assert_ne!(statement(&request(cap::FE06_GRANT, "digest"), "m"), base);
        let changed: [fn(&mut CapabilityControlRequest); 6] = [
            |r| r.seq += 1,
            |r| r.request_id.push('x'),
            |r| r.owner_epoch += 1,
            |r| r.capability = cap::FE06_REVOKE.into(),
            |r| r.requested.push('x'),
            |r| r.requested_unix_s += 1,
        ];
        for change in changed {
            let mut r = request(cap::FE06_GRANT, "digest");
            change(&mut r);
            assert_ne!(statement(&r, "n"), base);
        }
    }

    /// DEC-17a, L04 (authorization): the registered test key signs, the owner verifies exactly
    /// that request; an unsigned, a foreign, a changed request or another key is refused.
    #[cfg(feature = "operator-test-signer")]
    #[test]
    fn a_signed_request_verifies_and_nothing_else_does() {
        let project = std::env::temp_dir().join(format!(
            "p0-operator-{}-{}",
            std::process::id(),
            solana_sdk::pubkey::Pubkey::new_unique()
        ));
        std::fs::create_dir_all(&project).unwrap();
        let key = register(&project, true).unwrap();
        assert_eq!(key.public_key.len(), 65);
        assert!(register(&project, true)
            .unwrap_err()
            .starts_with("OPERATOR_KEY_REGISTERED"));
        let sha = key_sha256(&key.public_key);
        let r = request(cap::FE06_GRANT, "digest");
        let signature = sign(&project, &statement(&r, "nonce")).unwrap();
        verify(&project, &sha, "nonce", &r, &signature).unwrap();
        // The same epoch number reached again (its files removed) is another epoch's nonce.
        for nonce in ["another nonce", ""] {
            assert!(verify(&project, &sha, nonce, &r, &signature)
                .unwrap_err()
                .starts_with(OPERATOR_AUTH_REQUIRED));
        }
        let refused = |configured: &str, r: &CapabilityControlRequest, sig: &[u8]| {
            verify(&project, configured, "nonce", r, sig).unwrap_err()
        };
        assert!(refused("", &r, &signature).starts_with(OPERATOR_AUTH_REQUIRED));
        assert!(refused(&"0".repeat(64), &r, &signature).starts_with(OPERATOR_AUTH_REQUIRED));
        assert!(refused(&sha, &r, &[]).starts_with(OPERATOR_AUTH_REQUIRED));
        let mut other = r.clone();
        other.requested = "another digest".into();
        assert!(refused(&sha, &other, &signature).starts_with(OPERATOR_AUTH_REQUIRED));
        let mut later = r.clone();
        later.seq += 1;
        assert!(refused(&sha, &later, &signature).starts_with(OPERATOR_AUTH_REQUIRED));
        let _ = std::fs::remove_dir_all(&project);
    }

    /// DEC-17a: a production build (without the test signer) makes no test key and accepts none.
    #[cfg(not(feature = "operator-test-signer"))]
    #[test]
    fn a_production_build_refuses_test_keys() {
        let project = std::env::temp_dir().join(format!(
            "p0-operator-prod-{}-{}",
            std::process::id(),
            solana_sdk::pubkey::Pubkey::new_unique()
        ));
        std::fs::create_dir_all(project.join(STORE)).unwrap();
        assert_eq!(
            register(&project, true).unwrap_err(),
            "OPERATOR_TEST_SIGNER_NOT_BUILT"
        );
        // Any signer but the Secure Enclave is refused, whatever its label.
        for signer in [TEST_SIGNER, "OTHER"] {
            let key = OperatorKey {
                signer: signer.into(),
                public_key: vec![4; 65],
                wrapped: vec![],
                created_unix_s: 0,
            };
            let _ = std::fs::remove_file(key_path(&project));
            journal::create_typed_value(&key_path(&project), 0, &key).unwrap();
            let r = request(cap::FE06_GRANT, "digest");
            assert!(
                verify(&project, &key_sha256(&key.public_key), "n", &r, &[1])
                    .unwrap_err()
                    .starts_with("OPERATOR_TEST_SIGNER_NOT_BUILT")
            );
        }
        let _ = std::fs::remove_dir_all(&project);
    }
}
