//! G0.7 self-launch candidates (plan v8/v9). A keeps the frozen, publicly
//! known salt of the existing DRY; B draws a fresh salt from the OS CSPRNG.
//! Both go through the same G0.7 `predicted_token` and `launch_calldata` the
//! runner signs, recomputed independently with the trading crate's pinned
//! CREATE2 and newTokenV6 decoder. Offline only: no RPC, no key, no
//! signature. The salt lives solely in a private 0600 record and is never
//! printed, logged or part of an error.

use super::*;
use darkbot_bsc_record::schema::g06a::{dry as d, economics, preflight as pf};
use darkbot_bsc_record::schema::g07::readiness as rr;
use darkbot_bsc_record::schema::g07_candidate::{candidate as c, secret as s, selection as sel};
use darkbot_bsc_record::{Value, decode};
use darkbot_bsc_trading::venues::flap::{
    abi::decode_new_token_v6,
    address::{create2_address, minimal_proxy_init_code},
};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};

/// Private candidate state, git-ignored like the attempt state.
pub(crate) const CANDIDATE_STATE_ROOT: &str = "bsc/.local/g07_candidates";
pub(crate) const UNQUALIFIED: &str = "UNQUALIFIED";
/// What a switch invalidates for the newly selected candidate.
const INVALIDATED: &str = "READINESS,SIGNED_BYTE_AUTHORIZATION,ECONOMIC_APPROVAL";
const SALT_DOMAIN: &[u8] = b"darkbot.g07.candidate.salt.v1";
const ID_DOMAIN: &[u8] = b"darkbot.g07.candidate.id.v1";
/// The 7777 suffix needs 65 536 draws on average.
const MAX_SALT_DRAWS: u32 = 1 << 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Profile {
    A,
    B,
}

impl Profile {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::A => "SELF_A",
            Self::B => "SELF_B",
        }
    }

    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        match text {
            "A" | "SELF_A" => Ok(Self::A),
            "B" | "SELF_B" => Ok(Self::B),
            _ => Err(format!(
                "unknown candidate profile {text:?}; expected A or B"
            )),
        }
    }

    fn stem(self) -> &'static str {
        match self {
            Self::A => "a",
            Self::B => "b",
        }
    }

    fn salt_origin(self) -> &'static str {
        match self {
            Self::A => "FROZEN_PUBLIC_ECONOMICS_RECORD",
            Self::B => "OS_CSPRNG",
        }
    }
}

/// A launch salt. No `Debug`/`Display` of the value; zeroed on drop.
pub(crate) struct Salt(B256);

impl Drop for Salt {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.0.0);
    }
}

impl std::fmt::Debug for Salt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Salt(<redacted>)")
    }
}

impl Salt {
    /// Tests compare salts; production code never reads the value out.
    #[cfg(test)]
    pub(crate) fn expose(&self) -> B256 {
        self.0
    }

    fn commitment(&self) -> B256 {
        let mut preimage = SALT_DOMAIN.to_vec();
        preimage.extend_from_slice(self.0.as_slice());
        B256::from(darkbot_bsc_record::sha256(&preimage))
    }
}

/// The deployment a candidate address is derived against: a typed preflight
/// record, never a constant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Deployment {
    pub(crate) chain_id: u64,
    pub(crate) portal: Address,
    pub(crate) portal_version: String,
    pub(crate) portal_implementation: Address,
    pub(crate) portal_implementation_code_sha256: B256,
    pub(crate) tax_v3: Address,
    pub(crate) tax_v3_code_sha256: B256,
    pub(crate) record_sha256: B256,
}

impl Deployment {
    pub(crate) fn from_preflight(bytes: &[u8]) -> Result<Self, String> {
        let r = decode(&pf::SCHEMA, bytes).map_err(|e| format!("preflight: {e}"))?;
        let e = |e: darkbot_bsc_record::RecordError| format!("preflight: {e}");
        Ok(Self {
            chain_id: r.u64(pf::CHAIN_ID).map_err(e)?,
            portal: Address::from(r.address(pf::PORTAL).map_err(e)?),
            portal_version: r.text(pf::PORTAL_VERSION).map_err(e)?.to_string(),
            portal_implementation: Address::from(r.address(pf::PORTAL_IMPLEMENTATION).map_err(e)?),
            portal_implementation_code_sha256: B256::from(
                r.b256(pf::PORTAL_IMPLEMENTATION_CODE_SHA256).map_err(e)?,
            ),
            tax_v3: Address::from(r.address(pf::TAX_V3_IMPL).map_err(e)?),
            tax_v3_code_sha256: B256::from(r.b256(pf::TAX_V3_IMPL_CODE_SHA256).map_err(e)?),
            record_sha256: B256::from(darkbot_bsc_record::sha256(bytes)),
        })
    }
}

