//! DEC-18 (Nutzerentscheidung 01.11.2026): the provider registry store.
//!
//! Every committed version of the registry ([`darkbot_solana::providers::ProviderRegistry`]) is a
//! sealed single-value journal `data/providers/registry_<version>.p0j`, published create-new: the
//! next version number is the compare-and-swap. Every request has a receipt
//! `data/providers/receipt_<request_id>.p0j`: the same request id with the same action returns
//! its receipt (idempotent), another action under it is a conflict, and a receipt lost after its
//! version was committed is rebuilt from that version (the version names its request and its
//! action's digest). One decision at a time: the whole decision — receipt, rebuild, read, the
//! secret's store, commit, sweep, receipt — runs under an exclusive lock of the store, so a
//! retried or concurrent request never meets a half-made one.
//!
//! `p0ctl provider …` decides — and later the GUI through the same `decide`: every rule lives in
//! [`apply`] and [`ProviderAction::check`], none in a caller. The configuration is staged: a
//! runtime takes the version current at its start (DEC-05), a running one is never
//! reconfigured. A credential's secret never enters a record: it goes to the credential store
//! (the macOS keychain) under its new revision's account **before** the version naming it is
//! committed; every item of this registry that no active revision names — a superseded or
//! revoked revision, an orphan of an interrupted decision — is deleted by the sweep after it.
use crate::analytics::journal;
use darkbot_solana::providers::{
    id_ok, nonce_ok, CredentialMeta, CredentialState, CredentialStore, ProviderDescriptor,
    ProviderKind, ProviderRegistry,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const STORE: &str = "data/providers";
const VERSION_PREFIX: &str = "registry_";
const RECEIPT_PREFIX: &str = "receipt_";
const LOCK: &str = ".decide.lock";
/// The bound of a credential's secret (bytes).
pub const SECRET_LIMIT: usize = 4096;
/// DEC-18: the providers, and the credentials, a registry holds at most (its projection fits one
/// STATE frame).
pub const ENTRIES_MAX: usize =
    darkbot_solana::runtime_config::defaults::PROVIDER_REGISTRY_ENTRIES_MAX as usize;

/// One committed version, the request that committed it and its action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRegistryRecord {
    pub registry: ProviderRegistry,
    pub request_id: String,
    pub action: String,
    pub action_sha256: String,
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
    /// Replaces the provider of the descriptor's id (its kind and qualification are kept by
    /// [`apply`]: a qualification belongs to a binding, never to an edit).
    Update(ProviderDescriptor),
    Remove(String),
    Enable(String),
    Disable(String),
    Priority(String, u32),
    Select(ProviderKind, String),
    Unselect(ProviderKind),
    /// Names (or clears) the provider evidence record that qualified a provider's binding.
    Qualify(String, Option<String>),
    /// A new credential, or a new revision of a revoked one (`origin`: `input`, `env:NAME` — the
    /// one-time import).
    CredentialSet {
        credential: String,
        origin: String,
    },
    /// A new revision of an active credential.
    CredentialRotate(String),
    CredentialRevoke(String),
    /// Takes over a registry recorded for another project directory (a moved project, or a
    /// copied store): a new uid, this directory, every credential revoked — its secrets live
    /// under the old uid, so each is set again; nothing of the other project's items is touched.
    Adopt,
}

impl ProviderAction {
    /// The action's canonical text (receipts, status): only checked identifiers, never a secret.
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
            Self::Adopt => "adopt".into(),
        }
    }

    fn digest(&self) -> String {
        sha(&bincode::serialize(self).expect("bincode of plain data"))
    }

    /// Whether the action stores a new secret.
    fn stores_secret(&self) -> bool {
        matches!(self, Self::CredentialSet { .. } | Self::CredentialRotate(_))
    }

    /// Every identifier and value of the action, checked before anything is persisted: a
    /// refused value is never written (not even into a receipt) and never echoed — an operator's
    /// slip may be a key.
    pub fn check(&self) -> Result<(), String> {
        let id = |s: &str, what: &str| {
            if id_ok(s) {
                Ok(())
            } else {
                Err(format!("{what}_ID_INVALID ([a-z0-9_-]{{1,32}})"))
            }
        };
        match self {
            Self::Add(p) | Self::Update(p) => p.validate(),
            Self::Remove(i) | Self::Enable(i) | Self::Disable(i) | Self::Priority(i, _) => {
                id(i, "PROVIDER")
            }
            Self::Select(_, i) => id(i, "PROVIDER"),
            Self::Unselect(_) => Ok(()),
            Self::Qualify(i, e) => {
                id(i, "PROVIDER")?;
                match e {
                    Some(e) if !nonce_ok(e) => Err("PROVIDER_EVIDENCE_ID_INVALID".into()),
                    _ => Ok(()),
                }
            }
            Self::CredentialSet { credential, origin } => {
                id(credential, "CREDENTIAL")?;
                origin_ok(origin)
            }
            Self::CredentialRotate(c) | Self::CredentialRevoke(c) => id(c, "CREDENTIAL"),
            Self::Adopt => Ok(()),
        }
    }
}

/// A credential's origin: `input`, or `env:NAME` with an environment key's syntax.
fn origin_ok(origin: &str) -> Result<(), String> {
    let ok = origin == "input"
        || origin.strip_prefix("env:").is_some_and(|k| {
            !k.is_empty()
                && k.len() <= 64
                && k.bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
                && !k.as_bytes()[0].is_ascii_digit()
        });
    if ok {
        Ok(())
    } else {
        Err("CREDENTIAL_ORIGIN_INVALID (input|env:NAME)".into())
    }
}

fn sha(bytes: &[u8]) -> String {
    journal::hex(&solana_sdk::hash::hash(bytes).to_bytes())
}

fn io(e: std::io::Error) -> String {
    format!("PROVIDER_STORE_IO: {e}")
}

