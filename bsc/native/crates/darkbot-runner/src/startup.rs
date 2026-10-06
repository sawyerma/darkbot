//! Unit 6 startup ownership: typed operational configuration, one-shot
//! secret loading, immutable generation snapshots and the durable cold-start
//! fence that is the only production path to an admitting IntentLane.
//!
//! Nothing in this module runs on the BUY hot path. Filesystem access,
//! secret loading and fence fsync happen before the lane exists.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use alloy_primitives::{Address, B256, U256};
use darkbot_core::inclusion::capability::{AdapterKind, AdapterTable};
use darkbot_core::inclusion::release::ReleaseProfile;
use darkbot_core::intent::{
    AdapterId, AdapterSet, ArmedFence, IntentLane, LaneError, MAX_LANE_CAPACITY,
};
use darkbot_core::signer::{Signer, SignerError};

use crate::inclusion::tls::{
    AuthorizationToken, BLOCKRAZOR_DOCUMENTED_HOST, CLUB48_DOCUMENTED_HOST, HTTPS_PORT,
    ProviderTarget, TargetError, TlsEndpoint,
};

const BSC_MAINNET: u64 = 56;
const MAX_CAPTURE_CAPACITY: usize = 1_048_576;

/// One startup-selected builder endpoint. No credential is stored here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuilderEndpointConfig {
    pub adapter: AdapterId,
    pub kind: AdapterKind,
    pub host: String,
    pub port: u16,
}

/// Secret sources are read exactly once during startup.
///
/// Files may contain raw 32-byte key material or ASCII hex for the signer.
/// The BlockRazor authorization file is trimmed only at the line ending and
/// then validated as an HTTP Authorization value.
#[derive(Clone)]
pub struct SecretSources {
    pub signer_key_file: PathBuf,
    pub blockrazor_authorization_file: Option<PathBuf>,
}

impl fmt::Debug for SecretSources {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretSources")
            .field("signer_key_file", &"<redacted path>")
            .field(
                "blockrazor_authorization_file",
                &self
                    .blockrazor_authorization_file
                    .as_ref()
                    .map(|_| "<redacted path>"),
            )
            .finish()
    }
}

/// Economic and capacity policy that must be known before the lane exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupEconomics {
    pub buy_amount_wei: u128,
    pub gas_price_wei: u128,
    pub gas_limit: u64,
    pub provider_bid_wei: u128,
    pub slippage_bps: u16,
    pub max_buy_input_wei: u128,
    pub max_buy_gas_wei: u128,
    pub max_provider_payment_wei: u128,
    pub max_total_wei: u128,
}

/// Complete secret-free startup contract.
#[derive(Clone, Debug)]
pub struct StartupConfig {
    pub chain_id: u64,
    pub profile: ReleaseProfile,
    pub sender: Address,
    pub first_nonce: u64,
    pub lane_capacity: usize,
    pub capture_capacity: usize,
    pub clock_id: u32,
    pub policy_generation: u64,
    pub codehash_generation: u64,
    pub capability_generation: u64,
    pub authorization_generation: u64,
    pub adapters: AdapterSet,
    pub endpoints: Vec<BuilderEndpointConfig>,
    pub economics: StartupEconomics,
}

/// Independently observed startup facts. These are not operator policy:
/// they must come from the startup reconciliation owner before this module
/// can authorize an admitting lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupFacts {
    pub chain_id: u64,
    pub sender: Address,
    pub next_nonce: u64,
    pub balance_wei: U256,
    pub canonical_block_hash: B256,
}

/// Current immutable generations observed by the runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GenerationSnapshot {
    pub policy: u64,
    pub codehash: u64,
    pub capability: u64,
    pub authorization: u64,
}

#[derive(Debug)]
struct GenerationAtoms {
    policy: AtomicU64,
    codehash: AtomicU64,
    capability: AtomicU64,
    authorization: AtomicU64,
}

/// Background invalidation owner. Updating a generation never mutates an
/// already issued snapshot; callers compare against a fresh snapshot.
#[derive(Clone, Debug)]
pub struct GenerationOwner {
    inner: Arc<GenerationAtoms>,
}