/// Profile A only: the frozen evidence its salt and signed hashes come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Evidence {
    pub(crate) economics_sha256: B256,
    pub(crate) dry_sha256: B256,
    pub(crate) launch_tx_hash: B256,
    pub(crate) buy_tx_hash: B256,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) profile: Profile,
    pub(crate) id: B256,
    pub(crate) deployment: Deployment,
    pub(crate) launcher: Address,
    pub(crate) token: Address,
    pub(crate) salt_commitment: B256,
    pub(crate) launch_calldata_sha256: B256,
    pub(crate) evidence: Option<Evidence>,
    pub(crate) qualification: String,
    pub(crate) provenance: String,
    pub(crate) created_unix_ms: u64,
}

/// Token, salt commitment and launch calldata hash of `salt`, through the
/// G0.7 functions and independently re-derived; any disagreement or a
/// missing 7777 vanity suffix fails.
fn derive(
    deployment: &Deployment,
    launcher: Address,
    salt: &Salt,
) -> Result<(Address, B256, B256), String> {
    let token = predicted_token(deployment.portal, deployment.tax_v3, salt.0);
    let init_code_hash = keccak256(minimal_proxy_init_code(deployment.tax_v3));
    if create2_address(deployment.portal, salt.0, init_code_hash) != token {
        return Err("the G0.7 and the pinned trading CREATE2 derivations disagree".into());
    }
    if token.as_slice()[18..] != [0x77, 0x77] {
        return Err("token address misses the Tax-V3 vanity suffix 7777".into());
    }
    let calldata = launch_calldata(launcher, salt.0);
    let params = decode_new_token_v6(&calldata, calldata.len())
        .map_err(|_| "G0.7 launch calldata does not decode as newTokenV6")?;
    if params.salt != salt.0 || params.beneficiary != launcher {
        return Err(
            "decoded launch calldata does not carry the candidate salt and launcher".into(),
        );
    }
    Ok((
        token,
        salt.commitment(),
        B256::from(darkbot_bsc_record::sha256(&calldata)),
    ))
}

fn candidate_id(
    profile: Profile,
    deployment: &Deployment,
    launcher: Address,
    token: Address,
    commitment: B256,
    calldata_sha256: B256,
    evidence: Option<&Evidence>,
) -> B256 {
    let mut p = ID_DOMAIN.to_vec();
    p.extend_from_slice(profile.name().as_bytes());
    p.extend_from_slice(&deployment.chain_id.to_be_bytes());
    for a in [
        deployment.portal,
        deployment.portal_implementation,
        deployment.tax_v3,
        launcher,
        token,
    ] {
        p.extend_from_slice(a.as_slice());
    }
    let mut words = vec![
        deployment.portal_implementation_code_sha256,
        deployment.tax_v3_code_sha256,
        deployment.record_sha256,
        commitment,
        calldata_sha256,
    ];
    if let Some(e) = evidence {
        words.extend([
            e.economics_sha256,
            e.dry_sha256,
            e.launch_tx_hash,
            e.buy_tx_hash,
        ]);
    }
    p.extend_from_slice(deployment.portal_version.as_bytes());
    for w in words {
        p.extend_from_slice(w.as_slice());
    }
    B256::from(darkbot_bsc_record::sha256(&p))
}

fn build(
    profile: Profile,
    deployment: Deployment,
    launcher: Address,
    salt: &Salt,
    evidence: Option<Evidence>,
    provenance: String,
    now_ms: u64,
) -> Result<Candidate, String> {
    let (token, salt_commitment, launch_calldata_sha256) = derive(&deployment, launcher, salt)?;
    let id = candidate_id(
        profile,
        &deployment,
        launcher,
        token,
        salt_commitment,
        launch_calldata_sha256,
        evidence.as_ref(),
    );
    Ok(Candidate {
        profile,
        id,
        deployment,
        launcher,
        token,
        salt_commitment,
        launch_calldata_sha256,
        evidence,
        qualification: UNQUALIFIED.into(),
        provenance,
        created_unix_ms: now_ms,
    })
}