/// A fresh random 16-hex nonce (a revision's, or a registry's uid).
pub fn fresh_nonce() -> Result<String, String> {
    journal::random16()
        .map(|b| journal::hex(&b[..8]))
        .map_err(io)
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
                .map_err(|_| "PROVIDER_REGISTRY_NAME".to_string())?,
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
            // A registry a stricter rule refuses fails closed, with its recovery named (the
            // keychain items stay; nothing is repaired in place).
            r.registry.validate().map_err(|e| {
                format!("PROVIDER_REGISTRY_INVALID: {e}; recovery: archive {STORE}/ and add the providers again")
            })?;
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

/// The next registry of `action` on `current` (version and uid not yet advanced), every rule and
/// invariant checked. `nonce` is the decision's fresh nonce for a new credential revision, `now`
/// dates it.
pub fn apply(
    current: &ProviderRegistry,
    action: &ProviderAction,
    nonce: &str,
    now: u64,
) -> Result<ProviderRegistry, String> {
    use ProviderAction as A;
    action.check()?;
    let mut next = current.clone();
    let selected = |id: &str| current.selected.iter().any(|(_, s)| s == id);
    let position = |next: &ProviderRegistry, id: &str| -> Result<usize, String> {
        next.providers
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| format!("PROVIDER_UNKNOWN: {id}"))
    };
    // A changed credential revision changes the binding of every provider using it: their
    // qualifications were measured under another identity.
    let unqualify_users = |next: &mut ProviderRegistry, credential: &str| {
        for p in next
            .providers
            .iter_mut()
            .filter(|p| p.credential.as_deref() == Some(credential))
        {
            p.qualification = None;
        }
    };
    match action {
        A::Add(p) => {
            if current.provider(&p.id).is_some() {
                return Err(format!("PROVIDER_EXISTS: {}", p.id));
            }
            if p.qualification.is_some() {
                return Err(
                    "PROVIDER_QUALIFICATION_BY_EDIT: a qualification comes from provider evidence"
                        .into(),
                );
            }
            if current.providers.len() >= ENTRIES_MAX {
                return Err(format!(
                    "PROVIDER_REGISTRY_FULL: {ENTRIES_MAX} providers (remove one first)"
                ));
            }
            next.providers.push(p.clone());
        }
        A::Update(p) => {
            let i = position(&next, &p.id)?;
            let old = &current.providers[i];
            if old.kind != p.kind {
                return Err(
                    "PROVIDER_KIND_IMMUTABLE: remove and add a provider of another kind".into(),
                );
            }
            let mut p = p.clone();
            // The qualification is the binding's: kept for the same identity, cleared otherwise,
            // never set by an edit.
            p.qualification = if old.same_identity(&p) {
                old.qualification.clone()
            } else {
                None
            };
            if selected(&p.id) {
                let mut probe = current.clone();
                probe.providers[i] = p.clone();
                if let Some(block) = probe.selection_block(&p) {
                    return Err(format!("PROVIDER_SELECTED: {block} (unselect it first)"));
                }
            }
            next.providers[i] = p;
        }
        A::Remove(id) => {
            let i = position(&next, id)?;
            if selected(id) {
                return Err(format!("PROVIDER_SELECTED: {id} (unselect it first)"));
            }
            next.providers.remove(i);
        }
        A::Enable(id) => {
            let i = position(&next, id)?;
            next.providers[i].enabled = true;
        }
        A::Disable(id) => {
            let i = position(&next, id)?;
            if selected(id) {
                return Err(format!("PROVIDER_SELECTED: {id} (unselect it first)"));
            }
            next.providers[i].enabled = false;
        }
        A::Priority(id, n) => {
            let i = position(&next, id)?;
            next.providers[i].priority = *n;
        }
        A::Select(kind, id) => {
            let p = current
                .provider(id)
                .ok_or_else(|| format!("PROVIDER_UNKNOWN: {id}"))?;
            if p.kind != *kind {
                return Err(format!(
                    "PROVIDER_SELECTION_KIND: {id} is {}",
                    p.kind.name()
                ));
            }
            // A selection needs a free, enabled provider that can authenticate now.
            if let Some(block) = current.selection_block(p) {
                return Err(block);
            }
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
            let i = position(&next, id)?;
            next.providers[i].qualification = evidence.clone();
        }
        A::CredentialSet { credential, origin } => {
            if !nonce_ok(nonce) {
                return Err("CREDENTIAL_NONCE_INVALID".into());
            }
            let import = origin.starts_with("env:");
            // The `.env` import is the one-time migration: never over an existing credential,
            // never the same variable twice.
            if import && current.credential(credential).is_some() {
                return Err(format!(
                    "CREDENTIAL_EXISTS: {credential} (the import is the one-time migration; use set or rotate)"
                ));
            }
            if import && current.credentials.iter().any(|c| &c.origin == origin) {
                return Err(format!("CREDENTIAL_ENV_IMPORTED: {origin}"));
            }
            match next.credentials.iter_mut().find(|c| &c.id == credential) {
                Some(c) if c.state == CredentialState::Active => {
                    return Err(format!(
                        "CREDENTIAL_EXISTS: {credential} (rotate or revoke it)"
                    ))
                }
                // A revoked credential continues at its next revision, never a reused one.
                Some(c) => {
                    c.revision += 1;
                    c.nonce = nonce.to_string();
                    c.state = CredentialState::Active;
                    c.created_unix_s = now;
                    c.origin = origin.clone();
                }
                None if current.credentials.len() >= ENTRIES_MAX => {
                    return Err(format!(
                        "CREDENTIAL_REGISTRY_FULL: {ENTRIES_MAX} credentials (a revoked one takes its next revision)"
                    ))
                }
                None => next.credentials.push(CredentialMeta {
                    id: credential.clone(),
                    revision: 1,
                    nonce: nonce.to_string(),
                    state: CredentialState::Active,
                    created_unix_s: now,
                    origin: origin.clone(),
                }),
            }
            next.credentials.sort_by(|a, b| a.id.cmp(&b.id));
            unqualify_users(&mut next, credential);
        }
        A::CredentialRotate(credential) => {
            if !nonce_ok(nonce) {
                return Err("CREDENTIAL_NONCE_INVALID".into());
            }
            match next.credentials.iter_mut().find(|c| &c.id == credential) {
                Some(c) if c.state == CredentialState::Active => {
                    c.revision += 1;
                    c.nonce = nonce.to_string();
                    c.created_unix_s = now;
                }
                Some(_) => return Err(format!("CREDENTIAL_REVOKED: {credential}")),
                None => return Err(format!("CREDENTIAL_UNKNOWN: {credential}")),
            }
            unqualify_users(&mut next, credential);
        }
        A::CredentialRevoke(credential) => {
            match next.credentials.iter_mut().find(|c| &c.id == credential) {
                Some(c) if c.state == CredentialState::Active => c.state = CredentialState::Revoked,
                Some(_) => return Err(format!("CREDENTIAL_REVOKED: {credential}")),
                None => return Err(format!("CREDENTIAL_UNKNOWN: {credential}")),
            }
            unqualify_users(&mut next, credential);
        }
        A::Adopt => {
            // The new uid and project are the decision's (`decide`); the secrets stay with the
            // other project's uid.
            for c in &mut next.credentials {
                c.state = CredentialState::Revoked;
            }
            for p in &mut next.providers {
                p.qualification = None;
            }
        }
    }
    next.validate()?;
    Ok(next)
}

/// The account a stored-secret action writes in `next`.
fn new_account(next: &ProviderRegistry, action: &ProviderAction) -> Option<String> {
    let id = match action {
        ProviderAction::CredentialSet { credential, .. } => credential,
        ProviderAction::CredentialRotate(c) => c,
        _ => return None,
    };
    next.credential(id).map(|c| next.account(&c.reference()))
}

/// Deletes every item of `registry` that no active revision names (superseded, revoked, the
/// orphan of an interrupted decision); items of other registries are never touched. Returns the
/// accounts it could not delete.
fn sweep(registry: &ProviderRegistry, store: &dyn CredentialStore) -> Result<Vec<String>, String> {
    if registry.uid.is_empty() {
        return Ok(vec![]);
    }
    let keep: std::collections::BTreeSet<String> = registry
        .credentials
        .iter()
        .filter(|c| c.state == CredentialState::Active)
        .map(|c| registry.account(&c.reference()))
        .collect();
    let mut failed = vec![];
    for account in store.list(&registry.account_prefix())? {
        if !keep.contains(&account) && store.delete(&account).is_err() {
            failed.push(account);
        }
    }
    Ok(failed)
}

/// Holds the store's decision lock (exclusive, blocking) until dropped.
struct DecisionLock(#[allow(dead_code)] std::fs::File);

fn lock(project: &Path) -> Result<DecisionLock, String> {
    use std::os::fd::AsRawFd;
    std::fs::create_dir_all(dir(project)).map_err(io)?;
    let file = crate::analytics::jobs::open_lock(&dir(project).join(LOCK)).map_err(io)?;
    // SAFETY: flock on an owned, open file descriptor.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(io(std::io::Error::last_os_error()));
    }
    Ok(DecisionLock(file))
}