impl GenerationOwner {
    fn from_snapshot(snapshot: GenerationSnapshot) -> Self {
        Self {
            inner: Arc::new(GenerationAtoms {
                policy: AtomicU64::new(snapshot.policy),
                codehash: AtomicU64::new(snapshot.codehash),
                capability: AtomicU64::new(snapshot.capability),
                authorization: AtomicU64::new(snapshot.authorization),
            }),
        }
    }

    pub fn snapshot(&self) -> GenerationSnapshot {
        GenerationSnapshot {
            policy: self.inner.policy.load(Ordering::Acquire),
            codehash: self.inner.codehash.load(Ordering::Acquire),
            capability: self.inner.capability.load(Ordering::Acquire),
            authorization: self.inner.authorization.load(Ordering::Acquire),
        }
    }

    pub fn invalidate_policy(&self) -> u64 {
        bump(&self.inner.policy)
    }

    pub fn invalidate_codehash(&self) -> u64 {
        bump(&self.inner.codehash)
    }

    pub fn invalidate_capability(&self) -> u64 {
        bump(&self.inner.capability)
    }

    pub fn revoke_authorization(&self) -> u64 {
        bump(&self.inner.authorization)
    }
}

fn bump(value: &AtomicU64) -> u64 {
    let mut current = value.load(Ordering::Acquire);
    loop {
        let next = current.saturating_add(1);
        match value.compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return next,
            Err(observed) => current = observed,
        }
    }
}

/// One startup authorization. It contains no secret bytes and is valid only
/// while its generation snapshot remains current.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupAuthorization {
    chain_id: u64,
    sender: Address,
    first_nonce: u64,
    lane_capacity: usize,
    canonical_block_hash: B256,
    generations: GenerationSnapshot,
}

impl StartupAuthorization {
    pub fn generations(self) -> GenerationSnapshot {
        self.generations
    }

    pub fn is_current(self, owner: &GenerationOwner) -> bool {
        owner.snapshot() == self.generations
    }
}

/// Loaded one-shot runtime assets. Debug never formats credential material.
pub struct ValidatedStartup {
    config: StartupConfig,
    signer: Signer,
    blockrazor_authorization: Option<AuthorizationToken>,
    generations: GenerationOwner,
    authorization: StartupAuthorization,
}

impl fmt::Debug for ValidatedStartup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidatedStartup")
            .field("chain_id", &self.config.chain_id)
            .field("sender", &self.config.sender)
            .field("first_nonce", &self.config.first_nonce)
            .field("lane_capacity", &self.config.lane_capacity)
            .field("capture_capacity", &self.config.capture_capacity)
            .field("adapters", &self.config.adapters)
            .field(
                "blockrazor_authorization",
                &self.blockrazor_authorization.as_ref().map(|_| "<redacted>"),
            )
            .finish_non_exhaustive()
    }
}

impl ValidatedStartup {
    pub fn config(&self) -> &StartupConfig {
        &self.config
    }

    pub fn signer(&self) -> &Signer {
        &self.signer
    }

    pub fn generations(&self) -> &GenerationOwner {
        &self.generations
    }

    pub fn authorization(&self) -> StartupAuthorization {
        self.authorization
    }

    /// Creates a provider target from already loaded startup material.
    /// No file or environment access occurs here.
    pub fn provider_target(&self, adapter: AdapterId) -> Result<ProviderTarget, StartupError> {
        let endpoint = self
            .config
            .endpoints
            .iter()
            .find(|endpoint| endpoint.adapter == adapter)
            .ok_or(StartupError::MissingEndpoint)?;
        let tls = TlsEndpoint::new(&endpoint.host, endpoint.port).map_err(StartupError::Target)?;
        match endpoint.kind {
            AdapterKind::Club48Builder => Ok(ProviderTarget::club48(tls)),
            AdapterKind::BlockRazorBuilder => {
                let authorization = self
                    .blockrazor_authorization
                    .clone()
                    .ok_or(StartupError::MissingAuthorization)?;
                Ok(ProviderTarget::blockrazor(tls, authorization))
            }
            AdapterKind::LocalGeth | AdapterKind::BlockRazorRpc => {
                Err(StartupError::IneligibleAdapter)
            }
        }
    }

