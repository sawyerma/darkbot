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
