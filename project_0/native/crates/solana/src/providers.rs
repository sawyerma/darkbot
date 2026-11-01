//! DEC-18 (Nutzerentscheidung 01.11.2026): the typed provider descriptor of the one
//! configuration authority (DEC-05).
//!
//! Every provider — RPC, WSS, Geyser, Jupiter, Jito — is an id, a kind and a **canonical
//! endpoint without user info, query, fragment or token-like path segment**, how it
//! authenticates (none, a query parameter, a header or a path segment: the parameter's or
//! header's *name*, never its value), a credential reference, its plan (FREE_ONLY: a paid plan is
//! refused, an unproven one never selected), its configured capacity, its priority and the
//! provider evidence that qualified it. A [`ProviderRegistry`] is one committed version of every
//! descriptor, the selection per kind and the credentials' revisions; the runtime takes the
//! version current at its start (a change is staged, effective at the next controlled start,
//! DEC-05). [`ProviderRegistry::binding_of`] is the secret-free canonical identity an audit, a
//! provider evidence and a run manifest bind instead of a hash of the URL.
//!
//! A credential's secret lives in the macOS keychain only: one item per credential revision,
//! its account `<registry uid>/<id>@<revision>.<nonce>` — the registry's random uid and the
//! revision's random nonce make every revision globally unique and never reusable (a wiped
//! registry starts a new uid), and confine a store's sweep to its own registry's items.
use serde::{Deserialize, Serialize};
pub use zeroize::{Zeroize, Zeroizing};

/// The kinds of provider DEC-18 names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ProviderKind {
    Rpc,
    Wss,
    Geyser,
    Jupiter,
    Jito,
}

pub const PROVIDER_KINDS: [ProviderKind; 5] = [
    ProviderKind::Rpc,
    ProviderKind::Wss,
    ProviderKind::Geyser,
    ProviderKind::Jupiter,
    ProviderKind::Jito,
];

impl ProviderKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Rpc => "rpc",
            Self::Wss => "wss",
            Self::Geyser => "geyser",
            Self::Jupiter => "jupiter",
            Self::Jito => "jito",
        }
    }
    /// Never echoes a refused value (an operator's slip may be a key).
    pub fn parse(s: &str) -> Result<Self, String> {
        PROVIDER_KINDS
            .into_iter()
            .find(|k| k.name() == s)
            .ok_or_else(|| "PROVIDER_KIND_UNKNOWN (rpc|wss|geyser|jupiter|jito)".to_string())
    }
    /// The scheme of a remote endpoint of this kind (a loopback endpoint may use the plain one,
    /// for local nodes and offline tests).
    fn schemes(self) -> (&'static str, &'static str) {
        match self {
            Self::Wss => ("wss", "ws"),
            _ => ("https", "http"),
        }
    }
}

/// How a provider authenticates: the name of the query parameter or header that carries the
/// credential, or the credential as the endpoint's last path segment; never its value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthKind {
    None,
    Query(String),
    Header(String),
    PathSegment,
}

impl AuthKind {
    pub fn text(&self) -> String {
        match self {
            Self::None => "none".into(),
            Self::Query(n) => format!("query:{n}"),
            Self::Header(n) => format!("header:{n}"),
            Self::PathSegment => "path".into(),
        }
    }
    /// Never echoes a refused value.
    pub fn parse(s: &str) -> Result<Self, String> {
        let name = |n: &str| -> Result<String, String> {
            // A parameter or header name, never a key typed into it.
            let ok = !n.is_empty()
                && n.len() <= 64
                && !key_shaped(n)
                && !token_like(n)
                && n.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
            ok.then(|| n.to_string())
                .ok_or_else(|| "PROVIDER_AUTH_NAME ([A-Za-z0-9_-]{1,64})".to_string())
        };
        match s.split_once(':') {
            None if s == "none" => Ok(Self::None),
            None if s == "path" => Ok(Self::PathSegment),
            Some(("query", n)) => Ok(Self::Query(name(n)?)),
            Some(("header", n)) => Ok(Self::Header(name(n)?)),
            _ => Err("PROVIDER_AUTH_UNKNOWN (none|query:NAME|header:NAME|path)".into()),
        }
    }
}

/// FREE_ONLY: a provider's plan as it is evidenced; `Paid` is refused, `Unknown` is never
/// selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Plan {
    Free,
    Paid,
    Unknown,
}

impl Plan {
    pub fn name(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Paid => "paid",
            Self::Unknown => "unknown",
        }
    }
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "free" => Ok(Self::Free),
            "paid" => Ok(Self::Paid),
            "unknown" => Ok(Self::Unknown),
            _ => Err("PROVIDER_PLAN_UNKNOWN (free|unknown)".into()),
        }
    }
}

