//! Measured FREE_ONLY provider evidence (`PROVIDER_EVIDENCE`, X2): the typed record of a persisted
//! journal kind. Plain data, independent of the health engine, so every feature set registers and
//! reads the same kind (`fe_qualification` re-exports it).

fn sha(bytes: &[u8]) -> String {
    crate::analytics::journal::hex(&solana_sdk::hash::hash(bytes).to_bytes())
}

/// One measured FREE_ONLY provider qualification: the fields the capacity check reads.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderProof {
    pub policy: String,
    pub passed: bool,
    pub provider_binding: String,
    pub measured_at_unix_s: f64,
    /// `null` in a not-passing measurement; validation requires 64 hex digits.
    pub evidence_sha256: Option<String>,
    pub allowed_rps: f64,
    pub available_calls: u64,
}
/// Provider evidence (`PROVIDER_EVIDENCE`, X2): one proof per usable RPC endpoint, the WSS and
/// archive qualification, and the hash of the measured JSON it was imported from.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProviderEvidence {
    pub providers: Vec<ProviderProof>,
    pub wss_provider_binding: Option<String>,
    pub wss_free_only: bool,
    pub archive_qualified: bool,
    pub source_sha256: String,
}
impl crate::analytics::payload::Payload for ProviderEvidence {
    const KIND: u16 = crate::analytics::journal::kind::PROVIDER_EVIDENCE;
}
impl ProviderEvidence {
    /// Explicit offline import of the measured JSON format: one proof, or `providers: [...]`,
    /// each optionally with `wss_provider_binding`, `wss_free_only`, `archive_qualified`. Unknown
    /// fields and mixed shapes are refused. Productive paths read only the typed record.
    pub fn from_measured_json(bytes: &[u8]) -> Result<Self, String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Measured {
            providers: Option<Vec<ProviderProof>>,
            policy: Option<String>,
            passed: Option<bool>,
            provider_binding: Option<String>,
            measured_at_unix_s: Option<f64>,
            evidence_sha256: Option<String>,
            allowed_rps: Option<f64>,
            available_calls: Option<u64>,
            #[serde(default)]
            wss_provider_binding: Option<String>,
            #[serde(default)]
            wss_free_only: bool,
            #[serde(default)]
            archive_qualified: bool,
        }
        let m: Measured = serde_json::from_slice(bytes).map_err(|_| "PROVIDER_EVIDENCE_FORMAT")?;
        let single = (
            m.policy,
            m.passed,
            m.provider_binding,
            m.measured_at_unix_s,
            m.allowed_rps,
            m.available_calls,
        );
        let providers = match (m.providers, single) {
            (Some(p), (None, None, None, None, None, None))
                if !p.is_empty() && m.evidence_sha256.is_none() =>
            {
                p
            }
            (
                None,
                (
                    Some(policy),
                    Some(passed),
                    Some(provider_binding),
                    Some(measured_at_unix_s),
                    Some(allowed_rps),
                    Some(available_calls),
                ),
            ) => vec![ProviderProof {
                policy,
                passed,
                provider_binding,
                measured_at_unix_s,
                evidence_sha256: m.evidence_sha256,
                allowed_rps,
                available_calls,
            }],
            _ => return Err("PROVIDER_EVIDENCE_FORMAT".into()),
        };
        Ok(Self {
            providers,
            wss_provider_binding: m.wss_provider_binding,
            wss_free_only: m.wss_free_only,
            archive_qualified: m.archive_qualified,
            source_sha256: sha(bytes),
        })
    }
}

/// The size bound of a typed `PROVIDER_EVIDENCE` record (its format: a few proofs).
pub const RECORD_LIMIT: usize = 65536;
/// DEC-07: the controller's store of imported records, content-addressed — each at
/// `<id>.p0j`, its id the first 16 hex digits of the record's SHA-256. Commands name a record by
/// its id, never by a path; the controller resolves it here.
pub const STORE: &str = "data/provider_evidence";

/// Whether `s` has the form of a record id (16 lowercase hex digits).
pub fn is_id(s: &str) -> bool {
    s.len() == 16 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A record's bytes within [`RECORD_LIMIT`]; a missing file is `missing`.
fn read_limited(path: &std::path::Path, missing: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut data = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => missing.to_string(),
            _ => format!("BOOT_PROVIDER_EVIDENCE_IO: {e}"),
        })?
        .take(RECORD_LIMIT as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|e| format!("BOOT_PROVIDER_EVIDENCE_IO: {e}"))?;
    if data.len() > RECORD_LIMIT {
        return Err("BOOT_PROVIDER_EVIDENCE_LIMIT".into());
    }
    Ok(data)
}

