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
        && !id.starts_with('-')
        && !key_shaped(id)
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// A path segment that looks like a token, by three rules: a word of it (split at `-`, `_`, `.`)
/// of 16 or more characters mixing letters and digits; its characters without separators being
/// 16 or more hex digits mixing digits and letters (a UUID, hyphenated hex); 20 or more
/// characters mixing upper case, lower case and digits (a base64url key). A hyphenated name
/// (`solana-mainnet-archive-v2`) is no token. A provider that carries its credential in the path
/// uses `AuthKind::PathSegment`. Limits: a short token (fewer than 16 characters), a digit-free
/// one, or a lowercase hyphenated non-hex one cannot be told from a name — keeping such a token
/// out of an endpoint stays the operator's.
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
    let loopback = loopback_host(host);
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
            && !self.entitlement.starts_with('-')
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
        // A stored authentication name passes the same rule as an entered one.
        if AuthKind::parse(&self.auth.text())? != self.auth {
            return Err("PROVIDER_AUTH_NAME ([A-Za-z0-9_-]{1,64})".into());
        }
        // A Geyser provider is a Yellowstone gRPC source: none or the protocol's token header.
        let yellowstone = crate::ingress::sources::YELLOWSTONE_TOKEN_HEADER;
        if self.kind == ProviderKind::Geyser
            && !matches!(&self.auth, AuthKind::None)
            && self.auth != AuthKind::Header(yellowstone.into())
        {
            return Err(format!(
                "PROVIDER_AUTH_UNSUPPORTED: a Geyser provider authenticates by header:{yellowstone} or none"
            ));
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

    /// The providers of `kind` a run uses: the selected one, then — only with `fallback` — every
    /// other enabled provider of the kind that could be selected, by priority (then id). No
    /// selection: none (never an implicit choice). A selected provider that cannot be selected
    /// now (disabled, unproven plan, revoked credential) refuses the run.
    pub fn run_providers(
        &self,
        kind: ProviderKind,
        fallback: bool,
    ) -> Result<Vec<&ProviderDescriptor>, String> {
        let Some(selected) = self.selected(kind) else {
            return Ok(vec![]);
        };
        if let Some(block) = self.selection_block(selected) {
            return Err(block);
        }
        let mut out = vec![selected];
        if fallback {
            let mut others: Vec<&ProviderDescriptor> = self
                .providers
                .iter()
                .filter(|p| p.kind == kind && p.id != selected.id)
                .filter(|p| self.selection_block(p).is_none())
                .collect();
            others.sort_by(|a, b| (a.priority, &a.id).cmp(&(b.priority, &b.id)));
            out.extend(others);
        }
        Ok(out)
    }

    /// A provider of this version as a run binds it: its active credential revision and binding.
    pub fn bound(&self, p: &ProviderDescriptor) -> Result<BoundProvider, String> {
        let credential = self.credential_of(p)?;
        Ok(BoundProvider {
            kind: p.kind,
            id: p.id.clone(),
            endpoint: p.endpoint.clone(),
            auth: p.auth.clone(),
            binding: p.binding_with(credential.as_ref()),
            credential,
            plan: p.plan,
            capacity_rps: p.capacity_rps,
            qualification: p.qualification.clone(),
            registry_version: self.version,
        })
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
        let mut endpoints = std::collections::BTreeSet::new();
        for p in &self.providers {
            p.validate()?;
            if !ids.insert(p.id.as_str()) {
                return Err(format!("PROVIDER_DUPLICATE: {}", p.id));
            }
            // One provider per endpoint and kind: a run's endpoint names exactly one provider,
            // and a second credential on the same endpoint is no way around its plan (§4.2).
            if !endpoints.insert((p.kind, p.endpoint.as_str())) {
                return Err(format!("PROVIDER_ENDPOINT_DUPLICATE: {}", p.id));
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
            // The Geyser source unit names its provider (`source-geyser-provider`, S1-02): a
            // selection of the kind would be a second authority.
            if *kind == ProviderKind::Geyser {
                return Err("PROVIDER_SELECTION_GEYSER: the Geyser source unit names its provider (source-geyser-provider)".into());
            }
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

/// The loopback hosts (a local node, an offline test): the one list of the endpoint rules.
fn loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

/// Whether `endpoint` (canonical) names a loopback host: a local node or an offline test.
pub fn is_local(endpoint: &str) -> bool {
    reqwest::Url::parse(endpoint)
        .ok()
        .and_then(|u| u.host_str().map(loopback_host))
        .unwrap_or(false)
}

/// The id of a local endpoint's provider ([`BoundProvider::local`]).
pub const LOCAL_PROVIDER_ID: &str = "local";

/// The version of the registry contract's default profiles ([`default_profile`]).
pub const DEFAULT_PROFILES_VERSION: u32 = 1;

/// The registry contract's default profile of `kind` (DEC-18 §1.3.2: a validated, versioned
/// default, never a constructor's constant): the provider a run of that kind uses while the
/// registry selects none of it. Version 1: Jupiter's anonymous free Swap API (no credential: the
/// free tier, P6-MT.7's unauthenticated bucket). RPC, WSS, Geyser and Jito have none — their
/// provider is the operator's selection (no implicit choice).
pub fn default_profile(kind: ProviderKind) -> Option<ProviderDescriptor> {
    match kind {
        ProviderKind::Jupiter => Some(ProviderDescriptor {
            id: "jupiter-default".into(),
            kind,
            endpoint: "https://api.jup.ag/swap/v2".into(),
            auth: AuthKind::None,
            credential: None,
            priority: 0,
            capacity_rps: None,
            plan: Plan::Free,
            entitlement: "jupiter-anonymous".into(),
            qualification: None,
            enabled: true,
        }),
        ProviderKind::Rpc | ProviderKind::Wss | ProviderKind::Geyser | ProviderKind::Jito => None,
    }
}

/// A provider as a run uses it (DEC-18): a registry provider with the credential revision
/// active when the run was configured and its binding — or a loopback endpoint the operator
/// named (a local node, an offline test; no credential, plan unknown, never qualified). A remote
/// endpoint is never used outside the registry. Part of the run's configuration, so its digest
/// (the audit's and the run's identity) changes with every provider, endpoint, plan, capacity or
/// credential revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundProvider {
    pub kind: ProviderKind,
    pub id: String,
    pub endpoint: String,
    pub auth: AuthKind,
    pub credential: Option<CredentialRef>,
    pub plan: Plan,
    pub capacity_rps: Option<u64>,
    /// The provider evidence that qualified this binding in the registry, if any. Not part of
    /// the configuration's digest (an audit binds its evidence itself).
    #[serde(skip)]
    pub qualification: Option<String>,
    /// The registry version it was bound from; 0 for a local endpoint or a default profile. Not
    /// part of the configuration's digest (review L5: a registry change that leaves a provider's
    /// binding as it was — another provider, a priority — invalidates no audit of this one).
    #[serde(skip)]
    pub registry_version: u64,
    /// [`ProviderRegistry::binding_of`]: the secret-free identity audits, evidence and run
    /// manifests bind.
    pub binding: String,
}

impl BoundProvider {
    /// A loopback endpoint the operator named; a remote endpoint is refused (it is a registry
    /// provider, `p0ctl provider add` and `select`). A refusal never echoes the endpoint.
    pub fn local(kind: ProviderKind, raw: &str) -> Result<Self, String> {
        let endpoint = canonical_endpoint(kind, raw)?;
        if !is_local(&endpoint) {
            return Err(format!(
                "PROVIDER_ENDPOINT_UNREGISTERED: a remote {} endpoint is a provider of the registry (p0ctl provider add, select)",
                kind.name()
            ));
        }
        let descriptor = ProviderDescriptor {
            id: LOCAL_PROVIDER_ID.into(),
            kind,
            endpoint,
            auth: AuthKind::None,
            credential: None,
            priority: 0,
            capacity_rps: None,
            plan: Plan::Unknown,
            entitlement: LOCAL_PROVIDER_ID.into(),
            qualification: None,
            enabled: true,
        };
        Ok(Self {
            kind,
            binding: descriptor.binding_with(None),
            id: descriptor.id,
            endpoint: descriptor.endpoint,
            auth: AuthKind::None,
            credential: None,
            plan: Plan::Unknown,
            capacity_rps: None,
            qualification: None,
            registry_version: 0,
        })
    }

    /// The contract's default profile of `kind`, bound ([`default_profile`]).
    pub fn default_of(kind: ProviderKind) -> Option<Self> {
        let p = default_profile(kind)?;
        Some(Self {
            kind,
            binding: p.binding_with(None),
            id: p.id,
            endpoint: p.endpoint,
            auth: p.auth,
            credential: None,
            plan: p.plan,
            capacity_rps: p.capacity_rps,
            qualification: None,
            registry_version: 0,
        })
    }

    /// The structural rule of a bound provider: a canonical endpoint; outside the registry only a
    /// local one on loopback or exactly the contract's default profile; a credential exactly
    /// when it authenticates.
    pub fn validate(&self) -> Result<(), String> {
        if canonical_endpoint(self.kind, &self.endpoint)? != self.endpoint {
            return Err("PROVIDER_URL_NOT_CANONICAL".into());
        }
        if self.registry_version == 0 {
            let local = self.id == LOCAL_PROVIDER_ID && is_local(&self.endpoint);
            if !local && Self::default_of(self.kind).as_ref() != Some(self) {
                return Err(format!(
                    "PROVIDER_ENDPOINT_UNREGISTERED: a remote {} endpoint is a provider of the registry (p0ctl provider add, select)",
                    self.kind.name()
                ));
            }
        } else if !id_ok(&self.id) {
            return Err("PROVIDER_ID_INVALID ([a-z0-9_-]{1,32})".into());
        }
        if matches!(self.auth, AuthKind::None) != self.credential.is_none() {
            return Err("PROVIDER_AUTH_WITHOUT_CREDENTIAL".into());
        }
        if self.binding.len() != 64 || !self.binding.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("PROVIDER_BINDING_INVALID".into());
        }
        Ok(())
    }
}

/// A bound provider as an effective configuration and a run manifest record it: secret-free and
/// without its endpoint (the binding binds it).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderBinding {
    pub kind: ProviderKind,
    pub id: String,
    pub binding: String,
    pub registry_version: u64,
    /// The credential's id and revision (its nonce is part of the binding).
    pub credential: Option<(String, u64)>,
    pub plan: Plan,
}

impl BoundProvider {
    pub fn summary(&self) -> ProviderBinding {
        ProviderBinding {
            kind: self.kind,
            id: self.id.clone(),
            binding: self.binding.clone(),
            registry_version: self.registry_version,
            credential: self.credential.as_ref().map(|c| (c.id.clone(), c.revision)),
            plan: self.plan,
        }
    }
}

/// One provider credential as the transport applies it (DEC-18 §1.3.6): the canonical endpoint
/// it authorizes, how, and the secret — read from the credential store once at the run's start,
/// held in memory only, zeroized on drop, never printed.
pub struct ActiveCredential {
    pub endpoint: String,
    pub auth: AuthKind,
    pub secret: Secret,
}

impl std::fmt::Debug for ActiveCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ActiveCredential({} {})",
            self.endpoint,
            self.auth.text()
        )
    }
}