fn tag_named(schema: &darkbot_bsc_record::Schema, name: &str) -> Result<u16, String> {
    schema
        .fields
        .iter()
        .find(|f| f.name == name)
        .map(|f| f.tag)
        .ok_or_else(|| format!("{} has no field {name}", schema.name))
}

/// The DRY's launcher, chain and Portal, checked against the deployment.
fn dry_identity(dry: &Record, deployment: &Deployment) -> Result<Address, String> {
    let e = |e: darkbot_bsc_record::RecordError| format!("DRY: {e}");
    if dry.u64(d::CHAIN_ID).map_err(e)? != deployment.chain_id
        || deployment.chain_id != CHAIN_ID
        || Address::from(dry.address(d::PORTAL).map_err(e)?) != deployment.portal
        || deployment.portal != PORTAL.parse::<Address>().map_err(|e| e.to_string())?
    {
        return Err("DRY, preflight and the G0.7 chain/Portal differ".into());
    }
    Ok(Address::from(dry.address(d::LAUNCHER).map_err(e)?))
}

fn decode_dry(bytes: &[u8]) -> Result<Record, String> {
    decode(&d::NATIVE, bytes)
        .or_else(|_| decode(&d::SCHEMA, bytes))
        .map_err(|e| format!("DRY: {e}"))
}

/// Profile A from the frozen economics, DRY and preflight records: the salt,
/// token and signed hashes stay exactly what the evidence binds.
pub(crate) fn frozen_a(
    economics_bytes: &[u8],
    dry_bytes: &[u8],
    preflight_bytes: &[u8],
    now_ms: u64,
) -> Result<(Candidate, Salt), String> {
    let econ = decode(&economics::NATIVE, economics_bytes)
        .or_else(|_| decode(&economics::SCHEMA, economics_bytes))
        .map_err(|e| format!("economics: {e}"))?;
    let es = econ.schema();
    let e = |e: darkbot_bsc_record::RecordError| format!("economics: {e}");
    let salt = Salt(B256::from(econ.b256(tag_named(es, "salt")?).map_err(e)?));
    let econ_token = Address::from(econ.address(tag_named(es, "token")?).map_err(e)?);
    let econ_portal = Address::from(econ.address(tag_named(es, "portal")?).map_err(e)?);
    let econ_tax_v3 = Address::from(econ.address(tag_named(es, "tax_v3_impl")?).map_err(e)?);
    let dry = decode_dry(dry_bytes)?;
    let deployment = Deployment::from_preflight(preflight_bytes)?;
    let launcher = dry_identity(&dry, &deployment)?;
    let dry_token = Address::from(dry.address(d::TOKEN).map_err(|e| format!("DRY: {e}"))?);
    if econ_portal != deployment.portal || econ_tax_v3 != deployment.tax_v3 {
        return Err("economics and preflight Portal/Tax-V3 implementation differ".into());
    }
    let evidence = Evidence {
        economics_sha256: B256::from(darkbot_bsc_record::sha256(economics_bytes)),
        dry_sha256: B256::from(darkbot_bsc_record::sha256(dry_bytes)),
        launch_tx_hash: B256::from(
            dry.b256(d::LAUNCH_TX_HASH)
                .map_err(|e| format!("DRY: {e}"))?,
        ),
        buy_tx_hash: B256::from(dry.b256(d::BUY_TX_HASH).map_err(|e| format!("DRY: {e}"))?),
    };
    let provenance = format!(
        "frozen public salt of the existing DRY, derived offline against Portal {}; its historical DRY PASS is not carried to any other deployment; requalification of this exact candidate required",
        deployment.portal_version
    );
    let candidate = build(
        Profile::A,
        deployment,
        launcher,
        &salt,
        Some(evidence),
        provenance,
        now_ms,
    )?;
    if candidate.token != econ_token || candidate.token != dry_token {
        return Err("derived token differs from the frozen economics/DRY token".into());
    }
    Ok((candidate, salt))
}