/// Decides one request: one decision at a time (the store's lock), idempotent per request id,
/// compare-and-swap on `expected` (the version the operator saw; `None`: the current one), the
/// secret of a credential set or rotation into `store` under the new revision's account before
/// the commit, the sweep after it, a durable redacted receipt either way. `Err` (nothing
/// persisted) for a malformed request — an invalid identifier, a missing or unexpected secret —
/// or a failing store, and for a request id reused for another action; a refused request is an
/// `Ok` receipt with `applied = false`.
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
        return Err("PROVIDER_REQUEST_ID_INVALID ([A-Za-z0-9_-]{1,64})".into());
    }
    action.check()?;
    if action.stores_secret() != secret.is_some() {
        return Err(if secret.is_some() {
            "CREDENTIAL_SECRET_UNEXPECTED".into()
        } else {
            "CREDENTIAL_SECRET_REQUIRED".into()
        });
    }
    if secret.is_some_and(|s| {
        s.is_empty() || s.len() > SECRET_LIMIT || s.iter().any(|b| b.is_ascii_control())
    }) {
        return Err("CREDENTIAL_SECRET_FORMAT".into());
    }
    let _lock = lock(project)?;
    let digest = action.digest();
    let conflict = |other: &str| {
        Err(format!(
            "PROVIDER_REQUEST_ID_CONFLICT: {request_id} decided another action ({other})"
        ))
    };
    if let Some(r) = receipt(project, request_id)? {
        return if r.action_sha256 == digest {
            Ok(r)
        } else {
            conflict(&r.action)
        };
    }
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
        journal::create_typed_value(
            &receipt_path(project, request_id),
            journal::source::PROVIDER_CONTROL,
            &r,
        )
        .map_err(io)?;
        Ok(r)
    };
    // A receipt lost after its version was committed: rebuilt from that version, if it is
    // this action's.
    for v in versions(project)?.into_iter().rev() {
        let committed = read_version(project, v)?;
        if committed.request_id == request_id {
            if committed.action_sha256 != digest {
                return conflict(&committed.action);
            }
            return write(receipt_of(true, v, "committed (receipt rebuilt)".into()));
        }
    }
    let current = current(project)?;
    let here = canonical_project(project)?;
    let foreign = current.version > 0 && current.project != here;
    // A copied store never decides over this project's items; only an adoption takes it over.
    match (foreign, matches!(action, ProviderAction::Adopt)) {
        (false, false) | (true, true) => {}
        (false, true) => {
            return write(receipt_of(
                false,
                current.version,
                "PROVIDER_REGISTRY_OWN: the registry is this project's; nothing to adopt".into(),
            ))
        }
        (true, false) => {
            return Err("PROVIDER_REGISTRY_FOREIGN: recorded for another project directory; `p0ctl provider adopt` takes it over".into())
        }
    }
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
    if let ProviderAction::Qualify(id, Some(evidence)) = action {
        // An evidence id the store does not hold is no request (nothing persisted: a mistyped
        // value may be a key); a stored record of another binding is a refusal.
        let path = crate::orchestration::provider_evidence::resolve(project, evidence)?;
        if let Err(e) = qualifies(&current, id, &path, evidence) {
            return write(receipt_of(false, current.version, e));
        }
    }
    let nonce = fresh_nonce()?;
    let mut next = match apply(&current, action, &nonce, now) {
        Ok(n) => n,
        Err(e) => return write(receipt_of(false, current.version, e)),
    };
    next.version = current.version + 1;
    if next.uid.is_empty() || matches!(action, ProviderAction::Adopt) {
        next.uid = fresh_nonce()?;
    }
    next.project = here;
    let stored = new_account(&next, action);
    if let (Some(account), Some(s)) = (&stored, secret) {
        store.put(account, s)?;
    }
    let record = ProviderRegistryRecord {
        registry: next.clone(),
        request_id: request_id.to_string(),
        action: action.text(),
        action_sha256: digest.clone(),
        decided_unix_s: now,
    };
    if !commit(project, &record, stored.as_deref(), store)? {
        return write(receipt_of(
            false,
            current.version,
            "PROVIDER_VERSION_CONFLICT: another decision committed first".into(),
        ));
    }
    let mut detail = "committed (effective at the next controlled start)".to_string();
    if touches_credentials(action) {
        match sweep(&next, store) {
            Ok(failed) if failed.is_empty() => {}
            Ok(failed) => {
                detail = format!(
                    "{detail}; {} obsolete item(s) not deleted (swept again at the next credential decision)",
                    failed.len()
                )
            }
            Err(e) => detail = format!("{detail}; sweep: {e}"),
        }
    }
    write(receipt_of(true, next.version, detail))
}

/// The canonical directory of a project: what a registry is recorded for.
pub fn canonical_project(project: &Path) -> Result<String, String> {
    std::fs::canonicalize(project)
        .map(|p| p.display().to_string())
        .map_err(io)
}

/// Whether a registry may be decided over or used from `project`: it is recorded for this
/// project's directory (a copied store fails closed).
pub fn owned(project: &Path, registry: &ProviderRegistry) -> Result<(), String> {
    if registry.version > 0 && registry.project != canonical_project(project)? {
        return Err("PROVIDER_REGISTRY_FOREIGN: recorded for another project directory; `p0ctl provider adopt` takes it over".into());
    }
    Ok(())
}

/// DEC-18: `resolved` bound to the project's current registry
/// ([`darkbot_solana::runtime_config::Resolved::bind_registry`]): every kind without a named
/// loopback endpoint takes the registry's selection; a copied store fails closed. Returns the
/// version it was bound to (its credentials' accounts). The one binding step of every start,
/// audit and identity (so the audit and the run it admits bind the same providers).
pub fn bind(
    project: &Path,
    resolved: &mut darkbot_solana::runtime_config::Resolved,
) -> Result<ProviderRegistry, String> {
    let registry = current(project)?;
    owned(project, &registry)?;
    resolved.bind_registry(&registry)?;
    Ok(registry)
}

/// DEC-18 §1.2/§1.3.6: the credentials of `config`'s providers from `store` — exactly the item of
/// each bound revision: the registry still binds each provider to the same identity and
/// revision (a provider changed or a credential rotated or revoked since refuses the start,
/// never another revision in its place; an unrelated change of the registry does not).
pub fn credentials_of(
    registry: &ProviderRegistry,
    config: &darkbot_solana::runtime_config::RuntimeConfig,
    store: &dyn CredentialStore,
) -> Result<Vec<darkbot_solana::providers::ActiveCredential>, String> {
    let mut out = vec![];
    for p in &config.providers {
        let Some(credential) = &p.credential else {
            continue;
        };
        let now = registry
            .provider(&p.id)
            .map(|q| registry.binding_of(q))
            .transpose()?;
        if now.as_deref() != Some(p.binding.as_str()) {
            return Err(format!(
                "PROVIDER_BINDING_CHANGED: {} is no longer bound as the run was configured (a provider or credential change is effective at the next start)",
                p.id
            ));
        }
        out.push(darkbot_solana::providers::ActiveCredential {
            endpoint: p.endpoint.clone(),
            auth: p.auth.clone(),
            secret: store.get(&registry.account(credential))?,
        });
    }
    Ok(out)
}

/// DEC-18: the configuration of a process that reads from the network — bound to the project's
/// registry ([`bind`]) and its providers' credentials read once from the system store (the
/// macOS keychain) and installed for the transports, before any transport is used. Without a
/// project only named loopback endpoints exist (nothing to bind, no credential). A configuration
/// without an authenticating provider never opens the store.
pub fn network_config(
    project: Option<&Path>,
    resolved: &mut darkbot_solana::runtime_config::Resolved,
) -> Result<(), String> {
    let Some(project) = project else {
        return Ok(());
    };
    let registry = bind(project, resolved)?;
    install_credentials(&registry, &resolved.config)
}