/// One revision of a credential: the secret is the keychain's item of exactly this revision.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CredentialRef {
    pub id: String,
    pub revision: u64,
    /// The revision's random nonce (16 lowercase hex digits).
    pub nonce: String,
}

/// A credential's secret: zeroized on drop, never printed (`Debug` is redacted).
pub struct Secret(Zeroizing<Vec<u8>>);

impl Secret {
    pub fn new(bytes: Zeroizing<Vec<u8>>) -> Self {
        Self(bytes)
    }
    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// One provider.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderDescriptor {
    pub id: String,
    pub kind: ProviderKind,
    /// Canonical ([`canonical_endpoint`]).
    pub endpoint: String,
    pub auth: AuthKind,
    /// Exactly when `auth` is not `None`.
    pub credential: Option<String>,
    /// Lower first among the providers of a kind.
    pub priority: u32,
    /// The configured capacity in requests per second; `None`: not configured (the budget's
    /// `Limits` apply alone).
    pub capacity_rps: Option<u64>,
    pub plan: Plan,
    /// The plan's name as the provider publishes it (`helius-free`, `public`), for the evidence.
    pub entitlement: String,
    /// The provider evidence record that qualified this provider's binding, if any (set only by
    /// a qualification, cleared whenever the binding changes).
    pub qualification: Option<String>,
    pub enabled: bool,
}

/// A key-shaped value: 16 or more hex digits and nothing else (the form of many providers' API
/// keys) — never accepted as a name, so a key typed into a name field is never persisted.
fn key_shaped(s: &str) -> bool {
    s.len() >= 16 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// An identifier: 1–32 of `[a-z0-9_-]`, never key-shaped (a UUID-shaped key has 36 characters,
/// a hex key is refused by its form).
pub fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && !key_shaped(id)
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// A path segment that looks like a token: a word of it (split at `-`, `_`, `.`) of 16 or more
/// characters mixing letters and digits — a hyphenated name (`solana-mainnet-archive-v2`) is no
/// token. A provider that carries its credential in the path uses `AuthKind::PathSegment`. (A
/// short or digit-free token cannot be told from a name; it stays the operator's to keep out.)
fn token_like(segment: &str) -> bool {
    let has = |s: &str, f: fn(&u8) -> bool| s.as_bytes().iter().any(f);
    let word = segment.split(['-', '_', '.']).any(|word| {
        word.len() >= 16 && has(word, u8::is_ascii_digit) && has(word, u8::is_ascii_alphabetic)
    });
    // A UUID or hyphenated hex token: its characters without separators are 16+ hex digits with
    // digits and letters.
    let joined: String = segment
        .chars()
        .filter(|c| !matches!(c, '-' | '_' | '.'))
        .collect();
    let hex = joined.len() >= 16
        && joined.bytes().all(|b| b.is_ascii_hexdigit())
        && has(&joined, u8::is_ascii_digit)
        && has(&joined, u8::is_ascii_alphabetic);
    // A base64url key: 20+ characters mixing upper case, lower case and digits.
    let mixed = segment.len() >= 20
        && has(segment, u8::is_ascii_uppercase)
        && has(segment, u8::is_ascii_lowercase)
        && has(segment, u8::is_ascii_digit);
    word || hex || mixed
}

/// The canonical endpoint of `raw` for `kind`: TLS (`https`/`wss`), a plain scheme only for a
/// loopback host; no user info, no query (a credential is never part of an endpoint — its
/// parameter is the descriptor's `AuthKind`), no fragment, no token-like path segment; the host
/// lowercased, a default port and a lone trailing `/` dropped. A refusal never echoes the input.
pub fn canonical_endpoint(kind: ProviderKind, raw: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(raw.trim()).map_err(|_| "PROVIDER_URL_INVALID".to_string())?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err("PROVIDER_URL_USERINFO: credentials belong in a credential entry".into());
    }
    if url.query().is_some() {
        return Err("PROVIDER_URL_QUERY: a credential parameter is the provider's auth, never part of its endpoint".into());
    }
    if url.fragment().is_some() {
        return Err("PROVIDER_URL_FRAGMENT".into());
    }
    if url.path_segments().is_some_and(|mut s| s.any(token_like)) {
        return Err("PROVIDER_URL_PATH_SECRET: a credential in the path is the provider's auth (path), never part of its endpoint".into());
    }
    let host = url.host_str().ok_or("PROVIDER_URL_HOST")?;
    let loopback = matches!(host, "127.0.0.1" | "localhost" | "[::1]");
    let (tls, plain) = kind.schemes();
    if url.scheme() != tls && !(loopback && url.scheme() == plain) {
        return Err(format!("PROVIDER_URL_SCHEME: {} needs {tls}", kind.name()));
    }
    let text = url.as_str();
    Ok(if url.path() == "/" {
        text.trim_end_matches('/').to_string()
    } else {
        text.to_string()
    })
}