static ACTIVE_CREDENTIALS: std::sync::OnceLock<Vec<ActiveCredential>> = std::sync::OnceLock::new();

/// Installs the run's credentials, once, before any transport is used: a run never changes its
/// credentials (DEC-05; a rotation is effective at the next controlled start). Every entry
/// authenticates (`AuthKind` not `None`), one per endpoint, its secret applicable as its
/// authentication needs ([`authorize_with`]). Credentials are keyed by endpoint: two
/// authenticating providers of one endpoint (of different kinds) refuse the start
/// (`CREDENTIAL_ENDPOINT_DUPLICATE`, fail closed).
pub fn activate_credentials(credentials: Vec<ActiveCredential>) -> Result<(), String> {
    let mut endpoints = std::collections::BTreeSet::new();
    for c in &credentials {
        if matches!(c.auth, AuthKind::None) {
            return Err("CREDENTIAL_WITHOUT_AUTH".into());
        }
        if !endpoints.insert(c.endpoint.as_str()) {
            return Err("CREDENTIAL_ENDPOINT_DUPLICATE".into());
        }
        authorize_with(&credentials, &c.endpoint, "")?;
    }
    ACTIVE_CREDENTIALS
        .set(credentials)
        .map_err(|_| "CREDENTIALS_ALREADY_ACTIVE: a run's credentials are installed once".into())
}

