//! The versioned economics snapshot (Economics gate §15): what is priced
//! and costed, by which model, under which policies, from which evidence.
//!
//! This table is the one statement of the economic scope: every observed
//! venue/version/mode/path is `PRICED` (an exact local quote validated
//! against the protocol), `COSTED` (only its network cost is modeled) or
//! `UNSUPPORTED` with the reason (§19: explicit support or explicit
//! UNSUPPORTED, never an assumed zero cost). The snapshot identity is the
//! sha256 of [`manifest`]: the chain, the model version, the bytes of every
//! bound file (the model's sources and its evidence), the scopes and the
//! policies. Changing any of them is a new snapshot; replay binds to its
//! hash (`g_economics_snapshot`, `bsc/native/evidence/g/`).

use std::fmt::Write as _;

/// The model version: raised with every semantic change of a rule here.
pub const MODEL_VERSION: &str = "bsc-economics-2";

/// The chain every scope belongs to.
pub const CHAIN_ID: u64 = 56;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    /// An exact local quote, validated against the protocol.
    Priced,
    /// Only the network cost is modeled (no economic effect of its own).
    Costed,
    Unsupported,
}

impl Support {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Priced => "PRICED",
            Self::Costed => "COSTED",
            Self::Unsupported => "UNSUPPORTED",
        }
    }
}

/// One venue/version/mode/path and how it is priced or why not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scope {
    pub venue: &'static str,
    /// Contract, deployment version and token version/mode.
    pub surface: &'static str,
    pub path: &'static str,
    pub support: Support,
    /// The rule (and its evidence), or the reason it is unsupported.
    pub rule: &'static str,
}

const FLAP_V6: &str = "Portal 0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0 v5.27.0; token version 6, curve 20, native BNB quote";

pub const SCOPES: &[Scope] = &[
    Scope {
        venue: "FLAP",
        surface: FLAP_V6,
        path: "BUY",
        support: Support::Priced,
        rule: "native_curve20::quote_native_buy: combined fee floor(input*(protocol+tax)/1e4), tax the remainder; supply(reserve+net) with the ceil virtual reserve; the origin's holding allowance refuses a partial fill and reaching dexSupplyThresh refuses a migration; Portal.quoteExactInput parity to the unit (fixtures/flap/quote_parity_g02*.txt) and executed TokenBought parity (fixtures/flap/executed_parity_g02.txt)",
    },
    Scope {
        venue: "FLAP",
        surface: FLAP_V6,
        path: "SELL",
        support: Support::Priced,
        rule: "native_curve20::quote_native_sell: reserve ceil(k*1e18/(T+h-s))-r; protocol fee and tax each floored; Portal.quoteExactInput parity to the unit (fixtures/flap/quote_parity_g02*.txt) and executed TokenSold parity (fixtures/flap/executed_parity_g02.txt)",
    },
    Scope {
        venue: "FLAP",
        surface: FLAP_V6,
        path: "ADMITTED BUY",
        support: Support::Priced,
        rule: "admit_native_buy: a classified newTokenV6 launch without extension, the state directly after it, witnessed venue terms, 0.001 BNB (G0.6a EVM execution), no holding cap, the profile's DEX threshold 8e26, minOutput the slippage floor",
    },
    Scope {
        venue: "FLAP",
        surface: FLAP_V6,
        path: "ROUND TRIP",
        support: Support::Priced,
        rule: "flap::position: entry = BUY input + BUY network fee for the quoted tokens; exit = SELL of exactly those tokens, its quoted native amount less the approval (unless given) and SELL network fees; the network fees the classes' adverse counterfactual bound at the given gas price",
    },
    Scope {
        venue: "FLAP",
        surface: FLAP_V6,
        path: "APPROVE BEFORE SELL",
        support: Support::Costed,
        rule: "network: the approval receipt class of fixtures/bsc/receipts_20261026.txt",
    },
    Scope {
        venue: "FLAP",
        surface: "Portal v5.27.0; token version 7 (newTokenV7)",
        path: "BUY, SELL",
        support: Support::Unsupported,
        rule: "a live version-7 token is charged about 0.25 % more than the version-6 rule on both sides (g02_flap_parity); no verified rule",
    },
    Scope {
        venue: "FLAP",
        surface: "Portal v5.27.0; NFT-gated market (nftGate != 0)",
        path: "BUY, SELL",
        support: Support::Unsupported,
        rule: "trading needs nft.balanceOf(tx.origin) >= 1; buys capped at 2 % of the supply",
    },
    Scope {
        venue: "FLAP",
        surface: "Portal v5.27.0; DOGE quote token (extension)",
        path: "BUY, SELL",
        support: Support::Unsupported,
        rule: "its oracle signature and signer role are not verifiable in Rust; the profile refuses it",
    },
    Scope {
        venue: "FLAP",
        surface: "Portal v5.27.0; tax-disabled or fee-exempt caller",
        path: "BUY, SELL",
        support: Support::Unsupported,
        rule: "no witnessed execution of either branch",
    },
    Scope {
        venue: "FLAP",
        surface: "Portal v5.27.0; any token",
        path: "PERMIT SELL",
        support: Support::Unsupported,
        rule: "no witnessed permit path",
    },
    Scope {
        venue: "FLAP",
        surface: "Portal v5.27.0; any token",
        path: "MIGRATION, POST-DEX EXIT",
        support: Support::Unsupported,
        rule: "graduation and the DEX venue after it are not modeled: a BUY reaching dexSupplyThresh is refused, and a position whose token graduates has no exit cost",
    },
    Scope {
        venue: "FOUR",
        surface: "TokenManager2 0x5c952063c7fc8610ffdb798152d69f0b9550762b (implementation 0xf32ee25a)",
        path: "BUY, SELL",
        support: Support::Unsupported,
        rule: "the implementation source is unverified and no helper/eth_call parity is established; the Phase E decoder names amounts without economic meaning",
    },
    Scope {
        venue: "OPENFOUR",
        surface: "Core 0xebe7b6c1089d9f72ad07f34e36d898e44e5e27f3",
        path: "BUY, SELL",
        support: Support::Unsupported,
        rule: "the implementation source is unverified and no helper/eth_call parity is established",
    },
    Scope {
        venue: "BSC",
        surface: "chain 56",
        path: "NETWORK COST",
        support: Support::Costed,
        rule: "network: realized gas_used*effective_gas_price for a success and an included revert; one inclusion per nonce lifecycle, a non-included attempt free; the counterfactual the class's largest observed gas_used at the replay block's price (fixtures/bsc/receipts_20261026.txt)",
    },
];