/// The domain of a provider binding (version 1).
pub const BINDING_DOMAIN: &[u8] = b"darkbot.project0.provider-binding.v1\0";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl ProviderDescriptor {
    pub fn validate(&self) -> Result<(), String> {
        if !id_ok(&self.id) {
            return Err("PROVIDER_ID_INVALID ([a-z0-9_-]{1,32})".into());
        }
        if canonical_endpoint(self.kind, &self.endpoint)? != self.endpoint {
            return Err("PROVIDER_URL_NOT_CANONICAL".into());
        }
        match (&self.auth, &self.credential) {
            (AuthKind::None, None) => {}
            (AuthKind::None, Some(_)) => return Err("PROVIDER_CREDENTIAL_WITHOUT_AUTH".into()),
            (_, None) => return Err("PROVIDER_AUTH_WITHOUT_CREDENTIAL".into()),
            (_, Some(c)) if !id_ok(c) => {
                return Err("CREDENTIAL_ID_INVALID ([a-z0-9_-]{1,32})".into())
            }
            _ => {}
        }
        if self.plan == Plan::Paid {
            return Err("PROVIDER_PAID_REFUSED: FREE_ONLY".into());
        }
        let entitlement_ok = !self.entitlement.is_empty()
            && self.entitlement.len() <= 32
            && !key_shaped(&self.entitlement)
            && self.entitlement.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.')
            });
        if !entitlement_ok {
            return Err("PROVIDER_ENTITLEMENT_INVALID ([a-z0-9_.-]{1,32})".into());
        }
        if self.capacity_rps == Some(0) {
            return Err("PROVIDER_CAPACITY_ZERO".into());
        }
        if self.qualification.as_ref().is_some_and(|q| !nonce_ok(q)) {
            return Err("PROVIDER_QUALIFICATION_INVALID".into());
        }
        Ok(())
    }

    /// Whether `other` is the same provider identity apart from its credential's revision:
    /// endpoint, authentication, credential, plan, entitlement and capacity.
    pub fn same_identity(&self, other: &Self) -> bool {
        (
            &self.id,
            self.kind,
            &self.endpoint,
            &self.auth,
            &self.credential,
            self.plan,
            &self.entitlement,
            self.capacity_rps,
        ) == (
            &other.id,
            other.kind,
            &other.endpoint,
            &other.auth,
            &other.credential,
            other.plan,
            &other.entitlement,
            other.capacity_rps,
        )
    }

    /// The canonical identity with the credential revision it uses (the one implementation;
    /// callers go through [`ProviderRegistry::binding_of`], which supplies the active revision).
    /// Identity, transport, endpoint, authentication, plan, credential revision (id, number and
    /// nonce) and configured capacity are part of it; priority, enablement and qualification are
    /// not (they select or qualify a provider, they do not change what it is). The runtime's
    /// effective limits are bound beside it by the run's configuration digest.
    fn binding_with(&self, credential: Option<&CredentialRef>) -> String {
        #[derive(Serialize)]
        struct BindingV1<'a> {
            id: &'a str,
            kind: &'a str,
            endpoint: &'a str,
            auth: String,
            plan: &'a str,
            entitlement: &'a str,
            credential: Option<(&'a str, u64, &'a str)>,
            capacity_rps: Option<u64>,
        }
        let canonical = bincode::serialize(&BindingV1 {
            id: &self.id,
            kind: self.kind.name(),
            endpoint: &self.endpoint,
            auth: self.auth.text(),
            plan: self.plan.name(),
            entitlement: &self.entitlement,
            credential: credential.map(|c| (c.id.as_str(), c.revision, c.nonce.as_str())),
            capacity_rps: self.capacity_rps,
        })
        .expect("bincode of plain data");
        let mut bytes = BINDING_DOMAIN.to_vec();
        bytes.extend_from_slice(&canonical);
        hex(&solana_sdk::hash::hash(&bytes).to_bytes())
    }

    /// The host of the endpoint, for status views (no path, no credential).
    pub fn host(&self) -> String {
        reqwest::Url::parse(&self.endpoint)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_default()
    }
}