/// A request target with its credential applied — the URL (with the credential in its query or
/// path) and an authentication header. Zeroized on drop, never printed (`Debug` is redacted).
pub struct Authorized {
    url: Zeroizing<String>,
    header: Option<(String, Zeroizing<String>)>,
}

impl Authorized {
    pub fn url(&self) -> &str {
        &self.url
    }
    /// The authentication header (name, value), if the provider authenticates by one.
    pub fn header(&self) -> Option<(&str, &str)> {
        self.header.as_ref().map(|(n, v)| (n.as_str(), v.as_str()))
    }
}

impl std::fmt::Debug for Authorized {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Authorized(<redacted>)")
    }
}

/// RFC 3986 percent-encoding of everything but the unreserved characters.
fn percent_encoded(bytes: &[u8]) -> Zeroizing<String> {
    let mut out = Zeroizing::new(String::with_capacity(bytes.len() * 3));
    for &b in bytes {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `endpoint` + `suffix` (a sub-path of the endpoint, e.g. an API's `/build`; `""` for the
/// endpoint itself) with the credential of `credentials` for the endpoint applied — the path
/// segment between endpoint and suffix, the query parameter after both; an endpoint without one
/// is unchanged. A header value must be a valid HTTP header value (visible ASCII and spaces).
pub fn authorize_with(
    credentials: &[ActiveCredential],
    endpoint: &str,
    suffix: &str,
) -> Result<Authorized, String> {
    // One `/` between a path and the sub-path.
    let join = |url: &mut Zeroizing<String>| {
        if url.ends_with('/') && suffix.starts_with('/') {
            url.push_str(&suffix[1..]);
        } else {
            url.push_str(suffix);
        }
    };
    let mut url = Zeroizing::new(endpoint.to_string());
    // An endpoint without a path (its canonical form drops a lone `/`) is the root path `/`: a
    // request target in origin form (`GET /?api-key=…`, never `GET ?api-key=…`).
    if !url
        .split_once("://")
        .is_some_and(|(_, rest)| rest.contains('/'))
    {
        url.push('/');
    }
    let Some(c) = credentials.iter().find(|c| c.endpoint == endpoint) else {
        join(&mut url);
        return Ok(Authorized { url, header: None });
    };
    let secret = c.secret.expose();
    let mut header = None;
    match &c.auth {
        AuthKind::None => return Err("CREDENTIAL_WITHOUT_AUTH".into()),
        AuthKind::Query(name) => {
            join(&mut url);
            let separator = if url.contains('?') { '&' } else { '?' };
            url.push(separator);
            url.push_str(name);
            url.push('=');
            url.push_str(&percent_encoded(secret));
        }
        AuthKind::PathSegment => {
            if !url.ends_with('/') {
                url.push('/');
            }
            url.push_str(&percent_encoded(secret));
            join(&mut url);
        }
        AuthKind::Header(name) => {
            let value = std::str::from_utf8(secret)
                .ok()
                .filter(|v| v.bytes().all(|b| b == b' ' || b.is_ascii_graphic()))
                .ok_or("CREDENTIAL_HEADER_FORMAT: a header credential is visible ASCII")?;
            header = Some((name.clone(), Zeroizing::new(value.to_string())));
            join(&mut url);
        }
    }
    Ok(Authorized { url, header })
}

/// `endpoint` as its transport sends to it: with the run's credential for it, if any
/// ([`activate_credentials`]). The credential is applied here, after every comparison of
/// endpoints (fallback, budget drift), so no compared, logged or persisted URL ever holds it.
pub fn authorize(endpoint: &str) -> Result<Authorized, String> {
    authorize_path(endpoint, "")
}

/// [`authorize`] of the sub-path `suffix` of `endpoint` ([`authorize_with`]).
pub fn authorize_path(endpoint: &str, suffix: &str) -> Result<Authorized, String> {
    authorize_with(
        ACTIVE_CREDENTIALS.get().map_or(&[][..], Vec::as_slice),
        endpoint,
        suffix,
    )
}

/// The WebSocket handshake request of `endpoint` with the run's credential for it applied
/// ([`authorize`]): in the URL, or as a header marked sensitive. A failure is a fixed code.
pub fn ws_client_request(
    endpoint: &str,
) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request, &'static str> {
    use tokio_tungstenite::tungstenite::{
        client::IntoClientRequest,
        http::{HeaderName, HeaderValue},
    };
    let target = authorize(endpoint).map_err(|_| "WSS_CREDENTIAL")?;
    let mut request = target
        .url()
        .into_client_request()
        .map_err(|_| "WSS_ENDPOINT_INVALID")?;
    if let Some((name, value)) = target.header() {
        let mut v = HeaderValue::from_str(value).map_err(|_| "WSS_CREDENTIAL")?;
        v.set_sensitive(true);
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| "WSS_CREDENTIAL")?;
        request.headers_mut().insert(name, v);
    }
    Ok(request)
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

    /// DEC-18 package-4 review F9: an identifier or an entitlement never starts with `-`, so the
    /// controller never reads it as an option (`remove --id --project`).
    #[test]
    fn an_identifier_never_reads_as_an_option() {
        for bad in ["--project", "-x", "-"] {
            assert!(!id_ok(bad), "{bad}");
        }
        let mut p = helius();
        p.entitlement = "-free".into();
        assert!(p.validate().is_err());
        let mut p = helius();
        p.credential = Some("--project".into());
        assert!(p.validate().is_err());
        assert!(helius().validate().is_ok());
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
        // A Geyser provider authenticates by the Yellowstone token header or not at all; the
        // Geyser source unit names its provider, so the registry selects none of the kind.
        let mut geyser = helius();
        (geyser.id, geyser.kind) = ("geyser".into(), ProviderKind::Geyser);
        geyser.auth = AuthKind::Query("token".into());
        assert!(geyser
            .validate()
            .unwrap_err()
            .starts_with("PROVIDER_AUTH_UNSUPPORTED"));
        geyser.auth = AuthKind::Header("x-token".into());
        assert!(geyser.validate().is_ok());
        let mut selects_geyser = r.clone();
        selects_geyser.providers[0].enabled = true;
        selects_geyser.providers.push(geyser);
        selects_geyser
            .selected
            .push((ProviderKind::Geyser, "geyser".into()));
        assert!(selects_geyser
            .validate()
            .unwrap_err()
            .starts_with("PROVIDER_SELECTION_GEYSER"));
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

    /// DEC-18 §1.3.6: the transport applies a credential only to its own endpoint — percent-
    /// encoded into the query parameter or the last path segment, or as the header — and
    /// nowhere else; an endpoint without one is unchanged; nothing of it prints.
    #[test]
    fn a_credential_is_applied_by_its_endpoint_s_authentication_only() {
        let secret = |s: &str| Secret::new(Zeroizing::new(s.as_bytes().to_vec()));
        let credentials = vec![
            ActiveCredential {
                endpoint: "https://mainnet.helius-rpc.com".into(),
                auth: AuthKind::Query("api-key".into()),
                secret: secret("K&Y/1 2"),
            },
            ActiveCredential {
                endpoint: "https://rpc.example/v2".into(),
                auth: AuthKind::PathSegment,
                secret: secret("SECRET0"),
            },
            ActiveCredential {
                endpoint: "https://grpc.example".into(),
                auth: AuthKind::Header("x-token".into()),
                secret: secret("SECRET1"),
            },
        ];
        let q = authorize_with(&credentials, "https://mainnet.helius-rpc.com", "").unwrap();
        assert_eq!(
            q.url(),
            "https://mainnet.helius-rpc.com/?api-key=K%26Y%2F1%202"
        );
        assert_eq!(q.header(), None);
        let p = authorize_with(&credentials, "https://rpc.example/v2", "").unwrap();
        assert_eq!(p.url(), "https://rpc.example/v2/SECRET0");
        let h = authorize_with(&credentials, "https://grpc.example", "").unwrap();
        assert_eq!(
            (h.url(), h.header()),
            ("https://grpc.example/", Some(("x-token", "SECRET1")))
        );
        // A sub-path: the path segment before it, the query parameter after it.
        assert_eq!(
            authorize_with(&credentials, "https://mainnet.helius-rpc.com", "/build?a=1")
                .unwrap()
                .url(),
            "https://mainnet.helius-rpc.com/build?a=1&api-key=K%26Y%2F1%202"
        );
        assert_eq!(
            authorize_with(&credentials, "https://rpc.example/v2", "/build")
                .unwrap()
                .url(),
            "https://rpc.example/v2/SECRET0/build"
        );
        assert_eq!(
            authorize_with(&credentials, "https://grpc.example", "/x")
                .unwrap()
                .url(),
            "https://grpc.example/x"
        );
        assert_eq!(
            authorize_with(&credentials, "https://other.example/", "/api")
                .unwrap()
                .url(),
            "https://other.example/api"
        );
        let other = authorize_with(&credentials, "https://other.example", "").unwrap();
        assert_eq!(
            (other.url(), other.header()),
            ("https://other.example/", None)
        );
        for shown in [
            format!("{q:?}"),
            format!("{h:?}"),
            format!("{credentials:?}"),
        ] {
            assert!(
                !shown.contains("SECRET") && !shown.contains("K&Y"),
                "{shown}"
            );
        }
        let bad = vec![ActiveCredential {
            endpoint: "https://grpc.example".into(),
            auth: AuthKind::Header("x-token".into()),
            secret: secret("ä"),
        }];
        assert!(authorize_with(&bad, "https://grpc.example", "")
            .unwrap_err()
            .starts_with("CREDENTIAL_HEADER_FORMAT"));
    }

    /// DEC-18 (review H2): the WSS handshake of a provider authenticating by a query parameter at
    /// a root endpoint carries its credential in an origin-form request target (`/?api-key=…`;
    /// before: `?api-key=…`, refused by a server) — through the run's installed credentials, the
    /// WSS transport's request builder and a real handshake. The one credential installation of
    /// this test binary.
    #[test]
    fn a_wss_query_credential_is_an_origin_form_request_target() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("ws://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (socket, _) = listener.accept().await.unwrap();
                let mut target = String::new();
                let _ws =
                    tokio_tungstenite::accept_hdr_async(socket, |r: &Request, w: Response| {
                        target = r.uri().to_string();
                        Ok(w)
                    })
                    .await
                    .unwrap();
                target
            });
            activate_credentials(vec![
                ActiveCredential {
                    endpoint: endpoint.clone(),
                    auth: AuthKind::Query("api-key".into()),
                    secret: Secret::new(Zeroizing::new(b"WSS-SECRET".to_vec())),
                },
                ActiveCredential {
                    endpoint: "https://solana.publicnode.com".into(),
                    auth: AuthKind::Header("x-token".into()),
                    secret: Secret::new(Zeroizing::new(b"GEYSER-SECRET".to_vec())),
                },
            ])
            .unwrap();
            // The Geyser source's token: the installed credential of its bound provider (never
            // an environment variable); a provider without authentication has none.
            let mut geyser = crate::ingress::sources::ProviderConfig::yellowstone(
                &crate::ingress::sources::publicnode_bound(),
            )
            .unwrap();
            assert_eq!(
                crate::ingress::sources::installed_token(&geyser).as_deref(),
                Some("GEYSER-SECRET")
            );
            geyser.auth = None;
            assert_eq!(crate::ingress::sources::installed_token(&geyser), None);
            let request = ws_client_request(&endpoint).unwrap();
            assert_eq!(
                request.uri().path_and_query().unwrap().as_str(),
                "/?api-key=WSS-SECRET"
            );
            let _client = tokio_tungstenite::connect_async(request).await.unwrap();
            assert_eq!(server.await.unwrap(), "/?api-key=WSS-SECRET");
            // An endpoint without a credential stays as it is (its root path).
            let other = ws_client_request("ws://127.0.0.1:9").unwrap();
            assert_eq!(other.uri().path_and_query().unwrap().as_str(), "/");
        });
    }

    /// DEC-18: a named endpoint is a local provider only on loopback (a remote one is the
    /// registry's); a bound provider validates whole; one provider per endpoint and kind; a run
    /// takes the selection, with fallback the selectable others by priority, never an implicit
    /// choice.
    #[test]
    fn a_run_binds_the_selection_or_a_local_endpoint() {
        let local = BoundProvider::local(ProviderKind::Rpc, "http://127.0.0.1:8899/").unwrap();
        assert_eq!(
            (
                local.id.as_str(),
                local.endpoint.as_str(),
                local.registry_version
            ),
            ("local", "http://127.0.0.1:8899", 0)
        );
        assert!(local.validate().is_ok());
        for remote in ["https://rpc.example", "https://mainnet.helius-rpc.com"] {
            assert!(BoundProvider::local(ProviderKind::Rpc, remote)
                .unwrap_err()
                .starts_with("PROVIDER_ENDPOINT_UNREGISTERED"));
        }
        let mut forged = local.clone();
        forged.endpoint = "https://rpc.example".into();
        assert!(forged
            .validate()
            .unwrap_err()
            .starts_with("PROVIDER_ENDPOINT_UNREGISTERED"));
        let mut r = ProviderRegistry::empty();
        r.version = 2;
        r.uid = "0123456789abcdef".into();
        r.project = "/p".into();
        r.credentials.push(meta(1, "aaaaaaaaaaaaaaaa"));
        let mut a = helius();
        let mut b = helius();
        b.id = "backup".into();
        b.endpoint = "https://backup.example".into();
        b.priority = 7;
        let mut c = b.clone();
        c.id = "alpha".into();
        c.endpoint = "https://alpha.example".into();
        c.priority = 3;
        let mut off = b.clone();
        off.id = "off".into();
        off.endpoint = "https://off.example".into();
        off.enabled = false;
        a.priority = 9;
        r.providers = vec![a, b, c, off];
        assert!(r.run_providers(ProviderKind::Rpc, true).unwrap().is_empty());
        r.selected.push((ProviderKind::Rpc, "helius-rpc".into()));
        let ids = |fallback| {
            r.run_providers(ProviderKind::Rpc, fallback)
                .unwrap()
                .iter()
                .map(|p| p.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(false), ["helius-rpc"]);
        assert_eq!(ids(true), ["helius-rpc", "alpha", "backup"]);
        let bound = r.bound(&r.providers[0]).unwrap();
        assert_eq!(bound.binding, r.binding_of(&r.providers[0]).unwrap());
        assert_eq!(bound.registry_version, 2);
        assert!(bound.validate().is_ok());
        assert_eq!(bound.summary().credential, Some(("helius".into(), 1)));
        let mut dup = r.clone();
        dup.providers[1].endpoint = dup.providers[0].endpoint.clone();
        assert!(dup
            .validate()
            .unwrap_err()
            .starts_with("PROVIDER_ENDPOINT_DUPLICATE"));
        r.credentials[0].state = CredentialState::Revoked;
        assert!(r
            .run_providers(ProviderKind::Rpc, false)
            .unwrap_err()
            .starts_with("CREDENTIAL_REVOKED"));
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
