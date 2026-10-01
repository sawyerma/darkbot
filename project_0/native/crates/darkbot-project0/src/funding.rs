//! FundingClass — Kapitalbindung als explizite Execution-Dimension.
//!
//! WICHTIG: Die enum-Reihenfolge ist **keine** finale Handelsentscheidung.
//! `feasible_classes()` erzeugt nur technisch/protokollarisch mögliche Funding-Pfade.
//! Welche Variante tatsächlich ausgeführt wird, entscheidet P4A anhand vollständiger
//! Economics + TX-Feasibility + Safety. Damit wird ZERO_PREFUND nicht blind bevorzugt,
//! wenn NO_SWAP/JIT netto besser ist.

use solana_sdk::pubkey::Pubkey;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FundingClass {
    /// Start → Withdraw → Swap(allowlisted) → Repay → End in **einer** TX; Kapital ≈ Fees.
    ZeroPrefundSameTx = 0,
    /// Repay aus vorhandenem Liability-Inventar; seized Asset behalten/später verwerten.
    NoSwapInventory = 1,
    /// Flash-Borrow → Liquidation → Swap → Flash-Repay. Nur bei Protokollen, die es zulassen.
    FlashFunded = 2,
    /// Eva-Prinzip: nur den Fehlbetrag vor der Liquidation kaufen.
    JitShortfall = 3,
    /// Klassisch vorgehaltenes Liability-Inventar; höchste dauerhafte Kapitalbindung.
    PreheldInventory = 4,
}

impl FundingClass {
    /// Nur deterministische Iterationsreihenfolge. **Keine Ranking-/Ausführungspräferenz.**
    pub const ALL: [FundingClass; 5] = [
        FundingClass::ZeroPrefundSameTx,
        FundingClass::NoSwapInventory,
        FundingClass::FlashFunded,
        FundingClass::JitShortfall,
        FundingClass::PreheldInventory,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            FundingClass::ZeroPrefundSameTx => "ZERO_PREFUND_SAME_TX",
            FundingClass::NoSwapInventory => "NO_SWAP_INVENTORY",
            FundingClass::FlashFunded => "FLASH_FUNDED",
            FundingClass::JitShortfall => "JIT_SHORTFALL",
            FundingClass::PreheldInventory => "PREHELD_INVENTORY",
        }
    }

    /// Protokoll-Gültigkeit für Project 0 (Allowlist aus `liquidate_start.rs`).
    pub const fn valid_for_project0(self) -> bool {
        !matches!(self, FundingClass::FlashFunded)
    }

    /// Anzahl Value-TXs vor/inkl. Receivership, ohne separate Jito-Tip-TX.
    pub const fn tx_count(self) -> usize {
        match self {
            FundingClass::JitShortfall => 2,
            _ => 1,
        }
    }
}

/// Opportunity-spezifische Funding-Fakten.
#[derive(Clone, Debug)]
pub struct FundingContext {
    pub liability_mint: Pubkey,
    /// Token-Basis (Routing/Repay-Betrag).
    pub liability_needed: u64,
    pub liability_inventory: u64,
    /// [FIX-16] Werte in USD-Micro — Economics/Safety rechnen nur hiermit.
    pub liability_needed_value: u64,
    pub liability_inventory_value: u64,
    /// [FIX-17] Funding-Quelle für JIT (Base-Inventar, z. B. USDC): Bank + verfügbarer Wert.
    pub funding_bank: Option<Pubkey>,
    pub funding_available_value: u64,
    /// ZERO_PREFUND ist nur machbar, wenn Fixture/Simulation die Klasse für diese
    /// Opportunity-/Route-Klasse qualifiziert hat.
    pub zero_prefund_qualified: bool,
    /// Mindestens eine allowlisted, aktuell ausführbare Swap-Route existiert.
    pub route_available: bool,
}