    /// Consumes the validated startup and durably establishes the cold-start
    /// fence. Only after every write/fsync succeeds is an ArmedFence minted
    /// and consumed to create the admitting lane.
    ///
    /// If the fence already exists, startup is quarantined. If any write or
    /// sync fails after create_new, the file is deliberately left behind so
    /// the next startup is also quarantined.
    pub fn arm(self, root: &Path) -> Result<ArmedStartup, StartupError> {
        if !self.authorization.is_current(&self.generations) {
            return Err(StartupError::StaleAuthorization);
        }
        let fence_path = durable_fence(root, &self.config, self.authorization)?;
        let fence = ArmedFence::from_durable_startup(
            self.config.chain_id,
            self.config.sender,
            self.config.first_nonce,
        );
        let lane =
            IntentLane::armed(fence, self.config.lane_capacity).map_err(StartupError::Lane)?;
        Ok(ArmedStartup {
            lane,
            signer: self.signer,
            config: self.config,
            blockrazor_authorization: self.blockrazor_authorization,
            generations: self.generations,
            authorization: self.authorization,
            fence_path,
        })
    }
}

/// Fully armed startup state. The lane exists only after the durable fence.
pub struct ArmedStartup {
    lane: IntentLane,
    signer: Signer,
    config: StartupConfig,
    blockrazor_authorization: Option<AuthorizationToken>,
    generations: GenerationOwner,
    authorization: StartupAuthorization,
    fence_path: PathBuf,
}

impl fmt::Debug for ArmedStartup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArmedStartup")
            .field("chain_id", &self.config.chain_id)
            .field("sender", &self.config.sender)
            .field("fence_path", &self.fence_path)
            .field(
                "blockrazor_authorization",
                &self.blockrazor_authorization.as_ref().map(|_| "<redacted>"),
            )
            .finish_non_exhaustive()
    }
}

impl ArmedStartup {
    pub fn lane(&self) -> &IntentLane {
        &self.lane
    }

    pub fn lane_mut(&mut self) -> &mut IntentLane {
        &mut self.lane
    }

    pub fn signer(&self) -> &Signer {
        &self.signer
    }

    pub fn config(&self) -> &StartupConfig {
        &self.config
    }

    pub fn generations(&self) -> &GenerationOwner {
        &self.generations
    }

    pub fn authorization_current(&self) -> bool {
        self.authorization.is_current(&self.generations)
    }

    pub fn fence_path(&self) -> &Path {
        &self.fence_path
    }
}

/// Startup validation failures. No variant contains secret material.
#[derive(Debug)]
pub enum StartupError {
    WrongChain,
    UnsupportedProfile,
    ZeroGeneration,
    CapabilityGenerationMismatch,
    SenderMismatch,
    InvalidNonceRange,
    InvalidLaneCapacity,
    InvalidCaptureCapacity,
    InvalidClock,
    ObservedChainMismatch,
    ObservedSenderMismatch,
    ObservedNonceMismatch,
    MissingCanonicalCheckpoint,
    EmptyAdapters,
    IneligibleAdapter,
    AdapterKindMismatch,
    DuplicateEndpoint,
    MissingEndpoint,
    UnexpectedEndpoint,
    WrongEndpoint,
    MissingAuthorization,
    UnexpectedAuthorization,
    InvalidEconomics,
    BudgetOverflow,
    InsufficientFunding,
    SecretIo(io::ErrorKind),
    InvalidSignerEncoding,
    Signer(SignerError),
    Target(TargetError),
    FenceIo(io::ErrorKind),
    RestartQuarantined(PathBuf),
    StaleAuthorization,
    Lane(LaneError),
}

