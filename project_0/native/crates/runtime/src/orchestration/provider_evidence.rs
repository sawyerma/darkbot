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
/// archive qualification, and the hash of the measurement it was made from.
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
    /// fields and mixed shapes are refused. A historical source (DEC-18 §3.4): compiled only with
    /// the `historical` feature; productive paths read only the typed record.
    #[cfg(feature = "historical")]
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

/// Whether `id` names exactly these record bytes.
pub fn names(id: &str, data: &[u8]) -> bool {
    is_id(id) && sha(data)[..16] == *id
}

/// DEC-18 §3.4 (P0-023): the evidence of the record at `path` — a record of the native provider
/// measurement only (`journal::source::PROVIDER_MEASUREMENT`); one of another origin (the
/// historical build's JSON import, a default or foreign provenance) is refused. `format`: the
/// caller's code for a file that is no `PROVIDER_EVIDENCE` record.
pub fn read_measured(path: &std::path::Path, format: &str) -> Result<ProviderEvidence, String> {
    use crate::analytics::journal;
    let (provenance, evidence) =
        journal::read_typed_value_with_provenance::<ProviderEvidence>(path)
            .map_err(|_| format.to_string())?;
    if provenance.source != journal::source::PROVIDER_MEASUREMENT {
        return Err(format!(
            "PROVIDER_EVIDENCE_NOT_MEASURED: the record is no native provider measurement (origin {}); productive evidence is measured, never imported",
            provenance.source
        ));
    }
    Ok(evidence)
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

/// Imports a typed `PROVIDER_EVIDENCE` record (`p0ctl boot evidence import`) into the store
/// under its id: the record must decode, and an existing id holds the same bytes (an
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
        // The bytes stored are the bytes checked: a record of the native measurement only.
        read_measured(&temp, "BOOT_PROVIDER_EVIDENCE_FORMAT")?;
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
    if !names(id, &read_limited(&path, "BOOT_PROVIDER_EVIDENCE_UNKNOWN")?) {
        return Err("BOOT_PROVIDER_EVIDENCE_CHANGED".into());
    }
    Ok(path)
}

/// X2: the typed record at `path` and its bytes (bounded, read twice: a record changed while it
/// is read is refused). With `id` (DEC-07: a record named by its id) the bytes are exactly the
/// ones the id names.
pub fn read_record(
    path: &std::path::Path,
    id: Option<&str>,
) -> Result<(Vec<u8>, ProviderEvidence), String> {
    let read_bounded = || -> Result<Vec<u8>, String> {
        let mut data = Vec::new();
        use std::io::Read;
        std::fs::File::open(path)
            .map_err(|_| "BOOT_PROVIDER_EVIDENCE_IO")?
            .take(RECORD_LIMIT as u64 + 1)
            .read_to_end(&mut data)
            .map_err(|_| "BOOT_PROVIDER_EVIDENCE_IO")?;
        if data.len() > RECORD_LIMIT {
            return Err("BOOT_PROVIDER_EVIDENCE_LIMIT".into());
        }
        Ok(data)
    };
    let data = read_bounded()?;
    if id.is_some_and(|id| !names(id, &data)) {
        return Err("BOOT_PROVIDER_EVIDENCE_CHANGED".into());
    }
    let proof = read_measured(path, "BOOT_PROVIDER_EVIDENCE_FORMAT")?;
    if read_bounded()? != data {
        return Err("BOOT_PROVIDER_EVIDENCE_CHANGED".into());
    }
    Ok((data, proof))
}

/// The stored record of `id` and its bytes (DEC-18 §3.4: the productive evidence input, never a
/// measured JSON).
pub fn load(project: &std::path::Path, id: &str) -> Result<(Vec<u8>, ProviderEvidence), String> {
    read_record(&resolve(project, id)?, Some(id))
}

/// DEC-18 (FREE_ONLY): evidence qualifies a provider of the registry on a free plan only — never
/// a named local endpoint (a loopback forwarder is no provider of the selected pair) or a plan
/// not proven free.
pub fn free_registry_provider(p: &darkbot_solana::providers::BoundProvider) -> Result<(), String> {
    if p.registry_version == 0 || p.plan != darkbot_solana::providers::Plan::Free {
        return Err(format!(
            "FREE_ONLY_PROVIDER_UNREGISTERED: {} is no free registry provider (p0ctl provider add, select)",
            p.id
        ));
    }
    Ok(())
}

/// Validate the measured FREE_ONLY evidence for every usable endpoint: a proof per provider
/// binding (DEC-18 §1.2 — the bound provider's secret-free identity; a proof bound to a URL hash,
/// another provider or another credential revision never matches). This checks supplied
/// evidence, not the provider's current quota availability.
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
        let bound = config.bound(darkbot_solana::providers::ProviderKind::Rpc, endpoint)?;
        free_registry_provider(bound)?;
        let binding = &bound.binding;
        let p = proof
            .providers
            .iter()
            .find(|p| p.provider_binding == *binding)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::journal;

    /// DEC-18 §3.4 (P0-023, review M1): productive provider evidence is a record of the native
    /// provider measurement only. A record of another origin — the historical build's JSON
    /// import, a default or a foreign provenance — never enters the store, and one placed there
    /// by hand is refused by every productive reader.
    #[test]
    fn p0_023_a_record_not_of_the_native_measurement_is_no_evidence() {
        let d = std::env::temp_dir().join(format!("p0_023_origin_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join(STORE)).unwrap();
        let url = "https://archive.invalid";
        let mut config = darkbot_solana::runtime_config::RuntimeConfig {
            rpc_endpoints: vec![url.into()],
            ..Default::default()
        };
        crate::orchestration::provider_registry::bind_endpoints(&mut config);
        let evidence = ProviderEvidence {
            providers: vec![ProviderProof {
                policy: "FREE_ONLY".into(),
                passed: true,
                provider_binding: config
                    .bound(darkbot_solana::providers::ProviderKind::Rpc, url)
                    .unwrap()
                    .binding
                    .clone(),
                measured_at_unix_s: 1.0,
                evidence_sha256: Some("a".repeat(64)),
                allowed_rps: 1e6,
                available_calls: u64::MAX,
            }],
            wss_provider_binding: None,
            wss_free_only: false,
            archive_qualified: true,
            source_sha256: "0".repeat(64),
        };
        fn not_measured<T>(e: Result<T, String>) -> bool {
            e.is_err_and(|e| e.starts_with("PROVIDER_EVIDENCE_NOT_MEASURED"))
        }
        for source in [0, journal::source::BOOT_CONTROL] {
            let record = d.join(format!("origin-{source}.p0j"));
            journal::create_typed_value(&record, source, &evidence).unwrap();
            assert!(not_measured(import(&d, &record)), "{source}: import");
            let id = sha(&std::fs::read(&record).unwrap())[..16].to_string();
            let stored = d.join(STORE).join(format!("{id}.p0j"));
            assert!(!stored.exists(), "{source}: nothing stored");
            std::fs::copy(&record, &stored).unwrap();
            assert!(not_measured(load(&d, &id)), "{source}: load");
            assert!(not_measured(read_record(&record, None)), "{source}: read");
            assert!(
                not_measured(
                    crate::market::observer::strict_driver::strict_provider_gate(
                        &config, url, &stored, 2.0
                    )
                ),
                "{source}: the strict verifier's gate"
            );
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}