/// The credentials of `config`'s providers (bound from `registry`) read once from the system
/// store and installed for the transports; nothing to read leaves the store unopened.
pub fn install_credentials(
    registry: &ProviderRegistry,
    config: &darkbot_solana::runtime_config::RuntimeConfig,
) -> Result<(), String> {
    if config.providers.iter().all(|p| p.credential.is_none()) {
        return Ok(());
    }
    let store = darkbot_solana::providers::system_store()?;
    darkbot_solana::providers::activate_credentials(credentials_of(
        registry,
        config,
        store.as_ref(),
    )?)
}

/// DEC-18: the operational configuration of a network tool (`p0-prewarm-atas`,
/// `p0-probe-costs`): resolved from its environment, bound to the project's registry
/// (`P0_PROJECT_DIR`), its credentials installed, activated. Returns the primary RPC endpoint.
pub fn activate_tool() -> Result<String, String> {
    let mut resolved =
        darkbot_solana::runtime_config::Resolved::parse(&[], &|k| std::env::var(k).ok())?;
    network_config(
        std::env::var_os("P0_PROJECT_DIR")
            .map(PathBuf::from)
            .as_deref(),
        &mut resolved,
    )?;
    let rpc = resolved
        .config
        .rpc_url()
        .ok_or("RPC_URL_MISSING: the selected RPC provider (p0ctl provider select --kind rpc)")?;
    resolved.activate()?;
    Ok(rpc)
}

/// DEC-18 §3.7: a qualification names provider evidence of exactly this provider's binding —
/// a stored record (its bytes the id's) one of whose proofs (or its WSS proof) binds
/// `binding_of` the provider with its active revision. Evidence bound to a URL hash, another
/// provider or an older revision never qualifies.
fn qualifies(
    current: &ProviderRegistry,
    id: &str,
    path: &Path,
    evidence: &str,
) -> Result<(), String> {
    let p = current
        .provider(id)
        .ok_or_else(|| format!("PROVIDER_UNKNOWN: {id}"))?;
    let binding = current.binding_of(p)?;
    let record: crate::orchestration::provider_evidence::ProviderEvidence =
        journal::read_typed_value(path).map_err(|_| "PROVIDER_EVIDENCE_FORMAT".to_string())?;
    // A passing FREE_ONLY measurement of exactly this binding (the WSS proof: a FREE_ONLY WSS).
    let bound_rpc = record
        .providers
        .iter()
        .find(|proof| proof.provider_binding == binding);
    let bound_wss = record.wss_provider_binding.as_deref() == Some(binding.as_str());
    match (bound_rpc, bound_wss) {
        (Some(proof), _) if proof.passed && proof.policy == "FREE_ONLY" => Ok(()),
        (None, true) if record.wss_free_only => Ok(()),
        (None, false) => Err(format!(
            "PROVIDER_EVIDENCE_BINDING_MISMATCH: {evidence} does not bind provider {id}'s binding"
        )),
        _ => Err(format!(
            "PROVIDER_EVIDENCE_NOT_PASSING: {evidence} is no passing FREE_ONLY measurement"
        )),
    }
}

/// Publishes `record` as its version (create-new: the compare-and-swap). `false`: another
/// decision committed that version first — then the new revision's item `stored` is deleted, so
/// a lost race never leaves a secret behind. A failure after the version was linked (its
/// directory sync) is no lost commit: the version stands with its item.
fn commit(
    project: &Path,
    record: &ProviderRegistryRecord,
    stored: Option<&str>,
    store: &dyn CredentialStore,
) -> Result<bool, String> {
    let path = version_path(project, record.registry.version);
    match journal::create_typed_value(&path, journal::source::PROVIDER_CONTROL, record) {
        Ok(()) => Ok(true),
        Err(e) => {
            let ours = read_version(project, record.registry.version).is_ok_and(|v| {
                v.request_id == record.request_id && v.action_sha256 == record.action_sha256
            });
            if ours {
                return Ok(true);
            }
            if let Some(account) = stored {
                let _ = store.delete(account);
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
    fn put(&self, _: &str, _: &[u8]) -> Result<(), String> {
        Err("CREDENTIAL_STORE_NOT_OPENED".into())
    }
    fn get(&self, _: &str) -> Result<darkbot_solana::providers::Secret, String> {
        Err("CREDENTIAL_STORE_NOT_OPENED".into())
    }
    fn delete(&self, _: &str) -> Result<(), String> {
        Err("CREDENTIAL_STORE_NOT_OPENED".into())
    }
    fn list(&self, _: &str) -> Result<Vec<String>, String> {
        Err("CREDENTIAL_STORE_NOT_OPENED".into())
    }
}

/// Whether an action needs the credential store (it stores a revision's item, or makes one
/// obsolete for the sweep).
pub fn touches_credentials(action: &ProviderAction) -> bool {
    matches!(
        action,
        ProviderAction::CredentialSet { .. }
            | ProviderAction::CredentialRotate(_)
            | ProviderAction::CredentialRevoke(_)
    )
}

/// DEC-18 (P0-020): the registry's secret-free projection (topic project0.analytics.jobs): every
/// provider with its host (never the endpoint's path), authentication name, credential id,
/// binding and selection block, every credential's newest revision.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegistryView {
    pub version: u64,
    pub providers: Vec<ProviderView>,
    pub credentials: Vec<CredentialView>,
}

/// One provider of [`RegistryView`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderView {
    pub id: String,
    pub kind: &'static str,
    pub host: String,
    pub auth: String,
    pub credential: Option<String>,
    pub priority: u32,
    pub capacity_rps: Option<u64>,
    pub plan: &'static str,
    pub entitlement: String,
    pub qualification: Option<String>,
    pub enabled: bool,
    pub selected: bool,
    /// `None` while its credential is revoked or unknown.
    pub binding: Option<String>,
    pub selection_block: Option<String>,
}

/// One credential of [`RegistryView`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialView {
    pub id: String,
    pub revision: u64,
    pub state: &'static str,
    pub origin: String,
    pub created_unix_s: u64,
}

/// The registry's view of `project`, or why it is not readable (a copied store included).
pub fn projection(project: &Path) -> (Option<RegistryView>, Option<String>) {
    let registry = match current(project).and_then(|r| owned(project, &r).map(|()| r)) {
        Ok(r) => r,
        Err(e) => return (None, Some(e)),
    };
    let mut providers: Vec<ProviderView> = registry
        .providers
        .iter()
        .map(|p| ProviderView {
            id: p.id.clone(),
            kind: p.kind.name(),
            host: p.host(),
            auth: p.auth.text(),
            credential: p.credential.clone(),
            priority: p.priority,
            capacity_rps: p.capacity_rps,
            plan: p.plan.name(),
            entitlement: p.entitlement.clone(),
            qualification: p.qualification.clone(),
            enabled: p.enabled,
            selected: registry.selected(p.kind).is_some_and(|s| s.id == p.id),
            binding: registry.binding_of(p).ok(),
            selection_block: registry.selection_block(p),
        })
        .collect();
    providers.sort_by(|a, b| (a.kind, a.priority, &a.id).cmp(&(b.kind, b.priority, &b.id)));
    let credentials = registry
        .credentials
        .iter()
        .map(|c| CredentialView {
            id: c.id.clone(),
            revision: c.revision,
            state: match c.state {
                CredentialState::Active => "active",
                CredentialState::Revoked => "revoked",
            },
            origin: c.origin.clone(),
            created_unix_s: c.created_unix_s,
        })
        .collect();
    (
        Some(RegistryView {
            version: registry.version,
            providers,
            credentials,
        }),
        None,
    )
}