/// Imports a typed `PROVIDER_EVIDENCE` record (`p0-journal import-provider-evidence`) into the
/// store under its id: the record must decode, and an existing id holds the same bytes (an
/// import is idempotent, never an overwrite).
pub fn import(project: &std::path::Path, record: &std::path::Path) -> Result<String, String> {
    let io = |e: std::io::Error| format!("BOOT_PROVIDER_EVIDENCE_IO: {e}");
    let data = read_limited(record, "BOOT_PROVIDER_EVIDENCE_IO: no such file")?;
    let id = sha(&data)[..16].to_string();
    let store = project.join(STORE);
    std::fs::create_dir_all(&store).map_err(io)?;
    let temp = store.join(format!(".{id}.{}.tmp", std::process::id()));
    let staged = (|| {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(io)?;
        f.write_all(&data).map_err(io)?;
        f.sync_all().map_err(io)?;
        // The bytes stored are the bytes checked.
        crate::analytics::journal::read_typed_value::<ProviderEvidence>(&temp)
            .map_err(|_| "BOOT_PROVIDER_EVIDENCE_FORMAT".to_string())?;
        let target = store.join(format!("{id}.p0j"));
        match std::fs::hard_link(&temp, &target) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if read_limited(&target, "BOOT_PROVIDER_EVIDENCE_IO: no such file")? != data {
                    return Err("BOOT_PROVIDER_EVIDENCE_ID_COLLISION".to_string());
                }
            }
            Err(e) => return Err(io(e)),
        }
        std::fs::File::open(&store)
            .and_then(|d| d.sync_all())
            .map_err(io)
    })();
    let _ = std::fs::remove_file(&temp);
    staged.map(|()| id)
}

/// The stored record of `id`: a malformed id or one never imported is refused, and the bytes
/// must still be the ones the id names.
pub fn resolve(project: &std::path::Path, id: &str) -> Result<std::path::PathBuf, String> {
    if !is_id(id) {
        return Err("BOOT_PROVIDER_EVIDENCE_ID_INVALID: a record id is 16 lowercase hex digits (p0ctl boot evidence import FILE.p0j)".into());
    }
    let path = project.join(STORE).join(format!("{id}.p0j"));
    if sha(&read_limited(&path, "BOOT_PROVIDER_EVIDENCE_UNKNOWN")?)[..16] != *id {
        return Err("BOOT_PROVIDER_EVIDENCE_CHANGED".into());
    }
    Ok(path)
}

/// Validate the measured FREE_ONLY evidence for every usable endpoint (a proof per endpoint
/// binding). This checks supplied evidence, not the provider's current quota availability.
pub fn validate_provider_evidence(
    config: &darkbot_solana::runtime_config::RuntimeConfig,
    primary: &str,
    proof: &ProviderEvidence,
    now: f64,
) -> Result<(), String> {
    let endpoints: std::collections::BTreeSet<&str> = std::iter::once(primary)
        .chain(
            config
                .rpc_endpoints
                .iter()
                .filter(|_| config.fallback)
                .map(String::as_str),
        )
        .collect();
    for endpoint in endpoints {
        let binding = sha(endpoint.as_bytes());
        let p = proof
            .providers
            .iter()
            .find(|p| p.provider_binding == binding)
            .ok_or("FE_PROVIDER_EVIDENCE_MISSING")?;
        let measured = p.measured_at_unix_s;
        let min_rps = config.limits.rpc_rps.min(config.limits.bootstrap_rps) as f64;
        if !now.is_finite()
            || now < 0.0
            || !measured.is_finite()
            || measured < 0.0
            || measured > now
            || now - measured > config.limits.fe_qualification_age_seconds as f64
            || p.policy != "FREE_ONLY"
            || !p.passed
            || p.evidence_sha256
                .as_deref()
                .is_none_or(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
            || p.allowed_rps.is_nan()
            || p.allowed_rps < min_rps
            || p.available_calls < config.limits.max_requests.min(config.limits.fe_calls)
        {
            return Err("FREE_ONLY_PROVIDER_CAPACITY_UNQUALIFIED".into());
        }
    }
    Ok(())
}