/// The state of a credential's newest revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CredentialState {
    Active,
    Revoked,
}

/// A credential of the registry: its newest revision (revisions only grow and every one has its
/// own nonce, so a revoked one is never reused) and where it came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialMeta {
    pub id: String,
    pub revision: u64,
    pub nonce: String,
    pub state: CredentialState,
    pub created_unix_s: u64,
    /// `env:NAME` for the one-time `.env` import, `input` for a typed entry.
    pub origin: String,
}

impl CredentialMeta {
    pub fn reference(&self) -> CredentialRef {
        CredentialRef {
            id: self.id.clone(),
            revision: self.revision,
            nonce: self.nonce.clone(),
        }
    }
}

/// A 16-lowercase-hex nonce, uid or evidence record id.
pub fn nonce_ok(s: &str) -> bool {
    s.len() == 16 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// One committed version of the registry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRegistry {
    pub version: u64,
    /// The registry's random uid (16 lowercase hex digits), fixed at its first version: the
    /// namespace of its keychain items.
    pub uid: String,
    /// The canonical directory of the project the registry belongs to, fixed at its first
    /// version: a copied store (another project's directory) never decides over this one's
    /// keychain items (`p0ctl provider adopt` takes a moved project over with a new uid).
    pub project: String,
    pub providers: Vec<ProviderDescriptor>,
    /// At most one provider per kind; selection only, never a fallback.
    pub selected: Vec<(ProviderKind, String)>,
    pub credentials: Vec<CredentialMeta>,
}

impl ProviderRegistry {
    /// Version 0: no provider, no uid yet.
    pub fn empty() -> Self {
        Self {
            version: 0,
            uid: String::new(),
            project: String::new(),
            providers: vec![],
            selected: vec![],
            credentials: vec![],
        }
    }

    pub fn provider(&self, id: &str) -> Option<&ProviderDescriptor> {
        self.providers.iter().find(|p| p.id == id)
    }

    pub fn credential(&self, id: &str) -> Option<&CredentialMeta> {
        self.credentials.iter().find(|c| c.id == id)
    }

    /// The selected provider of `kind`.
    pub fn selected(&self, kind: ProviderKind) -> Option<&ProviderDescriptor> {
        self.selected
            .iter()
            .find(|(k, _)| *k == kind)
            .and_then(|(_, id)| self.provider(id))
    }

    /// The active revision a provider authenticates with (`None`: it needs none).
    pub fn credential_of(&self, p: &ProviderDescriptor) -> Result<Option<CredentialRef>, String> {
        let Some(id) = &p.credential else {
            return Ok(None);
        };
        match self.credential(id) {
            Some(c) if c.state == CredentialState::Active => Ok(Some(c.reference())),
            Some(_) => Err(format!("CREDENTIAL_REVOKED: {id}")),
            None => Err(format!("CREDENTIAL_UNKNOWN: {id}")),
        }
    }

    /// The secret-free canonical identity (hex SHA-256) of a provider with its active credential
    /// revision: the one entry point of the binding.
    pub fn binding_of(&self, p: &ProviderDescriptor) -> Result<String, String> {
        Ok(p.binding_with(self.credential_of(p)?.as_ref()))
    }

    /// Why a provider cannot be the selected one of its kind now (`None`: it can).
    pub fn selection_block(&self, p: &ProviderDescriptor) -> Option<String> {
        if !p.enabled {
            return Some(format!("PROVIDER_DISABLED: {}", p.id));
        }
        if p.plan != Plan::Free {
            return Some(format!(
                "PROVIDER_PLAN_UNPROVEN: {} (FREE_ONLY needs a free plan)",
                p.id
            ));
        }
        self.credential_of(p).err()
    }

    /// The keychain account of a credential revision of this registry.
    pub fn account(&self, credential: &CredentialRef) -> String {
        format!(
            "{}/{}@{}.{}",
            self.uid, credential.id, credential.revision, credential.nonce
        )
    }

    /// The account prefix of every item of this registry.
    pub fn account_prefix(&self) -> String {
        format!("{}/", self.uid)
    }

