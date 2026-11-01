//! DEC-18 (Nutzerentscheidung 01.11.2026): the provider registry store.
//!
//! Every committed version of the registry ([`darkbot_solana::providers::ProviderRegistry`]) is a
//! sealed single-value journal `data/providers/registry_<version>.p0j`, published create-new: the
//! next version number is the compare-and-swap, so of two concurrent decisions exactly one wins.
//! Every request has a receipt `data/providers/receipt_<request_id>.p0j`: the same request id
//! with the same action returns its receipt (idempotent), another action under it is a
//! conflict, and a receipt lost after its version was committed is rebuilt from that version
//! (the version names its request). `p0ctl provider …` decides — the configuration is staged:
//! a runtime takes the version current at its start (DEC-05), a running one is never
//! reconfigured. A credential's secret never enters a record: it goes to the credential store
//! (the macOS keychain) under its new revision **before** the version that names the revision is
//! committed, and a superseded or revoked revision's item is deleted **after** it.
use crate::analytics::journal;
use darkbot_solana::providers::{
    CredentialMeta, CredentialRef, CredentialState, CredentialStore, ProviderDescriptor,
    ProviderKind, ProviderRegistry,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const STORE: &str = "data/providers";
const VERSION_PREFIX: &str = "registry_";
const RECEIPT_PREFIX: &str = "receipt_";
/// The bound of a credential's secret (bytes).
pub const SECRET_LIMIT: usize = 4096;

/// One committed version and the request that committed it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRegistryRecord {
    pub registry: ProviderRegistry,
    pub request_id: String,
    pub decided_unix_s: u64,
}
impl crate::analytics::payload::Payload for ProviderRegistryRecord {
    const KIND: u16 = journal::kind::PROVIDER_REGISTRY;
}

/// The durable, redacted receipt of one request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderReceipt {
    pub request_id: String,
    /// The action's canonical text (no secret: a credential action names its credential only).
    pub action: String,
    /// SHA-256 of the action's canonical bytes: what the request id is bound to.
    pub action_sha256: String,
    pub applied: bool,
    /// The committed version (applied) or the version the request met (refused).
    pub version: u64,
    pub detail: String,
    pub decided_unix_s: u64,
}
impl crate::analytics::payload::Payload for ProviderReceipt {
    const KIND: u16 = journal::kind::PROVIDER_RECEIPT;
}

/// One registry change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderAction {
    Add(ProviderDescriptor),
    /// Replaces the provider of the descriptor's id.
    Update(ProviderDescriptor),
    Remove(String),
    Enable(String),
    Disable(String),
    Priority(String, u32),
    Select(ProviderKind, String),
    Unselect(ProviderKind),
    /// Names (or clears) the provider evidence record that qualified a provider.
    Qualify(String, Option<String>),
    /// A new credential, or a new revision of a revoked one (`origin`: `input`, `env:NAME`).
    CredentialSet {
        credential: String,
        origin: String,
    },
    /// A new revision of an active credential.
    CredentialRotate(String),
    CredentialRevoke(String),
}

impl ProviderAction {
    /// The action's canonical text (receipts, status): never a secret.
    pub fn text(&self) -> String {
        match self {
            Self::Add(p) => format!("add {} {}", p.id, p.kind.name()),
            Self::Update(p) => format!("update {} {}", p.id, p.kind.name()),
            Self::Remove(id) => format!("remove {id}"),
            Self::Enable(id) => format!("enable {id}"),
            Self::Disable(id) => format!("disable {id}"),
            Self::Priority(id, n) => format!("priority {id} {n}"),
            Self::Select(k, id) => format!("select {} {id}", k.name()),
            Self::Unselect(k) => format!("unselect {}", k.name()),
            Self::Qualify(id, e) => format!("qualify {id} {}", e.as_deref().unwrap_or("-")),
            Self::CredentialSet { credential, origin } => {
                format!("credential set {credential} {origin}")
            }
            Self::CredentialRotate(c) => format!("credential rotate {c}"),
            Self::CredentialRevoke(c) => format!("credential revoke {c}"),
        }
    }
    fn digest(&self) -> String {
        sha(&bincode::serialize(self).expect("bincode of plain data"))
    }
    /// Whether the action stores a new secret.
    fn stores_secret(&self) -> bool {
        matches!(self, Self::CredentialSet { .. } | Self::CredentialRotate(_))
    }
}