/// The policies every priced or costed path follows.
pub const POLICIES: &[(&str, &str)] = &[
    (
        "rounding",
        "integer base units, U256 with overflow errors; every division names its direction (fees floor, curve virtual reserve ceil, minOutput floor)",
    ),
    (
        "freshness",
        "the admitted state is the one directly after the classified launch: no trade of the token before the BUY, the same code generation and pending candidate as the state snapshot",
    ),
    (
        "slippage",
        "minOutput = floor(expected*(1e4-bps)/1e4) > 0, a bound and never the realized cost",
    ),
    (
        "holding_cap",
        "a BUY above the origin's allowance is refused, never quoted as the partial fill",
    ),
    (
        "migration",
        "a BUY whose supply after it reaches the token's dexSupplyThresh is refused",
    ),
    (
        "gas_evidence",
        "receipts only; no typical-gas constant; a class without evidence fails closed",
    ),
    (
        "unknown",
        "an unknown version, mode, tax or path is UNSUPPORTED, never a zero cost",
    ),
];

/// The repository files whose bytes are the model and its evidence.
pub const BOUND_FILES: &[&str] = &[
    "bsc/engine/crates/trading/src/economics/arith.rs",
    "bsc/engine/crates/trading/src/economics/fees.rs",
    "bsc/engine/crates/trading/src/economics/flap/curve.rs",
    "bsc/engine/crates/trading/src/economics/flap/mod.rs",
    "bsc/engine/crates/trading/src/economics/flap/native_curve20.rs",
    "bsc/engine/crates/trading/src/economics/flap/position.rs",
    "bsc/engine/crates/trading/src/economics/network.rs",
    "bsc/engine/crates/trading/src/economics/policy.rs",
    "bsc/engine/crates/trading/src/economics/slippage.rs",
    "bsc/engine/crates/trading/src/economics/snapshot.rs",
    "bsc/engine/crates/trading/src/venues/flap/profile.rs",
    "bsc/engine/crates/trading/fixtures/bsc/receipts_20261026.txt",
    "bsc/engine/crates/trading/fixtures/flap/executed_parity_g02.txt",
    "bsc/engine/crates/trading/fixtures/flap/quote_parity_g02.txt",
    "bsc/engine/crates/trading/fixtures/flap/quote_parity_g02_v2.txt",
];

/// The canonical text the snapshot hash is taken of: `files` are the
/// [`BOUND_FILES`] with their sha256, in that order.
pub fn manifest(files: &[(&str, [u8; 32])]) -> String {
    let mut out =
        format!("darkbot.bsc.economics-snapshot\nchain {CHAIN_ID}\nmodel {MODEL_VERSION}\n");
    for (path, sha) in files {
        let _ = write!(out, "file {path} ");
        for b in sha {
            let _ = write!(out, "{b:02x}");
        }
        out.push('\n');
    }
    for s in SCOPES {
        let _ = writeln!(
            out,
            "scope {} | {} | {} | {} | {}",
            s.venue,
            s.surface,
            s.path,
            s.support.name(),
            s.rule
        );
    }
    for (name, rule) in POLICIES {
        let _ = writeln!(out, "policy {name} | {rule}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every observed surface states its support; the priced ones are the
    /// version-6 Flap quotes only, and nothing of Four is priced.
    #[test]
    fn the_scope_is_explicit_and_four_is_not_priced() {
        let priced: Vec<_> = SCOPES
            .iter()
            .filter(|s| s.support == Support::Priced)
            .map(|s| (s.venue, s.path))
            .collect();
        assert_eq!(
            priced,
            [
                ("FLAP", "BUY"),
                ("FLAP", "SELL"),
                ("FLAP", "ADMITTED BUY"),
                ("FLAP", "ROUND TRIP")
            ]
        );
        for venue in ["FOUR", "OPENFOUR"] {
            assert!(
                SCOPES
                    .iter()
                    .filter(|s| s.venue == venue)
                    .all(|s| s.support == Support::Unsupported)
            );
        }
        assert!(SCOPES.iter().all(|s| !s.rule.is_empty()));
    }

    /// The manifest names every bound file in order and changes with any
    /// byte of them.
    #[test]
    fn the_manifest_binds_every_file() {
        let files: Vec<(&str, [u8; 32])> = BOUND_FILES.iter().map(|p| (*p, [0u8; 32])).collect();
        let a = manifest(&files);
        assert!(a.starts_with("darkbot.bsc.economics-snapshot\nchain 56\nmodel bsc-economics-2\n"));
        assert_eq!(a.matches("\nfile ").count(), BOUND_FILES.len());
        let mut other = files.clone();
        other[3].1[0] = 1;
        assert_ne!(manifest(&other), a);
    }
}
