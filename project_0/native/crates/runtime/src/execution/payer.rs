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
}