fn sha(bytes: &[u8]) -> String {
    journal::hex(&solana_sdk::hash::hash(bytes).to_bytes())
}

fn io(e: std::io::Error) -> String {
    format!("PROVIDER_STORE_IO: {e}")
}

/// A request id: 1–64 of `[A-Za-z0-9_-]` (a file name).
pub fn request_id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn dir(project: &Path) -> PathBuf {
    project.join(STORE)
}

fn version_path(project: &Path, version: u64) -> PathBuf {
    dir(project).join(format!("{VERSION_PREFIX}{version:08}.p0j"))
}

fn receipt_path(project: &Path, request_id: &str) -> PathBuf {
    dir(project).join(format!("{RECEIPT_PREFIX}{request_id}.p0j"))
}

/// The committed version numbers, ascending.
fn versions(project: &Path) -> Result<Vec<u64>, String> {
    let entries = match std::fs::read_dir(dir(project)) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(io(e)),
    };
    let mut out = vec![];
    for e in entries {
        let name = e.map_err(io)?.file_name();
        let Some(n) = name
            .to_str()
            .and_then(|n| n.strip_prefix(VERSION_PREFIX))
            .and_then(|n| n.strip_suffix(".p0j"))
        else {
            continue;
        };
        out.push(
            n.parse::<u64>()
                .map_err(|_| format!("PROVIDER_REGISTRY_NAME: {n}"))?,
        );
    }
    out.sort_unstable();
    Ok(out)
}

fn read_version(project: &Path, version: u64) -> Result<ProviderRegistryRecord, String> {
    let r: ProviderRegistryRecord = journal::read_typed_value(&version_path(project, version))
        .map_err(|e| format!("PROVIDER_REGISTRY_UNREADABLE: version {version}: {e}"))?;
    if r.registry.version != version {
        return Err(format!("PROVIDER_REGISTRY_VERSION_MISMATCH: {version}"));
    }
    Ok(r)
}

/// The current version (the newest committed one; version 0, empty, before the first). An
/// unreadable newest version fails closed — never an older one in its place.
pub fn current(project: &Path) -> Result<ProviderRegistry, String> {
    match versions(project)?.last() {
        None => Ok(ProviderRegistry::empty()),
        Some(v) => {
            let r = read_version(project, *v)?;
            r.registry.validate()?;
            Ok(r.registry)
        }
    }
}

/// The receipt of a request, if it has one.
pub fn receipt(project: &Path, request_id: &str) -> Result<Option<ProviderReceipt>, String> {
    let path = receipt_path(project, request_id);
    if !path.exists() {
        return Ok(None);
    }
    journal::read_typed_value(&path)
        .map(Some)
        .map_err(|e| format!("PROVIDER_RECEIPT_UNREADABLE: {e}"))
}

