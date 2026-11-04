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
pub const MODEL_VERSION: &str = "bsc-economics-6";

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

const FLAP_V6: &str = "Portal 0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0 v5.27.2; token version 6, curve 20, native BNB quote";

const FOUR_CLASSIC: &str = "TokenManager2 0x5c952063c7fc8610ffdb798152d69f0b9550762b, implementation 0xa38d9d48 (the curve rules also under 0xf32ee25a); native BNB quote, tradingFeeRate 100, minTradingFee 0, a template word of fourmeme::profile with its tax, on the curve";

pub const SCOPES: &[Scope] = &[
    Scope {
        venue: "FLAP",
        surface: FLAP_V6,
        path: "BUY",
        support: Support::Priced,
        rule: "native_curve20::quote_native_buy: combined fee floor(input*(protocol+tax)/1e4), tax the remainder; supply(reserve+net) with the ceil virtual reserve; the origin's holding allowance refuses a partial fill and reaching dexSupplyThresh refuses a migration; Portal.quoteExactInput parity to the unit (fixtures/flap/quote_parity_g02*.txt) and executed TokenBought parity (fixtures/flap/executed_parity_g02*.txt); on v5.27.2: quote v4 and executed v3 with the implementation checked, and its behaviour parity with the source-verified v5.27.0 (bsc/native/evidence/g01/flap_impl_parity_*); quote v3 and executed v1/v2 were taken after the upgrade at block 129871644 although their headers name v5.27.0",
    },
    Scope {
        venue: "FLAP",
        surface: FLAP_V6,
        path: "SELL",
        support: Support::Priced,
        rule: "native_curve20::quote_native_sell: reserve ceil(k*1e18/(T+h-s))-r; protocol fee and tax each floored; Portal.quoteExactInput parity to the unit (fixtures/flap/quote_parity_g02*.txt) and executed TokenSold parity (fixtures/flap/executed_parity_g02*.txt); on v5.27.2 as the BUY's evidence",
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
        path: "CURVE BUY (Strategy C)",
        support: Support::Priced,
        rule: "admit_curve_buy: a curve replayed exactly from canonical trades (strategy::book), in the PricedToken scope, witnessed venue terms, the policy's input, no holding cap (a capped or NFT-gated token is never tracked), minOutput the slippage floor; equal to the Portal's quote at the parity v3 states, taken on v5.27.2 (curve_admission_is_the_parity_quote)",
    },
    Scope {
        venue: "FLAP",
        surface: FLAP_V6,
        path: "CURVE SELL (Strategy C exit)",
        support: Support::Priced,
        rule: "admit_curve_sell: the position's tokens on its shadow curve, the witnessed sell fee, minOutput the ExitPolicy's slippage floor; approve(Portal, exactly those tokens) at the nonce before it; both calls as witnessed (fixtures/flap/exit_calls_witnessed.txt)",
    },
    Scope {
        venue: "FLAP",
        surface: FLAP_V6,
        path: "ROUND TRIP",
        support: Support::Priced,
        rule: "flap::position: entry = BUY input + BUY network fee for the quoted tokens; exit = SELL of exactly those tokens, its quoted native amount less the approval (unless given) and SELL network fees; the network fees each class's largest observed gas used at the given gas price (an assumption, not a measurement of the trade)",
    },
    Scope {
        venue: "FLAP",
        surface: FLAP_V6,
        path: "APPROVE BEFORE SELL",
        support: Support::Costed,
        rule: "network: the approval receipt class of bsc/native/evidence/gas/receipts_20261026.txt and bsc/native/evidence/gas/flap_gas_20261031*.txt (approve(Portal) of tokens the Portal knows)",
    },
    Scope {
        venue: "FLAP",
        surface: "Portal v5.27.2; every token version but 6 (live: 2 and 7)",
        path: "BUY, SELL",
        support: Support::Unsupported,
        rule: "PricedToken::new refuses it; a live version-7 token is charged about 0.25 % more than the version-6 rule on both sides (fixtures/flap/quote_parity_g02_v3.txt); no verified rule for any",
    },
    Scope {
        venue: "FLAP",
        surface: "Portal v5.27.2; another curve or quote token, or a token not Tradable",
        path: "BUY, SELL",
        support: Support::Unsupported,
        rule: "PricedToken::new refuses it; the g02 tools record such launches as skipped with the reason",
    },
    Scope {
        venue: "FLAP",
        surface: FLAP_V6,
        path: "REVERTED BUY, REVERTED SELL",
        support: Support::Unsupported,
        rule: "no reverted-trade outcome in flap::position; the gas sample's reverted swaps are a cost class only; reverts are keyed by the attempted action (network::Action), so a reverted launch is no evidence of one",
    },
    Scope {
        venue: "FLAP",
        surface: "Portal v5.27.2; NFT-gated market (nftGate != 0)",
        path: "BUY, SELL",
        support: Support::Unsupported,
        rule: "trading needs nft.balanceOf(tx.origin) >= 1; buys capped at 2 % of the supply",
    },
    Scope {
        venue: "FLAP",
        surface: "Portal v5.27.2; DOGE quote token (extension)",
        path: "BUY, SELL",
        support: Support::Unsupported,
        rule: "its oracle signature and signer role are not verifiable in Rust; the profile refuses it",
    },
    Scope {
        venue: "FLAP",
        surface: "Portal v5.27.2; tax-disabled or fee-exempt caller",
        path: "BUY, SELL",
        support: Support::Unsupported,
        rule: "no witnessed execution of either branch",
    },
    Scope {
        venue: "FLAP",
        surface: "Portal v5.27.2; any token",
        path: "PERMIT SELL",
        support: Support::Unsupported,
        rule: "no witnessed permit path",
    },
    Scope {
        venue: "FLAP",
        surface: "Portal v5.27.2; any token",
        path: "MIGRATION, POST-DEX EXIT",
        support: Support::Unsupported,
        rule: "graduation and the DEX venue after it are not modeled: a BUY reaching dexSupplyThresh is refused, and a position whose token graduates has no exit cost",
    },
    Scope {
        venue: "FOUR",
        surface: FOUR_CLASSIC,
        path: "BUY",
        support: Support::Priced,
        rule: "four_classic::quote_buy: budget floor(funds*1e4/(1e4+rate+tax)), tokens floor_gwei(T-ceil(K*1e18/(y+budget))) with y=floor(K*1e18/T), cost floor(K*1e18/(T-tokens))-y, fee floor(cost*rate/1e4), tax floor(cost*tax/1e4), the rest refunded; buyTokenAMAP executed by eth_call probes on mainnet state up to 461245 blocks after the launch (tokens, wei spent, minAmount accepted at the tokens and refused one above) and the launch receipts' creator purchases; executed direct trades of a BSC-USD-quote token (paid by approval) for the curve and fee; Helper3.tryBuy is the untaxed curve (fixtures/fourmeme/classic_parity_20261104.txt)",
    },
    Scope {
        venue: "FOUR",
        surface: FOUR_CLASSIC,
        path: "SELL",
        support: Support::Priced,
        rule: "four_classic::quote_sell: gross y-floor(K*1e18/(T+tokens)), less fee floor(gross*rate/1e4) and tax floor(gross*tax/1e4); sellToken executed by the probes; executed direct sales for the curve and fee; Helper3.trySell is the untaxed curve (fixtures/fourmeme/classic_parity_20261104.txt)",
    },
    Scope {
        venue: "FOUR",
        surface: FOUR_CLASSIC,
        path: "LAUNCH CURVE, CREATOR BUY",
        support: Support::Priced,
        rule: "four_classic::launch_curve from createToken totalSupply, maxOffers and maxRaising (every witnessed launch's K, every quote); the creator BUY a BUY of the args creator value with the word's tax, for a template word of fourmeme::profile and its witnessed creator values (their receipts' TokenPurchase to the wei, fixtures/fourmeme/classic_parity_20261104.txt)",
    },
    Scope {
        venue: "FOUR",
        surface: "TokenManager2; a template word outside fourmeme::profile (another quote asset, AntiSniper feeSetting, an unmeasured tax), minTradingFee above 0, an unwitnessed launch argument",
        path: "BUY, SELL",
        support: Support::Unsupported,
        rule: "classify_classic_launch refuses the launch with the field; the quote refuses a minimum fee (UnwitnessedMinFee); no witness of the AntiSniper fee schedule or a non-BNB quote path",
    },
    Scope {
        venue: "FOUR",
        surface: FOUR_CLASSIC,
        path: "GRADUATION",
        support: Support::Unsupported,
        rule: "a BUY taking every remaining offer is refused (ReachesMaxOffers): the helper caps it and the liquidity add is not modeled",
    },
    Scope {
        venue: "FOUR",
        surface: FOUR_CLASSIC,
        path: "ADMITTED BUY",
        support: Support::Priced,
        rule: "admit_classic_buy: a launch classified on a validated deployment (fourmeme::profile, FourDeployment::validate), the state directly after it, 0.001 or 0.1 BNB (buyTokenAMAP executed by probes on that state, tokens and wei to the unit), minAmount the slippage floor; signed and bound by fourmeme::binding",
    },
    Scope {
        venue: "FOUR",
        surface: FOUR_CLASSIC,
        path: "ROUND TRIP, NETWORK COST",
        support: Support::Unsupported,
        rule: "no gas evidence of a Four call and no runtime path yet (R12c open)",
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
        rule: "network: realized gas_used*effective_gas_price for a success and an included revert; one inclusion per nonce lifecycle, a non-included attempt free; the counterfactual the class's largest observed gas_used at the replay block's price, an assumption (bsc/native/evidence/gas/receipts_20261026.txt, bsc/native/evidence/gas/flap_gas_20261031*.txt: three windows of the v5.27.2 era, BSC3-079, the receipts per class in each header; a BUY in its token's launch block its own class)",
    },
];