impl fmt::Display for StartupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongChain => f.write_str("startup: chain must be BSC mainnet"),
            Self::UnsupportedProfile => f.write_str("startup: unsupported release profile"),
            Self::ZeroGeneration => f.write_str("startup: generation must be nonzero"),
            Self::CapabilityGenerationMismatch => {
                f.write_str("startup: capability generation mismatch")
            }
            Self::SenderMismatch => f.write_str("startup: signer does not match configured sender"),
            Self::InvalidNonceRange => f.write_str("startup: nonce range would overflow"),
            Self::InvalidLaneCapacity => f.write_str("startup: invalid lane capacity"),
            Self::InvalidCaptureCapacity => f.write_str("startup: invalid capture capacity"),
            Self::InvalidClock => f.write_str("startup: clock id must be nonzero"),
            Self::ObservedChainMismatch => {
                f.write_str("startup: observed chain does not match configuration")
            }
            Self::ObservedSenderMismatch => {
                f.write_str("startup: observed sender does not match configuration")
            }
            Self::ObservedNonceMismatch => {
                f.write_str("startup: observed next nonce does not match configuration")
            }
            Self::MissingCanonicalCheckpoint => {
                f.write_str("startup: canonical checkpoint is missing")
            }
            Self::EmptyAdapters => f.write_str("startup: no eligible adapter selected"),
            Self::IneligibleAdapter => f.write_str("startup: adapter is not currently eligible"),
            Self::AdapterKindMismatch => {
                f.write_str("startup: adapter kind does not match capability")
            }
            Self::DuplicateEndpoint => f.write_str("startup: duplicate adapter endpoint"),
            Self::MissingEndpoint => f.write_str("startup: selected adapter has no endpoint"),
            Self::UnexpectedEndpoint => {
                f.write_str("startup: endpoint exists for unselected adapter")
            }
            Self::WrongEndpoint => f.write_str("startup: endpoint differs from qualified builder"),
            Self::MissingAuthorization => f.write_str("startup: required authorization is missing"),
            Self::UnexpectedAuthorization => {
                f.write_str("startup: authorization supplied but unused")
            }
            Self::InvalidEconomics => f.write_str("startup: economic policy is invalid"),
            Self::BudgetOverflow => f.write_str("startup: economic budget overflow"),
            Self::InsufficientFunding => f.write_str("startup: funding is below maximum exposure"),
            Self::SecretIo(kind) => write!(f, "startup: secret source I/O {kind:?}"),
            Self::InvalidSignerEncoding => f.write_str("startup: signer key encoding is invalid"),
            Self::Signer(error) => write!(f, "startup: {error}"),
            Self::Target(error) => write!(f, "startup: {error}"),
            Self::FenceIo(kind) => write!(f, "startup: fence I/O {kind:?}"),
            Self::RestartQuarantined(_) => {
                f.write_str("startup: existing fence requires quarantine")
            }
            Self::StaleAuthorization => f.write_str("startup: authorization generation is stale"),
            Self::Lane(error) => write!(f, "startup: {error}"),
        }
    }
}

impl std::error::Error for StartupError {}

/// Validates the complete startup contract and loads secrets once.
pub fn validate_startup(
    config: StartupConfig,
    table: &AdapterTable,
    sources: &SecretSources,
    facts: StartupFacts,
) -> Result<ValidatedStartup, StartupError> {
    validate_config(&config, table)?;
    validate_facts(&config, facts)?;

    let mut secret = read_signer_secret(&sources.signer_key_file)?;
    let signer_result = Signer::from_secret_bytes(&secret).map_err(StartupError::Signer);
    secret.fill(0);
    let signer = signer_result?;
    if signer.address() != config.sender {
        return Err(StartupError::SenderMismatch);
    }

    let needs_blockrazor = config.endpoints.iter().any(|endpoint| {
        config.adapters.contains(endpoint.adapter)
            && endpoint.kind == AdapterKind::BlockRazorBuilder
    });
    let blockrazor_authorization = match (
        needs_blockrazor,
        sources.blockrazor_authorization_file.as_ref(),
    ) {
        (true, Some(path)) => Some(read_authorization(path)?),
        (true, None) => return Err(StartupError::MissingAuthorization),
        (false, Some(_)) => return Err(StartupError::UnexpectedAuthorization),
        (false, None) => None,
    };

    let snapshot = GenerationSnapshot {
        policy: config.policy_generation,
        codehash: config.codehash_generation,
        capability: config.capability_generation,
        authorization: config.authorization_generation,
    };
    let generations = GenerationOwner::from_snapshot(snapshot);
    let authorization = StartupAuthorization {
        chain_id: config.chain_id,
        sender: config.sender,
        first_nonce: config.first_nonce,
        lane_capacity: config.lane_capacity,
        canonical_block_hash: facts.canonical_block_hash,
        generations: snapshot,
    };
    Ok(ValidatedStartup {
        config,
        signer,
        blockrazor_authorization,
        generations,
        authorization,
    })
}