/// The next registry of `action` on `current` (version not yet advanced), every invariant
/// checked; `now` dates a new credential.
pub fn apply(
    current: &ProviderRegistry,
    action: &ProviderAction,
    now: u64,
) -> Result<ProviderRegistry, String> {
    use ProviderAction as A;
    let mut next = current.clone();
    let selected = |id: &str| current.selected.iter().any(|(_, s)| s == id);
    let provider = |next: &mut ProviderRegistry, id: &str| -> Result<usize, String> {
        next.providers
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| format!("PROVIDER_UNKNOWN: {id}"))
    };
    match action {
        A::Add(p) => {
            if current.provider(&p.id).is_some() {
                return Err(format!("PROVIDER_EXISTS: {}", p.id));
            }
            p.validate()?;
            next.providers.push(p.clone());
        }
        A::Update(p) => {
            let i = provider(&mut next, &p.id)?;
            p.validate()?;
            next.providers[i] = p.clone();
        }
        A::Remove(id) => {
            let i = provider(&mut next, id)?;
            if selected(id) {
                return Err(format!("PROVIDER_SELECTED: {id} (unselect it first)"));
            }
            next.providers.remove(i);
        }
        A::Enable(id) => {
            let i = provider(&mut next, id)?;
            next.providers[i].enabled = true;
        }
        A::Disable(id) => {
            let i = provider(&mut next, id)?;
            if selected(id) {
                return Err(format!("PROVIDER_SELECTED: {id} (unselect it first)"));
            }
            next.providers[i].enabled = false;
        }
        A::Priority(id, n) => {
            let i = provider(&mut next, id)?;
            next.providers[i].priority = *n;
        }
        A::Select(kind, id) => {
            let p = current
                .provider(id)
                .ok_or_else(|| format!("PROVIDER_UNKNOWN: {id}"))?;
            // A selection needs a provider that can authenticate now.
            current.credential_of(p)?;
            next.selected.retain(|(k, _)| k != kind);
            next.selected.push((*kind, id.clone()));
            next.selected.sort();
        }
        A::Unselect(kind) => {
            if !current.selected.iter().any(|(k, _)| k == kind) {
                return Err(format!("PROVIDER_NOT_SELECTED: {}", kind.name()));
            }
            next.selected.retain(|(k, _)| k != kind);
        }
        A::Qualify(id, evidence) => {
            let i = provider(&mut next, id)?;
            if let Some(e) = evidence {
                if !crate::orchestration::provider_evidence::is_id(e) {
                    return Err(format!("PROVIDER_EVIDENCE_ID_INVALID: {e}"));
                }
            }
            next.providers[i].qualification = evidence.clone();
        }
        A::CredentialSet { credential, origin } => {
            match next.credentials.iter_mut().find(|c| &c.id == credential) {
                Some(c) if c.state == CredentialState::Active => {
                    return Err(format!(
                        "CREDENTIAL_EXISTS: {credential} (rotate or revoke it)"
                    ))
                }
                // A revoked credential continues at its next revision, never a reused one.
                Some(c) => {
                    c.revision += 1;
                    c.state = CredentialState::Active;
                    c.created_unix_s = now;
                    c.origin = origin.clone();
                }
                None => next.credentials.push(CredentialMeta {
                    id: credential.clone(),
                    revision: 1,
                    state: CredentialState::Active,
                    created_unix_s: now,
                    origin: origin.clone(),
                }),
            }
            next.credentials.sort_by(|a, b| a.id.cmp(&b.id));
        }
        A::CredentialRotate(credential) => {
            match next.credentials.iter_mut().find(|c| &c.id == credential) {
                Some(c) if c.state == CredentialState::Active => {
                    c.revision += 1;
                    c.created_unix_s = now;
                }
                Some(_) => return Err(format!("CREDENTIAL_REVOKED: {credential}")),
                None => return Err(format!("CREDENTIAL_UNKNOWN: {credential}")),
            }
        }
        A::CredentialRevoke(credential) => {
            match next.credentials.iter_mut().find(|c| &c.id == credential) {
                Some(c) if c.state == CredentialState::Active => c.state = CredentialState::Revoked,
                Some(_) => return Err(format!("CREDENTIAL_REVOKED: {credential}")),
                None => return Err(format!("CREDENTIAL_UNKNOWN: {credential}")),
            }
        }
    }
    next.validate()?;
    Ok(next)
}