/// The operator's view of the current version: one line per provider (host only, never a path
/// or a credential) with its binding and why it cannot be selected, and one per credential (id,
/// revision, state, origin).
pub fn status_lines(registry: &ProviderRegistry) -> Vec<String> {
    let mut out = vec![format!(
        "PROVIDER_REGISTRY version={} restart_rule=next_controlled_start",
        registry.version
    )];
    let mut providers: Vec<&ProviderDescriptor> = registry.providers.iter().collect();
    providers.sort_by(|a, b| (a.kind, a.priority, &a.id).cmp(&(b.kind, b.priority, &b.id)));
    for p in providers {
        let selected = registry.selected(p.kind).map(|s| s.id == p.id) == Some(true);
        let credential = match registry.credential_of(p) {
            Ok(Some(c)) => format!("{}@{}", c.id, c.revision),
            Ok(None) => "-".into(),
            Err(e) => e,
        };
        out.push(format!(
            "provider {} kind={} host={} auth={} credential={} plan={} entitlement={} capacity_rps={} priority={} enabled={} selected={} qualification={} binding={} block={}",
            p.id,
            p.kind.name(),
            p.host(),
            p.auth.text(),
            credential,
            p.plan.name(),
            p.entitlement,
            p.capacity_rps.map_or("-".into(), |c| c.to_string()),
            p.priority,
            p.enabled,
            selected,
            p.qualification.as_deref().unwrap_or("-"),
            registry
                .binding_of(p)
                .map(|b| b[..16].to_string())
                .unwrap_or_else(|_| "-".into()),
            registry.selection_block(p).unwrap_or_else(|| "-".into()),
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

/// Test fixture: `config`'s endpoints bound through a registry of their own (version 1, one free
/// provider without credential per remote endpoint, by priority; a loopback one local) — the
/// production binding (`ProviderRegistry::bound`) of fixture endpoints. Returns the registry.
#[cfg(test)]
pub(crate) fn bind_endpoints(
    config: &mut darkbot_solana::runtime_config::RuntimeConfig,
) -> ProviderRegistry {
    use darkbot_solana::providers::{canonical_endpoint, is_local, AuthKind, BoundProvider, Plan};
    let mut registry = ProviderRegistry::empty();
    registry.version = 1;
    registry.uid = "f1f1f1f1f1f1f1f1".into();
    registry.project = "/fixture".into();
    let mut local = vec![];
    for (kind, list) in [
        (ProviderKind::Rpc, &mut config.rpc_endpoints),
        (ProviderKind::Wss, &mut config.wss_endpoints),
    ] {
        for (i, endpoint) in list.iter_mut().enumerate() {
            *endpoint = canonical_endpoint(kind, endpoint).expect("a canonical fixture endpoint");
            if is_local(endpoint) {
                local.push(BoundProvider::local(kind, endpoint).unwrap());
                continue;
            }
            registry.providers.push(ProviderDescriptor {
                id: format!("{}-{i}", kind.name()),
                kind,
                endpoint: endpoint.clone(),
                auth: AuthKind::None,
                credential: None,
                priority: i as u32,
                capacity_rps: None,
                plan: Plan::Free,
                entitlement: "fixture".into(),
                qualification: None,
                enabled: true,
            });
        }
    }
    config.providers = local;
    for p in &registry.providers {
        config.providers.push(registry.bound(p).unwrap());
    }
    registry
}

/// Test fixture: the bound providers of RPC `endpoints` ([`bind_endpoints`]).
#[cfg(test)]
pub(crate) fn fixture_rpc(endpoints: &[&str]) -> Vec<darkbot_solana::providers::BoundProvider> {
    let mut c = darkbot_solana::runtime_config::RuntimeConfig {
        rpc_endpoints: endpoints.iter().map(|e| e.to_string()).collect(),
        ..Default::default()
    };
    bind_endpoints(&mut c);
    c.providers
}

// The tests use the directory store, which exists in a debug build only.
#[cfg(all(test, debug_assertions))]
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

    /// DEC-18 package-4 review F6: a registry holds at most `PROVIDER_REGISTRY_ENTRIES_MAX`
    /// providers and credentials, so its projection fits one STATE frame (the reporter's worst
    /// case); a full registry still takes a new revision of a credential it has.
    #[test]
    fn a_registry_holds_at_most_its_bound_of_providers_and_credentials() {
        let max = darkbot_solana::runtime_config::defaults::PROVIDER_REGISTRY_ENTRIES_MAX as usize;
        let d = project("bound");
        let store = DirectoryStore(d.join("keychain"));
        let add = |n: usize| {
            let mut p = helius();
            p.id = format!("rpc-{n}");
            p.endpoint = format!("https://rpc-{n}.example");
            p.auth = AuthKind::None;
            p.credential = None;
            ProviderAction::Add(p)
        };
        let credential = |n: usize| ProviderAction::CredentialSet {
            credential: format!("cred-{n}"),
            origin: "input".into(),
        };
        for n in 0..max {
            let r = decide(&d, &format!("a{n}"), None, &add(n), None, &store, 1).unwrap();
            assert!(r.applied, "{r:?}");
            let r = decide(
                &d,
                &format!("c{n}"),
                None,
                &credential(n),
                Some(b"KEY"),
                &store,
                1,
            )
            .unwrap();
            assert!(r.applied, "{r:?}");
        }
        let full = decide(&d, "a-full", None, &add(max), None, &store, 1).unwrap();
        assert!(
            !full.applied && full.detail.starts_with("PROVIDER_REGISTRY_FULL"),
            "{full:?}"
        );
        let full = decide(
            &d,
            "c-full",
            None,
            &credential(max),
            Some(b"KEY"),
            &store,
            1,
        )
        .unwrap();
        assert!(
            !full.applied && full.detail.starts_with("CREDENTIAL_REGISTRY_FULL"),
            "{full:?}"
        );
        let rotate = ProviderAction::CredentialRotate("cred-0".into());
        let r = decide(&d, "c-rotate", None, &rotate, Some(b"KEY-2"), &store, 1).unwrap();
        assert!(r.applied, "{r:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    fn set() -> ProviderAction {
        ProviderAction::CredentialSet {
            credential: "helius".into(),
            origin: "input".into(),
        }
    }

    fn items(store: &DirectoryStore, registry: &ProviderRegistry) -> Vec<String> {
        store.list(&registry.account_prefix()).unwrap()
    }

    /// A stored provider evidence record with one proof of `provider_binding`; its id.
    fn evidence_for(d: &Path, tag: &str, provider_binding: &str) -> String {
        evidence_passing(d, tag, provider_binding, true)
    }

    /// `evidence_for` with the measurement's verdict.
    fn evidence_passing(d: &Path, tag: &str, provider_binding: &str, passed: bool) -> String {
        use crate::orchestration::provider_evidence::{self, ProviderEvidence, ProviderProof};
        let file = d.join(format!("{tag}.p0j"));
        journal::create_typed_value(
            &file,
            journal::source::BOOT_CONTROL,
            &ProviderEvidence {
                providers: vec![ProviderProof {
                    policy: "FREE_ONLY".into(),
                    passed,
                    provider_binding: provider_binding.into(),
                    measured_at_unix_s: 1.0,
                    evidence_sha256: Some("0".repeat(64)),
                    allowed_rps: 1.0,
                    available_calls: 1,
                }],
                wss_provider_binding: None,
                wss_free_only: true,
                archive_qualified: false,
                source_sha256: "0".repeat(64),
            },
        )
        .unwrap();
        provider_evidence::import(d, &file).unwrap()
    }

    /// DEC-18 §1.2: a run binds the credential revision active when it is configured — the
    /// configuration's digest (the audit's and the run's identity) changes with a rotation; the
    /// credentials a start reads are exactly the bound revision's (after a rotation the earlier
    /// configuration is refused: the registry moved on, the superseded item is gone); a copied
    /// store binds nothing; a named loopback endpoint wins over the selection (DEC-05).
    #[test]
    fn a_run_binds_its_revision_and_a_rotation_is_drift() {
        let d = project("bind");
        let store = DirectoryStore(d.join("keychain"));
        let ok = |id: &str, action: ProviderAction, secret: Option<&[u8]>, now| {
            let r = decide(&d, id, None, &action, secret, &store, now).unwrap();
            assert!(r.applied, "{r:?}");
        };
        ok("b1", ProviderAction::Add(helius()), None, 1);
        ok("b2", set(), Some(b"KEY-1"), 2);
        ok(
            "b3",
            ProviderAction::Select(ProviderKind::Rpc, "helius-rpc".into()),
            None,
            3,
        );
        let resolve = |args: &[&str]| {
            let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            let mut r =
                darkbot_solana::runtime_config::Resolved::for_mode("econ4h", &args, &|_| None)
                    .unwrap();
            let registry = bind(&d, &mut r).unwrap();
            (r, registry)
        };
        let (first, at_first) = resolve(&[]);
        assert_eq!(
            first.config.rpc_endpoints,
            ["https://mainnet.helius-rpc.com"]
        );
        assert_eq!(first.sources["rpc_endpoints"], "provider_registry:v3");
        let read = credentials_of(&at_first, &first.config, &store).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].secret.expose(), b"KEY-1");
        let digest = super::super::boot_audit::runtime_config_sha256;
        let audited = digest(&first.config).unwrap();
        assert_eq!(
            digest(&resolve(&[]).0.config).unwrap(),
            audited,
            "unchanged: same identity"
        );
        // Review L5: a registry commit that leaves this provider's binding as it was (another
        // provider added) is no drift, and its credential still installs.
        let mut other = helius();
        other.id = "other-rpc".into();
        other.endpoint = "https://other.example".into();
        ok("b3a", ProviderAction::Add(other), None, 3);
        let (unrelated, at_unrelated) = resolve(&[]);
        assert_eq!(digest(&unrelated.config).unwrap(), audited);
        assert_eq!(
            credentials_of(&at_unrelated, &first.config, &store).unwrap()[0]
                .secret
                .expose(),
            b"KEY-1"
        );

        ok(
            "b4",
            ProviderAction::CredentialRotate("helius".into()),
            Some(b"KEY-2"),
            4,
        );
        let (second, at_second) = resolve(&[]);
        assert_ne!(
            digest(&second.config).unwrap(),
            audited,
            "a rotation is drift"
        );
        assert_eq!(
            credentials_of(&at_second, &second.config, &store).unwrap()[0]
                .secret
                .expose(),
            b"KEY-2"
        );
        assert!(credentials_of(&at_second, &first.config, &store)
            .unwrap_err()
            .starts_with("PROVIDER_BINDING_CHANGED"));
        assert!(
            credentials_of(&at_first, &first.config, &store)
                .unwrap_err()
                .starts_with("CREDENTIAL_UNAVAILABLE"),
            "the superseded revision's item is gone"
        );

        let (named, _) = resolve(&["--rpc-endpoints", "http://127.0.0.1:8899"]);
        assert_eq!(named.config.rpc_endpoints, ["http://127.0.0.1:8899"]);
        assert!(credentials_of(&at_second, &named.config, &store)
            .unwrap()
            .is_empty());

        let copy = project("bind_copy");
        std::fs::create_dir_all(copy.join(STORE)).unwrap();
        for e in std::fs::read_dir(d.join(STORE)).unwrap() {
            let e = e.unwrap();
            std::fs::copy(e.path(), copy.join(STORE).join(e.file_name())).unwrap();
        }
        let mut r =
            darkbot_solana::runtime_config::Resolved::for_mode("econ4h", &[], &|_| None).unwrap();
        assert!(bind(&copy, &mut r)
            .unwrap_err()
            .starts_with("PROVIDER_REGISTRY_FOREIGN"));
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&copy);
    }

    /// DEC-18 (P0-020): the registry's projection is secret-free — every provider with its host
    /// (never the endpoint's path), authentication name, credential id, binding, selection and why
    /// it cannot be selected, every credential's revision; an empty store is version 0, a copied
    /// one is refused with its reason (nothing projected).
    #[test]
    fn the_projection_names_hosts_and_revisions_never_a_secret() {
        let d = project("projection");
        let store = DirectoryStore(d.join("keychain"));
        let (empty, error) = projection(&d);
        assert_eq!((empty.unwrap().version, error), (0, None));
        let mut p = helius();
        p.endpoint = "https://mainnet.helius-rpc.com/v0/archive".into();
        for (n, action, secret) in [
            (1, ProviderAction::Add(p), None),
            (2, set(), Some(&b"PROJECTED-KEY"[..])),
            (
                3,
                ProviderAction::Select(ProviderKind::Rpc, "helius-rpc".into()),
                None,
            ),
        ] {
            let r = decide(&d, &format!("p{n}"), None, &action, secret, &store, n).unwrap();
            assert!(r.applied, "{r:?}");
        }
        let (view, error) = projection(&d);
        let view = view.unwrap();
        assert_eq!((view.version, error), (3, None));
        let p = &view.providers[0];
        assert_eq!(
            (
                p.id.as_str(),
                p.kind,
                p.host.as_str(),
                p.auth.as_str(),
                p.selected
            ),
            (
                "helius-rpc",
                "rpc",
                "mainnet.helius-rpc.com",
                "query:api-key",
                true
            )
        );
        assert_eq!(p.binding.as_ref().map(String::len), Some(64));
        assert_eq!(p.selection_block, None);
        assert_eq!(
            (view.credentials[0].revision, view.credentials[0].state),
            (1, "active")
        );
        let shown = format!("{view:?}");
        assert!(
            !shown.contains("PROJECTED-KEY") && !shown.contains("/v0/archive"),
            "{shown}"
        );
        let copy = project("projection_copy");
        std::fs::create_dir_all(copy.join(STORE)).unwrap();
        for e in std::fs::read_dir(d.join(STORE)).unwrap() {
            let e = e.unwrap();
            std::fs::copy(e.path(), copy.join(STORE).join(e.file_name())).unwrap();
        }
        let (none, error) = projection(&copy);
        assert!(none.is_none() && error.unwrap().starts_with("PROVIDER_REGISTRY_FOREIGN"));
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&copy);
    }

    /// DEC-18: a provider and its credential through the store — the secret is stored under its
    /// revision's account before the version naming it is committed, never in a record or a
    /// receipt; a selection needs an active credential; a rotation stores the next revision and
    /// the sweep deletes the superseded item; a revocation is always possible and deletes the
    /// item; a revoked credential continues at its next revision with a new nonce.
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
        assert_eq!(
            decide(&d, "r3", None, &set(), None, &store, 12).unwrap_err(),
            "CREDENTIAL_SECRET_REQUIRED"
        );
        assert!(receipt(&d, "r3").unwrap().is_none(), "nothing persisted");
        ok(decide(&d, "r3", None, &set(), Some(b"KEY-ONE"), &store, 12));
        ok(decide(&d, "r4", None, &select, None, &store, 13));
        let reg = current(&d).unwrap();
        assert_eq!(reg.version, 3);
        let r1 = reg.credential("helius").unwrap().reference();
        assert_eq!(store.get(&reg.account(&r1)).unwrap().expose(), b"KEY-ONE");
        assert_eq!(items(&store, &reg).len(), 1);
        for e in std::fs::read_dir(d.join(STORE)).unwrap() {
            let bytes = std::fs::read(e.unwrap().path()).unwrap();
            assert!(!bytes.windows(7).any(|w| w == b"KEY-ONE"));
        }
        // Rotation: the next revision with a new nonce, the superseded item swept.
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
        let reg = current(&d).unwrap();
        let r2 = reg.credential("helius").unwrap().reference();
        assert_eq!((r2.revision, r2.nonce != r1.nonce), (2, true));
        assert_eq!(items(&store, &reg), vec![reg.account(&r2)]);
        assert_eq!(store.get(&reg.account(&r2)).unwrap().expose(), b"KEY-TWO");
        // Revocation while selected: possible, the item swept; a new set continues at 3.
        ok(decide(
            &d,
            "r6",
            None,
            &ProviderAction::CredentialRevoke("helius".into()),
            None,
            &store,
            15,
        ));
        let reg = current(&d).unwrap();
        assert!(items(&store, &reg).is_empty());
        assert!(reg
            .credential_of(reg.selected(ProviderKind::Rpc).unwrap())
            .unwrap_err()
            .starts_with("CREDENTIAL_REVOKED"));
        ok(decide(
            &d,
            "r7",
            None,
            &set(),
            Some(b"KEY-THREE"),
            &store,
            16,
        ));
        assert_eq!(
            current(&d).unwrap().credential("helius").unwrap().revision,
            3
        );
        let _ = std::fs::remove_dir_all(d);
    }

    /// DEC-18 (P8-CTRL.6 pattern): one request id is one decision — the same action returns its
    /// receipt, another action under it is a conflict (also when only the committed version
    /// remembers it); a stale expected version is refused with a receipt; a receipt lost after
    /// the commit is rebuilt, never a second commit.
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
        let remove = ProviderAction::Remove("helius-rpc".into());
        assert!(decide(&d, "a", None, &remove, None, &store, 3)
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
        // A lost receipt: rebuilt from the committed version for its own action only.
        std::fs::remove_file(receipt_path(&d, "a")).unwrap();
        assert!(decide(&d, "a", None, &remove, None, &store, 5)
            .unwrap_err()
            .starts_with("PROVIDER_REQUEST_ID_CONFLICT"));
        let rebuilt = decide(&d, "a", Some(0), &add, None, &store, 5).unwrap();
        assert!(rebuilt.applied && rebuilt.version == 1, "{rebuilt:?}");
        assert_eq!(current(&d).unwrap().version, 1, "never a second commit");
        let _ = std::fs::remove_dir_all(d);
    }

    /// Review M1: two decisions on one credential at once never destroy the committed secret —
    /// serialized by the store's lock, both rotations apply in turn (revisions 2 and 3) and the
    /// store holds exactly the active revision's item with its secret; two at the same expected
    /// version: one applies, the other is refused and its secret is never stored.
    #[test]
    fn concurrent_decisions_on_one_credential_keep_the_committed_secret() {
        let d = project("racers");
        let store_dir = d.join("keychain");
        let store = DirectoryStore(store_dir.clone());
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
        decide(&d, "s", None, &set(), Some(b"KEY-0"), &store, 2).unwrap();
        let race = |expected: Option<u64>, tag: &str| {
            let handles: Vec<_> = ["X", "Y"]
                .into_iter()
                .map(|who| {
                    let (d, store_dir, tag) = (d.clone(), store_dir.clone(), tag.to_string());
                    std::thread::spawn(move || {
                        let secret = format!("KEY-{tag}{who}");
                        decide(
                            &d,
                            &format!("{tag}{who}"),
                            expected,
                            &ProviderAction::CredentialRotate("helius".into()),
                            Some(secret.as_bytes()),
                            &DirectoryStore(store_dir),
                            3,
                        )
                        .unwrap()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect::<Vec<_>>()
        };
        let both = race(None, "n");
        assert!(both.iter().all(|r| r.applied), "{both:?}");
        let reg = current(&d).unwrap();
        let active = reg.credential("helius").unwrap().reference();
        assert_eq!(active.revision, 3);
        assert_eq!(items(&store, &reg), vec![reg.account(&active)]);
        let last = both.iter().max_by_key(|r| r.version).unwrap();
        let expected_secret = format!("KEY-{}", last.request_id);
        assert_eq!(
            store.get(&reg.account(&active)).unwrap().expose(),
            expected_secret.as_bytes()
        );
        let at = reg.version;
        let one = race(Some(at), "e");
        assert_eq!(one.iter().filter(|r| r.applied).count(), 1, "{one:?}");
        let reg = current(&d).unwrap();
        let active = reg.credential("helius").unwrap().reference();
        let winner = one.iter().find(|r| r.applied).unwrap();
        assert_eq!(items(&store, &reg), vec![reg.account(&active)]);
        assert_eq!(
            store.get(&reg.account(&active)).unwrap().expose(),
            format!("KEY-{}", winner.request_id).as_bytes()
        );
        let _ = std::fs::remove_dir_all(d);
    }

    /// The version is the compare-and-swap: a decision that loses the race for its version
    /// commits nothing and its new item is deleted; the sweep deletes an orphan of an
    /// interrupted decision of this registry and never touches another registry's items.
    #[test]
    fn a_lost_race_leaves_no_secret_and_the_sweep_removes_orphans() {
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
            action: "x".into(),
            action_sha256: request_id.into(),
            decided_unix_s: 2,
        };
        assert!(commit(&d, &record("winner"), None, &store).unwrap());
        store.put("loser-account", b"SECRET").unwrap();
        assert!(!commit(&d, &record("loser"), Some("loser-account"), &store).unwrap());
        assert!(
            store.get("loser-account").is_err(),
            "the loser's item is deleted"
        );
        assert_eq!(read_version(&d, 2).unwrap().request_id, "winner");
        // An orphan of this registry and an item of another registry.
        let reg = current(&d).unwrap();
        let orphan = format!("{}helius@9.0000000000000000", reg.account_prefix());
        store.put(&orphan, b"ORPHAN").unwrap();
        store
            .put("ffffffffffffffff/helius@1.0000000000000000", b"OTHER")
            .unwrap();
        decide(&d, "s", None, &set(), Some(b"KEY"), &store, 3).unwrap();
        let reg = current(&d).unwrap();
        assert!(store.get(&orphan).is_err(), "the orphan is swept");
        assert_eq!(items(&store, &reg).len(), 1);
        assert!(
            store
                .get("ffffffffffffffff/helius@1.0000000000000000")
                .is_ok(),
            "another registry's item kept"
        );
        let _ = std::fs::remove_dir_all(d);
    }

    /// Re-check L3/L5/L6: a decision that touches no credential carries no sweep note; a
    /// qualification names stored evidence of exactly this provider's binding (an evidence id the
    /// store does not hold is no request; a record of another binding is refused); a copied store
    /// never decides over another project's registry — only an adoption takes it over, with a
    /// new uid and every credential revoked, the original project's items untouched.
    #[test]
    fn a_qualification_is_the_binding_s_and_a_copy_is_never_this_project_s() {
        let d = project("qualify");
        let store = DirectoryStore(d.join("keychain"));
        let add = decide(
            &d,
            "a",
            None,
            &ProviderAction::Add(helius()),
            None,
            &store,
            1,
        )
        .unwrap();
        assert_eq!(
            add.detail,
            "committed (effective at the next controlled start)"
        );
        decide(&d, "s", None, &set(), Some(b"KEY"), &store, 2).unwrap();
        let reg = current(&d).unwrap();
        let binding = reg.binding_of(reg.provider("helius-rpc").unwrap()).unwrap();
        let evidence = |tag: &str, provider_binding: &str| evidence_for(&d, tag, provider_binding);
        let qualify = |rid: &str, e: &str| {
            decide(
                &d,
                rid,
                None,
                &ProviderAction::Qualify("helius-rpc".into(), Some(e.into())),
                None,
                &store,
                3,
            )
        };
        let url_hash =
            journal::hex(&solana_sdk::hash::hash(b"https://mainnet.helius-rpc.com").to_bytes());
        let old = qualify("q1", &evidence("old", &url_hash)).unwrap();
        assert!(
            !old.applied && old.detail.starts_with("PROVIDER_EVIDENCE_BINDING_MISMATCH"),
            "{old:?}"
        );
        assert!(
            qualify("q2", "0123456789abcdef").is_err(),
            "an unknown evidence id is no request"
        );
        assert!(receipt(&d, "q2").unwrap().is_none());
        let failed = qualify("q4", &evidence_passing(&d, "failed", &binding, false)).unwrap();
        assert!(
            !failed.applied && failed.detail.starts_with("PROVIDER_EVIDENCE_NOT_PASSING"),
            "{failed:?}"
        );
        let good = evidence("good", &binding);
        assert!(qualify("q3", &good).unwrap().applied);
        assert_eq!(
            current(&d)
                .unwrap()
                .provider("helius-rpc")
                .unwrap()
                .qualification
                .as_deref(),
            Some(good.as_str())
        );
        // A copied store in another project directory.
        let copy = project("qualify_copy");
        std::fs::create_dir_all(copy.join("data")).unwrap();
        assert!(std::process::Command::new("cp")
            .arg("-R")
            .arg(d.join(STORE))
            .arg(copy.join(STORE))
            .status()
            .unwrap()
            .success());
        let foreign = decide(
            &copy,
            "x",
            None,
            &ProviderAction::Enable("helius-rpc".into()),
            None,
            &store,
            4,
        )
        .unwrap_err();
        assert!(
            foreign.starts_with("PROVIDER_REGISTRY_FOREIGN"),
            "{foreign}"
        );
        // Review F11: the other project's path is never echoed.
        assert!(!foreign.contains(&*d.to_string_lossy()), "{foreign}");
        assert!(owned(&copy, &current(&copy).unwrap()).is_err());
        let own = decide(&d, "o", None, &ProviderAction::Adopt, None, &store, 4).unwrap();
        assert!(!own.applied && own.detail.starts_with("PROVIDER_REGISTRY_OWN"));
        let adopted = decide(
            &copy,
            "adopt",
            None,
            &ProviderAction::Adopt,
            None,
            &NoCredentialStore,
            5,
        )
        .unwrap();
        assert!(adopted.applied, "{adopted:?}");
        let theirs = current(&copy).unwrap();
        assert_ne!(theirs.uid, reg.uid);
        assert!(theirs
            .credentials
            .iter()
            .all(|c| c.state == CredentialState::Revoked));
        assert_eq!(theirs.provider("helius-rpc").unwrap().qualification, None);
        assert!(owned(&copy, &theirs).is_ok());
        assert_eq!(
            items(&store, &current(&d).unwrap()).len(),
            1,
            "the original project's item untouched"
        );
        let _ = std::fs::remove_dir_all(copy);
        let _ = std::fs::remove_dir_all(d);
    }

    /// Review L2/L3: every rule lives in `apply` (the GUI decides through the same path) and a
    /// refused value is never persisted: an invalid credential id (a key typed into it) is no
    /// receipt; the kind of a provider is immutable; an edit never sets a qualification and a
    /// changed binding clears it; the `.env` import is one-time; a selection needs a free plan.
    #[test]
    fn every_rule_is_the_store_s_and_a_refused_value_is_never_persisted() {
        let d = project("rules");
        let store = DirectoryStore(d.join("keychain"));
        let key = "0b6c2d4e-1f3a-4b5c-8d9e-0a1b2c3d4e5f";
        let e = decide(
            &d,
            "k",
            None,
            &ProviderAction::CredentialSet {
                credential: key.into(),
                origin: "input".into(),
            },
            Some(b"S"),
            &store,
            1,
        )
        .unwrap_err();
        assert!(
            e.starts_with("CREDENTIAL_ID_INVALID") && !e.contains(key),
            "{e}"
        );
        assert!(receipt(&d, "k").unwrap().is_none());
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
        let mut other_kind = helius();
        other_kind.kind = ProviderKind::Jupiter;
        other_kind.auth = AuthKind::Header("x-api-key".into());
        let r = decide(
            &d,
            "u1",
            None,
            &ProviderAction::Update(other_kind),
            None,
            &store,
            2,
        )
        .unwrap();
        assert!(
            !r.applied && r.detail.starts_with("PROVIDER_KIND_IMMUTABLE"),
            "{r:?}"
        );
        decide(&d, "s0", None, &set(), Some(b"KEY"), &store, 3).unwrap();
        let reg = current(&d).unwrap();
        let qualified = evidence_for(
            &d,
            "q",
            &reg.binding_of(reg.provider("helius-rpc").unwrap()).unwrap(),
        );
        assert!(
            decide(
                &d,
                "q",
                None,
                &ProviderAction::Qualify("helius-rpc".into(), Some(qualified.clone())),
                None,
                &store,
                3,
            )
            .unwrap()
            .applied
        );
        let mut same = helius();
        same.priority = 4;
        same.qualification = Some("fedcba9876543210".into());
        decide(
            &d,
            "u2",
            None,
            &ProviderAction::Update(same),
            None,
            &store,
            4,
        )
        .unwrap();
        assert_eq!(
            current(&d)
                .unwrap()
                .provider("helius-rpc")
                .unwrap()
                .qualification
                .as_deref(),
            Some(qualified.as_str()),
            "kept, never set by an edit"
        );
        let mut moved = helius();
        moved.capacity_rps = Some(5);
        decide(
            &d,
            "u3",
            None,
            &ProviderAction::Update(moved),
            None,
            &store,
            5,
        )
        .unwrap();
        assert_eq!(
            current(&d)
                .unwrap()
                .provider("helius-rpc")
                .unwrap()
                .qualification,
            None,
            "a changed binding clears it"
        );
        let import = |credential: &str| ProviderAction::CredentialSet {
            credential: credential.into(),
            origin: "env:HELIUS_API_KEY".into(),
        };
        let over = decide(&d, "i0", None, &import("helius"), Some(b"E"), &store, 6).unwrap();
        assert!(
            !over.applied && over.detail.starts_with("CREDENTIAL_EXISTS"),
            "{over:?}"
        );
        assert!(
            decide(&d, "i1", None, &import("helius-env"), Some(b"E"), &store, 6)
                .unwrap()
                .applied
        );
        let again = decide(&d, "i2", None, &import("helius2"), Some(b"E"), &store, 7).unwrap();
        assert!(!again.applied && again.detail.starts_with("CREDENTIAL_ENV_IMPORTED"));
        let mut unknown = helius();
        unknown.id = "anon".into();
        unknown.plan = Plan::Unknown;
        // DEC-18 §4.2: one provider per endpoint and kind — a second entry on the same endpoint
        // (another credential, another plan) is refused and never persisted.
        let same = decide(
            &d,
            "a1",
            None,
            &ProviderAction::Add(unknown.clone()),
            None,
            &store,
            8,
        )
        .unwrap();
        assert!(
            !same.applied && same.detail.starts_with("PROVIDER_ENDPOINT_DUPLICATE"),
            "{same:?}"
        );
        unknown.endpoint = "https://anon.example".into();
        decide(
            &d,
            "a2",
            None,
            &ProviderAction::Add(unknown),
            None,
            &store,
            8,
        )
        .unwrap();
        let sel = decide(
            &d,
            "s",
            None,
            &ProviderAction::Select(ProviderKind::Rpc, "anon".into()),
            None,
            &store,
            9,
        )
        .unwrap();
        assert!(
            !sel.applied && sel.detail.starts_with("PROVIDER_PLAN_UNPROVEN"),
            "{sel:?}"
        );
        let _ = std::fs::remove_dir_all(d);
    }
}