    /// Every structural invariant of a committed version: a uid, valid unique providers and
    /// credentials, every selection names an enabled provider of its kind once. A selected
    /// provider's credential may be revoked (a revocation is always possible); the runtime then
    /// refuses to start with it (`credential_of`), and a new selection needs an active one.
    pub fn validate(&self) -> Result<(), String> {
        if self.version > 0 && !nonce_ok(&self.uid) {
            return Err("PROVIDER_REGISTRY_UID_INVALID".into());
        }
        if self.version > 0 && self.project.is_empty() {
            return Err("PROVIDER_REGISTRY_PROJECT_MISSING".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for p in &self.providers {
            p.validate()?;
            if !ids.insert(p.id.as_str()) {
                return Err(format!("PROVIDER_DUPLICATE: {}", p.id));
            }
        }
        let mut creds = std::collections::BTreeSet::new();
        for c in &self.credentials {
            if !id_ok(&c.id) || c.revision == 0 || !nonce_ok(&c.nonce) {
                return Err(format!("CREDENTIAL_INVALID: {}", c.id));
            }
            if !creds.insert(c.id.as_str()) {
                return Err(format!("CREDENTIAL_DUPLICATE: {}", c.id));
            }
        }
        let mut kinds = std::collections::BTreeSet::new();
        for (kind, id) in &self.selected {
            if !kinds.insert(*kind) {
                return Err(format!("PROVIDER_SELECTION_DUPLICATE: {}", kind.name()));
            }
            let p = self
                .provider(id)
                .ok_or_else(|| format!("PROVIDER_SELECTION_UNKNOWN: {id}"))?;
            if p.kind != *kind {
                return Err(format!(
                    "PROVIDER_SELECTION_KIND: {id} is {}",
                    p.kind.name()
                ));
            }
            if !p.enabled {
                return Err(format!("PROVIDER_SELECTION_DISABLED: {id}"));
            }
            if let Some(c) = &p.credential {
                self.credential(c)
                    .ok_or_else(|| format!("CREDENTIAL_UNKNOWN: {c}"))?;
            }
        }
        Ok(())
    }
}

/// The store of provider credentials: one item per account (the registry's
/// [`ProviderRegistry::account`] of a revision). The secret is handed over and returned as a
/// [`Secret`]; the store never logs it.
pub trait CredentialStore {
    fn put(&self, account: &str, secret: &[u8]) -> Result<(), String>;
    fn get(&self, account: &str) -> Result<Secret, String>;
    /// A missing item is no error (deletion is idempotent).
    fn delete(&self, account: &str) -> Result<(), String>;
    /// The accounts of every item under `prefix` (their names, never their secrets).
    fn list(&self, prefix: &str) -> Result<Vec<String>, String>;
}

/// The keychain service of every Project_0 provider credential.
pub const KEYCHAIN_SERVICE: &str = "de.darkma.darkbot.project0.provider";

/// The macOS keychain (the user's login keychain): generic passwords of [`KEYCHAIN_SERVICE`].
#[cfg(target_os = "macos")]
pub struct Keychain;

/// `errSecItemNotFound`.
#[cfg(target_os = "macos")]
const ITEM_NOT_FOUND: i32 = -25300;

#[cfg(target_os = "macos")]
impl CredentialStore for Keychain {
    fn put(&self, account: &str, secret: &[u8]) -> Result<(), String> {
        security_framework::passwords::set_generic_password(KEYCHAIN_SERVICE, account, secret)
            .map_err(|e| format!("CREDENTIAL_STORE_WRITE: {}", e.code()))
    }
    fn get(&self, account: &str) -> Result<Secret, String> {
        security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, account)
            .map(|b| Secret::new(Zeroizing::new(b)))
            .map_err(|e| format!("CREDENTIAL_UNAVAILABLE: {account} ({})", e.code()))
    }
    fn delete(&self, account: &str) -> Result<(), String> {
        match security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, account) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == ITEM_NOT_FOUND => Ok(()),
            Err(e) => Err(format!("CREDENTIAL_STORE_DELETE: {}", e.code())),
        }
    }
    fn list(&self, prefix: &str) -> Result<Vec<String>, String> {
        use security_framework::item::{ItemClass, ItemSearchOptions, Limit};
        let found = ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(KEYCHAIN_SERVICE)
            .load_attributes(true)
            .limit(Limit::All)
            .search();
        let results = match found {
            Ok(r) => r,
            Err(e) if e.code() == ITEM_NOT_FOUND => return Ok(vec![]),
            Err(e) => return Err(format!("CREDENTIAL_STORE_LIST: {}", e.code())),
        };
        Ok(results
            .iter()
            .filter_map(|r| r.simplify_dict())
            .filter_map(|d| d.get("acct").cloned())
            .filter(|a| a.starts_with(prefix))
            .collect())
    }
}

/// A test store under a directory, one file per account: only in a debug build with
/// `test-support` (offline tests) — never in a release build, whatever its features, where the
/// keychain is the one store.
#[cfg(any(test, all(feature = "test-support", debug_assertions)))]
pub struct DirectoryStore(pub std::path::PathBuf);