fn validate_config(config: &StartupConfig, table: &AdapterTable) -> Result<(), StartupError> {
    if config.chain_id != BSC_MAINNET || table.chain_id() != BSC_MAINNET {
        return Err(StartupError::WrongChain);
    }
    match config.profile {
        ReleaseProfile::BirthEntryV1 => {}
    }
    if [
        config.policy_generation,
        config.codehash_generation,
        config.capability_generation,
        config.authorization_generation,
    ]
    .contains(&0)
    {
        return Err(StartupError::ZeroGeneration);
    }
    if table.generation() != config.capability_generation {
        return Err(StartupError::CapabilityGenerationMismatch);
    }
    if config.lane_capacity == 0 || config.lane_capacity > MAX_LANE_CAPACITY {
        return Err(StartupError::InvalidLaneCapacity);
    }
    let span = u64::try_from(config.lane_capacity).map_err(|_| StartupError::InvalidNonceRange)?;
    if config.first_nonce.checked_add(span).is_none() {
        return Err(StartupError::InvalidNonceRange);
    }
    if config.capture_capacity == 0 || config.capture_capacity > MAX_CAPTURE_CAPACITY {
        return Err(StartupError::InvalidCaptureCapacity);
    }
    if config.clock_id == 0 {
        return Err(StartupError::InvalidClock);
    }
    if config.adapters.count() == 0 {
        return Err(StartupError::EmptyAdapters);
    }

    let mut selected_endpoints = 0usize;
    for endpoint in &config.endpoints {
        if !config.adapters.contains(endpoint.adapter) {
            return Err(StartupError::UnexpectedEndpoint);
        }
        if config
            .endpoints
            .iter()
            .filter(|candidate| candidate.adapter == endpoint.adapter)
            .count()
            != 1
        {
            return Err(StartupError::DuplicateEndpoint);
        }
        let record = table
            .record(endpoint.adapter)
            .ok_or(StartupError::IneligibleAdapter)?;
        if record.kind() != endpoint.kind {
            return Err(StartupError::AdapterKindMismatch);
        }
        match endpoint.kind {
            AdapterKind::Club48Builder => {
                if endpoint.host != CLUB48_DOCUMENTED_HOST || endpoint.port != HTTPS_PORT {
                    return Err(StartupError::WrongEndpoint);
                }
            }
            AdapterKind::BlockRazorBuilder => {
                if endpoint.host != BLOCKRAZOR_DOCUMENTED_HOST || endpoint.port != HTTPS_PORT {
                    return Err(StartupError::WrongEndpoint);
                }
            }
            AdapterKind::LocalGeth | AdapterKind::BlockRazorRpc => {
                return Err(StartupError::IneligibleAdapter);
            }
        }
        selected_endpoints += 1;
    }
    if selected_endpoints != config.adapters.count() {
        return Err(StartupError::MissingEndpoint);
    }

    validate_economics(config.economics)
}

fn validate_facts(config: &StartupConfig, facts: StartupFacts) -> Result<(), StartupError> {
    if facts.chain_id != config.chain_id {
        return Err(StartupError::ObservedChainMismatch);
    }
    if facts.sender != config.sender {
        return Err(StartupError::ObservedSenderMismatch);
    }
    if facts.next_nonce != config.first_nonce {
        return Err(StartupError::ObservedNonceMismatch);
    }
    if facts.canonical_block_hash == B256::ZERO {
        return Err(StartupError::MissingCanonicalCheckpoint);
    }
    if facts.balance_wei < U256::from(config.economics.max_total_wei) {
        return Err(StartupError::InsufficientFunding);
    }
    Ok(())
}

fn validate_economics(e: StartupEconomics) -> Result<(), StartupError> {
    if e.buy_amount_wei == 0
        || e.gas_price_wei == 0
        || e.gas_limit == 0
        || e.slippage_bps > 10_000
        || e.buy_amount_wei > e.max_buy_input_wei
        || e.provider_bid_wei > e.max_provider_payment_wei
    {
        return Err(StartupError::InvalidEconomics);
    }
    let gas = e
        .gas_price_wei
        .checked_mul(u128::from(e.gas_limit))
        .ok_or(StartupError::BudgetOverflow)?;
    if gas > e.max_buy_gas_wei {
        return Err(StartupError::InvalidEconomics);
    }
    let total = e
        .buy_amount_wei
        .checked_add(gas)
        .and_then(|v| v.checked_add(e.provider_bid_wei))
        .ok_or(StartupError::BudgetOverflow)?;
    if total > e.max_total_wei {
        return Err(StartupError::InvalidEconomics);
    }
    Ok(())
}