/// Profile B: a fresh salt from `draw` (the OS CSPRNG in production) that
/// meets the vanity rule, for the same launcher and deployment.
pub(crate) fn fresh_b(
    preflight_bytes: &[u8],
    dry_bytes: &[u8],
    now_ms: u64,
    draw: &mut dyn FnMut() -> Result<[u8; 32], String>,
) -> Result<(Candidate, Salt), String> {
    let deployment = Deployment::from_preflight(preflight_bytes)?;
    let launcher = dry_identity(&decode_dry(dry_bytes)?, &deployment)?;
    let init_code_hash = keccak256(minimal_proxy_init_code(deployment.tax_v3));
    for _ in 0..MAX_SALT_DRAWS {
        let salt = Salt(B256::from(draw()?));
        if create2_address(deployment.portal, salt.0, init_code_hash).as_slice()[18..]
            != [0x77, 0x77]
        {
            continue;
        }
        let provenance = format!(
            "fresh OS CSPRNG salt, derived offline against Portal {} without RPC or signature; no DRY yet; requalification of this exact candidate required",
            deployment.portal_version
        );
        let candidate = build(
            Profile::B,
            deployment,
            launcher,
            &salt,
            None,
            provenance,
            now_ms,
        )?;
        return Ok((candidate, salt));
    }
    Err("no salt with the vanity suffix within the draw bound".into())
}

/// 32 bytes from the operating system's CSPRNG; an RNG failure fails.
pub(crate) fn os_random() -> Result<[u8; 32], String> {
    use k256::elliptic_curve::rand_core::{OsRng, RngCore};
    let mut bytes = [0u8; 32];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| "operating system CSPRNG unavailable".to_string())?;
    Ok(bytes)
}

impl Candidate {
    pub(crate) fn draft(&self) -> Draft {
        let mut r = Draft::new(&c::SCHEMA);
        let dep = &self.deployment;
        r.set(c::PROFILE, self.profile.name())
            .set(c::CANDIDATE_ID, Value::B256(self.id.0))
            .set(c::CHAIN_ID, dep.chain_id)
            .set(c::PORTAL, Value::Address(dep.portal.into_array()))
            .set(c::PORTAL_VERSION, dep.portal_version.as_str())
            .set(
                c::PORTAL_IMPLEMENTATION,
                Value::Address(dep.portal_implementation.into_array()),
            )
            .set(
                c::PORTAL_IMPLEMENTATION_CODE_SHA256,
                Value::B256(dep.portal_implementation_code_sha256.0),
            )
            .set(
                c::TAX_V3_IMPLEMENTATION,
                Value::Address(dep.tax_v3.into_array()),
            )
            .set(
                c::TAX_V3_IMPLEMENTATION_CODE_SHA256,
                Value::B256(dep.tax_v3_code_sha256.0),
            )
            .set(
                c::DEPLOYMENT_RECORD_SHA256,
                Value::B256(dep.record_sha256.0),
            )
            .set(c::LAUNCHER, Value::Address(self.launcher.into_array()))
            .set(c::TOKEN, Value::Address(self.token.into_array()))
            .set(c::SALT_COMMITMENT, Value::B256(self.salt_commitment.0))
            .set(c::SALT_ORIGIN, self.profile.salt_origin())
            .set(
                c::LAUNCH_CALLDATA_SHA256,
                Value::B256(self.launch_calldata_sha256.0),
            )
            .set(c::QUALIFICATION, self.qualification.as_str())
            .set(c::PROVENANCE, self.provenance.as_str())
            .set(c::CREATED_UNIX_MS, self.created_unix_ms);
        if let Some(e) = &self.evidence {
            r.set(
                c::ECONOMICS_RECORD_SHA256,
                Value::B256(e.economics_sha256.0),
            )
            .set(c::DRY_RECORD_SHA256, Value::B256(e.dry_sha256.0))
            .set(c::LAUNCH_TX_HASH, Value::B256(e.launch_tx_hash.0))
            .set(c::BUY_TX_HASH, Value::B256(e.buy_tx_hash.0));
        }
        r
    }