#[cfg(any(test, all(feature = "test-support", debug_assertions)))]
impl DirectoryStore {
    fn path(&self, account: &str) -> std::path::PathBuf {
        self.0.join(account.replace('/', "%"))
    }
}

#[cfg(any(test, all(feature = "test-support", debug_assertions)))]
impl CredentialStore for DirectoryStore {
    fn put(&self, account: &str, secret: &[u8]) -> Result<(), String> {
        std::fs::create_dir_all(&self.0).map_err(|e| format!("CREDENTIAL_STORE_WRITE: {e}"))?;
        std::fs::write(self.path(account), secret)
            .map_err(|e| format!("CREDENTIAL_STORE_WRITE: {e}"))
    }
    fn get(&self, account: &str) -> Result<Secret, String> {
        std::fs::read(self.path(account))
            .map(|b| Secret::new(Zeroizing::new(b)))
            .map_err(|_| format!("CREDENTIAL_UNAVAILABLE: {account}"))
    }
    fn delete(&self, account: &str) -> Result<(), String> {
        match std::fs::remove_file(self.path(account)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("CREDENTIAL_STORE_DELETE: {e}")),
        }
    }
    fn list(&self, prefix: &str) -> Result<Vec<String>, String> {
        let entries = match std::fs::read_dir(&self.0) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(format!("CREDENTIAL_STORE_LIST: {e}")),
        };
        let mut out = vec![];
        for e in entries {
            let name = e.map_err(|e| format!("CREDENTIAL_STORE_LIST: {e}"))?;
            let account = name.file_name().to_string_lossy().replace('%', "/");
            if account.starts_with(prefix) {
                out.push(account);
            }
        }
        out.sort();
        Ok(out)
    }
}

/// `P0_TEST_CREDENTIAL_STORE_DIR` selects the test store in a debug `test-support` build only.
pub const TEST_STORE_ENV: &str = "P0_TEST_CREDENTIAL_STORE_DIR";