/// The credential revision an action's secret is stored under in `next`.
fn new_revision(next: &ProviderRegistry, action: &ProviderAction) -> Option<CredentialRef> {
    let id = match action {
        ProviderAction::CredentialSet { credential, .. } => credential,
        ProviderAction::CredentialRotate(c) => c,
        _ => return None,
    };
    next.credential(id).map(|c| CredentialRef {
        id: c.id.clone(),
        revision: c.revision,
    })
}

/// The revision whose item an applied action makes obsolete in the store: the superseded one of
/// a rotation, the revoked one of a revocation.
fn obsolete_revision(current: &ProviderRegistry, action: &ProviderAction) -> Option<CredentialRef> {
    let id = match action {
        ProviderAction::CredentialRotate(c) | ProviderAction::CredentialRevoke(c) => c,
        _ => return None,
    };
    current.credential(id).map(|c| CredentialRef {
        id: c.id.clone(),
        revision: c.revision,
    })
}

/// Decides one request: idempotent per request id, compare-and-swap on `expected` (the version
/// the operator saw; `None`: the current one), the secret of a credential set or rotation into
/// `store` before the commit, an obsolete revision's item deleted after it, a durable redacted
/// receipt either way. `Err` only when the store itself fails or a request id is reused for
/// another action; a refused request is an `Ok` receipt with `applied = false`.
pub fn decide(
    project: &Path,
    request_id: &str,
    expected: Option<u64>,
    action: &ProviderAction,
    secret: Option<&[u8]>,
    store: &dyn CredentialStore,
    now: u64,
) -> Result<ProviderReceipt, String> {
    if !request_id_ok(request_id) {
        return Err(format!("PROVIDER_REQUEST_ID_INVALID: {request_id}"));
    }
    let digest = action.digest();
    if let Some(r) = receipt(project, request_id)? {
        return if r.action_sha256 == digest {
            Ok(r)
        } else {
            Err(format!(
                "PROVIDER_REQUEST_ID_CONFLICT: {request_id} decided another action ({})",
                r.action
            ))
        };
    }
    std::fs::create_dir_all(dir(project)).map_err(io)?;
    let receipt_of = |applied: bool, version: u64, detail: String| ProviderReceipt {
        request_id: request_id.to_string(),
        action: action.text(),
        action_sha256: digest.clone(),
        applied,
        version,
        detail,
        decided_unix_s: now,
    };
    let write = |r: ProviderReceipt| -> Result<ProviderReceipt, String> {
        match journal::create_typed_value(
            &receipt_path(project, request_id),
            journal::source::PROVIDER_CONTROL,
            &r,
        ) {
            Ok(()) => Ok(r),
            // A concurrent decision of the same request id wrote it first: that one stands.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                receipt(project, request_id)?.ok_or_else(|| "PROVIDER_RECEIPT_LOST".to_string())
            }
            Err(e) => Err(io(e)),
        }
    };
    // A receipt lost after its version was committed: rebuilt from that version.
    for v in versions(project)?.into_iter().rev() {
        if read_version(project, v)?.request_id == request_id {
            return write(receipt_of(true, v, "committed (receipt rebuilt)".into()));
        }
    }
    if action.stores_secret() != secret.is_some() {
        return Err(if secret.is_some() {
            "CREDENTIAL_SECRET_UNEXPECTED".into()
        } else {
            "CREDENTIAL_SECRET_REQUIRED".into()
        });
    }
    if let Some(s) = secret {
        if s.is_empty() || s.len() > SECRET_LIMIT || s.iter().any(|b| b.is_ascii_control()) {
            return Err("CREDENTIAL_SECRET_FORMAT".into());
        }
    }
    let current = current(project)?;
    if let Some(e) = expected.filter(|e| *e != current.version) {
        return write(receipt_of(
            false,
            current.version,
            format!(
                "PROVIDER_VERSION_CONFLICT: expected {e}, current {}",
                current.version
            ),
        ));
    }
    let mut next = match apply(&current, action, now) {
        Ok(n) => n,
        Err(e) => return write(receipt_of(false, current.version, e)),
    };
    next.version = current.version + 1;
    let stored = new_revision(&next, action);
    if let (Some(r), Some(s)) = (&stored, secret) {
        store.put(r, s)?;
    }
    let record = ProviderRegistryRecord {
        registry: next.clone(),
        request_id: request_id.to_string(),
        decided_unix_s: now,
    };
    if !commit(project, &record, stored.as_ref(), store)? {
        return write(receipt_of(
            false,
            current.version,
            "PROVIDER_VERSION_CONFLICT: another decision committed first".into(),
        ));
    }
    let mut detail = "committed (effective at the next controlled start)".to_string();
    if let Some(old) = obsolete_revision(&current, action) {
        if let Err(e) = store.delete(&old) {
            detail = format!(
                "{detail}; obsolete revision {}@{} not deleted: {e}",
                old.id, old.revision
            );
        }
    }
    write(receipt_of(true, next.version, detail))
}