    fn from_record(r: &Record) -> Result<Self, String> {
        let e = |e: darkbot_bsc_record::RecordError| format!("candidate record: {e}");
        let w = |tag| r.b256(tag).map(B256::from).map_err(e);
        let a = |tag| r.address(tag).map(Address::from).map_err(e);
        let opt = |tag| r.opt_b256(tag).map(|v| v.map(B256::from)).map_err(e);
        let evidence = match (
            opt(c::ECONOMICS_RECORD_SHA256)?,
            opt(c::DRY_RECORD_SHA256)?,
            opt(c::LAUNCH_TX_HASH)?,
            opt(c::BUY_TX_HASH)?,
        ) {
            (None, None, None, None) => None,
            (Some(economics_sha256), Some(dry_sha256), Some(launch_tx_hash), Some(buy_tx_hash)) => {
                Some(Evidence {
                    economics_sha256,
                    dry_sha256,
                    launch_tx_hash,
                    buy_tx_hash,
                })
            }
            _ => return Err("candidate record: partial evidence binding".into()),
        };
        let profile = Profile::parse(r.text(c::PROFILE).map_err(e)?)?;
        if r.text(c::SALT_ORIGIN).map_err(e)? != profile.salt_origin()
            || evidence.is_some() != (profile == Profile::A)
        {
            return Err(
                "candidate record: salt origin or evidence does not fit the profile".into(),
            );
        }
        Ok(Self {
            profile,
            id: w(c::CANDIDATE_ID)?,
            deployment: Deployment {
                chain_id: r.u64(c::CHAIN_ID).map_err(e)?,
                portal: a(c::PORTAL)?,
                portal_version: r.text(c::PORTAL_VERSION).map_err(e)?.to_string(),
                portal_implementation: a(c::PORTAL_IMPLEMENTATION)?,
                portal_implementation_code_sha256: w(c::PORTAL_IMPLEMENTATION_CODE_SHA256)?,
                tax_v3: a(c::TAX_V3_IMPLEMENTATION)?,
                tax_v3_code_sha256: w(c::TAX_V3_IMPLEMENTATION_CODE_SHA256)?,
                record_sha256: w(c::DEPLOYMENT_RECORD_SHA256)?,
            },
            launcher: a(c::LAUNCHER)?,
            token: a(c::TOKEN)?,
            salt_commitment: w(c::SALT_COMMITMENT)?,
            launch_calldata_sha256: w(c::LAUNCH_CALLDATA_SHA256)?,
            evidence,
            qualification: r.text(c::QUALIFICATION).map_err(e)?.to_string(),
            provenance: r.text(c::PROVENANCE).map_err(e)?.to_string(),
            created_unix_ms: r.u64(c::CREATED_UNIX_MS).map_err(e)?,
        })
    }