/// The process's credential store: the macOS keychain; without one, fail closed
/// (`CREDENTIAL_STORE_UNAVAILABLE`). A debug `test-support` build (offline tests, never a release
/// binary) uses the directory `P0_TEST_CREDENTIAL_STORE_DIR` names instead.
pub fn system_store() -> Result<Box<dyn CredentialStore>, String> {
    #[cfg(any(test, all(feature = "test-support", debug_assertions)))]
    if let Some(dir) = std::env::var_os(TEST_STORE_ENV) {
        return Ok(Box::new(DirectoryStore(dir.into())));
    }
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(Keychain))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("CREDENTIAL_STORE_UNAVAILABLE: no approved credential store on this system".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn meta(revision: u64, nonce: &str) -> CredentialMeta {
        CredentialMeta {
            id: "helius".into(),
            revision,
            nonce: nonce.into(),
            state: CredentialState::Active,
            created_unix_s: 1,
            origin: "input".into(),
        }
    }

    /// DEC-18 §1.2: an endpoint never carries a credential — user info, a query (an api-key
    /// parameter included), a fragment or a token-like path segment are refused without echoing
    /// the input; TLS outside loopback; the host lowercased, a default port and a lone trailing
    /// slash dropped.
    #[test]
    fn an_endpoint_is_canonical_and_never_carries_a_secret() {
        use ProviderKind::*;
        assert_eq!(
            canonical_endpoint(Rpc, "https://MAINNET.helius-rpc.com:443/").unwrap(),
            "https://mainnet.helius-rpc.com"
        );
        assert_eq!(
            canonical_endpoint(Jupiter, "https://api.jup.ag/swap/v2").unwrap(),
            "https://api.jup.ag/swap/v2"
        );
        assert_eq!(
            canonical_endpoint(Wss, "ws://127.0.0.1:8900").unwrap(),
            "ws://127.0.0.1:8900"
        );
        assert_eq!(
            canonical_endpoint(Rpc, "http://[::1]:8899").unwrap(),
            "http://[::1]:8899"
        );
        // A hyphenated name is no token.
        assert_eq!(
            canonical_endpoint(Rpc, "https://rpc.example/solana-mainnet-archive-v2").unwrap(),
            "https://rpc.example/solana-mainnet-archive-v2"
        );
        assert!(canonical_endpoint(Jito, "https://mainnet.block-engine.jito.wtf/api/v1").is_ok());
        for (kind, url, code) in [
            (
                Rpc,
                "https://mainnet.helius-rpc.com/?api-key=SECRET0",
                "PROVIDER_URL_QUERY",
            ),
            (
                Rpc,
                "https://user:SECRET0@rpc.example",
                "PROVIDER_URL_USERINFO",
            ),
            (Rpc, "https://rpc.example/#SECRET0", "PROVIDER_URL_FRAGMENT"),
            (
                Rpc,
                "https://solana.example/v2/SECRET0a1b2c3d4e5f6g7h8i9",
                "PROVIDER_URL_PATH_SECRET",
            ),
            (Rpc, "http://rpc.example", "PROVIDER_URL_SCHEME"),
            (Wss, "https://rpc.example", "PROVIDER_URL_SCHEME"),
            (Rpc, "not a url SECRET0", "PROVIDER_URL_INVALID"),
            (
                Rpc,
                "https://rpc.example/0123456789abcdef0123456789abcdef",
                "PROVIDER_URL_PATH_SECRET",
            ),
            (
                Rpc,
                "https://x.mainnet.rpcpool.com/7c1b4e7a-1234-4abc-9def-0123456789ab",
                "PROVIDER_URL_PATH_SECRET",
            ),
            (
                Rpc,
                "https://rpc.example/aB3-dE5_fG7-hJ9_kL2-mN4",
                "PROVIDER_URL_PATH_SECRET",
            ),
        ] {
            let e = canonical_endpoint(kind, url).unwrap_err();
            assert!(e.starts_with(code), "{url}: {e}");
            assert!(!e.contains("SECRET0"), "{e}");
        }
    }

    /// DEC-18 §1.2: the binding is the provider's identity, never a secret: a changed endpoint,
    /// plan, capacity, authentication or credential revision (number or nonce) changes it;
    /// priority, enablement and qualification do not; it is domain-separated (not the hash of
    /// the URL) and goes through the registry's active revision.
    #[test]
    fn the_binding_is_secret_free_and_changes_with_what_the_provider_is() {
        let p = helius();
        let mut r = ProviderRegistry::empty();
        r.version = 1;
        r.uid = "0123456789abcdef".into();
        r.project = "/p".into();
        r.providers.push(p.clone());
        r.credentials.push(meta(1, "aaaaaaaaaaaaaaaa"));
        let base = r.binding_of(&p).unwrap();
        assert_eq!(base.len(), 64);
        assert_ne!(
            base,
            hex(&solana_sdk::hash::hash(p.endpoint.as_bytes()).to_bytes())
        );
        let with = |m: CredentialMeta| {
            let mut s = r.clone();
            s.credentials = vec![m];
            s.binding_of(&p).unwrap()
        };
        assert_ne!(
            with(meta(2, "aaaaaaaaaaaaaaaa")),
            base,
            "a rotated revision"
        );
        assert_ne!(
            with(meta(1, "bbbbbbbbbbbbbbbb")),
            base,
            "a wiped registry's revision 1"
        );
        let changed = |f: &dyn Fn(&mut ProviderDescriptor)| {
            let mut q = p.clone();
            f(&mut q);
            r.binding_of(&q).unwrap()
        };
        assert_ne!(
            changed(&|q| q.endpoint = "https://rpc.example".into()),
            base
        );
        assert_ne!(changed(&|q| q.plan = Plan::Unknown), base);
        assert_ne!(changed(&|q| q.capacity_rps = Some(5)), base);
        assert_ne!(
            changed(&|q| q.auth = AuthKind::Header("x-token".into())),
            base
        );
        assert_eq!(changed(&|q| q.priority = 9), base);
        assert_eq!(changed(&|q| q.enabled = false), base);
        assert_eq!(
            changed(&|q| q.qualification = Some("0123456789abcdef".into())),
            base
        );
        let mut revoked = r.clone();
        revoked.credentials[0].state = CredentialState::Revoked;
        assert!(revoked
            .binding_of(&p)
            .unwrap_err()
            .starts_with("CREDENTIAL_REVOKED"));
    }

    /// DEC-18: a descriptor and a registry version are valid only whole: credential exactly with
    /// an authentication, FREE_ONLY, identifiers that are no key, one enabled provider of its kind
    /// per selection; a revocation stays possible while selected; a selection needs a free plan
    /// and an active credential.
    #[test]
    fn a_registry_version_is_valid_only_whole() {
        let mut p = helius();
        assert!(p.validate().is_ok());
        p.plan = Plan::Paid;
        assert!(p
            .validate()
            .unwrap_err()
            .starts_with("PROVIDER_PAID_REFUSED"));
        let mut p = helius();
        p.credential = None;
        assert_eq!(
            p.validate().unwrap_err(),
            "PROVIDER_AUTH_WITHOUT_CREDENTIAL"
        );
        let mut p = helius();
        p.credential = Some("0b6c2d4e-1f3a-4b5c-8d9e-0a1b2c3d4e5f".into());
        let e = p.validate().unwrap_err();
        assert!(
            e.starts_with("CREDENTIAL_ID_INVALID") && !e.contains("0b6c2d4e"),
            "{e}"
        );
        let mut p = helius();
        p.credential = Some("0123456789abcdef0123456789abcdef".into());
        assert!(
            p.validate()
                .unwrap_err()
                .starts_with("CREDENTIAL_ID_INVALID"),
            "a hex key is no id"
        );
        assert!(AuthKind::parse("query:0123456789abcdef0123456789abcdef").is_err());
        assert!(AuthKind::parse("header:7c1b4e7a-1234-4abc-9def-0123456789ab").is_err());
        assert_eq!(
            AuthKind::parse("header:x-api-key").unwrap(),
            AuthKind::Header("x-api-key".into())
        );
        let mut p = helius();
        p.entitlement = "0123456789abcdef".into();
        assert!(p
            .validate()
            .unwrap_err()
            .starts_with("PROVIDER_ENTITLEMENT_INVALID"));
        let mut p = helius();
        p.endpoint = "https://mainnet.helius-rpc.com/".into();
        assert_eq!(p.validate().unwrap_err(), "PROVIDER_URL_NOT_CANONICAL");
        let mut r = ProviderRegistry::empty();
        r.version = 1;
        r.uid = "0123456789abcdef".into();
        r.project = "/p".into();
        r.providers.push(helius());
        r.selected.push((ProviderKind::Rpc, "helius-rpc".into()));
        assert!(r.validate().unwrap_err().starts_with("CREDENTIAL_UNKNOWN"));
        r.credentials.push(meta(3, "aaaaaaaaaaaaaaaa"));
        assert!(r.validate().is_ok());
        assert_eq!(r.selection_block(&r.providers[0]), None);
        r.credentials[0].state = CredentialState::Revoked;
        assert!(
            r.validate().is_ok(),
            "a revocation stays possible while selected"
        );
        assert!(r
            .selection_block(&r.providers[0])
            .unwrap()
            .starts_with("CREDENTIAL_REVOKED"));
        r.credentials[0].state = CredentialState::Active;
        r.providers[0].plan = Plan::Unknown;
        assert!(r
            .selection_block(&r.providers[0])
            .unwrap()
            .starts_with("PROVIDER_PLAN_UNPROVEN"));
        r.providers[0].plan = Plan::Free;
        r.selected.push((ProviderKind::Wss, "helius-rpc".into()));
        assert!(r
            .validate()
            .unwrap_err()
            .starts_with("PROVIDER_SELECTION_KIND"));
        r.selected.pop();
        r.providers[0].enabled = false;
        assert!(r
            .validate()
            .unwrap_err()
            .starts_with("PROVIDER_SELECTION_DISABLED"));
        let mut bad = r.clone();
        bad.uid = String::new();
        assert_eq!(bad.validate().unwrap_err(), "PROVIDER_REGISTRY_UID_INVALID");
        let mut bad = r.clone();
        bad.project = String::new();
        assert_eq!(
            bad.validate().unwrap_err(),
            "PROVIDER_REGISTRY_PROJECT_MISSING"
        );
    }

    /// The store keeps one item per account; a missing one is unavailable (fail closed), a
    /// deletion idempotent; a listing names accounts under a prefix only; a secret never prints.
    #[test]
    fn a_credential_revision_is_its_own_item() {
        let dir = std::env::temp_dir().join(format!("p0_cred_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = DirectoryStore(dir.clone());
        store.put("reg1/helius@1.aaaa", b"one").unwrap();
        store.put("reg2/helius@1.bbbb", b"other").unwrap();
        let secret = store.get("reg1/helius@1.aaaa").unwrap();
        assert_eq!(secret.expose(), b"one");
        assert_eq!(format!("{secret:?}"), "Secret(<redacted>)");
        assert!(store
            .get("reg1/helius@2.aaaa")
            .unwrap_err()
            .starts_with("CREDENTIAL_UNAVAILABLE"));
        assert_eq!(
            store.list("reg1/").unwrap(),
            vec!["reg1/helius@1.aaaa".to_string()]
        );
        store.delete("reg1/helius@1.aaaa").unwrap();
        store.delete("reg1/helius@1.aaaa").unwrap();
        assert!(store.get("reg1/helius@1.aaaa").is_err());
        assert_eq!(
            store.list("reg2/").unwrap().len(),
            1,
            "another registry untouched"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