/// The policies every priced or costed path follows.
pub const POLICIES: &[(&str, &str)] = &[
    (
        "rounding",
        "integer base units, U256 with overflow errors; every division names its direction (fees floor, minOutput floor; Flap: curve virtual reserve ceil; Four Classic: virtual quote reserve floor, the BUY's new token reserve ceil, tokens gwei-floored)",
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
        "a BUY whose supply after it reaches the token's dexSupplyThresh is refused, and a BUY or SELL of a token at or past it (QuoteError::Migrated): a migrated token is never quoted",
    ),
    (
        "gas_evidence",
        "receipts only; no typical-gas constant; a class without evidence fails closed; the counterfactual is the largest observed gas used of its frozen sample, an assumption; a BUY that graduated its token is the migration class, not a BUY",
    ),
    (
        "unknown",
        "an unknown version, mode, tax or path is UNSUPPORTED, never a zero cost",
    ),
];

/// The repository files whose bytes are the model and its evidence.
pub const BOUND_FILES: &[&str] = &[
    "bsc/engine/crates/trading/src/economics/arith.rs",
    "bsc/engine/crates/trading/src/economics/flap/doge_v3.rs",
    "bsc/engine/crates/trading/src/economics/fees.rs",
    "bsc/engine/crates/trading/src/economics/flap/curve.rs",
    "bsc/engine/crates/trading/src/economics/flap/mod.rs",
    "bsc/engine/crates/trading/src/economics/flap/native_curve20.rs",
    "bsc/engine/crates/trading/src/economics/flap/position.rs",
    "bsc/engine/crates/trading/src/economics/four_classic.rs",
    "bsc/engine/crates/trading/src/economics/network.rs",
    "bsc/engine/crates/trading/src/economics/policy.rs",
    "bsc/engine/crates/trading/src/economics/slippage.rs",
    "bsc/engine/crates/trading/src/economics/snapshot.rs",
    "bsc/engine/crates/trading/src/venues/flap/abi.rs",
    "bsc/engine/crates/trading/src/venues/flap/deployment.rs",
    "bsc/engine/crates/trading/src/venues/flap/profile.rs",
    "bsc/engine/crates/trading/src/venues/fourmeme/abi.rs",
    "bsc/engine/crates/trading/src/venues/fourmeme/deployment.rs",
    "bsc/engine/crates/trading/src/venues/fourmeme/profile.rs",
    "bsc/engine/crates/trading/src/venues/fourmeme/recognize.rs",
    "bsc/native/evidence/gas/flap_gas_20261031.txt",
    "bsc/native/evidence/gas/flap_gas_20261031_w130100000.txt",
    "bsc/native/evidence/gas/flap_gas_20261031_w130500000.txt",
    "bsc/native/evidence/gas/receipts_20261026.txt",
    "bsc/engine/crates/trading/fixtures/flap/executed_parity_g02.txt",
    "bsc/engine/crates/trading/fixtures/flap/executed_parity_g02_v2.txt",
    "bsc/engine/crates/trading/fixtures/flap/executed_parity_g02_v3.txt",
    "bsc/engine/crates/trading/fixtures/flap/exit_calls_witnessed.txt",
    "bsc/engine/crates/trading/fixtures/flap/quote_parity_g02.txt",
    "bsc/engine/crates/trading/fixtures/flap/quote_parity_g02_v2.txt",
    "bsc/engine/crates/trading/fixtures/flap/quote_parity_g02_v3.txt",
    "bsc/engine/crates/trading/fixtures/flap/quote_parity_g02_v4.txt",
    "bsc/engine/crates/trading/fixtures/fourmeme/classic_parity_20261104.txt",
];