    /// Recomputes every derived value from the salt; nothing stored is
    /// trusted on its own.
    fn verify(&self, salt: &Salt) -> Result<(), String> {
        let (token, commitment, calldata) = derive(&self.deployment, self.launcher, salt)?;
        let id = candidate_id(
            self.profile,
            &self.deployment,
            self.launcher,
            token,
            commitment,
            calldata,
            self.evidence.as_ref(),
        );
        if token != self.token
            || commitment != self.salt_commitment
            || calldata != self.launch_calldata_sha256
            || id != self.id
        {
            return Err(format!(
                "candidate {} fails re-derivation (salt, token, calldata or identity changed)",
                self.profile.name()
            ));
        }
        if self.qualification != UNQUALIFIED {
            return Err("only a requalification may promote a candidate".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Selection {
    pub(crate) generation: u64,
    pub(crate) profile: Profile,
    pub(crate) candidate_id: B256,
    pub(crate) previous: Option<B256>,
    pub(crate) activated_unix_ms: u64,
}

impl Selection {
    /// The binding a readiness verdict must carry to count for this
    /// selection.
    pub(crate) fn binding(&self) -> String {
        format!(
            "candidate={} profile={} generation={}",
            self.candidate_id,
            self.profile.name(),
            self.generation
        )
    }
}

/// Private candidate state: a 0700 directory of 0600 records.
pub(crate) struct Store {
    dir: PathBuf,
}

impl Store {
    /// Creates the directory (and a missing parent) privately if needed.
    pub(crate) fn create(dir: &Path) -> Result<Self, String> {
        let parent = dir
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or("candidate state needs a parent directory")?;
        for d in [parent, dir] {
            match fs::DirBuilder::new().mode(0o700).create(d) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(format!("create {}: {e}", d.display())),
            }
            check_private_dir(d)?;
        }
        sync_dir(parent)?;
        Ok(Self {
            dir: dir.to_path_buf(),
        })
    }

    /// The existing directory, read-only; `None` when absent.
    pub(crate) fn existing(dir: &Path) -> Result<Option<Self>, String> {
        match fs::symlink_metadata(dir) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: {e}", dir.display())),
            Ok(_) => {
                check_private_dir(dir)?;
                Ok(Some(Self {
                    dir: dir.to_path_buf(),
                }))
            }
        }
    }

    fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        let path = self.dir.join(name);
        let meta = match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("{name}: {e}")),
            Ok(m) => m,
        };
        if !meta.file_type().is_file()
            || meta.permissions().mode() & 0o077 != 0
            || meta.nlink() != 1
            || meta.uid()
                != fs::symlink_metadata(&self.dir)
                    .map_err(|e| e.to_string())?
                    .uid()
        {
            return Err(format!(
                "{name} must be a private (0600) regular file of the state owner"
            ));
        }
        let mut file = fs::File::open(&path).map_err(|e| format!("{name}: {e}"))?;
        if !same_file(&path, &file) {
            return Err(format!("{name} changed while opening"));
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|e| format!("{name}: {e}"))?;
        Ok(Some(bytes))
    }

    fn staged(&self, name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let tmp = self
            .dir
            .join(format!(".{name}.{}.{nanos}.tmp", std::process::id()));
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| format!("stage {name}: {e}"))?;
        f.write_all(bytes)
            .and_then(|()| f.sync_all())
            .map_err(|e| format!("stage {name}: {e}"))?;
        Ok(tmp)
    }

    /// Atomic create-once: an existing record is never overwritten.
    fn create_once(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let tmp = self.staged(name, bytes)?;
        let linked = fs::hard_link(&tmp, self.dir.join(name));
        let removed = fs::remove_file(&tmp);
        match linked {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(format!(
                    "{name} already exists; a candidate is never re-created"
                ));
            }
            Err(e) => return Err(format!("commit {name}: {e}")),
            Ok(()) => {}
        }
        removed.map_err(|e| format!("remove staged {name}: {e}"))?;
        sync_dir(&self.dir)
    }

    fn replace(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let tmp = self.staged(name, bytes)?;
        fs::rename(&tmp, self.dir.join(name)).map_err(|e| format!("commit {name}: {e}"))?;
        sync_dir(&self.dir)
    }

    fn names(profile: Profile) -> (String, String) {
        (
            format!("{}.candidate.dbrec", profile.stem()),
            format!("{}.salt.dbrec", profile.stem()),
        )
    }

    /// The verified candidate of `profile`; `None` when absent. A half
    /// written or inconsistent candidate fails and is never re-rolled.
    pub(crate) fn load(&self, profile: Profile) -> Result<Option<(Candidate, Salt)>, String> {
        let (public, private) = Self::names(profile);
        let (record, secret) = match (self.read(&public)?, self.read(&private)?) {
            (None, None) => return Ok(None),
            (Some(r), Some(s)) => (r, s),
            _ => {
                return Err(format!(
                    "candidate {} is incomplete; reconcile manually, it is never re-rolled",
                    profile.name()
                ));
            }
        };
        let candidate = Candidate::from_record(
            &decode(&c::SCHEMA, &record).map_err(|e| format!("{public}: {e}"))?,
        )?;
        let secret = decode(&s::SCHEMA, &secret)
            .map_err(|_| format!("{private}: not a valid salt record"))?;
        let salt = Salt(B256::from(
            secret
                .b256(s::SALT)
                .map_err(|_| format!("{private}: no salt"))?,
        ));
        if candidate.profile != profile || secret.b256(s::CANDIDATE_ID).ok() != Some(candidate.id.0)
        {
            return Err(format!(
                "candidate {} and its salt record do not belong together",
                profile.name()
            ));
        }
        candidate.verify(&salt)?;
        Ok(Some((candidate, salt)))
    }

    fn save(&self, candidate: &Candidate, salt: &Salt) -> Result<(), String> {
        let (public, private) = Self::names(candidate.profile);
        let mut secret = Draft::new(&s::SCHEMA);
        secret
            .set(s::CANDIDATE_ID, Value::B256(candidate.id.0))
            .set(s::SALT, Value::B256(salt.0.0));
        let secret = zeroize::Zeroizing::new(secret.encode().map_err(|e| e.to_string())?);
        // The salt first: a crash in between leaves an incomplete candidate,
        // which fails closed instead of silently drawing another salt.
        self.create_once(&private, &secret)?;
        self.create_once(
            &public,
            &candidate.draft().encode().map_err(|e| e.to_string())?,
        )
    }

    /// Writes A once; re-running with the same frozen inputs returns it.
    pub(crate) fn init_a(&self, candidate: Candidate, salt: Salt) -> Result<Candidate, String> {
        if let Some((existing, _)) = self.load(Profile::A)? {
            return if existing.id == candidate.id {
                Ok(existing)
            } else {
                Err("a different candidate A already exists".into())
            };
        }
        self.save(&candidate, &salt)?;
        Ok(candidate)
    }

    /// Writes B once; an existing B is never re-rolled.
    pub(crate) fn create_b(&self, candidate: Candidate, salt: Salt) -> Result<Candidate, String> {
        let (public, private) = Self::names(Profile::B);
        if self.read(&public)?.is_some() || self.read(&private)?.is_some() {
            return Err("candidate B already exists; it is never re-rolled".into());
        }
        if let Some((a, _)) = self.load(Profile::A)?
            && (a.salt_commitment == candidate.salt_commitment || a.token == candidate.token)
        {
            return Err("candidate B must not reuse the salt or token of A".into());
        }
        self.save(&candidate, &salt)?;
        Ok(candidate)
    }

    pub(crate) fn selection(&self) -> Result<Option<Selection>, String> {
        let Some(bytes) = self.read("selection.dbrec")? else {
            return Ok(None);
        };
        let r = decode(&sel::SCHEMA, &bytes).map_err(|e| format!("selection: {e}"))?;
        let e = |e: darkbot_bsc_record::RecordError| format!("selection: {e}");
        let selection = Selection {
            generation: r.u64(sel::GENERATION).map_err(e)?,
            profile: Profile::parse(r.text(sel::PROFILE).map_err(e)?)?,
            candidate_id: B256::from(r.b256(sel::CANDIDATE_ID).map_err(e)?),
            previous: r
                .opt_b256(sel::PREVIOUS_CANDIDATE_ID)
                .map_err(e)?
                .map(B256::from),
            activated_unix_ms: r.u64(sel::ACTIVATED_UNIX_MS).map_err(e)?,
        };
        if selection.generation == 0 {
            return Err("selection: generation 0 is never written".into());
        }
        Ok(Some(selection))
    }

    /// Switches to `profile` under compare-and-swap on the generation. Never
    /// while the guard is armed or any G0.7 attempt state exists, never to a
    /// candidate that fails verification, never with a fallback.
    pub(crate) fn select(
        &self,
        profile: Profile,
        expected_generation: u64,
        guard: &str,
        attempt_root: &Path,
        now_ms: u64,
    ) -> Result<Selection, String> {
        let lock = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(self.dir.join("selection.lock"))
            .map_err(|e| format!("selection lock: {e}"))?;
        lock.lock().map_err(|e| format!("selection lock: {e}"))?;
        if guard != "UNARMED" {
            return Err("the G0.7 guard is armed: candidate switching is forbidden".into());
        }
        if attempt_state_present(attempt_root)? {
            return Err("G0.7 attempt state exists (an economic intent may be armed or running): candidate switching is forbidden".into());
        }
        let current = self.selection()?;
        let generation = current.as_ref().map_or(0, |s| s.generation);
        if generation != expected_generation {
            return Err(format!(
                "stale selection: generation is {generation}, not {expected_generation}"
            ));
        }
        if current.as_ref().is_some_and(|s| s.profile == profile) {
            return Err(format!("{} is already selected", profile.name()));
        }
        let (candidate, _) = self
            .load(profile)?
            .ok_or_else(|| format!("candidate {} does not exist", profile.name()))?;
        let next = Selection {
            generation: generation
                .checked_add(1)
                .ok_or("selection generation overflow")?,
            profile,
            candidate_id: candidate.id,
            previous: current.map(|s| s.candidate_id),
            activated_unix_ms: now_ms,
        };
        let mut r = Draft::new(&sel::SCHEMA);
        r.set(sel::GENERATION, next.generation)
            .set(sel::PROFILE, profile.name())
            .set(sel::CANDIDATE_ID, Value::B256(next.candidate_id.0))
            .set_opt(
                sel::PREVIOUS_CANDIDATE_ID,
                next.previous.map(|p| Value::B256(p.0)),
            )
            .set(sel::ACTIVATED_UNIX_MS, now_ms)
            .set(sel::INVALIDATED, INVALIDATED)
            .set(sel::GUARD_AT_SWITCH, guard);
        self.replace("selection.dbrec", &r.encode().map_err(|e| e.to_string())?)?;
        Ok(next)
    }
}