/// Publishes `record` as its version (create-new: the compare-and-swap). `false`: another
/// decision committed that version first — then the new revision's item `stored` is deleted, so
/// a lost race never leaves a secret behind.
fn commit(
    project: &Path,
    record: &ProviderRegistryRecord,
    stored: Option<&CredentialRef>,
    store: &dyn CredentialStore,
) -> Result<bool, String> {
    match journal::create_typed_value(
        &version_path(project, record.registry.version),
        journal::source::PROVIDER_CONTROL,
        record,
    ) {
        Ok(()) => Ok(true),
        Err(e) => {
            if let Some(r) = stored {
                let _ = store.delete(r);
            }
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                Ok(false)
            } else {
                Err(io(e))
            }
        }
    }
}

/// The store of an action that touches no credential: never asked (a call is a defect, refused).
pub struct NoCredentialStore;

impl CredentialStore for NoCredentialStore {
    fn put(&self, _: &CredentialRef, _: &[u8]) -> Result<(), String> {
        Err("CREDENTIAL_STORE_NOT_OPENED".into())
    }
    fn get(
        &self,
        _: &CredentialRef,
    ) -> Result<darkbot_solana::providers::Zeroizing<Vec<u8>>, String> {
        Err("CREDENTIAL_STORE_NOT_OPENED".into())
    }
    fn delete(&self, _: &CredentialRef) -> Result<(), String> {
        Err("CREDENTIAL_STORE_NOT_OPENED".into())
    }
}

/// Whether an action needs the credential store (it stores or deletes a revision's item).
pub fn touches_credentials(action: &ProviderAction) -> bool {
    matches!(
        action,
        ProviderAction::CredentialSet { .. }
            | ProviderAction::CredentialRotate(_)
            | ProviderAction::CredentialRevoke(_)
    )
}

