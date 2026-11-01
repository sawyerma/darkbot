//! DEC-18 (Nutzerentscheidung 01.11.2026): the typed provider descriptor of the one
//! configuration authority (DEC-05).
//!
//! Every provider — RPC, WSS, Geyser, Jupiter, Jito — is an id, a kind and a **canonical
//! endpoint without user info, query or fragment**, how it authenticates (none, a query
//! parameter or a header: the parameter's or header's *name*, never its value), a credential
//! reference (id and revision; the secret lives in the macOS keychain, never in a record), its
//! plan (FREE_ONLY: a paid plan is refused), its configured capacity, its priority and the
//! provider evidence that qualified it. [`ProviderDescriptor::binding`] is the secret-free
//! canonical identity an audit, a provider evidence and a run manifest bind instead of a hash of
//! the URL. A [`ProviderRegistry`] is one committed version of every descriptor, the selection
//! per kind and the credentials' revisions; the runtime takes the version current at its start
//! (a change is staged and effective at the next controlled start, DEC-05).
use serde::{Deserialize, Serialize};
pub use zeroize::Zeroizing;

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
    pub fn parse(s: &str) -> Result<Self, String> {
        PROVIDER_KINDS
            .into_iter()
            .find(|k| k.name() == s)
            .ok_or_else(|| format!("PROVIDER_KIND_UNKNOWN: {s} (rpc|wss|geyser|jupiter|jito)"))
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
/// credential; never its value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthKind {
    None,
    Query(String),
    Header(String),
}

impl AuthKind {
    pub fn text(&self) -> String {
        match self {
            Self::None => "none".into(),
            Self::Query(n) => format!("query:{n}"),
            Self::Header(n) => format!("header:{n}"),
        }
    }
    pub fn parse(s: &str) -> Result<Self, String> {
        let name = |n: &str| -> Result<String, String> {
            let ok = !n.is_empty()
                && n.len() <= 64
                && n.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
            ok.then(|| n.to_string())
                .ok_or_else(|| format!("PROVIDER_AUTH_NAME: {n}"))
        };
        match s.split_once(':') {
            None if s == "none" => Ok(Self::None),
            Some(("query", n)) => Ok(Self::Query(name(n)?)),
            Some(("header", n)) => Ok(Self::Header(name(n)?)),
            _ => Err(format!(
                "PROVIDER_AUTH_UNKNOWN: {s} (none|query:NAME|header:NAME)"
            )),
        }
    }
}

/// FREE_ONLY: a provider's plan as it is evidenced; `Paid` is refused.
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
            _ => Err(format!("PROVIDER_PLAN_UNKNOWN: {s} (free|unknown)")),
        }
    }
}

/// A credential of the registry, by id and revision: the secret is the keychain's item of
/// exactly that revision.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CredentialRef {
    pub id: String,
    pub revision: u64,
}

/// One provider.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderDescriptor {
    pub id: String,
    pub kind: ProviderKind,
    /// Canonical ([`canonical_endpoint`]): no user info, no query, no fragment.
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
    /// The provider evidence record that qualified it (`p0ctl provider measure`), if any.
    pub qualification: Option<String>,
    pub enabled: bool,
}

fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// The canonical endpoint of `raw` for `kind`: TLS (`https`/`wss`), a plain scheme only for a
/// loopback host; no user info, no query (a credential is never part of an endpoint — its
/// parameter is the descriptor's `AuthKind`), no fragment; the host lowercased, a default port
/// and a lone trailing `/` dropped.
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