/// Any entry under the attempt state root means an attempt was latched.
pub(crate) fn attempt_state_present(root: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(root) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("attempt state inspection failed: {e}")),
        Ok(m) if !m.file_type().is_dir() => Err("attempt state is not a real directory".into()),
        Ok(_) => Ok(fs::read_dir(root)
            .map_err(|e| format!("attempt state inspection failed: {e}"))?
            .next()
            .is_some()),
    }
}

/// The readiness of `candidate` from a readiness record: it counts only when
/// it carries a PASSing CANDIDATE_SELECTION for the current selection of
/// exactly this candidate and was observed after that switch.
pub(crate) fn readiness_for(
    readiness: &Record,
    selection: Option<&Selection>,
    candidate: &Candidate,
) -> &'static str {
    let Some(selection) = selection else {
        return "UNKNOWN";
    };
    if selection.candidate_id != candidate.id {
        return "NOT_SELECTED";
    }
    let bound = readiness
        .u64(rr::OBSERVED_UNIX_MS)
        .is_ok_and(|t| t >= selection.activated_unix_ms)
        && readiness.nested_all(rr::CHECKS).is_ok_and(|checks| {
            checks.iter().any(|row| {
                row.text(rr::check::NAME).ok() == Some("CANDIDATE_SELECTION")
                    && row.text(rr::check::STATUS).ok() == Some("PASS")
                    && row.text(rr::check::DETAIL).ok() == Some(selection.binding().as_str())
            })
        });
    if !bound {
        return "STALE";
    }
    match readiness.text(rr::STATUS) {
        Ok("PASS_READ_ONLY") => "PASS_READ_ONLY",
        Ok("BLOCKED") => "BLOCKED",
        Ok("STOP") => "STOP",
        _ => "UNKNOWN",
    }
}