/// The operator's view of the current version: one line per provider (host only, never a
/// path or a credential) with its binding, and one per credential (id, revision, state, origin).
pub fn status_lines(registry: &ProviderRegistry) -> Vec<String> {
    let mut out = vec![format!("PROVIDER_REGISTRY version={}", registry.version)];
    let mut providers: Vec<&ProviderDescriptor> = registry.providers.iter().collect();
    providers.sort_by(|a, b| (a.kind, a.priority, &a.id).cmp(&(b.kind, b.priority, &b.id)));
    for p in providers {
        let selected = registry.selected(p.kind).map(|s| s.id == p.id) == Some(true);
        let credential = registry.credential_of(p);
        let auth = match &credential {
            Ok(Some(c)) => format!("{}@{}", c.id, c.revision),
            Ok(None) => "-".into(),
            Err(e) => e.clone(),
        };
        out.push(format!(
            "provider {} kind={} host={} auth={} credential={} plan={} entitlement={} capacity_rps={} priority={} enabled={} selected={} qualification={} binding={}",
            p.id,
            p.kind.name(),
            p.host(),
            p.auth.text(),
            auth,
            p.plan.name(),
            p.entitlement,
            p.capacity_rps.map_or("-".into(), |c| c.to_string()),
            p.priority,
            p.enabled,
            selected,
            p.qualification.as_deref().unwrap_or("-"),
            credential
                .ok()
                .map(|c| p.binding(c.as_ref())[..16].to_string())
                .unwrap_or_else(|| "-".into()),
        ));
    }
    for c in &registry.credentials {
        out.push(format!(
            "credential {} revision={} state={} origin={} created_unix_s={}",
            c.id,
            c.revision,
            match c.state {
                CredentialState::Active => "active",
                CredentialState::Revoked => "revoked",
            },
            c.origin,
            c.created_unix_s
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkbot_solana::providers::{AuthKind, DirectoryStore, Plan};

    fn project(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("p0_providers_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn helius() -> ProviderDescriptor {
        ProviderDescriptor {
            id: "helius-rpc".into(),
            kind: ProviderKind::Rpc,
            endpoint: "https://mainnet.helius-rpc.com".into(),
            auth: AuthKind::Query("api-key".into()),
            credential: Some("helius".into()),
            priority: 0,
            capacity_rps: Some(10),
            plan: Plan::Free,
            entitlement: "helius-free".into(),
            qualification: None,
            enabled: true,
        }
    }

    /// DEC-18: a provider and its credential through the store — the secret is stored under its
    /// revision before the version naming it is committed, never in a record or a receipt; a
    /// selection needs an active credential; a rotation stores the next revision and deletes the
    /// superseded item; a revocation is always possible and deletes the item; a revoked
    /// credential continues at its next revision, never a reused one.
    #[test]
    fn a_provider_and_its_credential_through_the_store() {
        let d = project("flow");
        let store = DirectoryStore(d.join("keychain"));
        let ok = |r: Result<ProviderReceipt, String>| {
            let r = r.unwrap();
            assert!(r.applied, "{r:?}");
            r
        };
        ok(decide(
            &d,
            "r1",
            Some(0),
            &ProviderAction::Add(helius()),
            None,
            &store,
            10,
        ));
        let select = ProviderAction::Select(ProviderKind::Rpc, "helius-rpc".into());
        let refused = decide(&d, "r2", None, &select, None, &store, 11).unwrap();
        assert!(!refused.applied && refused.detail.starts_with("CREDENTIAL_UNKNOWN"));
        let set = ProviderAction::CredentialSet {
            credential: "helius".into(),
            origin: "input".into(),
        };
        assert_eq!(
            decide(&d, "r3", None, &set, None, &store, 12).unwrap_err(),
            "CREDENTIAL_SECRET_REQUIRED"
        );
        ok(decide(&d, "r3", None, &set, Some(b"KEY-ONE"), &store, 12));
        let r1 = CredentialRef {
            id: "helius".into(),
            revision: 1,
        };
        assert_eq!(&store.get(&r1).unwrap()[..], b"KEY-ONE");
        ok(decide(&d, "r4", None, &select, None, &store, 13));
        let reg = current(&d).unwrap();
        assert_eq!(reg.version, 3);
        assert_eq!(
            reg.selected(ProviderKind::Rpc).map(|p| p.id.as_str()),
            Some("helius-rpc")
        );
        // No secret in any committed byte.
        for e in std::fs::read_dir(d.join(STORE)).unwrap() {
            let bytes = std::fs::read(e.unwrap().path()).unwrap();
            assert!(!bytes.windows(7).any(|w| w == b"KEY-ONE"));
        }
        // Rotation: the next revision, the superseded item deleted.
        let rotate = ProviderAction::CredentialRotate("helius".into());
        ok(decide(
            &d,
            "r5",
            None,
            &rotate,
            Some(b"KEY-TWO"),
            &store,
            14,
        ));
        let r2 = CredentialRef {
            revision: 2,
            ..r1.clone()
        };
        assert_eq!(&store.get(&r2).unwrap()[..], b"KEY-TWO");
        assert!(store.get(&r1).is_err(), "superseded revision deleted");
        // Revocation while selected: possible, the item deleted, the provider can no longer
        // authenticate; a new set continues at revision 3.
        ok(decide(
            &d,
            "r6",
            None,
            &ProviderAction::CredentialRevoke("helius".into()),
            None,
            &store,
            15,
        ));
        assert!(store.get(&r2).is_err());
        let reg = current(&d).unwrap();
        assert!(reg
            .credential_of(reg.selected(ProviderKind::Rpc).unwrap())
            .unwrap_err()
            .starts_with("CREDENTIAL_REVOKED"));
        ok(decide(&d, "r7", None, &set, Some(b"KEY-THREE"), &store, 16));
        assert_eq!(
            current(&d).unwrap().credential("helius").unwrap().revision,
            3
        );
        let _ = std::fs::remove_dir_all(d);
    }

    /// DEC-18 (P8-CTRL.6 pattern): one request id is one decision — the same action returns its
    /// receipt, another action under it is a conflict; a stale expected version is refused
    /// with a receipt; a receipt lost after the commit is rebuilt, never a second commit.
    #[test]
    fn a_request_id_is_one_decision_and_the_version_is_the_cas() {
        let d = project("cas");
        let store = DirectoryStore(d.join("keychain"));
        let add = ProviderAction::Add(helius());
        let first = decide(&d, "a", Some(0), &add, None, &store, 1).unwrap();
        assert!(first.applied);
        assert_eq!(
            decide(&d, "a", Some(0), &add, None, &store, 2).unwrap(),
            first
        );
        assert!(decide(
            &d,
            "a",
            None,
            &ProviderAction::Remove("helius-rpc".into()),
            None,
            &store,
            3
        )
        .unwrap_err()
        .starts_with("PROVIDER_REQUEST_ID_CONFLICT"));
        let stale = decide(
            &d,
            "b",
            Some(0),
            &ProviderAction::Disable("helius-rpc".into()),
            None,
            &store,
            4,
        )
        .unwrap();
        assert!(!stale.applied && stale.detail.starts_with("PROVIDER_VERSION_CONFLICT"));
        assert_eq!(current(&d).unwrap().version, 1);
        // A lost receipt: rebuilt from the committed version.
        std::fs::remove_file(receipt_path(&d, "a")).unwrap();
        let rebuilt = decide(&d, "a", Some(0), &add, None, &store, 5).unwrap();
        assert!(rebuilt.applied && rebuilt.version == 1, "{rebuilt:?}");
        assert_eq!(current(&d).unwrap().version, 1, "never a second commit");
        let _ = std::fs::remove_dir_all(d);
    }

    /// The version is the compare-and-swap: a concurrent decision that committed the same
    /// version first wins; the loser commits nothing and its new secret is never left behind.
    #[test]
    fn a_lost_race_commits_nothing_and_leaves_no_secret() {
        let d = project("race");
        let store = DirectoryStore(d.join("keychain"));
        decide(
            &d,
            "a",
            None,
            &ProviderAction::Add(helius()),
            None,
            &store,
            1,
        )
        .unwrap();
        let mut v2 = current(&d).unwrap();
        v2.version = 2;
        let record = |request_id: &str| ProviderRegistryRecord {
            registry: v2.clone(),
            request_id: request_id.into(),
            decided_unix_s: 2,
        };
        assert!(commit(&d, &record("winner"), None, &store).unwrap());
        let secret = CredentialRef {
            id: "helius".into(),
            revision: 1,
        };
        store.put(&secret, b"SECRET").unwrap();
        assert!(!commit(&d, &record("loser"), Some(&secret), &store).unwrap());
        assert!(store.get(&secret).is_err(), "the loser's secret is deleted");
        assert_eq!(
            read_version(&d, 2).unwrap().request_id,
            "winner",
            "the winner's version stands"
        );
        let _ = std::fs::remove_dir_all(d);
    }
}
