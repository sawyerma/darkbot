//! DEC-18 §4.3 (P0-024): the fee payer an evaluation builds and signs with — its keypair where a
//! real signature may be made (a SIM_ONLY simulation, LIVE), or only its configured public key on
//! a read-only measurement path (the econ4h calibration): no private key is loaded there, every
//! simulation is an `UNSIGNED_SIMULATION` (sigVerify off, never sent) and nothing can be signed.
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

/// The payer of one evaluation (borrowed from its owner).
#[derive(Clone, Copy)]
pub enum Payer<'a> {
    Signer(&'a Keypair),
    PublicOnly(Pubkey),
}
impl<'a> Payer<'a> {
    pub fn pubkey(&self) -> Pubkey {
        match self {
            Payer::Signer(kp) => kp.pubkey(),
            Payer::PublicOnly(pk) => *pk,
        }
    }
    /// The keypair a real signature needs; `None` on a read-only path.
    pub fn signer(&self) -> Option<&'a Keypair> {
        match self {
            Payer::Signer(kp) => Some(kp),
            Payer::PublicOnly(_) => None,
        }
    }
}
impl<'a> From<&'a Keypair> for Payer<'a> {
    fn from(kp: &'a Keypair) -> Self {
        Payer::Signer(kp)
    }
}

/// The owner's payer: its keypair, or its public identity only (the econ4h calibration).
pub enum OwnerPayer {
    Signer(Keypair),
    PublicOnly(Pubkey),
}
impl OwnerPayer {
    pub fn pubkey(&self) -> Pubkey {
        self.as_ref().pubkey()
    }
    pub fn as_ref(&self) -> Payer<'_> {
        match self {
            OwnerPayer::Signer(kp) => Payer::Signer(kp),
            OwnerPayer::PublicOnly(pk) => Payer::PublicOnly(*pk),
        }
    }
    pub fn signer(&self) -> Option<&Keypair> {
        self.as_ref().signer()
    }
    /// DEC-18 §4.3: the owner's payer from its environment — a read-only measurement (`econ`, the
    /// econ4h calibration) loads no private key: its payer is the configured public identity
    /// (`P0_EXPECTED_PUBKEY`, `config/wallet.public.env`); every other owner loads its keypair
    /// (`P0_KEYPAIR_PATH`).
    pub fn of(get: &dyn Fn(&str) -> Option<String>, econ: bool) -> Result<Self, String> {
        if econ {
            let key = get("P0_EXPECTED_PUBKEY").ok_or(
                "P0_EXPECTED_PUBKEY required: the econ4h calibration's public payer (no private key is loaded)",
            )?;
            return key
                .parse()
                .map(OwnerPayer::PublicOnly)
                .map_err(|_| "P0_EXPECTED_PUBKEY invalid".into());
        }
        let path = get("P0_KEYPAIR_PATH").ok_or("missing P0_KEYPAIR_PATH")?;
        darkbot_solana::tx::load_keypair_json(&path)
            .map(OwnerPayer::Signer)
            .map_err(|e| e.to_string())
    }
}

/// DEC-17b: one configured signer of the owner — its key, roles and payer (its keypair; the
/// public key only for a treasury or on a read-only measurement).
pub struct OwnerSigner {
    pub pubkey: Pubkey,
    pub roles: Vec<&'static str>,
    pub payer: OwnerPayer,
}