/// Technische/protokollarische Machbarkeit einer Funding-Klasse.
/// Keine Economics- oder Ranking-Entscheidung.
pub fn is_feasible(c: FundingClass, ctx: &FundingContext) -> bool {
    if !c.valid_for_project0() {
        return false;
    }
    match c {
        FundingClass::ZeroPrefundSameTx => ctx.zero_prefund_qualified && ctx.route_available,
        FundingClass::NoSwapInventory => ctx.liability_inventory >= ctx.liability_needed,
        FundingClass::FlashFunded => false,
        FundingClass::JitShortfall => {
            ctx.funding_bank.is_some()
                && ctx.liability_inventory < ctx.liability_needed
                && ctx.funding_available_value >= ctx.liability_needed_value.saturating_sub(ctx.liability_inventory_value)
        }
        FundingClass::PreheldInventory => ctx.liability_inventory >= ctx.liability_needed,
    }
}

/// Alle machbaren Klassen. P4A/P5 müssen daraus separate Execution-Candidates bauen
/// und nach realem Netto + Kapital + TX-Feasibility auswählen.
pub fn feasible_classes(ctx: &FundingContext) -> Vec<FundingClass> {
    FundingClass::ALL
        .into_iter()
        .filter(|c| is_feasible(*c, ctx))
        .collect()
}

/// [FIX-16] Gebundenes Liability-Kapital in **USD-Micro** (Working Capital; SOL für Fees/Tip ist ein
/// separates Safety-Limit). ZERO_PREFUND: 0, weil das Collateral den Repay finanziert.
pub fn capital_required(c: FundingClass, ctx: &FundingContext) -> u64 {
    match c {
        FundingClass::ZeroPrefundSameTx | FundingClass::FlashFunded => 0,
        FundingClass::NoSwapInventory | FundingClass::PreheldInventory => ctx.liability_needed_value,
        FundingClass::JitShortfall => ctx.liability_needed_value.saturating_sub(ctx.liability_inventory_value),
    }
}

/// [FIX-17] Token-Fehlbetrag für die JIT-Buy-Route (Token-Basis der Liability).
pub fn shortfall_token_amount(ctx: &FundingContext) -> u64 {
    ctx.liability_needed.saturating_sub(ctx.liability_inventory)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(inv: u64, need: u64, zp: bool, route: bool) -> FundingContext {
        FundingContext {
            liability_mint: Pubkey::new_unique(),
            liability_needed: need,
            liability_inventory: inv,
            liability_needed_value: need,
            liability_inventory_value: inv,
            funding_bank: if route { Some(Pubkey::new_unique()) } else { None },
            funding_available_value: 1_000_000,
            zero_prefund_qualified: zp,
            route_available: route,
        }
    }

    #[test]
    fn emits_all_feasible_candidates_instead_of_picking_one() {
        let c = feasible_classes(&ctx(100, 100, true, true));
        assert!(c.contains(&FundingClass::ZeroPrefundSameTx));
        assert!(c.contains(&FundingClass::NoSwapInventory));
        assert!(c.contains(&FundingClass::PreheldInventory));
        assert!(!c.contains(&FundingClass::FlashFunded));
    }

    #[test]
    fn jit_is_only_shortfall_fallback() {
        let c = feasible_classes(&ctx(30, 100, false, true));
        assert_eq!(c, vec![FundingClass::JitShortfall]);
        assert_eq!(capital_required(FundingClass::JitShortfall, &ctx(30, 100, false, true)), 70);
    }

    #[test]
    fn no_route_no_inventory_means_no_candidate() {
        assert!(feasible_classes(&ctx(0, 100, false, false)).is_empty());
        assert!(!FundingClass::FlashFunded.valid_for_project0());
        assert_eq!(capital_required(FundingClass::ZeroPrefundSameTx, &ctx(0, 100, true, true)), 0);
        assert_eq!(FundingClass::JitShortfall.tx_count(), 2);
    }
}