impl ProviderDescriptor {
    pub fn validate(&self) -> Result<(), String> {
        if !id_ok(&self.id) {
            return Err(format!(
                "PROVIDER_ID_INVALID: {} ([a-z0-9_-]{{1,32}})",
                self.id
            ));
        }
        if canonical_endpoint(self.kind, &self.endpoint)? != self.endpoint {
            return Err("PROVIDER_URL_NOT_CANONICAL".into());
        }
        match (&self.auth, &self.credential) {
            (AuthKind::None, None) => {}
            (AuthKind::None, Some(_)) => return Err("PROVIDER_CREDENTIAL_WITHOUT_AUTH".into()),
            (_, None) => return Err("PROVIDER_AUTH_WITHOUT_CREDENTIAL".into()),
            (_, Some(c)) if !id_ok(c) => return Err(format!("CREDENTIAL_ID_INVALID: {c}")),
            _ => {}
        }
        if self.plan == Plan::Paid {
            return Err("PROVIDER_PAID_REFUSED: FREE_ONLY".into());
        }
        if self.entitlement.is_empty()
            || self.entitlement.len() > 64
            || !self.entitlement.bytes().all(|b| b.is_ascii_graphic())
        {
            return Err("PROVIDER_ENTITLEMENT_INVALID".into());
        }
        if self.capacity_rps == Some(0) {
            return Err("PROVIDER_CAPACITY_ZERO".into());
        }
        Ok(())
    }

    /// The secret-free canonical identity (hex SHA-256) of this provider with the credential
    /// revision it uses (`credential`: the registry's active revision of its credential, `None`
    /// for a provider without one): domain-separated, binary-canonical (bincode of a versioned
    /// tuple). Identity, transport, endpoint, authentication, plan, credential revision and
    /// configured capacity are part of it; priority, enablement and qualification are not (they
    /// select or qualify a provider, they do not change what it is). One implementation for RPC,
    /// WSS, evidence, audit and run manifest.
    pub fn binding(&self, credential: Option<&CredentialRef>) -> String {
        #[derive(Serialize)]
        struct BindingV1<'a> {
            id: &'a str,
            kind: &'a str,
            endpoint: &'a str,
            auth: String,
            plan: &'a str,
            entitlement: &'a str,
            credential: Option<(&'a str, u64)>,
            capacity_rps: Option<u64>,
        }
        let canonical = bincode::serialize(&BindingV1 {
            id: &self.id,
            kind: self.kind.name(),
            endpoint: &self.endpoint,
            auth: self.auth.text(),
            plan: self.plan.name(),
            entitlement: &self.entitlement,
            credential: credential.map(|c| (c.id.as_str(), c.revision)),
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

/// The domain of [`ProviderDescriptor::binding`] (version 1).
pub const BINDING_DOMAIN: &[u8] = b"darkbot.project0.provider-binding.v1\0";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The state of a credential's newest revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CredentialState {
    Active,
    Revoked,
}

/// A credential of the registry: its newest revision (revisions only grow, a revoked one is
/// never reused) and where it came from (the `.env` variable's name for the one-time import).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialMeta {
    pub id: String,
    pub revision: u64,
    pub state: CredentialState,
    pub created_unix_s: u64,
    /// `env:NAME` for the one-time `.env` import, `input` for a typed entry.
    pub origin: String,
}

/// One committed version of the registry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRegistry {
    pub version: u64,
    pub providers: Vec<ProviderDescriptor>,
    /// At most one provider per kind; selection only, never a fallback.
    pub selected: Vec<(ProviderKind, String)>,
    pub credentials: Vec<CredentialMeta>,
}

impl ProviderRegistry {
    pub fn empty() -> Self {
        Self {
            version: 0,
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
            Some(c) if c.state == CredentialState::Active => Ok(Some(CredentialRef {
                id: c.id.clone(),
                revision: c.revision,
            })),
            Some(_) => Err(format!("CREDENTIAL_REVOKED: {id}")),
            None => Err(format!("CREDENTIAL_UNKNOWN: {id}")),
        }
    }