fn read_signer_secret(path: &Path) -> Result<[u8; 32], StartupError> {
    let mut bytes = fs::read(path).map_err(|error| StartupError::SecretIo(error.kind()))?;
    let result = if bytes.len() == 32 {
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        Ok(out)
    } else {
        (|| {
            let text =
                std::str::from_utf8(&bytes).map_err(|_| StartupError::InvalidSignerEncoding)?;
            let trimmed = text.trim();
            let hex = trimmed.strip_prefix("0x").unwrap_or(trimmed);
            if hex.len() != 64 {
                return Err(StartupError::InvalidSignerEncoding);
            }
            let mut out = [0u8; 32];
            for (index, slot) in out.iter_mut().enumerate() {
                let pair = &hex[index * 2..index * 2 + 2];
                *slot =
                    u8::from_str_radix(pair, 16).map_err(|_| StartupError::InvalidSignerEncoding)?;
            }
            Ok(out)
        })()
    };
    bytes.fill(0);
    result
}

fn read_authorization(path: &Path) -> Result<AuthorizationToken, StartupError> {
    let bytes = fs::read(path).map_err(|error| StartupError::SecretIo(error.kind()))?;
    let text = String::from_utf8(bytes).map_err(|_| StartupError::MissingAuthorization)?;
    let value = text.trim_end_matches(['\r', '\n']).to_owned();
    AuthorizationToken::new(value).map_err(StartupError::Target)
}