/// Readiness: the selected candidate must be exactly the one the v4 contract
/// binds (its DRY token, launcher and signed hashes). B, or no selection, is
/// a missing precondition; a contradiction or a failed re-derivation STOPs.
pub(crate) fn contract_candidate_check(
    dir: &Path,
    token: Address,
    launcher: Address,
    launch_tx_hash: B256,
    buy_tx_hash: B256,
) -> (&'static str, String) {
    let store = match Store::existing(dir) {
        Ok(Some(store)) => store,
        Ok(None) => {
            return (
                "BLOCKED",
                "No G0.7 candidate state; create and select candidate A or B (g07_candidate)"
                    .into(),
            );
        }
        Err(e) => return ("STOP", format!("candidate state unusable: {e}")),
    };
    let selection = match store.selection() {
        Ok(Some(s)) => s,
        Ok(None) => return ("BLOCKED", "No G0.7 candidate selected".into()),
        Err(e) => return ("STOP", e),
    };
    let candidate = match store.load(selection.profile) {
        Ok(Some((candidate, _))) => candidate,
        Ok(None) => {
            return (
                "STOP",
                format!("selected {} does not exist", selection.profile.name()),
            );
        }
        Err(e) => return ("STOP", e),
    };
    if candidate.id != selection.candidate_id {
        return (
            "STOP",
            "selection names another candidate than the stored one".into(),
        );
    }
    let bound = candidate.token == token
        && candidate.launcher == launcher
        && candidate
            .evidence
            .as_ref()
            .is_some_and(|e| e.launch_tx_hash == launch_tx_hash && e.buy_tx_hash == buy_tx_hash);
    match (bound, candidate.profile) {
        (true, _) => ("PASS", selection.binding()),
        (false, Profile::B) => (
            "BLOCKED",
            format!(
                "selected SELF_B {} is not the candidate the v4 contract binds; B needs its own DRY, requalification and contract",
                candidate.id
            ),
        ),
        (false, Profile::A) => (
            "STOP",
            "selected SELF_A differs from the v4-bound DRY token, launcher or signed hashes".into(),
        ),
    }
}
