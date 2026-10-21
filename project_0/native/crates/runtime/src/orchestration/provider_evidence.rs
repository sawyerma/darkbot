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