fn durable_fence(
    root: &Path,
    config: &StartupConfig,
    authorization: StartupAuthorization,
) -> Result<PathBuf, StartupError> {
    fs::create_dir_all(root).map_err(|error| StartupError::FenceIo(error.kind()))?;
    if let Some(parent) = root.parent() {
        File::open(parent)
            .and_then(|file| file.sync_all())
            .map_err(|error| StartupError::FenceIo(error.kind()))?;
    }

    let sender = format!("{:#x}", config.sender);
    let path = root.join(format!("{}-{}.fence", config.chain_id, sender));
    let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Err(StartupError::RestartQuarantined(path));
        }
        Err(error) => return Err(StartupError::FenceIo(error.kind())),
    };

    let generations = authorization.generations();
    let record = format!(
        "version=1\nchain_id={}\nsender={}\nfirst_nonce={}\nlane_capacity={}\ncapture_capacity={}\nclock_id={}\ncanonical_block_hash={}\npolicy_generation={}\ncodehash_generation={}\ncapability_generation={}\nauthorization_generation={}\nrestart_action=QUARANTINE\nautomatic_resume=false\n",
        authorization.chain_id,
        sender,
        authorization.first_nonce,
        authorization.lane_capacity,
        config.capture_capacity,
        config.clock_id,
        authorization.canonical_block_hash,
        generations.policy,
        generations.codehash,
        generations.capability,
        generations.authorization,
    );
    file.write_all(record.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|error| StartupError::FenceIo(error.kind()))?;
    File::open(root)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| StartupError::FenceIo(error.kind()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use darkbot_core::inclusion::capability::{AdapterBinding, CapabilityId};

    use super::*;

    static ID: AtomicU64 = AtomicU64::new(0);

    fn tmp(name: &str) -> PathBuf {
        let id = ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("darkbot-u6-{}-{}-{}", std::process::id(), id, name));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn adapter(index: u8) -> AdapterId {
        AdapterId::new(index).unwrap()
    }

    fn table() -> AdapterTable {
        AdapterTable::new(
            56,
            7,
            &[
                AdapterBinding {
                    adapter: adapter(0),
                    capability: CapabilityId::Club48BuilderV1,
                },
                AdapterBinding {
                    adapter: adapter(1),
                    capability: CapabilityId::BlockRazorBuilderV1,
                },
            ],
        )
        .unwrap()
    }

    fn sources(root: &Path) -> SecretSources {
        let key = root.join("signer.key");
        fs::write(&key, [0x22u8; 32]).unwrap();
        let auth = root.join("blockrazor.auth");
        fs::write(&auth, "Bearer unit-test-secret\n").unwrap();
        SecretSources {
            signer_key_file: key,
            blockrazor_authorization_file: Some(auth),
        }
    }

    fn config(sender: Address) -> StartupConfig {
        StartupConfig {
            chain_id: 56,
            profile: ReleaseProfile::BirthEntryV1,
            sender,
            first_nonce: 17,
            lane_capacity: 8,
            capture_capacity: 1024,
            clock_id: 9,
            policy_generation: 11,
            codehash_generation: 12,
            capability_generation: 7,
            authorization_generation: 13,
            adapters: AdapterSet::new(&[adapter(0), adapter(1)]).unwrap(),
            endpoints: vec![
                BuilderEndpointConfig {
                    adapter: adapter(0),
                    kind: AdapterKind::Club48Builder,
                    host: CLUB48_DOCUMENTED_HOST.to_owned(),
                    port: HTTPS_PORT,
                },
                BuilderEndpointConfig {
                    adapter: adapter(1),
                    kind: AdapterKind::BlockRazorBuilder,
                    host: BLOCKRAZOR_DOCUMENTED_HOST.to_owned(),
                    port: HTTPS_PORT,
                },
            ],
            economics: StartupEconomics {
                buy_amount_wei: 1_000_000_000_000u128,
                gas_price_wei: 1_000_000_000u128,
                gas_limit: 300_000,
                provider_bid_wei: 0,
                slippage_bps: 100,
                max_buy_input_wei: 2_000_000_000_000u128,
                max_buy_gas_wei: 400_000_000_000_000u128,
                max_provider_payment_wei: 0,
                max_total_wei: 500_000_000_000_000u128,
            },
        }
    }

    fn valid(root: &Path) -> (StartupConfig, SecretSources) {
        let sources = sources(root);
        let secret = [0x22u8; 32];
        let signer = Signer::from_secret_bytes(&secret).unwrap();
        (config(signer.address()), sources)
    }

    fn facts(config: &StartupConfig) -> StartupFacts {
        StartupFacts {
            chain_id: config.chain_id,
            sender: config.sender,
            next_nonce: config.first_nonce,
            balance_wei: U256::from(config.economics.max_total_wei),
            canonical_block_hash: B256::repeat_byte(0x44),
        }
    }

    #[test]
    fn complete_startup_validates_and_loads_secrets_once_without_debug_leakage() {
        let root = tmp("valid");
        let (config, sources) = valid(&root);
        let observed = facts(&config);
        let startup = validate_startup(config, &table(), &sources, observed).unwrap();
        let printed = format!("{startup:?} {sources:?}");
        assert!(!printed.contains("unit-test-secret"));
        assert!(!printed.contains("2222222222"));
        assert!(startup.authorization().is_current(startup.generations()));
        assert_eq!(startup.signer().address(), startup.config().sender);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn configuration_fails_closed_on_chain_generation_endpoint_capacity_and_budget() {
        let root = tmp("bad-config");
        let (base, sources) = valid(&root);

        let mut c = base.clone();
        c.chain_id = 97;
        let observed = facts(&c);
        assert!(matches!(
            validate_startup(c, &table(), &sources, observed),
            Err(StartupError::WrongChain)
        ));

        let mut c = base.clone();
        c.capability_generation = 99;
        let observed = facts(&c);
        assert!(matches!(
            validate_startup(c, &table(), &sources, observed),
            Err(StartupError::CapabilityGenerationMismatch)
        ));

        let mut c = base.clone();
        c.endpoints[0].host = "example.invalid".into();
        let observed = facts(&c);
        assert!(matches!(
            validate_startup(c, &table(), &sources, observed),
            Err(StartupError::WrongEndpoint)
        ));

        let mut c = base.clone();
        c.lane_capacity = 0;
        let observed = facts(&c);
        assert!(matches!(
            validate_startup(c, &table(), &sources, observed),
            Err(StartupError::InvalidLaneCapacity)
        ));

        let mut c = base.clone();
        c.economics.max_total_wei = c.economics.max_buy_input_wei - 1;
        let observed = facts(&c);
        assert!(matches!(
            validate_startup(c, &table(), &sources, observed),
            Err(StartupError::InvalidEconomics)
        ));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn independently_observed_nonce_funding_and_checkpoint_fail_closed() {
        let root = tmp("facts");
        let (config, sources) = valid(&root);

        let mut observed = facts(&config);
        observed.next_nonce += 1;
        assert!(matches!(
            validate_startup(config.clone(), &table(), &sources, observed),
            Err(StartupError::ObservedNonceMismatch)
        ));

        let mut observed = facts(&config);
        observed.balance_wei = U256::ZERO;
        assert!(matches!(
            validate_startup(config.clone(), &table(), &sources, observed),
            Err(StartupError::InsufficientFunding)
        ));

        let mut observed = facts(&config);
        observed.canonical_block_hash = B256::ZERO;
        assert!(matches!(
            validate_startup(config.clone(), &table(), &sources, observed),
            Err(StartupError::MissingCanonicalCheckpoint)
        ));

        let mut observed = facts(&config);
        observed.sender = Address::repeat_byte(0x99);
        assert!(matches!(
            validate_startup(config, &table(), &sources, observed),
            Err(StartupError::ObservedSenderMismatch)
        ));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_or_wrong_secret_never_arms_startup() {
        let root = tmp("bad-secret");
        let (config, mut sources) = valid(&root);
        let observed = facts(&config);
        sources.blockrazor_authorization_file = None;
        assert!(matches!(
            validate_startup(config.clone(), &table(), &sources, observed),
            Err(StartupError::MissingAuthorization)
        ));

        let bad = root.join("bad.key");
        fs::write(&bad, [0u8; 32]).unwrap();
        sources.signer_key_file = bad;
        sources.blockrazor_authorization_file = Some(root.join("blockrazor.auth"));
        assert!(matches!(
            validate_startup(config, &table(), &sources, observed),
            Err(StartupError::Signer(SignerError::InvalidSecretKey))
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn durable_fence_arms_once_and_every_restart_is_quarantined() {
        let root = tmp("fence");
        let secrets = root.join("secrets");
        fs::create_dir(&secrets).unwrap();
        let (config, sources) = valid(&secrets);
        let observed = facts(&config);
        let fence_root = root.join("fences");

        let startup =
            validate_startup(config.clone(), &table(), &sources, observed).unwrap();
        let armed = startup.arm(&fence_root).unwrap();
        assert_eq!(armed.lane().identity(), (56, config.sender));
        assert!(armed.fence_path().exists());
        let fence = fs::read_to_string(armed.fence_path()).unwrap();
        assert!(fence.contains("canonical_block_hash=0x4444444444444444444444444444444444444444444444444444444444444444"));
        drop(armed);

        let startup = validate_startup(config, &table(), &sources, observed).unwrap();
        assert!(matches!(
            startup.arm(&fence_root),
            Err(StartupError::RestartQuarantined(_))
        ));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn generation_revocation_invalidates_authorization_without_mutating_old_snapshot() {
        let root = tmp("generation");
        let (config, sources) = valid(&root);
        let observed = facts(&config);
        let startup = validate_startup(config, &table(), &sources, observed).unwrap();
        let authorization = startup.authorization();
        let before = authorization.generations();
        assert!(authorization.is_current(startup.generations()));
        let next = startup.generations().revoke_authorization();
        assert_eq!(next, before.authorization + 1);
        assert_eq!(authorization.generations(), before);
        assert!(!authorization.is_current(startup.generations()));
        assert!(matches!(
            startup.arm(&root.join("fence")),
            Err(StartupError::StaleAuthorization)
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn provider_targets_are_built_only_from_validated_loaded_material() {
        let root = tmp("targets");
        let (config, sources) = valid(&root);
        let observed = facts(&config);
        let startup = validate_startup(config, &table(), &sources, observed).unwrap();
        let club = startup.provider_target(adapter(0)).unwrap();
        let razor = startup.provider_target(adapter(1)).unwrap();
        assert_eq!(club.kind(), AdapterKind::Club48Builder);
        assert_eq!(razor.kind(), AdapterKind::BlockRazorBuilder);
        let printed = format!("{razor:?}");
        assert!(!printed.contains("unit-test-secret"));
        fs::remove_dir_all(root).unwrap();
    }
}