/// The economics sources not bound, each with its reason (Phase G gate M4:
/// every file under `economics/` is bound or named here).
pub const EXCLUDED_FILES: &[(&str, &str)] = &[
    (
        "bsc/engine/crates/trading/src/economics/mod.rs",
        "module list only",
    ),
    (
        "bsc/engine/crates/trading/src/economics/network/tests.rs",
        "tests of network.rs against its bound fixture",
    ),
    (
        "bsc/engine/crates/trading/src/economics/four_classic/tests.rs",
        "tests of four_classic.rs against its bound fixture",
    ),
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
    /// version-6 Flap quotes and the Four Classic curve (not yet admitted),
    /// nothing of OpenFour.
    #[test]
    fn the_scope_is_explicit_and_openfour_is_not_priced() {
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
                ("FLAP", "CURVE BUY (Strategy C)"),
                ("FLAP", "CURVE SELL (Strategy C exit)"),
                ("FLAP", "ROUND TRIP"),
                ("FOUR", "BUY"),
                ("FOUR", "SELL"),
                ("FOUR", "LAUNCH CURVE, CREATOR BUY"),
                ("FOUR", "ADMITTED BUY")
            ]
        );
        assert!(
            SCOPES
                .iter()
                .filter(|s| s.venue == "OPENFOUR")
                .all(|s| s.support == Support::Unsupported)
        );
        assert!(SCOPES.iter().all(|s| !s.rule.is_empty()));
    }

    /// Phase G gate M4: every source under `economics/` is bound or
    /// excluded with a reason, and nothing is both.
    #[test]
    fn every_economics_source_is_bound_or_excluded() {
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(p);
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        walk(&root.join("src/economics"), &mut files);
        assert!(files.len() > 10);
        for f in files {
            let rel = format!(
                "bsc/engine/crates/trading/{}",
                f.strip_prefix(root).unwrap().to_string_lossy()
            );
            let bound = BOUND_FILES.contains(&rel.as_str());
            let excluded = EXCLUDED_FILES.iter().any(|(p, _)| *p == rel);
            assert!(
                bound != excluded,
                "{rel}: bound {bound}, excluded {excluded}"
            );
        }
    }

    /// The manifest names every bound file in order and changes with any
    /// byte of them.
    #[test]
    fn the_manifest_binds_every_file() {
        let files: Vec<(&str, [u8; 32])> = BOUND_FILES.iter().map(|p| (*p, [0u8; 32])).collect();
        let a = manifest(&files);
        assert!(a.starts_with("darkbot.bsc.economics-snapshot\nchain 56\nmodel bsc-economics-6\n"));
        assert_eq!(a.matches("\nfile ").count(), BOUND_FILES.len());
        let mut other = files.clone();
        other[3].1[0] = 1;
        assert_ne!(manifest(&other), a);
    }
}