/// DEC-17b: the owner's signers from its configuration (`signers`; none configured: the one
/// keypair as LIQUIDATOR+FEE_PAYER). A signing one's keypair is `P0_SIGNER_KEYPAIRS`' file for
/// its key (`<pubkey>=<path>,…`) or else `P0_KEYPAIR_PATH`, its public key checked against the
/// configured one; a treasury has no key (one named for it is refused); the econ4h calibration
/// loads none. The first is the pilot wallet — the first one that is its own fee payer (DEC-17b,
/// the first pilot: one wallet, LIQUIDATOR and FEE_PAYER), else (B4) the first liquidator. A
/// liquidator without its own fees needs a fee payer alone to pay it (`SIGNERS_NO_FEE_PAYER`).
pub fn owner_signers(
    configured: &[String],
    get: &dyn Fn(&str) -> Option<String>,
    econ: bool,
) -> Result<Vec<OwnerSigner>, String> {
    use darkbot_solana::runtime_config::{roles, signer_entry};
    if configured.is_empty() {
        let payer = OwnerPayer::of(get, econ)?;
        return Ok(vec![OwnerSigner {
            pubkey: payer.pubkey(),
            roles: roles::SIGNING.to_vec(),
            payer,
        }]);
    }
    let mut keypairs = std::collections::BTreeMap::new();
    for item in get("P0_SIGNER_KEYPAIRS")
        .unwrap_or_default()
        .split(',')
        .filter(|i| !i.trim().is_empty())
    {
        let (key, path) = item
            .split_once('=')
            .ok_or("P0_SIGNER_KEYPAIRS: PUBKEY=PATH,…")?;
        let key: Pubkey = key
            .trim()
            .parse()
            .map_err(|_| "P0_SIGNER_KEYPAIRS: invalid pubkey")?;
        keypairs.insert(key, path.trim().to_string());
    }
    let mut out = vec![];
    for entry in configured {
        let (pubkey, r) = signer_entry(entry)?;
        let payer = if r == [roles::TREASURY] {
            if keypairs.contains_key(&pubkey) {
                return Err(format!(
                    "SIGNER_TREASURY_KEY: {pubkey} is a treasury (read only, no key)"
                ));
            }
            OwnerPayer::PublicOnly(pubkey)
        } else if econ {
            OwnerPayer::PublicOnly(pubkey)
        } else {
            let path = keypairs
                .get(&pubkey)
                .cloned()
                .or_else(|| get("P0_KEYPAIR_PATH"))
                .ok_or_else(|| format!("SIGNER_KEYPAIR_MISSING: {pubkey} (P0_SIGNER_KEYPAIRS)"))?;
            let kp = darkbot_solana::tx::load_keypair_json(&path).map_err(|e| e.to_string())?;
            if kp.pubkey() != pubkey {
                return Err(format!(
                    "SIGNER_KEY_MISMATCH: the keypair for {pubkey} is another key's"
                ));
            }
            OwnerPayer::Signer(kp)
        };
        out.push(OwnerSigner {
            pubkey,
            roles: r,
            payer,
        });
    }
    let liquidates = |s: &OwnerSigner| s.roles.contains(&roles::LIQUIDATOR);
    if out.iter().any(|s| s.roles == [roles::LIQUIDATOR])
        && !out.iter().any(|s| s.roles == [roles::FEE_PAYER])
    {
        return Err("SIGNERS_NO_FEE_PAYER: a LIQUIDATOR without its own FEE_PAYER role is paid by a signer that is FEE_PAYER alone (DEC-17b B4)".into());
    }
    let first = out
        .iter()
        .position(|s| s.roles == roles::SIGNING)
        .or_else(|| out.iter().position(liquidates))
        .ok_or("SIGNERS_NO_PILOT_WALLET: the owner runs a LIQUIDATOR — its own FEE_PAYER, or paid by one (DEC-17b)")?;
    out.swap(0, first);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DEC-18 §4.3 (P0-024): the econ4h calibration never reads a private key — a keypair path
    /// that does not exist is not opened; its payer is the public identity. Every other owner
    /// loads its keypair (a missing one refuses the start).
    #[test]
    fn p0_024_the_calibration_loads_no_private_key() {
        let public = Pubkey::new_unique();
        let env = |k: &str| match k {
            "P0_EXPECTED_PUBKEY" => Some(public.to_string()),
            "P0_KEYPAIR_PATH" => Some("/nonexistent/keypair.json".to_string()),
            _ => None,
        };
        let payer = OwnerPayer::of(&env, true).unwrap();
        assert!(matches!(payer, OwnerPayer::PublicOnly(pk) if pk == public));
        assert!(payer.signer().is_none());
        assert!(
            OwnerPayer::of(&env, false).is_err(),
            "a signing owner loads its keypair"
        );
        assert!(OwnerPayer::of(&|_| None, true)
            .err()
            .unwrap()
            .starts_with("P0_EXPECTED_PUBKEY required"));
        let kp = Keypair::new();
        assert_eq!(Payer::from(&kp).pubkey(), kp.pubkey());
    }

    /// DEC-17b: the configured signers — each key checked against its entry, a treasury without
    /// a key, the econ4h calibration loading none; the wallet the owner runs is the first that is
    /// its own fee payer; unconfigured, the one keypair.
    #[test]
    fn dec17b_the_owner_loads_its_configured_signers() {
        let dir = std::env::temp_dir().join(format!(
            "p0-signers-{}-{}",
            std::process::id(),
            Pubkey::new_unique()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |kp: &Keypair| {
            let path = dir.join(format!("{}.json", kp.pubkey()));
            std::fs::write(&path, format!("{:?}", kp.to_bytes().to_vec())).unwrap();
            path.to_string_lossy().into_owned()
        };
        let (fee_payer, pilot, treasury) = (Keypair::new(), Keypair::new(), Pubkey::new_unique());
        let configured = vec![
            format!("{}:FEE_PAYER", fee_payer.pubkey()),
            format!("{}:LIQUIDATOR+FEE_PAYER", pilot.pubkey()),
            format!("{treasury}:TREASURY"),
        ];
        let keys = format!(
            "{}={},{}={}",
            fee_payer.pubkey(),
            write(&fee_payer),
            pilot.pubkey(),
            write(&pilot)
        );
        let env = |keys: String| move |k: &str| (k == "P0_SIGNER_KEYPAIRS").then(|| keys.clone());
        let signers = owner_signers(&configured, &env(keys.clone()), false).unwrap();
        assert_eq!(
            signers
                .iter()
                .map(|s| (s.pubkey, s.payer.signer().is_some()))
                .collect::<Vec<_>>(),
            [
                (pilot.pubkey(), true),
                (fee_payer.pubkey(), true),
                (treasury, false)
            ]
        );
        // The calibration loads no key.
        assert!(owner_signers(&configured, &env(keys.clone()), true)
            .unwrap()
            .iter()
            .all(|s| s.payer.signer().is_none()));
        // A key for the treasury, another key's file, a missing one, no pilot wallet: refused.
        let e = |c: &[String], keys: String| owner_signers(c, &env(keys), false).err().unwrap();
        assert!(e(
            &configured,
            format!("{keys},{treasury}={}", write(&Keypair::new()))
        )
        .starts_with("SIGNER_TREASURY_KEY"));
        assert!(e(
            &configured,
            format!(
                "{}={},{}={}",
                fee_payer.pubkey(),
                write(&fee_payer),
                pilot.pubkey(),
                write(&fee_payer)
            )
        )
        .starts_with("SIGNER_KEY_MISMATCH"));
        assert!(e(&configured, String::new()).starts_with("SIGNER_KEYPAIR_MISSING"));
        assert!(e(&configured[..1], keys.clone()).starts_with("SIGNERS_NO_PILOT_WALLET"));
        // DEC-17b B4: a liquidator without its own fees is the pilot when no wallet pays itself,
        // paid by a fee payer alone — never without one (a wallet with both roles pays itself).
        let liquidator = Keypair::new();
        let split = vec![
            format!("{}:LIQUIDATOR", liquidator.pubkey()),
            format!("{}:FEE_PAYER", fee_payer.pubkey()),
        ];
        let split_keys = format!(
            "{}={},{}={}",
            liquidator.pubkey(),
            write(&liquidator),
            fee_payer.pubkey(),
            write(&fee_payer)
        );
        let signers = owner_signers(&split, &env(split_keys.clone()), false).unwrap();
        assert_eq!(
            signers.iter().map(|s| s.pubkey).collect::<Vec<_>>(),
            [liquidator.pubkey(), fee_payer.pubkey()]
        );
        assert!(e(&split[..1], split_keys.clone()).starts_with("SIGNERS_NO_FEE_PAYER"));
        let with_pilot = vec![
            split[0].clone(),
            format!("{}:LIQUIDATOR+FEE_PAYER", pilot.pubkey()),
        ];
        assert!(e(
            &with_pilot,
            format!("{split_keys},{}={}", pilot.pubkey(), write(&pilot))
        )
        .starts_with("SIGNERS_NO_FEE_PAYER"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