    /// Every structural invariant of a committed version: valid unique providers and
    /// credentials, every selection names an enabled provider of its kind once. A selected
    /// provider's credential may be revoked (a revocation is always possible); the runtime then
    /// refuses to start with it (`credential_of`), and a new selection needs an active one.
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = std::collections::BTreeSet::new();
        for p in &self.providers {
            p.validate()?;
            if !ids.insert(p.id.as_str()) {
                return Err(format!("PROVIDER_DUPLICATE: {}", p.id));
            }
        }
        let mut creds = std::collections::BTreeSet::new();
        for c in &self.credentials {
            if !id_ok(&c.id) || c.revision == 0 {
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

/// The store of provider credentials: one item per credential id and revision. The secret is
/// handed over and returned zeroizing; the store never logs it.
pub trait CredentialStore {
    fn put(&self, credential: &CredentialRef, secret: &[u8]) -> Result<(), String>;
    fn get(&self, credential: &CredentialRef) -> Result<Zeroizing<Vec<u8>>, String>;
    fn delete(&self, credential: &CredentialRef) -> Result<(), String>;
}

/// The keychain service of every Project_0 provider credential.
pub const KEYCHAIN_SERVICE: &str = "de.darkma.darkbot.project0.provider";

/// The keychain account of one revision.
pub fn keychain_account(credential: &CredentialRef) -> String {
    format!("{}@{}", credential.id, credential.revision)
}

/// The macOS keychain (the user's login keychain): generic passwords of [`KEYCHAIN_SERVICE`].
#[cfg(target_os = "macos")]
pub struct Keychain;

#[cfg(target_os = "macos")]
impl CredentialStore for Keychain {
    fn put(&self, credential: &CredentialRef, secret: &[u8]) -> Result<(), String> {
        security_framework::passwords::set_generic_password(
            KEYCHAIN_SERVICE,
            &keychain_account(credential),
            secret,
        )
        .map_err(|e| format!("CREDENTIAL_STORE_WRITE: {}", e.code()))
    }
    fn get(&self, credential: &CredentialRef) -> Result<Zeroizing<Vec<u8>>, String> {
        security_framework::passwords::get_generic_password(
            KEYCHAIN_SERVICE,
            &keychain_account(credential),
        )
        .map(Zeroizing::new)
        .map_err(|e| {
            format!(
                "CREDENTIAL_UNAVAILABLE: {} ({})",
                keychain_account(credential),
                e.code()
            )
        })
    }
    fn delete(&self, credential: &CredentialRef) -> Result<(), String> {
        security_framework::passwords::delete_generic_password(
            KEYCHAIN_SERVICE,
            &keychain_account(credential),
        )
        .map_err(|e| format!("CREDENTIAL_STORE_DELETE: {}", e.code()))
    }
}

/// A test store under a directory, one file per revision: only in a debug build with
/// `test-support` (offline tests) — never in a release build, whatever its features, where the
/// keychain is the one store.
#[cfg(any(test, all(feature = "test-support", debug_assertions)))]
pub struct DirectoryStore(pub std::path::PathBuf);

#[cfg(any(test, all(feature = "test-support", debug_assertions)))]
impl CredentialStore for DirectoryStore {
    fn put(&self, credential: &CredentialRef, secret: &[u8]) -> Result<(), String> {
        std::fs::create_dir_all(&self.0).map_err(|e| format!("CREDENTIAL_STORE_WRITE: {e}"))?;
        std::fs::write(self.0.join(keychain_account(credential)), secret)
            .map_err(|e| format!("CREDENTIAL_STORE_WRITE: {e}"))
    }
    fn get(&self, credential: &CredentialRef) -> Result<Zeroizing<Vec<u8>>, String> {
        std::fs::read(self.0.join(keychain_account(credential)))
            .map(Zeroizing::new)
            .map_err(|_| format!("CREDENTIAL_UNAVAILABLE: {}", keychain_account(credential)))
    }
    fn delete(&self, credential: &CredentialRef) -> Result<(), String> {
        std::fs::remove_file(self.0.join(keychain_account(credential)))
            .map_err(|e| format!("CREDENTIAL_STORE_DELETE: {e}"))
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

    /// DEC-18 §1.2: an endpoint never carries a credential — user info, a query (an api-key
    /// parameter included) or a fragment are refused; TLS outside loopback; the host is
    /// lowercased, a default port and a lone trailing slash dropped.
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
        for (kind, url, code) in [
            (
                Rpc,
                "https://mainnet.helius-rpc.com/?api-key=SECRET",
                "PROVIDER_URL_QUERY",
            ),
            (Rpc, "https://user:pw@rpc.example", "PROVIDER_URL_USERINFO"),
            (Rpc, "https://rpc.example/#x", "PROVIDER_URL_FRAGMENT"),
            (Rpc, "http://rpc.example", "PROVIDER_URL_SCHEME"),
            (Wss, "https://rpc.example", "PROVIDER_URL_SCHEME"),
            (Rpc, "not a url", "PROVIDER_URL_INVALID"),
        ] {
            let e = canonical_endpoint(kind, url).unwrap_err();
            assert!(e.starts_with(code), "{url}: {e}");
            assert!(!e.contains("SECRET"), "{e}");
        }
    }

    /// DEC-18 §1.2: the binding is the provider's identity, never a secret: a changed endpoint,
    /// plan, capacity, authentication or credential revision changes it; priority, enablement
    /// and qualification do not; it is domain-separated (not the hash of the URL).
    #[test]
    fn the_binding_is_secret_free_and_changes_with_what_the_provider_is() {
        let p = helius();
        let rev = |r| CredentialRef {
            id: "helius".into(),
            revision: r,
        };
        let base = p.binding(Some(&rev(1)));
        assert_eq!(base.len(), 64);
        assert_ne!(
            base,
            hex(&solana_sdk::hash::hash(p.endpoint.as_bytes()).to_bytes())
        );
        assert_ne!(p.binding(Some(&rev(2))), base, "a rotated credential");
        let changed = |f: &dyn Fn(&mut ProviderDescriptor)| {
            let mut q = p.clone();
            f(&mut q);
            q.binding(Some(&rev(1)))
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
        assert_eq!(changed(&|q| q.qualification = Some("e1".into())), base);
    }

    /// DEC-18: a descriptor and a registry version are valid only whole: credential exactly with
    /// an authentication, FREE_ONLY, one enabled provider of its kind per selection, an active
    /// credential for every selected provider.
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
        p.endpoint = "https://mainnet.helius-rpc.com/".into();
        assert_eq!(p.validate().unwrap_err(), "PROVIDER_URL_NOT_CANONICAL");
        let mut r = ProviderRegistry::empty();
        r.providers.push(helius());
        r.selected.push((ProviderKind::Rpc, "helius-rpc".into()));
        assert!(r.validate().unwrap_err().starts_with("CREDENTIAL_UNKNOWN"));
        r.credentials.push(CredentialMeta {
            id: "helius".into(),
            revision: 3,
            state: CredentialState::Active,
            created_unix_s: 1,
            origin: "input".into(),
        });
        assert!(r.validate().is_ok());
        assert_eq!(
            r.credential_of(r.selected(ProviderKind::Rpc).unwrap())
                .unwrap(),
            Some(CredentialRef {
                id: "helius".into(),
                revision: 3
            })
        );
        // A revocation is always possible; the selected provider then cannot authenticate.
        r.credentials[0].state = CredentialState::Revoked;
        assert!(r.validate().is_ok());
        assert!(r
            .credential_of(r.selected(ProviderKind::Rpc).unwrap())
            .unwrap_err()
            .starts_with("CREDENTIAL_REVOKED"));
        r.credentials[0].state = CredentialState::Active;
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
    }

    /// The test store keeps one item per revision; a missing revision is unavailable (fail
    /// closed), never another revision.
    #[test]
    fn a_credential_revision_is_its_own_item() {
        let dir = std::env::temp_dir().join(format!("p0_cred_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = DirectoryStore(dir.clone());
        let r1 = CredentialRef {
            id: "helius".into(),
            revision: 1,
        };
        let r2 = CredentialRef {
            revision: 2,
            ..r1.clone()
        };
        store.put(&r1, b"one").unwrap();
        assert_eq!(&store.get(&r1).unwrap()[..], b"one");
        assert!(store
            .get(&r2)
            .unwrap_err()
            .starts_with("CREDENTIAL_UNAVAILABLE"));
        store.delete(&r1).unwrap();
        assert!(store.get(&r1).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
