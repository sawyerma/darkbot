//! [FIX-2] `P0ReceivershipBuilder`: erzeugt pro machbarer FundingClass einen eigenen Plan.
//!
//! Zahlen (Beträge, realisierbarer Wert, Kosten) kommen aus einem `Sizing`-Provider (P4A-03);
//! ohne Sizing gibt es **keinen** Plan (fail closed), nie geschätzte Beträge.
//! Routen kommen aus einem `RouteProvider` (allowlisted Instructions); ohne Route keine
//! ZERO_PREFUND-/JIT-Pläne.
//!
//! Nur mit Feature `crate_health`. Status: UNTESTED / VERIFY.

/// [P5] Plan rejection code when the FeeState (fee receiver) could not be read: UNKNOWN.
pub const FEE_STATE_UNAVAILABLE: &str = "FEE_STATE_UNAVAILABLE";

#[cfg(feature = "crate_health")]
pub mod p0 {
    use crate::execution::pipeline::{
        PlanError, PlanErrorKind, ReceivershipBuilder, ReceivershipPlan, SolCosts,
    };
    use crate::execution::prepared::{Context, PreparedCache, Skeleton};
    use crate::execution::route_cap::{
        FitTrace, Hint, RouteCapCache, RouteCapKey, RouteFootprintClass,
    };
    use darkbot_project0::economics::{CandidateInput, Path};
    use darkbot_project0::funding::{self, FundingClass, FundingContext};
    use darkbot_project0::health::crate_impl::CrateHealth;
    use darkbot_project0::health::{AccountSource, HealthEngine, HealthView};
    use darkbot_project0::planner::{InstructionTable, RouteKind};
    use darkbot_project0::receivership::{builders, ReceivershipParams};
    use darkbot_project0::sizing::{NoSizeReason, SizingError, SizingErrorKind};
    use darkbot_solana::tx::{self, BuildError, BuildVersion, Resources, TxLimits, TxPlan};
    use marginfi_type_crate::types::Bank;
    use solana_message::AddressLookupTableAccount; /* [FIX-13] */
    use solana_sdk::hash::Hash;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;
    use spl_associated_token_account::get_associated_token_address_with_program_id;
    use std::time::Instant;

    /// B4: account size the pinned program allocates for a LiquidationRecord
    /// (`init_liquid_record.rs`: `space = 8 + size_of::<LiquidationRecord>()`).
    pub(crate) const LIQUIDATION_RECORD_SPACE: usize =
        8 + std::mem::size_of::<marginfi_type_crate::types::LiquidationRecord>();

    /// B4: rent-exempt minimum of a new LiquidationRecord from the canonical Rent sysvar.
    pub(crate) fn liquidation_record_rent_lamports<S: AccountSource + ?Sized>(
        state: &S,
    ) -> Result<u64, &'static str> {
        let (owner, _, data) = state
            .account(&solana_sdk::sysvar::rent::id())
            .ok_or("RENT_SYSVAR_MISSING")?;
        if owner != solana_sdk::sysvar::id() {
            return Err("RENT_SYSVAR_OWNER");
        }
        let rent: solana_sdk::rent::Rent =
            bincode::deserialize(data).map_err(|_| "RENT_SYSVAR_DECODE")?;
        Ok(rent.minimum_balance(LIQUIDATION_RECORD_SPACE))
    }

    /// B4: USD-micro cost of the new record's rent: canonical Rent sysvar, conservative SOL price.
    pub(crate) fn record_rent_usd_micro<S: AccountSource + ?Sized>(
        state: &S,
        lamports_to_usd_micro: &dyn Fn(u64) -> Option<u64>,
    ) -> Option<u64> {
        liquidation_record_rent_lamports(state)
            .ok()
            .and_then(lamports_to_usd_micro)
    }

    /// B4: a plan that creates the record carries its rent on top of its other rent/token
    /// costs; an unknown part or an overflow keeps the whole cost UNKNOWN.
    pub(crate) fn plus_record_rent(base: Option<u64>, rent: Option<u64>) -> Option<u64> {
        base?.checked_add(rent?)
    }

    /// B15: collateral a plan leaves in the wallet, valued conservatively
    /// (`Sizing::seized_hold_value` of the exact `withdraw_amount`).
    /// Inventory classes keep the whole withdrawal.
    /// N7 (stage 1): ZERO_PREFUND and flash-funded plans hold the full seized value too. That
    /// their in-TX swap/repayment leaves nothing behind is not proven from the real token flows
    /// (credited native amount and mint of the withdrawal incl. integration conversion, ExactIn
    /// input, remainder, wallet deltas); until it is, the hold is never optimistically zero.
    pub(crate) fn inventory_hold_usd_micro(class: FundingClass, sz: &Sizing) -> u64 {
        match class {
            FundingClass::NoSwapInventory
            | FundingClass::PreheldInventory
            | FundingClass::JitShortfall
            | FundingClass::ZeroPrefundSameTx
            | FundingClass::FlashFunded => sz.seized_hold_value,
        }
    }

    /// DEC-04: the sizing facts B of a plan is checked against.
    pub(crate) fn hold_facts(
        sz: &Sizing,
        asset_mint: Pubkey,
        liab_mint: Pubkey,
        funding_mint: Option<Pubkey>,
    ) -> crate::execution::hold_b::HoldFacts {
        crate::execution::hold_b::HoldFacts {
            asset_bank: sz.asset_bank,
            liab_bank: sz.liab_bank,
            asset_mint,
            liab_mint,
            withdraw_bank_units: sz.withdraw_amount,
            repay_amount: sz.repay_amount,
            repay_all: sz.repay_all,
            seized_hold_value: sz.seized_hold_value,
            repay_low_value: sz.repay_low_value,
            funding_mint,
        }
    }

    /// B15: the hold value of a seizure — its conservative price (high bias, the larger of EMA
    /// and spot, rounded up), never below the protocol equity value; an unknown price is a hold
    /// no inventory limit admits.
    pub(crate) fn conservative_hold_usd_micro(
        equity: u64,
        conservative: Result<u64, String>,
    ) -> u64 {
        conservative.map_or(u64::MAX, |v| v.max(equity))
    }

    /// Protocol-Sizing für ein Asset×Liability-Paar. Markt-Realisierbarkeit gehört nicht hierher.
    #[derive(Clone, Debug)]
    pub struct Sizing {
        pub asset_bank: Pubkey,
        pub liab_bank: Pubkey,
        pub withdraw_amount: u64,
        pub repay_amount: u64,
        pub repay_all: bool,
        pub seized_equity_value: u64, // USD-Micro, Protocol/EMA only
        /// B15: USD-Micro, conservative value of the seizure (`conservative_hold_usd_micro`).
        pub seized_hold_value: u64,
        /// Actual economic value of the SPL amount transferred for repayment.
        pub repay_value: u64, // USD-Micro
        /// FE-02: the same `repay_amount` at the low-biased liability price (rounded down);
        /// `None` = price unknown. Values a ZERO_PREFUND swap surplus conservatively.
        pub repay_low_value: Option<u64>,
        pub liability_needed_value: u64, // USD-Micro
        pub liability_inventory_value: u64,
        pub funding_bank: Option<Pubkey>,
        pub funding_available_value: u64,
        pub protocol_fees: u64, // USD-Micro
        pub flat_sol_fee: u64,  // USD-Micro after explicit SOL conversion
        /// FE-02: the same flat protocol fee in lamports (a SOL debit of the wallet).
        pub flat_sol_fee_lamports: u64,
        pub rent_and_token: u64, // USD-Micro
        pub premium_bps: u64,
        pub slippage_bps: u64,
        /// P2-05: the pair's growing tag premium cap (`ProtocolSizing::tag_growth`).
        pub tag_growth: Option<darkbot_project0::economics::TagGrowth>,
    }

    pub trait SizingProvider {
        fn sizes(
            &self,
            account: &Pubkey,
            now_unix_s: u64,
            slot: u64,
        ) -> Result<Vec<Sizing>, PlanError>;
        /// MT2: `sizes` with the account's exact Health already computed on this state at
        /// exactly (`now_unix_s`, `slot`). Default: a provider that computes no Health of its
        /// own (test fixtures) sizes as before.
        fn sizes_with_health(
            &self,
            account: &Pubkey,
            _health: &HealthView,
            now_unix_s: u64,
            slot: u64,
        ) -> Result<Vec<Sizing>, PlanError> {
            self.sizes(account, now_unix_s, slot)
        }
        fn no_size_reason(&self, _account: &Pubkey) -> Result<NoSizeReason, String> {
            Ok(NoSizeReason::NoValidProtocolPair)
        }
    }

    /// Q5: a sizing failure keeps its typed kind into planning.
    impl From<SizingError> for PlanError {
        fn from(e: SizingError) -> Self {
            let kind = match e.kind {
                SizingErrorKind::NoValidPair => PlanErrorKind::Sizing,
                SizingErrorKind::IntegrationStale => PlanErrorKind::IntegrationStale,
                SizingErrorKind::HealthUnavailable => PlanErrorKind::HealthUnavailable,
                SizingErrorKind::Other => PlanErrorKind::Other,
            };
            PlanError::new(kind, e.text)
        }
    }

    /// Q5: no sizing candidate — `SIZING:<code>`, an unsupported integration typed as such.
    fn no_size_error(reason: NoSizeReason) -> PlanError {
        let kind = if reason.unsupported_integration() {
            PlanErrorKind::UnsupportedIntegration
        } else {
            PlanErrorKind::Sizing
        };
        PlanError::new(kind, format!("SIZING:{}", reason.code()))
    }

    /// Concrete bridge from protocol-exact sizing into runner economics. It adds only facts that
    /// exist in reconciled RAM state; unsupported funding/rent paths remain disabled. [FIX-67]
    pub struct ProtocolSizingProvider<'a, S: AccountSource + ?Sized> {
        pub state: &'a S,
        pub program: Pubkey,
        pub group: Pubkey,
        pub sol_bank: Pubkey,
        pub inventory: &'a dyn Fn(&Pubkey) -> u64,
        pub slippage_bps: u64,
        /// P5-JIT-01: the JIT funding bank (`P0_JIT_FUNDING_BANK`); `None`: no JIT plan.
        pub funding_bank: Option<Pubkey>,
    }

    impl<'a, S: AccountSource + ?Sized> ProtocolSizingProvider<'a, S> {
        pub fn no_size_reason(&self, account: &Pubkey) -> Result<NoSizeReason, String> {
            self.sizer().no_size_reason(account)
        }

        fn sizer(&self) -> darkbot_project0::sizing::ProtocolSizer<'a, S> {
            darkbot_project0::sizing::ProtocolSizer {
                state: self.state,
                program: self.program,
                group: self.group,
                headroom_bps: darkbot_project0::sizing::DEFAULT_HEADROOM_BPS,
            }
        }

        /// The runner-economics view of the protocol-exact sizings, at the same clock.
        fn sizings(
            &self,
            sizer: &darkbot_project0::sizing::ProtocolSizer<'a, S>,
            raw: Vec<darkbot_project0::sizing::ProtocolSizing>,
            now_unix_s: u64,
            slot: u64,
        ) -> Result<Vec<Sizing>, PlanError> {
            let mut out = Vec::with_capacity(raw.len());
            for sz in raw {
                let inv_native = (self.inventory)(&sz.liab_bank).min(sz.repay_amount);
                let inv_value = sizer.native_amount_value_usd_micro(
                    &sz.liab_bank,
                    inv_native,
                    now_unix_s as i64,
                    slot,
                )?;
                // FE-02: a cost, converted at the conservative (high-bias) SOL price.
                let flat_sol_fee = sizer.native_amount_conservative_cost_usd_micro(
                    &self.sol_bank,
                    sz.flat_sol_fee_lamports,
                    now_unix_s as i64,
                    slot,
                )?;
                // P5-JIT-01: the funding bank's free inventory at its conservative cost (the buy
                // spends it). The liability's own bank or an unknown price: no JIT plan.
                let funding = self
                    .funding_bank
                    .filter(|fb| *fb != sz.liab_bank)
                    .and_then(|fb| {
                        sizer
                            .native_amount_conservative_cost_usd_micro(
                                &fb,
                                (self.inventory)(&fb),
                                now_unix_s as i64,
                                slot,
                            )
                            .ok()
                            .map(|value| (fb, value))
                    });
                out.push(Sizing {
                    asset_bank: sz.asset_bank,
                    liab_bank: sz.liab_bank,
                    withdraw_amount: sz.withdraw_amount,
                    repay_amount: sz.repay_amount,
                    repay_all: sz.repay_all,
                    seized_equity_value: sz.seized_equity_value_micro,
                    seized_hold_value: conservative_hold_usd_micro(
                        sz.seized_equity_value_micro,
                        sizer.native_amount_conservative_cost_usd_micro(
                            &sz.asset_bank,
                            sz.withdraw_amount,
                            now_unix_s as i64,
                            slot,
                        ),
                    ),
                    // [FIX-128] Economics/funding pay the actual SPL transfer. EndLiquidation
                    // premium/health parity is already encoded in premium_bps by ProtocolSizer.
                    repay_value: sz.repay_transfer_value_micro,
                    repay_low_value: sizer
                        .native_amount_conservative_value_usd_micro(
                            &sz.liab_bank,
                            sz.repay_amount,
                            now_unix_s as i64,
                            slot,
                        )
                        .ok(),
                    liability_needed_value: sz.repay_transfer_value_micro,
                    liability_inventory_value: inv_value,
                    funding_bank: funding.map(|f| f.0),
                    funding_available_value: funding.map_or(0, |f| f.1),
                    // EndLiquidation premium is already represented by seized-vs-repaid value.
                    protocol_fees: 0,
                    flat_sol_fee,
                    flat_sol_fee_lamports: sz.flat_sol_fee_lamports,
                    // [FIX-102] Receivership uses prewarmed ATAs created outside the liquidation
                    // transaction. Historical ATA rent is not charged to every opportunity.
                    rent_and_token: 0,
                    premium_bps: sz.premium_bps,
                    tag_growth: sz.tag_growth,
                    slippage_bps: self.slippage_bps,
                });
            }
            Ok(out)
        }
    }

    impl<'a, S: AccountSource + ?Sized> SizingProvider for ProtocolSizingProvider<'a, S> {
        fn sizes(
            &self,
            account: &Pubkey,
            now_unix_s: u64,
            slot: u64,
        ) -> Result<Vec<Sizing>, PlanError> {
            let sizer = self.sizer();
            let raw = sizer.sizes(account, now_unix_s as i64, slot)?;
            self.sizings(&sizer, raw, now_unix_s, slot)
        }

        fn sizes_with_health(
            &self,
            account: &Pubkey,
            health: &HealthView,
            now_unix_s: u64,
            slot: u64,
        ) -> Result<Vec<Sizing>, PlanError> {
            let sizer = self.sizer();
            let raw = sizer.sizes_with_health(account, health, now_unix_s as i64, slot)?;
            self.sizings(&sizer, raw, now_unix_s, slot)
        }

        fn no_size_reason(&self, account: &Pubkey) -> Result<NoSizeReason, String> {
            ProtocolSizingProvider::no_size_reason(self, account)
        }
    }

    /// Allowlisted Swap-Instructions (Jupiter/Titan) für collateral→liability.
    #[derive(Clone, Debug, Default)]
    pub struct Route {
        pub instructions: Vec<Instruction>,
        pub lookup_tables: Vec<AddressLookupTableAccount>,
        pub swap_program: Pubkey,
        /// Vom Router bestimmte native Tokenmengen. Marktwert wird erst im Builder aus dem
        /// executable Output relativ zum protocol-exakten Liability-Wert normalisiert. [FIX-64]
        pub amount_in: u64,
        /// [P1-C] Router quote (expected output, native units). Diagnostics / route comparison
        /// only. Never a realizable value and never a feasibility proof.
        pub quoted_out: u64,
        /// [P1-C] Minimum output the swap instruction enforces on-chain: FE-02 derives it from
        /// the signed instruction bytes (Jupiter `otherAmountThreshold` only if lower).
        /// `None` = unknown -> fail closed.
        pub minimum_executable_out: Option<u64>,
        /// FE-02: the slippage the signed swap instruction enforces (`None`: unknown route).
        pub enforced_slippage_bps: Option<u16>,
    }

    /// [P1-C] Classification of a fitted ZERO_PREFUND route by its executable floor.
    /// The quote never proves feasibility and is never used as realizable value.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum ZeroPrefundFloor {
        /// Floor known and >= repay: executable, realizable value at the floor.
        Executable(u64),
        /// Floor not provided: economics must be `Evaluation::Unknown` (fail closed).
        Unknown,
        /// Floor known and < repay: the swap cannot be guaranteed to fund the repay.
        BelowRepay { floor: u64, repay: u64 },
        /// Floor > quote: provider/schema invalid.
        Invalid { floor: u64, quoted_out: u64 },
    }

    pub(crate) fn classify_zero_prefund_floor(
        route: &Route,
        repay_amount: u64,
    ) -> ZeroPrefundFloor {
        match route.minimum_executable_out {
            None => ZeroPrefundFloor::Unknown,
            Some(floor) if floor > route.quoted_out => ZeroPrefundFloor::Invalid {
                floor,
                quoted_out: route.quoted_out,
            },
            Some(floor) if floor < repay_amount => ZeroPrefundFloor::BelowRepay {
                floor,
                repay: repay_amount,
            },
            Some(floor) => ZeroPrefundFloor::Executable(floor),
        }
    }

    /// FE-02: a ZERO_PREFUND plan's slippage is the larger of the configured evaluation slippage
    /// and what its signed swap instruction enforces (a route without decoded terms keeps the
    /// configured value), so the safety and approval limits bind the real swap.
    pub(crate) fn enforced_slippage(route: &Route, configured: u64) -> u64 {
        route
            .enforced_slippage_bps
            .map_or(configured, |s| configured.max(u64::from(s)))
    }

    /// Reason string for a rejected classification; `None` for Executable/Unknown.
    pub(crate) fn zero_prefund_rejection(route: &Route, repay_amount: u64) -> Option<String> {
        match classify_zero_prefund_floor(route, repay_amount) {
            ZeroPrefundFloor::Executable(_) | ZeroPrefundFloor::Unknown => None,
            ZeroPrefundFloor::BelowRepay { floor, repay } => Some(format!(
                "ROUTE_FLOOR_BELOW_REPAY: floor={floor} repay={repay} quoted_out={}",
                route.quoted_out
            )),
            ZeroPrefundFloor::Invalid { floor, quoted_out } => Some(format!(
                "ROUTE_MIN_OUT_INVALID: floor={floor} > quoted_out={quoted_out}"
            )),
        }
    }

    /// Economics input for one plan (single construction used by every funding class).
    pub(crate) fn candidate_input(
        sz: &Sizing,
        funding: FundingClass,
        realizable_value_usd_micro: Option<u64>,
        capital_usd_micro: u64,
        jito_tip_usd_micro: u64,
        path: Path,
    ) -> CandidateInput {
        CandidateInput {
            gross_seized_realizable_usd_micro: realizable_value_usd_micro,
            repay_usd_micro: sz.repay_value,
            protocol_fees_usd_micro: sz.protocol_fees,
            flat_sol_fee_usd_micro: sz.flat_sol_fee,
            swap_fees_and_impact_usd_micro: 0,
            transaction_fee_usd_micro: None,
            jito_tip_usd_micro,
            rent_and_token_usd_micro: Some(sz.rent_and_token),
            required_capital_usd_micro: capital_usd_micro,
            path,
            funding,
        }
    }

    /// P4A-JIT-02: a JIT plan's economics input — `candidate_input` plus the buy's spread: the
    /// funding the ExactIn buy spends above the value of the shortfall it buys is a swap cost
    /// (`capital` is the spend, `shortfall_value` what it buys), never a free conversion.
    pub(crate) fn jit_candidate_input(
        sz: &Sizing,
        realizable_value_usd_micro: Option<u64>,
        capital_usd_micro: u64,
        shortfall_value_usd_micro: u64,
        jito_tip_usd_micro: u64,
        path: Path,
    ) -> CandidateInput {
        CandidateInput {
            swap_fees_and_impact_usd_micro: capital_usd_micro
                .saturating_sub(shortfall_value_usd_micro),
            ..candidate_input(
                sz,
                FundingClass::JitShortfall,
                realizable_value_usd_micro,
                capital_usd_micro,
                jito_tip_usd_micro,
                path,
            )
        }
    }

    /// V7/FE-04: the inventory a JIT plan spends — the liability inventory its repay uses and the
    /// funding its buy spends (zero amounts carry no hold).
    pub(crate) fn jit_inventory_holds(
        liability: (Pubkey, u64),
        funding: (Pubkey, u64),
    ) -> Vec<(Pubkey, u64)> {
        [liability, funding]
            .into_iter()
            .filter(|(_, units)| *units > 0)
            .collect()
    }

    /// [P1-C] The production mapping from a fitted ZERO_PREFUND route to its economics input:
    /// known floor >= repay -> realizable value at the floor; unknown floor -> `None`, which
    /// `economics::evaluate` turns into `Evaluation::Unknown`; known floor < repay or invalid
    /// floor -> `Err` (no plan). The quote is never a substitute.
    pub(crate) fn zero_prefund_input(
        sz: &Sizing,
        route: &Route,
        capital_usd_micro: u64,
        jito_tip_usd_micro: u64,
        path: Path,
    ) -> Result<CandidateInput, String> {
        if let Some(reason) = zero_prefund_rejection(route, sz.repay_amount) {
            return Err(reason);
        }
        let realizable = match classify_zero_prefund_floor(route, sz.repay_amount) {
            ZeroPrefundFloor::Executable(floor) => zero_prefund_realizable_usd_micro(sz, floor)?,
            _ => None,
        };
        Ok(candidate_input(
            sz,
            FundingClass::ZeroPrefundSameTx,
            realizable,
            capital_usd_micro,
            jito_tip_usd_micro,
            path,
        ))
    }

    /// FE-02: what a ZERO_PREFUND swap realizes: the repay it funds at the repay's own value plus
    /// the surplus `floor − repay` tokens at the low-biased liability price (rounded down).
    /// `None` without a low price (economics stays UNKNOWN).
    pub(crate) fn zero_prefund_realizable_usd_micro(
        sz: &Sizing,
        floor: u64,
    ) -> Result<Option<u64>, String> {
        let surplus = floor
            .checked_sub(sz.repay_amount)
            .ok_or("ZERO_PREFUND floor below repay")?;
        let Some(low) = sz.repay_low_value else {
            return Ok(None);
        };
        if sz.repay_amount == 0 {
            return Err("repay_amount=0".into());
        }
        let surplus_value =
            u64::try_from(u128::from(surplus) * u128::from(low) / u128::from(sz.repay_amount))
                .map_err(|_| "realizable value > u64")?;
        sz.repay_value
            .checked_add(surplus_value)
            .map(Some)
            .ok_or_else(|| "realizable value overflow".into())
    }

    /// Economic value of `liability_out` native units, normalised by the protocol-exact repay
    /// value/amount ratio (tests; production values a surplus with
    /// `zero_prefund_realizable_usd_micro`).
    #[cfg(test)]
    pub(crate) fn realizable_value_usd_micro(
        sz: &Sizing,
        liability_out: u64,
    ) -> Result<u64, String> {
        if sz.repay_amount == 0 {
            return Err("repay_amount=0".into());
        }
        let v = (liability_out as u128)
            .checked_mul(sz.repay_value as u128)
            .ok_or("realizable value overflow")?
            / sz.repay_amount as u128;
        u64::try_from(v).map_err(|_| "realizable value > u64".into())
    }

    /// Canonical liquidator ATA already proven present in reconciled state.
    /// Route sanitization may use this only to remove an exact redundant idempotent ATA-create
    /// emitted by a router; it is not permission to accept arbitrary ATA instructions.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct PrewarmedAta {
        pub address: Pubkey,
        pub mint: Pubkey,
        pub token_program: Pubkey,
    }

    /// N2: every quote takes the candidate's monotonic deadline; a provider starts no request
    /// after it and ends one in flight at it.
    pub trait RouteProvider {
        /// ExactIn: ZERO_PREFUND (seized asset → liability) mit `withdraw_amount` als Input.
        fn quote_exact_in(
            &self,
            from_mint: &Pubkey,
            to_mint: &Pubkey,
            amount_in: u64,
            prewarmed_atas: &[PrewarmedAta],
            deadline: Instant,
        ) -> Result<Option<Route>, String>;
        fn max_accounts(&self) -> u8 {
            64
        }
        /// ExactIn with an account cap; a provider without caps answers the uncapped quote under
        /// the same deadline.
        fn quote_exact_in_with_budget(
            &self,
            from_mint: &Pubkey,
            to_mint: &Pubkey,
            amount_in: u64,
            prewarmed_atas: &[PrewarmedAta],
            _max_accounts: u8,
            deadline: Instant,
        ) -> Result<Option<Route>, String> {
            self.quote_exact_in(from_mint, to_mint, amount_in, prewarmed_atas, deadline)
        }
    }

    const ROUTE_DIAGNOSTICS_MAX: usize = 64;
    thread_local! {
        /// Route-fit diagnostics of this thread, in memory only (PERSIST-02): the Shadow owner
        /// forwards them through its evidence writer after each evaluation. Bounded; lines
        /// beyond the bound are counted, never silently dropped.
        static ROUTE_DIAGNOSTICS: std::cell::RefCell<(Vec<String>, u64)> =
            const { std::cell::RefCell::new((Vec::new(), 0)) };
    }
    fn route_diagnostic(line: String) {
        ROUTE_DIAGNOSTICS.with(|d| {
            let mut d = d.borrow_mut();
            if d.0.len() < ROUTE_DIAGNOSTICS_MAX {
                d.0.push(line);
            } else {
                d.1 += 1;
            }
        });
    }
    /// Lines recorded on this thread since the last call, and how many exceeded the bound.
    pub fn take_route_diagnostics() -> (Vec<String>, u64) {
        ROUTE_DIAGNOSTICS.with(|d| std::mem::take(&mut *d.borrow_mut()))
    }

    /// P5-JIT-01: the working capital of a JIT plan is the funding its ExactIn buy actually
    /// spends — the buy input valued at the funding inventory's own value per token, rounded up —
    /// never less than the shortfall value the class check used; `None` without funding.
    pub(crate) fn jit_capital_usd_micro(
        shortfall_value: u64,
        buy_input: u64,
        funding_amount: u64,
        funding_value: u64,
    ) -> Option<u64> {
        if funding_amount == 0 {
            return None;
        }
        let spent = (u128::from(buy_input) * u128::from(funding_value))
            .div_ceil(u128::from(funding_amount));
        u64::try_from(spent).ok().map(|s| s.max(shortfall_value))
    }

    /// P4A-JIT-01: remote quotes of one JIT shortfall solve (the same bound as a route fit).
    pub(crate) const JIT_MAX_QUOTES: u32 = 6;

    /// P4A-JIT-01: bounded ExactIn shortfall solver. Jupiter V2 `/build` documents no ExactOut, so
    /// the JIT buy is the smallest ExactIn input whose on-chain enforced minimum output
    /// (`minimum_executable_out`) covers `amount_out`: one reverse quote (the shortfall sold back)
    /// gives the scale, then proportional steps from the observed floor and bisection — at most
    /// `JIT_MAX_QUOTES` remote requests, none after the candidate deadline. An unknown floor never
    /// covers; only a quoted covering route at or below `max_input` (the funding inventory) is
    /// returned.
    pub(crate) fn shortfall_exact_in<R: RouteProvider + ?Sized>(
        routes: &R,
        funding_mint: &Pubkey,
        liab_mint: &Pubkey,
        amount_out: u64,
        max_input: u64,
        prewarmed_atas: &[PrewarmedAta],
        deadline: Instant,
    ) -> Result<Option<Route>, String> {
        if amount_out == 0 || max_input == 0 || Instant::now() >= deadline {
            return Ok(None);
        }
        let mut used = 1;
        let Some(reverse) = routes.quote_exact_in(
            liab_mint,
            funding_mint,
            amount_out,
            prewarmed_atas,
            deadline,
        )?
        else {
            return Ok(None);
        };
        let mut input = reverse.quoted_out.clamp(1, max_input);
        // Largest input known not to cover; smallest covering input and its route.
        let mut lo = 0u64;
        let mut best: Option<(u64, Route)> = None;
        while used < JIT_MAX_QUOTES && Instant::now() < deadline {
            used += 1;
            let route =
                routes.quote_exact_in(funding_mint, liab_mint, input, prewarmed_atas, deadline)?;
            let floor = route.as_ref().and_then(|r| r.minimum_executable_out);
            let next = match (route, floor) {
                (Some(r), Some(f)) if f >= amount_out => {
                    best = Some((input, r));
                    lo + (input - lo) / 2
                }
                _ => {
                    lo = input;
                    match &best {
                        Some((hi, _)) => lo + (hi - lo) / 2,
                        None if input >= max_input => break,
                        // Proportional to the observed floor (+1/64), else double.
                        None => {
                            let step = match floor {
                                // A23-05: checked — a product beyond u128 saturates to
                                // the inventory bound below, never wraps to a small step.
                                Some(f) if f > 0 => u128::from(input)
                                    .checked_mul(u128::from(amount_out))
                                    .and_then(|v| v.checked_mul(65))
                                    .map_or(u128::MAX, |v| v / (64 * u128::from(f)) + 1),
                                _ => u128::from(input) * 2,
                            };
                            step.clamp(u128::from(input) + 1, u128::from(max_input)) as u64
                        }
                    }
                }
            };
            if next <= lo || best.as_ref().is_some_and(|(hi, _)| next >= *hi) {
                break;
            }
            input = next;
        }
        Ok(best.map(|(_, r)| r))
    }

    /// [FIX-123][R7/R6] Remote quote requests of one route-fit search (the JIT solve shares the
    /// bound, `JIT_MAX_QUOTES`); MT4 derives the hint re-probe period from it.
    pub(crate) const ROUTE_FIT_MAX_ATTEMPTS: u8 = 6;

    /// [FIX-123][R7/R6] Router caps are hints; only the complete compiled TX proves fit.
    /// Bound remote work to `ROUTE_FIT_MAX_ATTEMPTS` requests. Every changed quote is
    /// revalidated and repriced. (The production path takes the traced search below.)
    #[cfg(test)]
    fn quote_fitting_route<T>(
        initial_cap: u8,
        deadline: Instant,
        quote: impl FnMut(u8) -> Result<Option<Route>, String>,
        compile: impl FnMut(&Route) -> Result<T, BuildError>,
    ) -> Result<Option<(Route, T)>, String> {
        Ok(
            quote_fitting_route_traced(initial_cap, deadline, quote, compile)?
                .map(|(route, compiled, _)| (route, compiled)),
        )
    }

    /// `quote_fitting_route` with what the search proved (MT4 route-cap hint): the cap the
    /// real TX fit at and the smallest cap above it the compile rejected in this search. The
    /// start cap is only where the search begins — a hinted cap that does not fit shrinks
    /// exactly like any other (P6-MT.14 #31).
    fn quote_fitting_route_traced<T>(
        initial_cap: u8,
        deadline: Instant,
        mut quote: impl FnMut(u8) -> Result<Option<Route>, String>,
        mut compile: impl FnMut(&Route) -> Result<T, BuildError>,
    ) -> Result<Option<(Route, T, FitTrace)>, String> {
        let mut cap = initial_cap.clamp(1, 64);
        let mut rejected = Vec::new();
        // Caps only decrease, so the last rejected cap is the smallest one above the fit.
        let mut rejected_above = None;
        darkbot_common::mt0::MT0.add(darkbot_common::mt0::Counter::RouteFitCalls, 1); // [MT0]
        for attempt in 0..u32::from(ROUTE_FIT_MAX_ATTEMPTS) {
            // N2: no re-quote after the candidate deadline.
            if Instant::now() >= deadline {
                return Err(format!("ROUTE_FIT_DEADLINE after {attempt} attempt(s)"));
            }
            darkbot_common::mt0::MT0.add(darkbot_common::mt0::Counter::RouteFitAttempts, 1); // [MT0]
            let Some(route) = quote(cap)? else {
                return Ok(None);
            };
            match compile(&route) {
                Ok(compiled) => {
                    let trace = FitTrace {
                        fit_cap: cap,
                        rejected_above,
                        attempts: attempt + 1,
                    };
                    return Ok(Some((route, compiled, trace)));
                }
                Err(error) => {
                    let reduction = match &error {
                        BuildError::TooLarge { bytes, max } => {
                            bytes.saturating_sub(*max).div_ceil(32).max(2)
                        }
                        BuildError::TooManyAccounts { count, max } => {
                            count.saturating_sub(*max).max(2)
                        }
                        _ => return Err(format!("route compilation: {error}")),
                    };
                    let route_accounts = route
                        .instructions
                        .iter()
                        .flat_map(|ix| {
                            std::iter::once(ix.program_id)
                                .chain(ix.accounts.iter().map(|a| a.pubkey))
                        })
                        .collect::<std::collections::HashSet<_>>()
                        .len();
                    route_diagnostic(format!("route_budget: attempt={} max_accounts={} route_accounts={} alts={} rejected={}",
                        attempt + 1, cap, route_accounts, route.lookup_tables.len(), error));
                    rejected.push(format!("maxAccounts={cap}: {error}"));
                    rejected_above = Some(cap);
                    if cap == 1 {
                        break;
                    }
                    // A cap above the actual route footprint can return the same route over
                    // and over (R118). Reduce from the observed footprint as well as the hint.
                    let observed = route_accounts.clamp(1, 64) as u8;
                    cap = cap
                        .min(observed)
                        .saturating_sub(reduction.min(63) as u8)
                        .max(1);
                }
            }
        }
        Err(format!("TX_BUDGET_EXHAUSTED: {}", rejected.join(" | ")))
    }

    pub struct P0ReceivershipBuilder<
        'a,
        S: AccountSource + ?Sized,
        Z: SizingProvider,
        R: RouteProvider,
    > {
        pub state: &'a S,
        pub program: Pubkey,
        pub group: Pubkey,
        pub liquidator: Pubkey,
        /// [P5] FeeState read result; an unreadable FeeState blocks every plan.
        pub global_fee_wallet: Result<Pubkey, String>,
        pub sizing: &'a Z,
        pub routes: &'a R,
        /// Inventar je Liability-Bank (Token-Basis), aus P8-Reconciliation gepflegt.
        pub inventory: &'a dyn Fn(&Pubkey) -> u64,
        /// ATA des Liquidators je Mint.
        pub ata: &'a dyn Fn(&Pubkey) -> Option<Pubkey>,
        /// Economics cost after explicit conversion, never raw lamports. [FIX-61]
        pub lamports_to_usd_micro: &'a dyn Fn(u64) -> Option<u64>,
        /// B4: conservative (high-biased) conversion for the rent of a created account.
        pub rent_lamports_to_usd_micro: &'a dyn Fn(u64) -> Option<u64>,
        pub jito_tip_usd_micro: u64,
        /// Execution amount for Jito tip transaction.
        pub jito_tip_lamports: u64,
        pub zero_prefund_qualified: bool,
        /// E2: pinned instruction table of the receivership TX (built once at setup).
        pub instructions: &'a InstructionTable,
        /// B9: offer V1 candidates only when the transaction layer builds and signs V1
        /// (`tx::v1_build_capability`) and the submission adapter supports V1.
        pub v1_plans: bool,
        /// P2-07: the owner's prepared-candidate cache and its current generation; `None`
        /// prepares every template afresh (the former path).
        pub prepared: Option<(&'a std::cell::RefCell<PreparedCache>, u64)>,
        /// P6-MT.8 (MT4): the owner's route-cap cache — the start cap of a route-fit search
        /// per (input mint, output mint, footprint class), never a route or quote; `None`
        /// searches from the configured maximum every time (the former path).
        pub route_cap: Option<&'a std::cell::RefCell<RouteCapCache>>,
        pub path: Path,
        pub resources: Resources,
        pub limits: TxLimits,
    }

    fn end_banks_after_repay(
        mut active_banks: Vec<Pubkey>,
        liability_bank: Pubkey,
        repay_all: bool,
    ) -> Vec<Pubkey> {
        if repay_all {
            active_banks.retain(|k| *k != liability_bank);
        }
        active_banks
    }

    /// [P5] The EndLiquidation fee receiver comes only from a successfully read FeeState; a
    /// default key would build a transaction with an invalid fee recipient.
    fn required_fee_wallet(fee: &Result<Pubkey, String>) -> Result<&Pubkey, PlanError> {
        fee.as_ref().map_err(|e| {
            PlanError::new(
                PlanErrorKind::FeeStateUnavailable,
                format!("{}:{e}", super::FEE_STATE_UNAVAILABLE),
            )
        })
    }

    fn liquidation_record_init_decision(linked: Pubkey, expected: Pubkey) -> Result<bool, String> {
        if linked == Pubkey::default() {
            Ok(true)
        } else if linked == expected {
            Ok(false)
        } else {
            Err(format!(
                "LIQUIDATION_RECORD_MISMATCH: linked={} expected={}",
                linked, expected
            ))
        }
    }

    /// [FIX-100] MarginfiAccount itself is the authoritative source for whether its
    /// liquidation-record PDA exists. The protocol writes this field when the record is
    /// created and resets it to Pubkey::default() when the record is closed. This avoids
    /// both hot-path RPC and the invalid assumption that "not present in our RAM state"
    /// means "not present on-chain".
    fn should_init_liquidation_record<S: AccountSource + ?Sized>(
        state: &S,
        program: &Pubkey,
        account: &Pubkey,
    ) -> Result<bool, String> {
        let (owner, _, data) = state
            .account(account)
            .ok_or("marginfi account not in state")?;
        if owner != *program {
            return Err(format!("UNKNOWN_LAYOUT: marginfi account owner {}", owner));
        }
        // B2: strict pinned layout (discriminator, exact length), never a prefix decode.
        let a =
            darkbot_project0::protocol::decode_marginfi_account(data).map_err(|e| e.to_string())?;
        let expected = darkbot_project0::protocol::liquidation_record_pda(program, account);
        liquidation_record_init_decision(a.liquidation_record, expected)
            .map_err(|e| format!("{} account={}", e, account))
    }

    /// [FIX-107] Exact bank-only layout used by pinned p0-cli for EndLiquidation.
    /// Active balance order is normalized descending by raw pubkey bytes, matching
    /// load_observation_bank_only_metas().
    fn active_bank_keys<S: AccountSource + ?Sized>(
        state: &S,
        program: &Pubkey,
        account: &Pubkey,
    ) -> Result<Vec<Pubkey>, String> {
        let (owner, _, data) = state
            .account(account)
            .ok_or("marginfi account not in state")?;
        if owner != *program
            || darkbot_project0::protocol::classify(data)
                != darkbot_project0::protocol::AccountType::MarginfiAccount
        {
            return Err("UNKNOWN_LAYOUT: invalid MarginfiAccount for observation layout".into());
        }
        // B2: strict pinned layout, never a prefix decode.
        let a =
            darkbot_project0::protocol::decode_marginfi_account(data).map_err(|e| e.to_string())?;
        let mut banks: Vec<Pubkey> = a
            .lending_account
            .balances
            .iter()
            .filter(|bal| bal.is_active())
            .map(|bal| bal.bank_pk)
            .collect();
        banks.sort_by(|x, y| y.cmp(x));
        Ok(banks)
    }

    /// P2-06: the account-dependent template of `account`'s receivership plans, read only
    /// through `state` — the parts the builder formerly re-derived for every pair and class.
    /// An unknown observation set ends planning; the other errors stay per pair, as before.
    pub(crate) fn prepare_skeleton<S: AccountSource + ?Sized>(
        state: &S,
        program: &Pubkey,
        group: &Pubkey,
        account: &Pubkey,
    ) -> Result<Skeleton, PlanError> {
        let observation = CrateHealth {
            state,
            group: *group,
        }
        .observation_accounts(account)
        .map_err(SizingError::from)?;
        let banks_and_refreshes = active_bank_keys(state, program, account).and_then(|banks| {
            // [FIX-109][R6] Refresh every active integration balance before StartLiquidation.
            // Deterministic from canonical RAM state; fails closed on any unknown layout.
            let refreshes =
                darkbot_project0::integration::refresh_plan_for_account(state, program, account)?;
            Ok((banks, refreshes))
        });
        Ok(Skeleton {
            observation,
            banks_and_refreshes,
            init_record: should_init_liquidation_record(state, program, account),
        })
    }

    #[cfg(test)]
    thread_local! {
        /// V2-14 P1-04 test seam: receivership instruction builds on this thread.
        static RECEIVERSHIP_BUILDS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    }

    impl<'a, S: AccountSource + ?Sized, Z: SizingProvider, R: RouteProvider>
        P0ReceivershipBuilder<'a, S, Z, R>
    {
        /// [FIX-19] fail closed: Owner, Discriminator, Länge prüfen — kein Index-Panic bei fremdem Layout.
        fn bank(&self, k: &Pubkey) -> Result<(Pubkey, Pubkey, Pubkey), String> {
            let (owner, _, data) = self.state.account(k).ok_or("bank not in state")?;
            if owner != self.program {
                return Err(format!("UNKNOWN_LAYOUT: bank owner {}", owner));
            }
            let b = darkbot_project0::protocol::decode_bank(data).map_err(|e| e.to_string())?;
            let (mint_owner, _, _) = self.state.account(&b.mint).ok_or("mint not in state")?;
            Ok((b.liquidity_vault, b.mint, mint_owner))
        }

        /// FE-02: both mints' transfers are modelled (SPL Token, or Token-2022 without fee or
        /// hook); otherwise the stated cause joins the route errors and ZERO_PREFUND is not
        /// quoted at all.
        fn mints_modelled(
            &self,
            asset_mint: &Pubkey,
            liab_mint: &Pubkey,
            route_errors: &mut Vec<String>,
        ) -> bool {
            let unmodelled = [asset_mint, liab_mint].into_iter().find_map(|m| {
                let reason = match self.state.account(m) {
                    None => Some("MINT_STATE_UNKNOWN"),
                    Some((owner, lamports, data)) => {
                        darkbot_project0::sizing::mint_transfer_unmodeled(m, &owner, lamports, data)
                    }
                };
                reason.map(|r| (r, m))
            });
            match unmodelled {
                Some((reason, mint)) => {
                    route_errors.push(format!(
                        "exact-in {asset_mint}→{liab_mint}: {reason} mint={mint}"
                    ));
                    false
                }
                None => true,
            }
        }

        fn bank_full(&self, k: &Pubkey) -> Result<Bank, String> {
            let (owner, _, data) = self.state.account(k).ok_or("bank not in state")?;
            if owner != self.program {
                return Err(format!("UNKNOWN_LAYOUT: bank owner {}", owner));
            }
            darkbot_project0::protocol::decode_bank(data)
                .map(std::borrow::Cow::into_owned)
                .map_err(|e| e.to_string())
        }

        /// External integration accounts only (E1), with the pinned owner and discriminator (Z18).
        fn pod_after_disc<T: darkbot_project0::integration::IntegrationAccount>(
            &self,
            k: &Pubkey,
        ) -> Result<&T, String> {
            darkbot_project0::integration::integration_account::<T, S>(self.state, k)
        }

        fn asset_withdraw_instruction(
            &self,
            account: &Pubkey,
            obs: &[Pubkey],
            sz: &Sizing,
            asset_ata: &Pubkey,
            asset_tp: &Pubkey,
        ) -> Result<(Instruction, u64), String> {
            use darkbot_project0::integration::{execution_amount_for_asset, IntegrationKind};
            use marginfi_type_crate::pdas::{
                derive_drift_signer, derive_drift_spot_market_vault, derive_drift_state,
                derive_juplend_claim_account, derive_juplend_lending_admin,
                derive_juplend_liquidity, derive_juplend_liquidity_vault,
                derive_juplend_rate_model, derive_juplend_rewards_rate_model,
                JUPLEND_LIQUIDITY_PROGRAM_ID,
            };

            let bank = self.bank_full(&sz.asset_bank)?;
            let exec = execution_amount_for_asset(self.state, &bank, sz.withdraw_amount)?;

            let Some(exec) = exec else {
                return Ok((
                    builders::withdraw(
                        &self.program,
                        &self.group,
                        account,
                        &self.liquidator,
                        &sz.asset_bank,
                        &bank.mint,
                        asset_ata,
                        &bank.liquidity_vault,
                        asset_tp,
                        sz.withdraw_amount,
                        None,
                        obs,
                    ),
                    sz.withdraw_amount,
                ));
            };

            let ix = match exec.kind {
                IntegrationKind::Kamino => {
                    let reserve: &kamino_mocks::state::MinimalReserve =
                        self.pod_after_disc(&bank.integration_acc_1)?;
                    let lending_market = reserve.lending_market;
                    let supply_vault = reserve.supply_vault;
                    let collateral_mint = reserve.collateral_mint_pubkey;
                    let collateral_supply_vault = reserve.collateral_supply_vault;
                    let farm = reserve.farm_collateral;
                    let (farm_user, farm_state) = if farm == Pubkey::default() {
                        (None, None)
                    } else {
                        (
                            Some(
                                marginfi_type_crate::pdas::derive_kamino_user_state(
                                    &farm,
                                    &bank.integration_acc_2,
                                )
                                .0,
                            ),
                            Some(farm),
                        )
                    };
                    builders::kamino_withdraw_from_bank(
                        &self.program,
                        &self.group,
                        account,
                        &self.liquidator,
                        &sz.asset_bank,
                        &bank,
                        asset_ata,
                        asset_tp,
                        &lending_market,
                        &supply_vault,
                        &collateral_mint,
                        &collateral_supply_vault,
                        farm_user,
                        farm_state,
                        exec.instruction_amount,
                        false,
                        obs,
                    )
                }
                IntegrationKind::Drift => {
                    let market: &drift_mocks::state::MinimalSpotMarket =
                        self.pod_after_disc(&bank.integration_acc_1)?;
                    let main_index = market.market_index;
                    let main_oracle = (market.oracle != Pubkey::default()).then_some(market.oracle);
                    let main_vault = if market.vault == Pubkey::default() {
                        derive_drift_spot_market_vault(main_index).0
                    } else {
                        market.vault
                    };

                    let reward_keys =
                        darkbot_project0::integration::drift_reward_market_keys(self.state, &bank)?;
                    let mut rewards: Vec<(Pubkey, Pubkey, Pubkey)> = Vec::new();
                    for key in reward_keys {
                        let m: &drift_mocks::state::MinimalSpotMarket =
                            self.pod_after_disc(&key)?;
                        rewards.push((key, m.oracle, m.mint));
                    }
                    if rewards.len() > 2 {
                        return Err(format!("DRIFT_TOO_MANY_REWARD_MARKETS:{}", rewards.len()));
                    }

                    let r1 = rewards.first().copied();
                    let r2 = rewards.get(1).copied();
                    builders::drift_withdraw_from_bank(
                        &self.program,
                        &self.group,
                        account,
                        &self.liquidator,
                        &sz.asset_bank,
                        &bank,
                        asset_ata,
                        asset_tp,
                        main_oracle,
                        derive_drift_state().0,
                        main_vault,
                        r1.and_then(|(_, o, _)| (o != Pubkey::default()).then_some(o)),
                        r1.map(|(m, _, _)| m),
                        r1.map(|(_, _, mint)| mint),
                        r2.and_then(|(_, o, _)| (o != Pubkey::default()).then_some(o)),
                        r2.map(|(m, _, _)| m),
                        r2.map(|(_, _, mint)| mint),
                        derive_drift_signer().0,
                        exec.instruction_amount,
                        false,
                        obs,
                    )
                }
                IntegrationKind::JupLend => {
                    let lending: &juplend_mocks::state::Lending =
                        self.pod_after_disc(&bank.integration_acc_1)?;
                    let mint = lending.mint;
                    let f_token_mint = lending.f_token_mint;
                    let reserves = lending.token_reserves_liquidity;
                    let supply_position = lending.supply_position_on_liquidity;
                    let rewards_rate_model = lending.rewards_rate_model;
                    if mint != bank.mint {
                        return Err(format!(
                            "JUPLEND_MINT_MISMATCH:lending_mint={} bank_mint={}",
                            mint, bank.mint
                        ));
                    }
                    let liquidity_vault_authority =
                        darkbot_project0::protocol::liquidity_vault_authority_pda(
                            &self.program,
                            &sz.asset_bank,
                        );
                    let cpi = builders::JuplendCpiAccounts {
                        f_token_mint,
                        lending_admin: derive_juplend_lending_admin().0,
                        supply_token_reserves_liquidity: reserves,
                        lending_supply_position_on_liquidity: supply_position,
                        rate_model: derive_juplend_rate_model(&mint).0,
                        vault: derive_juplend_liquidity_vault(&mint, asset_tp),
                        claim_account: derive_juplend_claim_account(
                            &liquidity_vault_authority,
                            &mint,
                        )
                        .0,
                        liquidity: derive_juplend_liquidity().0,
                        liquidity_program: JUPLEND_LIQUIDITY_PROGRAM_ID,
                        rewards_rate_model: if rewards_rate_model == Pubkey::default() {
                            derive_juplend_rewards_rate_model(&mint).0
                        } else {
                            rewards_rate_model
                        },
                    };
                    builders::juplend_withdraw_from_bank(
                        &self.program,
                        &self.group,
                        account,
                        &self.liquidator,
                        &sz.asset_bank,
                        &bank,
                        asset_ata,
                        asset_tp,
                        &cpi,
                        exec.instruction_amount,
                        false,
                        obs,
                    )
                }
            };

            Ok((ix, exec.underlying_out))
        }

        /// MT4: the footprint class of the non-route part of this pair's receivership TX — the
        /// TX compiled with an empty route at the same version, payer, resources and LUT plan
        /// as the route-fit compile (no remote call). `None` when that TX does not compile:
        /// then no route fits either and the search reports it as before, uncached.
        fn route_footprint_class(
            &self,
            account: &Pubkey,
            prep: &Skeleton,
            sz: &Sizing,
            protocol_luts: &std::sync::Arc<[AddressLookupTableAccount]>,
        ) -> Option<RouteFootprintClass> {
            let instructions = self.receivership(account, prep, sz, vec![]).ok()?;
            let built = tx::build(
                &TxPlan {
                    version: BuildVersion::V0,
                    payer: self.liquidator,
                    instructions,
                    lookup_tables: protocol_luts.clone(),
                    resources: self.resources,
                },
                Hash::default(),
                &self.limits,
            )
            .ok()?;
            Some(RouteFootprintClass {
                locked_accounts: u8::try_from(built.locked_accounts).ok()?,
                serialized_len: u16::try_from(built.serialized_len).ok()?,
            })
        }

        fn receivership(
            &self,
            account: &Pubkey,
            prep: &Skeleton,
            sz: &Sizing,
            swaps: Vec<Instruction>,
        ) -> Result<Vec<Instruction>, String> {
            #[cfg(test)]
            RECEIVERSHIP_BUILDS.with(|n| n.set(n.get() + 1));
            let (_asset_vault, asset_mint, asset_tp) = self.bank(&sz.asset_bank)?;
            let (liab_vault, liab_mint, liab_tp) = self.bank(&sz.liab_bank)?;

            // [FIX-102] Marginfi's validate_ix_first rejects every non-ComputeBudget
            // top-level instruction with data shorter than 8 bytes while scanning the whole TX.
            // Associated Token Program create/idempotent uses a 1-byte discriminator, so an ATA
            // create is invalid anywhere inside a receivership TX, including after StartLiquidation.
            // Required liquidator token accounts must therefore be prewarmed outside the
            // receivership TX (or later supplied by a separately qualified sequential bundle).
            let asset_ata = match (self.ata)(&asset_mint) {
                Some(existing) => existing,
                None => {
                    let ata = get_associated_token_address_with_program_id(
                        &self.liquidator,
                        &asset_mint,
                        &asset_tp,
                    );
                    return Err(format!(
                        "PREWARM_REQUIRED: missing asset ATA={} mint={} token_program={}",
                        ata, asset_mint, asset_tp
                    ));
                }
            };

            let liab_ata = match (self.ata)(&liab_mint) {
                Some(existing) => existing,
                None => {
                    let ata = get_associated_token_address_with_program_id(
                        &self.liquidator,
                        &liab_mint,
                        &liab_tp,
                    );
                    return Err(format!(
                        "PREWARM_REQUIRED: missing liability ATA={} mint={} token_program={}",
                        ata, liab_mint, liab_tp
                    ));
                }
            };

            let obs = &prep.observation;
            let (withdraw, _underlying_out) =
                self.asset_withdraw_instruction(account, obs, sz, &asset_ata, &asset_tp)?;
            let repay = builders::repay(
                &self.program,
                &self.group,
                account,
                &self.liquidator,
                &sz.liab_bank,
                &liab_mint,
                &liab_ata,
                &liab_vault,
                &liab_tp,
                sz.repay_amount,
                sz.repay_all.then_some(true),
                obs,
            );
            // P2-06: the prepared template; its errors surface here, where they always did.
            let (active_banks_before, pre_refreshes) = prep.banks_and_refreshes.clone()?;
            let start_observation: Vec<AccountMeta> = obs
                .iter()
                .map(|k| {
                    if active_banks_before.contains(k) {
                        AccountMeta::new(*k, false)
                    } else {
                        AccountMeta::new_readonly(*k, false)
                    }
                })
                .collect();
            // Pinned receivership tests exclude a bank closed by repay_all from EndLiquidation's
            // bank-only layout. Start/Repay still observe the pre-close bank and its oracle.
            let end_banks = end_banks_after_repay(active_banks_before, sz.liab_bank, sz.repay_all);

            let init_record = prep.init_record.clone()?;
            let ixs = builders::build_receivership(ReceivershipParams {
                program: &self.program,
                group: &self.group,
                marginfi_account: account,
                liquidator: &self.liquidator,
                // `plans` checks the FeeState first (typed); here only its text can remain.
                global_fee_wallet: required_fee_wallet(&self.global_fee_wallet)
                    .map_err(|e| e.text)?,
                start_observation,
                end_banks,
                init_record,
                pre_refreshes,
                withdraws: vec![withdraw],
                swaps,
                repays: vec![repay],
            });
            // E2: the signed amounts are the sizing's — the withdraw its units (an integration
            // bank's converted instruction amount, recomputed here), never withdraw-all; the
            // repay its amount and `repay_all` — decoded back from the built bytes.
            let expected_withdraw = {
                let bank = self.bank_full(&sz.asset_bank)?;
                darkbot_project0::integration::execution_amount_for_asset(
                    self.state,
                    &bank,
                    sz.withdraw_amount,
                )?
                .map_or(sz.withdraw_amount, |e| e.instruction_amount)
            };
            let built = builders::built_amounts(&ixs, &self.program)?;
            let sized = builders::BuiltAmounts {
                withdraw: (expected_withdraw, false),
                repay: (sz.repay_amount, sz.repay_all),
            };
            if built != sized {
                return Err(format!(
                    "E2_AMOUNTS_NOT_SIZED: built {built:?} sized {sized:?}"
                ));
            }
            // E2: the planner gate derives its steps from exactly these instructions.
            Ok(ixs)
        }

        fn input(
            &self,
            sz: &Sizing,
            funding: FundingClass,
            realizable_value_usd_micro: Option<u64>,
            capital_usd_micro: u64,
        ) -> CandidateInput {
            candidate_input(
                sz,
                funding,
                realizable_value_usd_micro,
                capital_usd_micro,
                self.jito_tip_usd_micro,
                self.path,
            )
        }
    }

    impl<'a, S: AccountSource + ?Sized, Z: SizingProvider, R: RouteProvider> ReceivershipBuilder
        for P0ReceivershipBuilder<'a, S, Z, R>
    {
        fn native_cost_usd_micro(&self, lamports: u64) -> Option<u64> {
            (self.lamports_to_usd_micro)(lamports)
        }

        fn instruction_table(&self) -> &InstructionTable {
            self.instructions
        }

        fn hold_b(
            &self,
            plan: &ReceivershipPlan,
            sol_debit: Option<u64>,
        ) -> crate::execution::hold_b::BHold {
            crate::execution::hold_b::hold_b_of(
                self.state,
                &self.program,
                &self.liquidator,
                plan,
                sol_debit,
            )
        }

        fn plans(
            &self,
            account: &Pubkey,
            now_unix_s: u64,
            slot: u64,
            deadline: Instant,
        ) -> Result<Vec<ReceivershipPlan>, PlanError> {
            required_fee_wallet(&self.global_fee_wallet)?;
            let sizings = self.sizing.sizes(account, now_unix_s, slot)?;
            self.plans_of(account, sizings, slot, deadline)
        }

        fn plans_with_health(
            &self,
            account: &Pubkey,
            health: &HealthView,
            now_unix_s: u64,
            slot: u64,
            deadline: Instant,
        ) -> Result<Vec<ReceivershipPlan>, PlanError> {
            required_fee_wallet(&self.global_fee_wallet)?;
            let sizings = self
                .sizing
                .sizes_with_health(account, health, now_unix_s, slot)?;
            self.plans_of(account, sizings, slot, deadline)
        }
    }

    impl<'a, S: AccountSource + ?Sized, Z: SizingProvider, R: RouteProvider>
        P0ReceivershipBuilder<'a, S, Z, R>
    {
        /// Every plan of the account's sizings (`plans` / `plans_with_health` differ only in
        /// where the Health behind the sizings came from).
        fn plans_of(
            &self,
            account: &Pubkey,
            sizings: Vec<Sizing>,
            slot: u64,
            deadline: Instant,
        ) -> Result<Vec<ReceivershipPlan>, PlanError> {
            if sizings.is_empty() {
                return Err(no_size_error(self.sizing.no_size_reason(account)?));
            }
            // P2-07: the prepared template and LUT plan, reused only while every account they
            // were derived from is byte-identical (otherwise prepared afresh, as before).
            let (protocol_luts, prep) = match self.prepared {
                Some((cache, generation)) => {
                    let context = Context {
                        generation,
                        program: self.program,
                        group: self.group,
                        liquidator: self.liquidator,
                    };
                    let mut cache = cache.borrow_mut();
                    let luts = cache.lookup_tables(self.state, context, slot, |rec, at| {
                        crate::execution::lookup_tables::from_state(rec, &self.group, at)
                    });
                    let prep = cache.skeleton(self.state, account, context, slot, |rec| {
                        prepare_skeleton(rec, &self.program, &self.group, account)
                    })?;
                    (luts, prep)
                }
                None => (
                    crate::execution::lookup_tables::from_state(self.state, &self.group, slot)
                        .into(),
                    std::sync::Arc::new(prepare_skeleton(
                        self.state,
                        &self.program,
                        &self.group,
                        account,
                    )?),
                ),
            };
            let mut out = Vec::new();

            let mut route_errors = Vec::new();
            let mut ata_errors = Vec::new();

            // Z12: an error of one asset/liability pair skips that pair only; account-wide
            // checks (fee wallet, sizing, observation, record link) stay fatal above and below.
            let mut pair_errors: Vec<PlanError> = Vec::new();
            macro_rules! pair_try {
                ($pair:lifetime, $e:expr) => {
                    match $e {
                        Ok(v) => v,
                        Err(e) => {
                            pair_errors.push(PlanError::from(e));
                            continue $pair;
                        }
                    }
                };
            }
            'pairs: for sz in sizings {
                let (_, liab_mint, liab_tp) = pair_try!('pairs, self.bank(&sz.liab_bank));
                let (_, asset_mint, asset_tp) = pair_try!('pairs, self.bank(&sz.asset_bank));

                let asset_ata = (self.ata)(&asset_mint);
                let liab_ata = (self.ata)(&liab_mint);
                let mut missing = Vec::new();
                if asset_ata.is_none() {
                    missing.push(format!(
                        "asset ATA={} mint={} token_program={}",
                        get_associated_token_address_with_program_id(
                            &self.liquidator,
                            &asset_mint,
                            &asset_tp
                        ),
                        asset_mint,
                        asset_tp
                    ));
                }
                if liab_ata.is_none() {
                    missing.push(format!(
                        "liability ATA={} mint={} token_program={}",
                        get_associated_token_address_with_program_id(
                            &self.liquidator,
                            &liab_mint,
                            &liab_tp
                        ),
                        liab_mint,
                        liab_tp
                    ));
                }
                if !missing.is_empty() {
                    ata_errors.extend(missing);
                    continue;
                }
                let prewarmed_atas = [
                    PrewarmedAta {
                        address: asset_ata.expect("checked above"),
                        mint: asset_mint,
                        token_program: asset_tp,
                    },
                    PrewarmedAta {
                        address: liab_ata.expect("checked above"),
                        mint: liab_mint,
                        token_program: liab_tp,
                    },
                ];

                let inv = (self.inventory)(&sz.liab_bank);

                // [FIX-97] Funding feasibility is independent from an asset→liability market
                // quote. Inventory/JIT candidates are built first with UNKNOWN realization
                // economics; LIVE cannot act on UNKNOWN, while Shadow may Build/Sign/Sim them.
                let base_ctx = FundingContext {
                    liability_mint: liab_mint,
                    liability_needed: sz.repay_amount,
                    liability_inventory: inv,
                    liability_needed_value: sz.liability_needed_value,
                    liability_inventory_value: sz.liability_inventory_value,
                    funding_bank: sz.funding_bank,
                    funding_available_value: sz.funding_available_value,
                    zero_prefund_qualified: self.zero_prefund_qualified,
                    route_available: false,
                };

                // V2-14 P1-04: the no-swap receivership instructions are identical for every
                // class that uses them (inventory and JIT): built once per pair.
                let mut no_swap_ixs: Option<Result<Vec<Instruction>, String>> = None;
                for class in funding::feasible_classes(&base_ctx) {
                    let capital = funding::capital_required(class, &base_ctx);
                    let plan = match class {
                        // route_available=false above, therefore ZERO_PREFUND is handled
                        // separately below after its own ExactIn quote.
                        FundingClass::ZeroPrefundSameTx => continue,
                        FundingClass::NoSwapInventory | FundingClass::PreheldInventory => {
                            let ixs = pair_try!(
                                'pairs,
                                no_swap_ixs
                                    .get_or_insert_with(|| {
                                        self.receivership(account, &prep, &sz, vec![])
                                    })
                                    .clone()
                            );
                            ReceivershipPlan {
                                market_identity: Some(
                                    crate::analytics::record::MarketPlanIdentity {
                                        liquidation_type: "RECEIVERSHIP".into(),
                                        asset_bank: sz.asset_bank.to_string(),
                                        liability_bank: sz.liab_bank.to_string(),
                                    },
                                ),
                                instructions: ixs,
                                route: RouteKind::NoSwap,
                                input: self.input(&sz, class, None, capital),
                                premium_bps: sz.premium_bps,
                                tag_growth: sz.tag_growth,
                                exposure: capital,
                                inventory_hold_usd_micro: inventory_hold_usd_micro(class, &sz),
                                tip: self.jito_tip_lamports,
                                slippage_bps: 0,
                                assets: vec![asset_mint, liab_mint],
                                tx_version: BuildVersion::V0,
                                lookup_tables: protocol_luts.clone(),
                                prerequisite_txs: vec![],
                                inventory_holds: vec![(liab_mint, sz.repay_amount)],
                                hold_facts: Some(hold_facts(&sz, asset_mint, liab_mint, None)),
                                sol_costs: SolCosts {
                                    flat_fee_lamports: sz.flat_sol_fee_lamports,
                                    record_rent_lamports: Some(0),
                                },
                            }
                        }
                        FundingClass::JitShortfall => {
                            let Some(fb) = sz.funding_bank else { continue };
                            let (_, funding_mint, _) = pair_try!('pairs, self.bank(&fb));
                            let shortfall = funding::shortfall_token_amount(&base_ctx);
                            // P4A-JIT-01: the bounded ExactIn solve (no documented ExactOut).
                            let buy = match shortfall_exact_in(
                                self.routes,
                                &funding_mint,
                                &liab_mint,
                                shortfall,
                                (self.inventory)(&fb),
                                &prewarmed_atas,
                                deadline,
                            ) {
                                Ok(Some(route)) => route,
                                Ok(None) => continue,
                                Err(e) => {
                                    route_errors.push(format!(
                                        "jit exact-in {}→{}: {}",
                                        funding_mint, liab_mint, e
                                    ));
                                    continue;
                                }
                            };
                            // P5-JIT-01: the capital is the funding the buy actually spends.
                            let shortfall_value = capital;
                            let Some(capital) = jit_capital_usd_micro(
                                shortfall_value,
                                buy.amount_in,
                                (self.inventory)(&fb),
                                sz.funding_available_value,
                            ) else {
                                continue;
                            };
                            let ixs = pair_try!(
                                'pairs,
                                no_swap_ixs
                                    .get_or_insert_with(|| {
                                        self.receivership(account, &prep, &sz, vec![])
                                    })
                                    .clone()
                            );
                            ReceivershipPlan {
                                market_identity: Some(
                                    crate::analytics::record::MarketPlanIdentity {
                                        liquidation_type: "RECEIVERSHIP".into(),
                                        asset_bank: sz.asset_bank.to_string(),
                                        liability_bank: sz.liab_bank.to_string(),
                                    },
                                ),
                                instructions: ixs,
                                route: RouteKind::NoSwap,
                                input: jit_candidate_input(
                                    &sz,
                                    None,
                                    capital,
                                    shortfall_value,
                                    self.jito_tip_usd_micro,
                                    self.path,
                                ),
                                premium_bps: sz.premium_bps,
                                tag_growth: sz.tag_growth,
                                exposure: capital,
                                inventory_hold_usd_micro: inventory_hold_usd_micro(class, &sz),
                                tip: self.jito_tip_lamports,
                                // FE-02: the buy's enforced slippage binds the approval limits.
                                slippage_bps: enforced_slippage(&buy, sz.slippage_bps),
                                // DEC-11: the funding the buy spends is an asset of the plan
                                // too (Safety's asset allowlist, FE-04's expected wallet deltas).
                                assets: {
                                    let mut assets = vec![asset_mint, liab_mint];
                                    if !assets.contains(&funding_mint) {
                                        assets.push(funding_mint);
                                    }
                                    assets
                                },
                                tx_version: BuildVersion::V0,
                                lookup_tables: crate::execution::lookup_tables::merge(
                                    &protocol_luts,
                                    &buy.lookup_tables,
                                ),
                                prerequisite_txs: vec![buy.instructions],
                                // V7/FE-04: the liability inventory the repay uses and the
                                // funding the buy spends are both held.
                                inventory_holds: jit_inventory_holds(
                                    (liab_mint, inv.min(sz.repay_amount)),
                                    (funding_mint, buy.amount_in),
                                ),
                                hold_facts: Some(hold_facts(
                                    &sz,
                                    asset_mint,
                                    liab_mint,
                                    Some(funding_mint),
                                )),
                                sol_costs: SolCosts {
                                    flat_fee_lamports: sz.flat_sol_fee_lamports,
                                    record_rent_lamports: Some(0),
                                },
                            }
                        }
                        FundingClass::FlashFunded => continue,
                    };
                    out.push(plan);
                }

                // ZERO_PREFUND has a real collateral→liability swap in the receivership TX,
                // so this class alone requires ExactIn market evidence. A provider error or
                // missing route must not invalidate already-built inventory/JIT candidates.
                if self.zero_prefund_qualified
                    && self.mints_modelled(&asset_mint, &liab_mint, &mut route_errors)
                {
                    let asset_bank = pair_try!('pairs, self.bank_full(&sz.asset_bank));
                    let route_input_amount =
                        pair_try!('pairs, darkbot_project0::integration::execution_amount_for_asset(
                            self.state,
                            &asset_bank,
                            sz.withdraw_amount,
                        ))
                        .map(|x| x.underlying_out)
                        .unwrap_or(sz.withdraw_amount);
                    // MT4: the route-cap hint — only where the search starts; every route
                    // below is quoted afresh and proven by its own compile.
                    let configured_cap = self.routes.max_accounts();
                    let route_cap_key = self.route_cap.and_then(|_| {
                        self.route_footprint_class(account, &prep, &sz, &protocol_luts)
                            .map(|class| RouteCapKey {
                                input_mint: asset_mint,
                                output_mint: liab_mint,
                                class,
                            })
                    });
                    let hint = match (self.route_cap, &route_cap_key) {
                        (Some(cache), Some(key)) => {
                            Some(cache.borrow_mut().hint(key, configured_cap))
                        }
                        _ => None,
                    };
                    if let (Some(Hint::Hit { start_cap }), Some(key)) = (hint, &route_cap_key) {
                        route_diagnostic(format!(
                            "route_cap_hint: start={start_cap} configured={configured_cap} class={}/{} input={} output={}",
                            key.class.locked_accounts, key.class.serialized_len, asset_mint, liab_mint
                        ));
                    }
                    let fitted = quote_fitting_route_traced(
                        hint.map_or(configured_cap, Hint::start_cap),
                        deadline,
                        |cap| {
                            let route = self.routes.quote_exact_in_with_budget(
                                &asset_mint,
                                &liab_mint,
                                route_input_amount,
                                &prewarmed_atas,
                                cap,
                                deadline,
                            )?;
                            if let Some(ref route) = route {
                                if route.amount_in != route_input_amount {
                                    return Err(format!("quote amounts invalid: in={} expected={} quoted_out={} repay={}",
                                        route.amount_in, route_input_amount, route.quoted_out, sz.repay_amount));
                                }
                                // [P1-C] Known floor below repay or invalid floor: not executable.
                                // An unknown floor passes and becomes Evaluation::Unknown downstream.
                                if let Some(reason) = zero_prefund_rejection(route, sz.repay_amount)
                                {
                                    return Err(reason);
                                }
                            }
                            Ok(route)
                        },
                        |route| {
                            let ixs = self
                                .receivership(account, &prep, &sz, route.instructions.clone())
                                .map_err(BuildError::Compile)?;
                            // Same payer/resources/LUTs/signature placeholders as final build.
                            // Hash bytes have fixed width; no RPC or signing needed for sizing.
                            tx::build(
                                &TxPlan {
                                    version: BuildVersion::V0,
                                    payer: self.liquidator,
                                    instructions: ixs.clone(),
                                    lookup_tables: crate::execution::lookup_tables::merge(
                                        &protocol_luts,
                                        &route.lookup_tables,
                                    ),
                                    resources: self.resources,
                                },
                                Hash::default(),
                                &self.limits,
                            )?;
                            Ok(ixs)
                        },
                    );
                    let fitted = match fitted {
                        Ok(Some((route, ixs, trace))) => {
                            // Only a proven fit updates the cache; an error, a missing route
                            // or an exhausted budget leaves the remembered cap as it was.
                            if let (Some(cache), Some(key)) = (self.route_cap, route_cap_key) {
                                cache.borrow_mut().record_fit(key, configured_cap, trace);
                            }
                            Some((route, ixs))
                        }
                        Ok(None) => None,
                        Err(e) => {
                            route_errors
                                .push(format!("exact-in {}→{}: {}", asset_mint, liab_mint, e));
                            None
                        }
                    };
                    if let Some((mark, ixs)) = fitted {
                        let mut zp_ctx = base_ctx.clone();
                        zp_ctx.route_available = true;
                        if funding::is_feasible(FundingClass::ZeroPrefundSameTx, &zp_ctx) {
                            let capital =
                                funding::capital_required(FundingClass::ZeroPrefundSameTx, &zp_ctx);
                            // [P1-C] One production mapping: floor known -> realizable at the floor,
                            // floor unknown -> Evaluation::Unknown, floor < repay / invalid -> no plan.
                            match zero_prefund_input(
                                &sz,
                                &mark,
                                capital,
                                self.jito_tip_usd_micro,
                                self.path,
                            ) {
                                Err(e) => route_errors
                                    .push(format!("exact-in {}→{}: {}", asset_mint, liab_mint, e)),
                                Ok(zp_input) => {
                                    // B9: a V1 candidate only with a buildable V1 transaction.
                                    if self.v1_plans {
                                        out.push(ReceivershipPlan {
                                            market_identity: Some(
                                                crate::analytics::record::MarketPlanIdentity {
                                                    liquidation_type: "RECEIVERSHIP".into(),
                                                    asset_bank: sz.asset_bank.to_string(),
                                                    liability_bank: sz.liab_bank.to_string(),
                                                },
                                            ),
                                            instructions: ixs.clone(),
                                            route: RouteKind::FreePublicAllowed,
                                            input: zp_input,
                                            premium_bps: sz.premium_bps,
                                            tag_growth: sz.tag_growth,
                                            exposure: capital,
                                            inventory_hold_usd_micro: inventory_hold_usd_micro(
                                                FundingClass::ZeroPrefundSameTx,
                                                &sz,
                                            ),
                                            tip: self.jito_tip_lamports,
                                            slippage_bps: enforced_slippage(&mark, sz.slippage_bps),
                                            assets: vec![asset_mint, liab_mint],
                                            tx_version: BuildVersion::V1,
                                            lookup_tables: vec![].into(),
                                            prerequisite_txs: vec![],
                                            inventory_holds: vec![],
                                            hold_facts: Some(hold_facts(
                                                &sz, asset_mint, liab_mint, None,
                                            )),
                                            sol_costs: SolCosts {
                                                flat_fee_lamports: sz.flat_sol_fee_lamports,
                                                record_rent_lamports: Some(0),
                                            },
                                        });
                                    }
                                    out.push(ReceivershipPlan {
                                        market_identity: Some(
                                            crate::analytics::record::MarketPlanIdentity {
                                                liquidation_type: "RECEIVERSHIP".into(),
                                                asset_bank: sz.asset_bank.to_string(),
                                                liability_bank: sz.liab_bank.to_string(),
                                            },
                                        ),
                                        instructions: ixs,
                                        route: RouteKind::FreePublicAllowed,
                                        input: zp_input,
                                        premium_bps: sz.premium_bps,
                                        tag_growth: sz.tag_growth,
                                        exposure: capital,
                                        inventory_hold_usd_micro: inventory_hold_usd_micro(
                                            FundingClass::ZeroPrefundSameTx,
                                            &sz,
                                        ),
                                        tip: self.jito_tip_lamports,
                                        // FE-02: what the signed swap enforces, so the
                                        // safety and approval slippage limits bind it.
                                        slippage_bps: enforced_slippage(&mark, sz.slippage_bps),
                                        assets: vec![asset_mint, liab_mint],
                                        tx_version: BuildVersion::V0,
                                        lookup_tables: crate::execution::lookup_tables::merge(
                                            &protocol_luts,
                                            &mark.lookup_tables,
                                        ),
                                        prerequisite_txs: vec![],
                                        inventory_holds: vec![],
                                        hold_facts: Some(hold_facts(
                                            &sz, asset_mint, liab_mint, None,
                                        )),
                                        sol_costs: SolCosts {
                                            flat_fee_lamports: sz.flat_sol_fee_lamports,
                                            record_rent_lamports: Some(0),
                                        },
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // Z12: without any plan, the first pair error ends planning exactly as it did before
            // (same text, same classification); otherwise it stays a diagnostic.
            if out.is_empty() {
                if let Some(first) = pair_errors.first() {
                    return Err(first.clone());
                }
            }
            for error in &pair_errors {
                route_diagnostic(format!("pair_skipped: {error}"));
            }
            // FE-02: a rejected class beside other plans keeps its stated cause (e.g.
            // ROUTE_FLOOR_BELOW_REPAY, SWAP_*), never silently dropped.
            if !out.is_empty() {
                for error in &route_errors {
                    route_diagnostic(format!("route_rejected: {error}"));
                }
            }
            if out.is_empty() && !ata_errors.is_empty() {
                ata_errors.sort();
                ata_errors.dedup();
                let routing = if route_errors.is_empty() {
                    String::new()
                } else {
                    format!("; other pairs ROUTING: {}", route_errors.join(" | "))
                };
                return Err(PlanError::new(
                    PlanErrorKind::MissingAta,
                    format!(
                        "ATA:PREWARM_REQUIRED: {}{}",
                        ata_errors.join(" | "),
                        routing
                    ),
                ));
            }
            if out.is_empty() && !route_errors.is_empty() {
                return Err(PlanError::new(
                    PlanErrorKind::Routing,
                    format!("ROUTING:{}", route_errors.join(" | ")),
                ));
            }
            let creates_record = prep.init_record.clone()?;
            // B4: the record rent in lamports, the SOL the intent binds (unknown sysvar: None).
            let record_rent_lamports = if creates_record {
                liquidation_record_rent_lamports(self.state).ok()
            } else {
                Some(0)
            };
            for plan in &mut out {
                plan.sol_costs.record_rent_lamports = record_rent_lamports;
            }
            if creates_record {
                // B4: the record this plan creates costs its rent-exempt minimum. The pinned
                // program can return it (`close_liquid_record`, to the record payer), but no plan
                // contains that instruction, so no recovery is credited. Known only with the
                // canonical Rent sysvar and a conservative SOL price; otherwise UNKNOWN.
                let rent = record_rent_usd_micro(self.state, self.rent_lamports_to_usd_micro);
                for plan in &mut out {
                    plan.input.rent_and_token_usd_micro =
                        plus_record_rent(plan.input.rent_and_token_usd_micro, rent);
                }
            }
            Ok(out)
        }
    }
    #[cfg(test)]
    mod fix100_tests {
        use super::*;

        #[test]
        fn p5_unreadable_fee_state_blocks_instead_of_default_receiver() {
            let err = required_fee_wallet(&Err("FeeState account missing".into())).unwrap_err();
            assert_eq!(err.text, "FEE_STATE_UNAVAILABLE:FeeState account missing");
            assert_eq!(err.kind, PlanErrorKind::FeeStateUnavailable);
            let wallet = Pubkey::new_unique();
            assert_eq!(required_fee_wallet(&Ok(wallet)), Ok(&wallet));
        }

        fn floor_route(quoted_out: u64, floor: Option<u64>) -> Route {
            Route {
                amount_in: 1_000,
                quoted_out,
                minimum_executable_out: floor,
                enforced_slippage_bps: None,
                ..Route::default()
            }
        }

        fn sizing(repay_amount: u64, repay_value: u64) -> Sizing {
            Sizing {
                asset_bank: Pubkey::new_unique(),
                liab_bank: Pubkey::new_unique(),
                withdraw_amount: 1_000,
                repay_amount,
                repay_all: false,
                seized_equity_value: 0,
                seized_hold_value: 0,
                repay_value,
                repay_low_value: Some(repay_value),
                liability_needed_value: repay_value,
                liability_inventory_value: 0,
                funding_bank: None,
                funding_available_value: 0,
                protocol_fees: 0,
                flat_sol_fee: 150_000,
                flat_sol_fee_lamports: 1_000,
                rent_and_token: 0,
                premium_bps: 0,
                slippage_bps: 50,
                tag_growth: None,
            }
        }

        /// B4: the record rent is the canonical Rent sysvar's exempt minimum for the pinned
        /// record size; a missing, foreign or undecodable sysvar is no rent (UNKNOWN).
        #[test]
        fn b4_liquidation_record_rent_comes_from_the_rent_sysvar() {
            use std::collections::HashMap;
            struct Source(HashMap<Pubkey, (Pubkey, Vec<u8>)>);
            impl AccountSource for Source {
                fn account(&self, k: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
                    self.0.get(k).map(|(o, d)| (*o, 1, d.as_slice()))
                }
            }
            assert_eq!(LIQUIDATION_RECORD_SPACE, 520);
            let rent_key = solana_sdk::sysvar::rent::id();
            let with =
                |owner: Pubkey, data: Vec<u8>| Source(HashMap::from([(rent_key, (owner, data))]));
            let mainnet = solana_sdk::rent::Rent::default();
            let ok = with(
                solana_sdk::sysvar::id(),
                bincode::serialize(&mainnet).unwrap(),
            );
            // (128 overhead + 520) bytes × 3480 lamports/byte-year × 2 years exemption.
            assert_eq!(liquidation_record_rent_lamports(&ok), Ok(4_510_080));
            // The sysvar data as the chain carries it: first field lamports per byte-year.
            let mut doubled = bincode::serialize(&mainnet).unwrap();
            doubled[..8].copy_from_slice(&6960u64.to_le_bytes());
            let changed = with(solana_sdk::sysvar::id(), doubled);
            assert_eq!(liquidation_record_rent_lamports(&changed), Ok(9_020_160));
            assert_eq!(
                liquidation_record_rent_lamports(&Source(HashMap::new())),
                Err("RENT_SYSVAR_MISSING")
            );
            let foreign = with(Pubkey::new_unique(), bincode::serialize(&mainnet).unwrap());
            assert_eq!(
                liquidation_record_rent_lamports(&foreign),
                Err("RENT_SYSVAR_OWNER")
            );
            let short = with(solana_sdk::sysvar::id(), vec![1, 2, 3]);
            assert_eq!(
                liquidation_record_rent_lamports(&short),
                Err("RENT_SYSVAR_DECODE")
            );
        }

        /// B4: a plan that creates the record carries the sysvar rent at the conservative SOL
        /// price on top of its other costs; unknown price, missing sysvar, unknown base cost or
        /// overflow keep the plan's rent/token cost UNKNOWN (never zero, never base alone).
        #[test]
        fn b4_record_creating_plans_carry_the_rent_or_stay_unknown() {
            use std::collections::HashMap;
            struct Source(HashMap<Pubkey, (Pubkey, Vec<u8>)>);
            impl AccountSource for Source {
                fn account(&self, k: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
                    self.0.get(k).map(|(o, d)| (*o, 1, d.as_slice()))
                }
            }
            let sysvar = Source(HashMap::from([(
                solana_sdk::sysvar::rent::id(),
                (
                    solana_sdk::sysvar::id(),
                    bincode::serialize(&solana_sdk::rent::Rent::default()).unwrap(),
                ),
            )]));
            // 150 USD/SOL, rounded up: 4_510_080 lamports ⇒ 676_512 micro.
            let price = |lamports: u64| Some((u128::from(lamports) * 150).div_ceil(1_000) as u64);
            let unknown_price = |_: u64| None;
            let rent = record_rent_usd_micro(&sysvar, &price);
            assert_eq!(rent, Some(676_512));
            assert_eq!(plus_record_rent(Some(1_000), rent), Some(677_512));
            assert_eq!(record_rent_usd_micro(&sysvar, &unknown_price), None);
            assert_eq!(record_rent_usd_micro(&Source(HashMap::new()), &price), None);
            assert_eq!(plus_record_rent(Some(1_000), None), None);
            assert_eq!(plus_record_rent(None, rent), None);
            assert_eq!(plus_record_rent(Some(u64::MAX), Some(1)), None);
        }

        /// A real `P0ReceivershipBuilder::plans()` setup: a MarginfiAccount with one asset and one
        /// liability balance on two Fixed-oracle banks, their mints, the group and (optionally)
        /// the Rent sysvar; sizing and routes are the inputs under test.
        struct PlansFixture {
            program: Pubkey,
            group: Pubkey,
            liquidator: Pubkey,
            account: Pubkey,
            banks: [(Pubkey, Pubkey); 2],
            token_program: Pubkey,
            table: darkbot_project0::planner::InstructionTable,
            sizing: OneSizing,
            /// The liquidator's ATAs, derived once like the owner's `LiquidatorAtas` (C11).
            atas: std::cell::RefCell<std::collections::HashMap<Pubkey, Pubkey>>,
            /// Inventory of the liability bank (every other bank holds 10_000).
            liability_inventory: u64,
        }
        struct FixtureSource(std::collections::HashMap<Pubkey, (Pubkey, Vec<u8>)>);
        impl AccountSource for FixtureSource {
            fn account(&self, k: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
                self.0.get(k).map(|(o, d)| (*o, 1, d.as_slice()))
            }
        }
        struct OneSizing(Sizing);
        impl SizingProvider for OneSizing {
            fn sizes(&self, _: &Pubkey, _: u64, _: u64) -> Result<Vec<Sizing>, PlanError> {
                Ok(vec![self.0.clone()])
            }
        }
        /// The ExactIn route a router would return (or none).
        /// A deterministic market: `out = input * num / den`, the on-chain floor 1 % below the
        /// quote (`None` when `floor_known` is false); counts every request.
        struct LinearMarket {
            num: u64,
            den: u64,
            floor_known: bool,
            calls: std::cell::Cell<u32>,
        }
        impl RouteProvider for LinearMarket {
            fn quote_exact_in(
                &self,
                from: &Pubkey,
                _: &Pubkey,
                amount_in: u64,
                _: &[PrewarmedAta],
                _: Instant,
            ) -> Result<Option<Route>, String> {
                self.calls.set(self.calls.get() + 1);
                // The reverse direction (liability sold for funding) is the inverse price.
                let out = if *from == Pubkey::default() {
                    amount_in * self.den / self.num
                } else {
                    amount_in * self.num / self.den
                };
                Ok(Some(Route {
                    instructions: vec![],
                    lookup_tables: vec![],
                    swap_program: Pubkey::new_unique(),
                    amount_in,
                    quoted_out: out,
                    minimum_executable_out: self.floor_known.then_some(out * 99 / 100),
                    enforced_slippage_bps: None,
                }))
            }
        }

        /// A23-05: a reverse quote and a shortfall near `u64::MAX` with a tiny forward floor: the
        /// proportional step saturates to the inventory bound (before: `input × out × 65`
        /// overflowed u128 — a debug panic, a release wrap to a small step); no covering buy.
        #[test]
        fn a23_05_jit_step_saturates_instead_of_overflowing() {
            struct TinyFloor(std::cell::Cell<u32>);
            impl RouteProvider for TinyFloor {
                fn quote_exact_in(
                    &self,
                    from: &Pubkey,
                    _: &Pubkey,
                    amount_in: u64,
                    _: &[PrewarmedAta],
                    _: Instant,
                ) -> Result<Option<Route>, String> {
                    self.0.set(self.0.get() + 1);
                    let out = if *from == Pubkey::default() {
                        1 << 62
                    } else {
                        1
                    };
                    Ok(Some(Route {
                        instructions: vec![],
                        lookup_tables: vec![],
                        swap_program: Pubkey::new_unique(),
                        amount_in,
                        quoted_out: out,
                        minimum_executable_out: Some(out),
                        enforced_slippage_bps: None,
                    }))
                }
            }
            let m = TinyFloor(std::cell::Cell::new(0));
            let later = Instant::now() + std::time::Duration::from_secs(5);
            let (liab, funding) = (Pubkey::default(), Pubkey::new_unique());
            assert!(
                shortfall_exact_in(&m, &funding, &liab, 1 << 62, u64::MAX, &[], later)
                    .unwrap()
                    .is_none()
            );
            assert!(m.0.get() <= JIT_MAX_QUOTES, "{}", m.0.get());
        }

        /// P4A-JIT-01: the bounded ExactIn shortfall solve finds a covering buy near the smallest
        /// input in at most `JIT_MAX_QUOTES` requests; never an uncovered, unknown-floor, above
        /// inventory or post-deadline route.
        #[test]
        fn jit_shortfall_exact_in_is_bounded_and_covers_on_chain() {
            let (liab, funding) = (Pubkey::default(), Pubkey::new_unique());
            let later = Instant::now() + std::time::Duration::from_secs(5);
            let market = |floor_known| LinearMarket {
                num: 3,
                den: 2,
                floor_known,
                calls: std::cell::Cell::new(0),
            };
            let m = market(true);
            let need = 1_000_000;
            let r = shortfall_exact_in(&m, &funding, &liab, need, u64::MAX, &[], later)
                .unwrap()
                .expect("covering buy");
            let floor = r.minimum_executable_out.unwrap();
            assert!(floor >= need, "covers on chain: {floor}");
            // Minimal input to cover: need / (1.5 * 0.99); the solve stays within 2 %.
            let minimal = need * 2 * 100 / (3 * 99) + 1;
            assert!(
                r.amount_in >= minimal && r.amount_in <= minimal * 102 / 100,
                "{}",
                r.amount_in
            );
            assert!(m.calls.get() <= JIT_MAX_QUOTES, "{}", m.calls.get());
            // Unknown on-chain floor: never a buy.
            let unknown = market(false);
            assert!(
                shortfall_exact_in(&unknown, &funding, &liab, need, u64::MAX, &[], later)
                    .unwrap()
                    .is_none()
            );
            assert!(unknown.calls.get() <= JIT_MAX_QUOTES);
            // The funding inventory cannot cover: no buy above it.
            assert!(shortfall_exact_in(
                &market(true),
                &funding,
                &liab,
                need,
                minimal - 1,
                &[],
                later
            )
            .unwrap()
            .is_none());
            // Nothing after the deadline, no shortfall, no inventory: no request at all.
            let none = market(true);
            let past = Instant::now() - std::time::Duration::from_millis(1);
            assert!(
                shortfall_exact_in(&none, &funding, &liab, need, u64::MAX, &[], past)
                    .unwrap()
                    .is_none()
            );
            assert!(
                shortfall_exact_in(&none, &funding, &liab, 0, u64::MAX, &[], later)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(none.calls.get(), 0);
        }

        /// P5-JIT-01: a JIT plan binds the capital its buy spends (spread and floor included),
        /// never the bare shortfall value; no funding, no capital.
        #[test]
        fn jit_capital_is_the_funding_the_buy_spends() {
            // 1_000 USD-micro shortfall; 2_000 tokens worth 2_000 USD-micro of funding.
            assert_eq!(
                jit_capital_usd_micro(1_000, 1_020, 2_000, 2_000),
                Some(1_020)
            );
            // Rounded up, never below the shortfall value.
            assert_eq!(jit_capital_usd_micro(1_000, 1, 3, 2), Some(1_000));
            assert_eq!(jit_capital_usd_micro(0, 1, 3, 2), Some(1));
            assert_eq!(jit_capital_usd_micro(1_000, 5, 0, 2_000), None);
        }

        struct TestRoutes(Option<Route>);
        impl RouteProvider for TestRoutes {
            fn quote_exact_in(
                &self,
                _: &Pubkey,
                _: &Pubkey,
                _: u64,
                _: &[PrewarmedAta],
                _: Instant,
            ) -> Result<Option<Route>, String> {
                Ok(self.0.clone())
            }
        }
        impl PlansFixture {
            fn new() -> Self {
                let program: Pubkey = darkbot_project0::protocol::PROGRAM_ID_MAINNET
                    .parse()
                    .unwrap();
                let mut banks = [Pubkey::new_unique(), Pubkey::new_unique()];
                banks.sort_unstable_by(|a, b| b.cmp(a));
                Self {
                    program,
                    group: Pubkey::new_unique(),
                    liquidator: Pubkey::new_unique(),
                    account: Pubkey::new_unique(),
                    banks: [
                        (banks[0], Pubkey::new_unique()),
                        (banks[1], Pubkey::new_unique()),
                    ],
                    token_program: darkbot_project0::protocol::TOKEN_PROGRAM_ID
                        .parse()
                        .unwrap(),
                    table: darkbot_project0::receivership::builders::instruction_table(&program)
                        .unwrap(),
                    sizing: OneSizing(Sizing {
                        asset_bank: banks[0],
                        liab_bank: banks[1],
                        withdraw_amount: 1_000,
                        repay_amount: 500,
                        repay_all: false,
                        seized_equity_value: 1_050,
                        seized_hold_value: 1_100,
                        repay_value: 500,
                        repay_low_value: Some(500),
                        liability_needed_value: 500,
                        liability_inventory_value: 10_000,
                        funding_bank: None,
                        funding_available_value: 0,
                        protocol_fees: 0,
                        flat_sol_fee: 0,
                        flat_sol_fee_lamports: 0,
                        rent_and_token: 7,
                        premium_bps: 500,
                        slippage_bps: 0,
                        tag_growth: None,
                    }),
                    atas: Default::default(),
                    liability_inventory: 10_000,
                }
            }
            fn state(&self, record: Pubkey, rent_sysvar: bool) -> FixtureSource {
                use darkbot_project0::protocol;
                use marginfi_type_crate::types::{
                    BankOperationalState, MarginfiAccount, MarginfiGroup, OracleSetup,
                    WrappedI80F48,
                };
                // I80F48 has 48 fractional bits: `v` as its raw fixed-point value.
                let one = |v: i128| WrappedI80F48 {
                    value: (v << 48).to_le_bytes(),
                };
                let wrap = |disc: [u8; 8], bytes: &[u8]| [disc.as_slice(), bytes].concat();
                let mut m = std::collections::HashMap::new();
                m.insert(
                    self.group,
                    (
                        self.program,
                        wrap(
                            protocol::DISC_MARGINFI_GROUP,
                            bytemuck::bytes_of(&<MarginfiGroup as bytemuck::Zeroable>::zeroed()),
                        ),
                    ),
                );
                for (bank_key, mint) in self.banks {
                    let mut b: Bank = bytemuck::Zeroable::zeroed();
                    b.group = self.group;
                    b.mint = mint;
                    b.mint_decimals = 6;
                    b.asset_share_value = one(1);
                    b.liability_share_value = one(1);
                    b.config.asset_weight_init = one(1);
                    b.config.asset_weight_maint = one(1);
                    b.config.liability_weight_init = one(1);
                    b.config.liability_weight_maint = one(1);
                    b.config.operational_state = BankOperationalState::Operational;
                    b.config.oracle_setup = OracleSetup::Fixed;
                    b.config.fixed_price = one(1);
                    m.insert(
                        bank_key,
                        (
                            self.program,
                            wrap(protocol::DISC_BANK, bytemuck::bytes_of(&b)),
                        ),
                    );
                    m.insert(mint, (self.token_program, vec![0; 82]));
                }
                let mut a: MarginfiAccount = bytemuck::Zeroable::zeroed();
                a.group = self.group;
                a.liquidation_record = record;
                let bal = &mut a.lending_account.balances;
                bal[0].active = 1;
                bal[0].bank_pk = self.banks[0].0;
                bal[0].asset_shares = one(1_000);
                bal[1].active = 1;
                bal[1].bank_pk = self.banks[1].0;
                bal[1].liability_shares = one(1_500);
                m.insert(
                    self.account,
                    (
                        self.program,
                        wrap(protocol::DISC_MARGINFI_ACCOUNT, bytemuck::bytes_of(&a)),
                    ),
                );
                if rent_sysvar {
                    m.insert(
                        solana_sdk::sysvar::rent::id(),
                        (
                            solana_sdk::sysvar::id(),
                            bincode::serialize(&solana_sdk::rent::Rent::default()).unwrap(),
                        ),
                    );
                }
                FixtureSource(m)
            }
            fn plans(
                &self,
                src: &FixtureSource,
                rent: &dyn Fn(u64) -> Option<u64>,
                routes: &TestRoutes,
                zero_prefund: bool,
            ) -> Vec<ReceivershipPlan> {
                self.plans_with(src, rent, routes, zero_prefund, None, None)
            }
            fn plans_with<R: RouteProvider>(
                &self,
                src: &FixtureSource,
                rent: &dyn Fn(u64) -> Option<u64>,
                routes: &R,
                zero_prefund: bool,
                prepared: Option<(&std::cell::RefCell<PreparedCache>, u64)>,
                route_cap: Option<&std::cell::RefCell<RouteCapCache>>,
            ) -> Vec<ReceivershipPlan> {
                let liab_bank = self.sizing.0.liab_bank;
                let liability_inventory = self.liability_inventory;
                let inventory = move |bank: &Pubkey| {
                    if *bank == liab_bank {
                        liability_inventory
                    } else {
                        10_000u64
                    }
                };
                let ata = |mint: &Pubkey| {
                    Some(*self.atas.borrow_mut().entry(*mint).or_insert_with(|| {
                        get_associated_token_address_with_program_id(
                            &self.liquidator,
                            mint,
                            &self.token_program,
                        )
                    }))
                };
                let lamports = |l: u64| Some(l);
                P0ReceivershipBuilder {
                    state: src,
                    program: self.program,
                    group: self.group,
                    liquidator: self.liquidator,
                    global_fee_wallet: Ok(Pubkey::new_from_array([7; 32])),
                    sizing: &self.sizing,
                    routes,
                    inventory: &inventory,
                    ata: &ata,
                    lamports_to_usd_micro: &lamports,
                    rent_lamports_to_usd_micro: rent,
                    jito_tip_usd_micro: 0,
                    jito_tip_lamports: 0,
                    zero_prefund_qualified: zero_prefund,
                    instructions: &self.table,
                    v1_plans: false,
                    prepared,
                    route_cap,
                    path: Path::Rpc,
                    resources: Resources {
                        compute_unit_limit: 400_000,
                        priority_micro_lamports_per_cu: 0,
                        loaded_accounts_data_size: None,
                    },
                    limits: TxLimits::default(),
                }
                .plans(
                    &self.account,
                    1_000,
                    10,
                    Instant::now() + std::time::Duration::from_secs(60),
                )
                .unwrap()
            }
        }

        /// A router quoting ExactIn 1:1 with a 1 % enforced floor; like a real provider it returns
        /// `amount_in` = the requested input.
        struct ProportionalRoutes;
        impl RouteProvider for ProportionalRoutes {
            fn quote_exact_in(
                &self,
                _: &Pubkey,
                _: &Pubkey,
                amount_in: u64,
                _: &[PrewarmedAta],
                _: Instant,
            ) -> Result<Option<Route>, String> {
                Ok(Some(Route {
                    instructions: vec![],
                    lookup_tables: vec![],
                    swap_program: Pubkey::new_from_array([8; 32]),
                    amount_in,
                    quoted_out: amount_in,
                    minimum_executable_out: Some(amount_in * 99 / 100),
                    enforced_slippage_bps: Some(100),
                }))
            }
        }

        /// P5-JIT-01 / P4A-JIT-02 / V7 through the production `plans()` path: with 200 of 500
        /// liability units in inventory and a funding bank, the JIT plan buys the shortfall with
        /// a covering ExactIn route (its floor >= 300), holds both inventories it spends (the 200
        /// liability units and the buy's funding input), binds the buy's enforced slippage,
        /// charges the buy's spread as a swap cost and carries the plan's class name.
        #[test]
        fn p5_jit_01_plan_holds_both_inventories_and_charges_the_buy() {
            let mut f = PlansFixture::new();
            f.liability_inventory = 200;
            let funding_bank = f.banks[0].0;
            let s = &mut f.sizing.0;
            (s.liability_needed_value, s.liability_inventory_value) = (500, 200);
            (s.funding_bank, s.funding_available_value) = (Some(funding_bank), 10_000);
            let src = f.state(Pubkey::default(), true);
            let price = |l: u64| Some(l);
            let plans = f.plans_with(&src, &price, &ProportionalRoutes, false, None, None);
            let classes: Vec<_> = plans.iter().map(|p| p.input.funding).collect();
            let jit: Vec<_> = plans
                .iter()
                .filter(|p| p.input.funding == FundingClass::JitShortfall)
                .collect();
            assert_eq!(jit.len(), 1, "{classes:?}");
            let jit = jit[0];
            assert_eq!(jit.input.funding.as_str(), "JIT_SHORTFALL_EXACT_IN");
            assert_eq!(jit.prerequisite_txs.len(), 1, "the buy TX");
            let (liab_mint, funding_mint) = (f.banks[1].1, f.banks[0].1);
            let buy_in = jit
                .inventory_holds
                .iter()
                .find(|(m, _)| *m == funding_mint)
                .map(|h| h.1)
                .expect("funding hold");
            assert!(
                buy_in * 99 / 100 >= 300,
                "the buy's floor covers 300: {buy_in}"
            );
            assert_eq!(
                jit.inventory_holds,
                vec![(liab_mint, 200), (funding_mint, buy_in)]
            );
            assert_eq!(jit.slippage_bps, 100, "the buy's enforced slippage");
            assert!(
                jit.assets.contains(&funding_mint),
                "DEC-11: the funding the buy spends is an asset of the plan"
            );
            // Capital = the buy's spend at the funding inventory's own value per unit (10_000
            // value / 10_000 units); the shortfall it buys is worth 300.
            assert_eq!(jit.input.required_capital_usd_micro, buy_in);
            assert_eq!(jit.input.swap_fees_and_impact_usd_micro, buy_in - 300);
            assert!(
                !classes.contains(&FundingClass::NoSwapInventory),
                "200 < 500: no NO_SWAP plan"
            );
        }

        /// P5-JIT-01: the production sizing names the configured funding bank and values its free
        /// inventory at its conservative cost; no bank or the liability's own bank funds no JIT.
        #[test]
        fn p5_jit_01_protocol_sizing_names_the_configured_funding_bank() {
            let f = PlansFixture::new();
            let mut src = f.state(Pubkey::default(), true);
            // The real sizer reads the protocol fee state (12.5 % liquidation fee cap).
            let mut fee: marginfi_type_crate::types::FeeState = bytemuck::Zeroable::zeroed();
            fee.liquidation_max_fee.value = ((1i128 << 48) >> 3).to_le_bytes();
            src.0.insert(
                darkbot_project0::protocol::fee_state_pda(&f.program),
                (
                    f.program,
                    [
                        darkbot_project0::protocol::DISC_FEE_STATE.as_slice(),
                        bytemuck::bytes_of(&fee),
                    ]
                    .concat(),
                ),
            );
            let (asset_bank, liab_bank) = (f.banks[0].0, f.banks[1].0);
            // Asset weight 0.5 (b7 profile): a repay heals the account, so a pair sizes.
            let data = &mut src.0.get_mut(&asset_bank).unwrap().1;
            let mut bank: Bank = bytemuck::pod_read_unaligned(&data[8..]);
            let half = ((1i128 << 48) >> 1).to_le_bytes();
            bank.config.asset_weight_init.value = half;
            bank.config.asset_weight_maint.value = half;
            data[8..].copy_from_slice(bytemuck::bytes_of(&bank));
            let inventory = |_: &Pubkey| 2_000_000u64;
            let sized = |funding_bank| {
                ProtocolSizingProvider {
                    state: &src,
                    program: f.program,
                    group: f.group,
                    sol_bank: asset_bank,
                    inventory: &inventory,
                    slippage_bps: 0,
                    funding_bank,
                }
                .sizes(&f.account, 1_000, 10)
                .unwrap()
            };
            let value = darkbot_project0::sizing::ProtocolSizer {
                state: &src,
                program: f.program,
                group: f.group,
                headroom_bps: darkbot_project0::sizing::DEFAULT_HEADROOM_BPS,
            }
            .native_amount_conservative_cost_usd_micro(&asset_bank, 2_000_000, 1_000, 10)
            .unwrap();
            assert!(value >= 2_000_000, "{value}");
            let with = sized(Some(asset_bank));
            assert!(!with.is_empty());
            for sz in &with {
                assert_eq!(
                    (sz.funding_bank, sz.funding_available_value),
                    (Some(asset_bank), value)
                );
            }
            for none in [None, Some(liab_bank)] {
                for sz in sized(none) {
                    assert_eq!((sz.funding_bank, sz.funding_available_value), (None, 0));
                }
            }
        }

        /// V2-14 P1-04: the inventory classes share one receivership build per pair (their
        /// instructions are identical: no swap); the plans keep their own classes.
        #[test]
        fn p1_04_inventory_classes_build_the_receivership_once() {
            let f = PlansFixture::new();
            let price = |l: u64| Some(l);
            let src = f.state(Pubkey::default(), true);
            RECEIVERSHIP_BUILDS.with(|n| n.set(0));
            let plans = f.plans(&src, &price, &TestRoutes(None), false);
            assert_eq!(
                plans.iter().map(|p| p.input.funding).collect::<Vec<_>>(),
                [
                    FundingClass::NoSwapInventory,
                    FundingClass::PreheldInventory
                ]
            );
            assert_eq!(plans[0].instructions, plans[1].instructions);
            assert_eq!(RECEIVERSHIP_BUILDS.with(|n| n.get()), 1);
        }

        /// B4 through the production `plans()` path: every plan of an account whose record is
        /// still unlinked carries the record rent (Rent sysvar × conservative SOL price) on top
        /// of its sizing's rent/token cost; a linked record adds nothing; an unknown price or a
        /// missing Rent sysvar keeps every plan's rent/token cost UNKNOWN.
        #[test]
        fn b4_plans_add_the_record_rent_only_when_the_record_is_created() {
            let f = PlansFixture::new();
            // 150 USD/SOL, rounded up: the record rent 4_510_080 lamports ⇒ 676_512 micro.
            let price = |l: u64| Some((u128::from(l) * 150).div_ceil(1_000) as u64);
            let unknown_price = |_: u64| None;
            let rent_of = |src: &FixtureSource, rent: &dyn Fn(u64) -> Option<u64>| {
                f.plans(src, rent, &TestRoutes(None), false)
                    .iter()
                    .map(|p| (p.input.funding, p.input.rent_and_token_usd_micro))
                    .collect::<Vec<_>>()
            };
            let unlinked = f.state(Pubkey::default(), true);
            assert_eq!(
                rent_of(&unlinked, &price),
                [
                    (FundingClass::NoSwapInventory, Some(7 + 676_512)),
                    (FundingClass::PreheldInventory, Some(7 + 676_512)),
                ]
            );
            let pda = darkbot_project0::protocol::liquidation_record_pda(&f.program, &f.account);
            let linked = f.state(pda, true);
            assert!(rent_of(&linked, &price).iter().all(|(_, c)| *c == Some(7)));
            assert!(rent_of(&unlinked, &unknown_price)
                .iter()
                .all(|(_, c)| c.is_none()));
            assert!(rent_of(&f.state(Pubkey::default(), false), &price)
                .iter()
                .all(|(_, c)| c.is_none()));
        }

        /// P2-07 through the production `plans()` path: with the prepared cache the plans are
        /// exactly the uncached ones; a repeated call reuses template and LUT plan; a route (here a
        /// ZERO_PREFUND swap) is per call and never cached; the generation, the account (record
        /// link), a bank (its oracle configuration) and the ProgramData each invalidate.
        #[test]
        fn p2_07_prepared_plans_equal_uncached_and_invalidate_deterministically() {
            use std::cell::RefCell;
            let f = PlansFixture::new();
            let program_data = Pubkey::new_unique();
            let cache = RefCell::new(PreparedCache::new(16, vec![program_data]));
            let price = |l: u64| Some(l);
            let shape = |plans: &[ReceivershipPlan]| {
                plans
                    .iter()
                    .map(|p| {
                        (
                            p.input.funding,
                            p.instructions.clone(),
                            p.lookup_tables.clone(),
                            p.input.rent_and_token_usd_micro,
                        )
                    })
                    .collect::<Vec<_>>()
            };
            let run = |src: &FixtureSource, routes: &TestRoutes, generation| {
                let cached =
                    f.plans_with(src, &price, routes, true, Some((&cache, generation)), None);
                assert!(!cached.is_empty());
                assert_eq!(shape(&cached), shape(&f.plans(src, &price, routes, true)));
                let s = cache.borrow().stats;
                (s.hits, s.misses)
            };
            let with_program = |mut src: FixtureSource, version: u8| {
                src.0
                    .insert(program_data, (Pubkey::default(), vec![version]));
                src
            };
            let src = with_program(f.state(Pubkey::default(), true), 1);
            let none = TestRoutes(None);
            assert_eq!(run(&src, &none, 1), (0, 2), "template + LUT plan prepared");
            assert_eq!(run(&src, &none, 1), (2, 2), "both reused");
            let swap_program = Pubkey::new_unique();
            let route = Route {
                instructions: vec![Instruction {
                    program_id: swap_program,
                    accounts: vec![],
                    data: vec![1],
                }],
                lookup_tables: vec![],
                swap_program,
                amount_in: 1_000,
                quoted_out: 600,
                minimum_executable_out: Some(550),
                enforced_slippage_bps: None,
            };
            assert_eq!(run(&src, &TestRoutes(Some(route)), 1), (4, 2));
            assert_eq!(run(&src, &none, 2), (4, 4), "new generation");
            let pda = darkbot_project0::protocol::liquidation_record_pda(&f.program, &f.account);
            let linked = with_program(f.state(pda, true), 1);
            assert_eq!(run(&linked, &none, 2), (5, 5), "account changed");
            let mut repriced = with_program(f.state(pda, true), 1);
            let data = &mut repriced.0.get_mut(&f.banks[0].0).unwrap().1;
            let mut bank: Bank = bytemuck::pod_read_unaligned(&data[8..]);
            bank.config.fixed_price.value = (2i128 << 48).to_le_bytes();
            data[8..].copy_from_slice(bytemuck::bytes_of(&bank));
            assert_eq!(run(&repriced, &none, 2), (6, 6), "bank changed");
            assert_eq!(run(&repriced, &none, 2), (8, 6));
            let upgraded = {
                let mut s = with_program(f.state(pda, true), 2);
                s.0.insert(f.banks[0].0, repriced.0[&f.banks[0].0].clone());
                s
            };
            assert_eq!(run(&upgraded, &none, 2), (8, 8), "program upgraded");
        }

        /// P2-07/P5-07 offline benchmark on the fixture account (2 banks, Fixed oracles, no
        /// integration refresh, no published LUT): template preparation, `plans()` (template +
        /// dynamic patch) and the transaction build, uncached against a warm cache. Run explicitly
        /// (`--release -- --ignored --nocapture`); it prints measurements and asserts no speed.
        #[test]
        #[ignore]
        fn p2_07_prepared_latency_benchmark() {
            use std::cell::RefCell;
            use std::hint::black_box;
            const N: usize = 5_000;
            let f = PlansFixture::new();
            let src = f.state(Pubkey::default(), true);
            let price = |l: u64| Some(l);
            let routes = TestRoutes(None);
            let cache = RefCell::new(PreparedCache::new(16, vec![]));
            let context = Context {
                generation: 0,
                program: f.program,
                group: f.group,
                liquidator: f.liquidator,
            };
            let measure = |label: &str, run: &dyn Fn()| {
                for _ in 0..200 {
                    run();
                }
                let mut ns: Vec<u128> = (0..N)
                    .map(|_| {
                        let t = Instant::now();
                        run();
                        t.elapsed().as_nanos()
                    })
                    .collect();
                ns.sort_unstable();
                println!(
                    "P2_07_BENCH {label} n={N} p50_ns={} p95_ns={} p99_ns={}",
                    ns[N / 2],
                    ns[N * 95 / 100],
                    ns[N * 99 / 100]
                );
            };
            let build = |plans: Vec<ReceivershipPlan>| {
                let p = &plans[0];
                let tx = tx::build(
                    &TxPlan {
                        version: p.tx_version,
                        payer: f.liquidator,
                        instructions: p.instructions.clone(),
                        lookup_tables: p.lookup_tables.clone(),
                        resources: Resources {
                            compute_unit_limit: 400_000,
                            priority_micro_lamports_per_cu: 0,
                            loaded_accounts_data_size: None,
                        },
                    },
                    Hash::default(),
                    &TxLimits::default(),
                );
                black_box(tx.unwrap());
            };
            let uncached = || f.plans(&src, &price, &routes, false);
            let cached = || f.plans_with(&src, &price, &routes, false, Some((&cache, 0)), None);
            measure("prepare_uncached", &|| {
                black_box(prepare_skeleton(&src, &f.program, &f.group, &f.account).unwrap());
            });
            measure("prepare_cached_hit", &|| {
                let hit = cache
                    .borrow_mut()
                    .skeleton(&src, &f.account, context, 10, |rec| {
                        prepare_skeleton(rec, &f.program, &f.group, &f.account)
                    });
                black_box(hit.unwrap());
            });
            measure("plans_uncached", &|| {
                black_box(uncached());
            });
            measure("plans_cached", &|| {
                black_box(cached());
            });
            measure("plans_build_uncached", &|| build(uncached()));
            measure("plans_build_cached", &|| build(cached()));
            // B14: the published mainnet LUT plan shape (5 tables x 256 addresses), per copy.
            let mainnet: Vec<AddressLookupTableAccount> = (0..5)
                .map(|_| AddressLookupTableAccount {
                    key: Pubkey::new_unique(),
                    addresses: (0..256).map(|_| Pubkey::new_unique()).collect(),
                })
                .collect();
            let shared: std::sync::Arc<[AddressLookupTableAccount]> = mainnet.clone().into();
            measure("lut_plan_vec_clone_5x256", &|| {
                black_box(mainnet.clone());
            });
            measure("lut_plan_arc_clone_5x256", &|| {
                black_box(shared.clone());
            });
            let s = cache.borrow().stats;
            println!("P2_07_BENCH cache hits={} misses={}", s.hits, s.misses);
        }

        /// Z12: an error of one asset/liability pair (here: the liability bank's mint is not in
        /// state) no longer discards the valid plans of the account's other pairs; when every pair
        /// fails, the first pair error comes back unchanged (same classification as before).
        #[test]
        fn z12_one_invalid_pair_keeps_the_valid_pairs_plans() {
            struct TwoSizings(Vec<Sizing>);
            impl SizingProvider for TwoSizings {
                fn sizes(&self, _: &Pubkey, _: u64, _: u64) -> Result<Vec<Sizing>, PlanError> {
                    Ok(self.0.clone())
                }
            }
            let f = PlansFixture::new();
            let mut src = f.state(Pubkey::default(), true);
            // A third bank whose mint account is missing from state.
            let broken_bank = Pubkey::new_unique();
            let (_, liab_data) = src.0.get(&f.banks[1].0).unwrap().clone();
            let mut broken: Bank = *bytemuck::from_bytes(&liab_data[8..]);
            broken.mint = Pubkey::new_unique();
            src.0.insert(
                broken_bank,
                (
                    f.program,
                    [
                        darkbot_project0::protocol::DISC_BANK.as_slice(),
                        bytemuck::bytes_of(&broken),
                    ]
                    .concat(),
                ),
            );
            let valid = f.sizing.0.clone();
            let mut invalid = valid.clone();
            invalid.liab_bank = broken_bank;
            let plans_with = |sizings: Vec<Sizing>| {
                let sizing = TwoSizings(sizings);
                let inventory = |_: &Pubkey| 10_000u64;
                let ata = |mint: &Pubkey| {
                    Some(get_associated_token_address_with_program_id(
                        &f.liquidator,
                        mint,
                        &f.token_program,
                    ))
                };
                let lamports = |l: u64| Some(l);
                P0ReceivershipBuilder {
                    state: &src,
                    program: f.program,
                    group: f.group,
                    liquidator: f.liquidator,
                    global_fee_wallet: Ok(Pubkey::new_unique()),
                    sizing: &sizing,
                    routes: &TestRoutes(None),
                    inventory: &inventory,
                    ata: &ata,
                    lamports_to_usd_micro: &lamports,
                    rent_lamports_to_usd_micro: &lamports,
                    jito_tip_usd_micro: 0,
                    jito_tip_lamports: 0,
                    zero_prefund_qualified: false,
                    instructions: &f.table,
                    v1_plans: false,
                    prepared: None,
                    route_cap: None,
                    path: Path::Rpc,
                    resources: Resources {
                        compute_unit_limit: 400_000,
                        priority_micro_lamports_per_cu: 0,
                        loaded_accounts_data_size: None,
                    },
                    limits: TxLimits::default(),
                }
                .plans(
                    &f.account,
                    1_000,
                    10,
                    Instant::now() + std::time::Duration::from_secs(60),
                )
            };
            // Invalid pair first, valid pair second: the valid pair's plans survive.
            let plans = plans_with(vec![invalid.clone(), valid.clone()]).unwrap();
            assert_eq!(plans.len(), 2, "NoSwap + Preheld of the valid pair");
            assert!(plans
                .iter()
                .all(|p| p.market_identity.as_ref().unwrap().liability_bank
                    == valid.liab_bank.to_string()));
            // Only invalid pairs: the first pair error, unchanged.
            assert_eq!(
                plans_with(vec![invalid.clone()]).err(),
                Some(PlanError::new(PlanErrorKind::Other, "mint not in state"))
            );
        }

        /// Q5 through the production `plans()` path: every planning failure carries its typed
        /// kind from its source — FeeState, an empty sizing with its no-size code (unsupported
        /// integration or not), a typed sizing error (stale integration reserve), a missing
        /// prewarmed ATA — and its text is exactly the former message.
        #[test]
        fn q5_plans_carry_the_typed_cause_and_the_former_text() {
            use darkbot_project0::sizing::{NoSizeReason as R, SizingError, SizingErrorKind};
            struct Scripted(Result<Vec<Sizing>, PlanError>, NoSizeReason);
            impl SizingProvider for Scripted {
                fn sizes(&self, _: &Pubkey, _: u64, _: u64) -> Result<Vec<Sizing>, PlanError> {
                    self.0.clone()
                }
                fn no_size_reason(&self, _: &Pubkey) -> Result<NoSizeReason, String> {
                    Ok(self.1)
                }
            }
            let f = PlansFixture::new();
            let src = f.state(Pubkey::default(), true);
            let run = |sizing: Scripted, fee: Result<Pubkey, String>, with_atas: bool| {
                let inventory = |_: &Pubkey| 10_000u64;
                let ata = |mint: &Pubkey| {
                    with_atas.then(|| {
                        get_associated_token_address_with_program_id(
                            &f.liquidator,
                            mint,
                            &f.token_program,
                        )
                    })
                };
                let lamports = |l: u64| Some(l);
                P0ReceivershipBuilder {
                    state: &src,
                    program: f.program,
                    group: f.group,
                    liquidator: f.liquidator,
                    global_fee_wallet: fee,
                    sizing: &sizing,
                    routes: &TestRoutes(None),
                    inventory: &inventory,
                    ata: &ata,
                    lamports_to_usd_micro: &lamports,
                    rent_lamports_to_usd_micro: &lamports,
                    jito_tip_usd_micro: 0,
                    jito_tip_lamports: 0,
                    zero_prefund_qualified: false,
                    instructions: &f.table,
                    v1_plans: false,
                    prepared: None,
                    route_cap: None,
                    path: Path::Rpc,
                    resources: Resources {
                        compute_unit_limit: 400_000,
                        priority_micro_lamports_per_cu: 0,
                        loaded_accounts_data_size: None,
                    },
                    limits: TxLimits::default(),
                }
                .plans(
                    &f.account,
                    1_000,
                    10,
                    Instant::now() + std::time::Duration::from_secs(60),
                )
                .err()
                .expect("a planning failure")
            };
            let wallet = Ok(Pubkey::new_unique());
            // Empty sizing: `SIZING:<code>`, an unsupported integration typed as such.
            for (reason, code, kind) in [
                (
                    R::NotMarginfiAccount,
                    "NOT_MARGINFI_ACCOUNT",
                    PlanErrorKind::Sizing,
                ),
                (R::NoAsset, "NO_ASSET", PlanErrorKind::Sizing),
                (R::NoLiability, "NO_LIABILITY", PlanErrorKind::Sizing),
                (
                    R::UnsupportedIntegrationAsset,
                    "UNSUPPORTED_INTEGRATION_ASSET",
                    PlanErrorKind::UnsupportedIntegration,
                ),
                (
                    R::UnsupportedIntegrationLiability,
                    "UNSUPPORTED_INTEGRATION_LIABILITY",
                    PlanErrorKind::UnsupportedIntegration,
                ),
                (
                    R::NoSupportedAssetBank,
                    "NO_SUPPORTED_ASSET_BANK",
                    PlanErrorKind::Sizing,
                ),
                (
                    R::NoSupportedLiabilityBank,
                    "NO_SUPPORTED_LIABILITY_BANK",
                    PlanErrorKind::Sizing,
                ),
                (
                    R::NoValidProtocolPair,
                    "NO_VALID_PROTOCOL_PAIR",
                    PlanErrorKind::Sizing,
                ),
            ] {
                assert_eq!(
                    run(Scripted(Ok(vec![]), reason), wallet.clone(), true),
                    PlanError::new(kind, format!("SIZING:{code}"))
                );
            }
            // A typed sizing error keeps its kind; its text (with the reader's marker) is as before.
            let stale = SizingError {
                kind: SizingErrorKind::IntegrationStale,
                text: "INTEGRATION_STALE:equity price: AnchorError code 6504".into(),
            };
            let e = run(
                Scripted(Err(stale.clone().into()), R::NoValidProtocolPair),
                wallet.clone(),
                true,
            );
            assert_eq!(e.kind, PlanErrorKind::IntegrationStale);
            assert_eq!(e.text, stale.text);
            let census = SizingError {
                kind: SizingErrorKind::NoValidPair,
                text: "SIZING:NO_VALID_PROTOCOL_PAIR:asset_eq_nonpositive=1".into(),
            };
            let e = run(
                Scripted(Err(census.into()), R::NoValidProtocolPair),
                wallet.clone(),
                true,
            );
            assert_eq!(e.kind, PlanErrorKind::Sizing);
            // FeeState unreadable: checked first, typed.
            let e = run(
                Scripted(Ok(vec![f.sizing.0.clone()]), R::NoValidProtocolPair),
                Err("FeeState account missing".into()),
                true,
            );
            assert_eq!(
                e,
                PlanError::new(
                    PlanErrorKind::FeeStateUnavailable,
                    "FEE_STATE_UNAVAILABLE:FeeState account missing"
                )
            );
            // No prewarmed ATA for the only pair.
            let e = run(
                Scripted(Ok(vec![f.sizing.0.clone()]), R::NoValidProtocolPair),
                wallet.clone(),
                false,
            );
            assert_eq!(e.kind, PlanErrorKind::MissingAta);
            assert!(
                e.text.starts_with("ATA:PREWARM_REQUIRED: asset ATA="),
                "{e}"
            );
        }

        /// DEC-04: the plans of the fixture with a pinned Jupiter ExactIn swap of the whole
        /// withdrawal (1 000 in, 600 quoted, 50 bps: floor 597) and the fixture state.
        fn dec04_plans(f: &PlansFixture) -> (FixtureSource, Vec<ReceivershipPlan>) {
            use crate::execution::route::tests::swap_ix;
            use crate::execution::route::JUP_ROUTE;
            let (asset_mint, liab_mint) = (f.banks[0].1, f.banks[1].1);
            let route = Route {
                instructions: vec![swap_ix(
                    JUP_ROUTE,
                    &f.liquidator,
                    &asset_mint,
                    &liab_mint,
                    (1_000, 600, 50, 0),
                )],
                lookup_tables: vec![],
                swap_program: darkbot_project0::protocol::JUP_SWAP_PROGRAM_ID
                    .parse()
                    .unwrap(),
                amount_in: 1_000,
                quoted_out: 600,
                minimum_executable_out: Some(597),
                enforced_slippage_bps: Some(50),
            };
            let src = f.state(Pubkey::default(), true);
            let plans = f.plans(&src, &|l: u64| Some(l), &TestRoutes(Some(route)), true);
            (src, plans)
        }

        /// DEC-04 (§4, B): one parity test per component through the production `plans()` path
        /// — the signed withdraw and repay against the sizing, the swap's ExactIn input against
        /// the credited collateral, its floor and quote for the liability, the SOL debit; an
        /// inventory plan keeps the whole seizure and swaps nothing; an unmodelled mint, a
        /// flash-funded plan and missing facts are unqualified (A applies).
        #[test]
        fn dec04_b_flows_come_from_the_signed_plan() {
            use crate::execution::hold_b::{hold_b_of, AssetFlow, BHold};
            let f = PlansFixture::new();
            let (src, plans) = dec04_plans(&f);
            let (asset, liab) = (f.banks[0].1.to_string(), f.banks[1].1.to_string());
            let of = |class: FundingClass| {
                plans
                    .iter()
                    .find(|p| p.input.funding == class)
                    .unwrap()
                    .clone()
            };
            let b = |p: &ReceivershipPlan| hold_b_of(&src, &f.program, &f.liquidator, p, Some(7));
            let flow = |reserve, residual_max| AssetFlow {
                reserve,
                residual_max,
            };
            let zp = of(FundingClass::ZeroPrefundSameTx);
            let BHold::Exact(x) = b(&zp) else {
                panic!("{:?}", b(&zp));
            };
            // The whole withdrawal is swapped; the swap credits ≥ 597 for the 500 repaid, its
            // surplus has no bound in units — its value is the swapped input at the high price
            // (1 100) less the repay at the low price (500).
            assert_eq!(
                x.flows,
                [
                    ("SOL".to_string(), flow(7, Some(0))),
                    (asset.clone(), flow(0, Some(0))),
                    (liab.clone(), flow(0, None)),
                ]
                .into()
            );
            assert_eq!((x.inventory_usd_micro, x.kind.as_str()), (600, "PLAIN"));
            // A keeps the whole seizure in the inventory limit.
            assert_eq!(zp.inventory_hold_usd_micro, 1_100);
            let inv = of(FundingClass::NoSwapInventory);
            let BHold::Exact(y) = b(&inv) else {
                panic!("{:?}", b(&inv));
            };
            assert_eq!(y.flows[&liab], flow(500, Some(0)));
            assert_eq!(y.flows[&asset], flow(0, Some(1_000)));
            assert_eq!(y.inventory_usd_micro, 1_100);
            let unqualified = |p: &ReceivershipPlan| match b(p) {
                BHold::Unqualified(r) => r,
                exact => panic!("{exact:?}"),
            };
            let mut p = zp.clone();
            p.hold_facts.as_mut().unwrap().withdraw_bank_units = 999;
            assert_eq!(unqualified(&p), "B_PARITY_WITHDRAW");
            let mut p = zp.clone();
            p.hold_facts.as_mut().unwrap().repay_amount = 499;
            assert_eq!(unqualified(&p), "B_PARITY_REPAY");
            // The signed swap spends more than the withdrawal credits.
            let mut p = zp.clone();
            let swap = p
                .instructions
                .iter_mut()
                .find(|ix| {
                    ix.program_id
                        == darkbot_project0::protocol::JUP_SWAP_PROGRAM_ID
                            .parse()
                            .unwrap()
                })
                .unwrap();
            let at = swap.data.len() - 19;
            swap.data[at..at + 8].copy_from_slice(&1_001u64.to_le_bytes());
            assert_eq!(unqualified(&p), "B_SWAP_INPUT_EXCEEDS_CREDIT");
            let mut p = zp.clone();
            p.input.funding = FundingClass::FlashFunded;
            assert_eq!(unqualified(&p), "B_FLASH_FUNDED_BLOCKED");
            let mut p = zp.clone();
            p.hold_facts = None;
            assert_eq!(unqualified(&p), "B_HOLD_FACTS_MISSING");
            // A's unknown-price sentinel is no value; a ZERO_PREFUND plan has no prerequisites.
            let mut p = zp.clone();
            p.hold_facts.as_mut().unwrap().seized_hold_value = u64::MAX;
            assert_eq!(unqualified(&p), "B_SEIZURE_PRICE_UNKNOWN");
            let mut p = zp.clone();
            p.prerequisite_txs = vec![vec![]];
            assert_eq!(unqualified(&p), "B_ZERO_PREFUND_PREREQUISITES");
            assert_eq!(
                hold_b_of(&src, &f.program, &f.liquidator, &zp, None),
                BHold::Unqualified("B_SOL_DEBIT_UNKNOWN".into())
            );
            // An inventory plan that swaps is not an inventory plan.
            let mut p = inv.clone();
            p.instructions.extend(
                zp.instructions
                    .iter()
                    .filter(|ix| {
                        ix.program_id
                            == darkbot_project0::protocol::JUP_SWAP_PROGRAM_ID
                                .parse()
                                .unwrap()
                    })
                    .cloned(),
            );
            assert_eq!(unqualified(&p), "B_INVENTORY_PLAN_SWAPS");
            // A mint the state does not know has no transfer model.
            let mut thin = f.state(Pubkey::default(), true);
            thin.0.remove(&f.banks[1].1);
            assert_eq!(
                hold_b_of(&thin, &f.program, &f.liquidator, &zp, Some(7)),
                BHold::Unqualified("B_MINT_UNMODELLED:MINT_STATE_UNKNOWN".into())
            );
        }

        /// DEC-04 (§4, B): the integration conversion — the credited underlying of a Kamino,
        /// Drift or JupLend withdrawal, its kind — and a JIT buy: the funding it spends, the
        /// liability its floor leaves to the inventory.
        #[test]
        fn dec04_b_integration_conversion_and_jit_buy() {
            use crate::execution::hold_b::{b_hold, AssetFlow, BHold, BInputs};
            use crate::execution::route::tests::swap_ix;
            use crate::execution::route::JUP_ROUTE;
            use darkbot_project0::integration::{IntegrationExecutionAmount, IntegrationKind};
            let f = PlansFixture::new();
            let (_, plans) = dec04_plans(&f);
            let zp = plans
                .iter()
                .find(|p| p.input.funding == FundingClass::ZeroPrefundSameTx)
                .unwrap();
            let asset = f.banks[0].1.to_string();
            let with = |plan: &ReceivershipPlan, exec| {
                b_hold(BInputs {
                    plan,
                    program: &f.program,
                    taker: &f.liquidator,
                    exec,
                    unmodelled_mint: None,
                    sol_debit: Some(7),
                })
            };
            for (kind, code) in [
                (IntegrationKind::Kamino, "KAMINO"),
                (IntegrationKind::Drift, "DRIFT"),
                (IntegrationKind::JupLend, "JUPLEND"),
            ] {
                // The withdraw instruction carries 1 000 (the converted instruction amount);
                // the venue credits 1 200, the swap spends 1 000: 200 stay, 200/1 200 of the
                // seizure's 1 100.
                let exec = IntegrationExecutionAmount {
                    kind,
                    bank_units: 1_000,
                    instruction_amount: 1_000,
                    underlying_out: 1_200,
                };
                let BHold::Exact(x) = with(zp, Ok(Some(exec))) else {
                    panic!("{kind:?}");
                };
                // 200 stay (184 of the seizure's 1 100); the swapped 1 000 are worth ≤ 917,
                // less the 500 repaid at the low price: 417.
                assert_eq!(x.flows[&asset].residual_max, Some(200), "{kind:?}");
                assert_eq!((x.inventory_usd_micro, x.kind.as_str()), (601, code));
                let other = IntegrationExecutionAmount {
                    instruction_amount: 999,
                    ..exec
                };
                assert_eq!(
                    with(zp, Ok(Some(other))),
                    BHold::Unqualified("B_PARITY_WITHDRAW".into())
                );
            }
            assert_eq!(
                with(zp, Err("KAMINO_UNDERLYING_ZERO".into())),
                BHold::Unqualified("B_INTEGRATION:KAMINO_UNDERLYING_ZERO".into())
            );
            // JIT: the buy spends 300 funding for ≥ 199 (quote 200) of the 500 repaid.
            let funding = Pubkey::new_unique();
            let mut jit = plans
                .iter()
                .find(|p| p.input.funding == FundingClass::NoSwapInventory)
                .unwrap()
                .clone();
            jit.input.funding = FundingClass::JitShortfall;
            jit.hold_facts.as_mut().unwrap().funding_mint = Some(funding);
            jit.prerequisite_txs = vec![vec![swap_ix(
                JUP_ROUTE,
                &f.liquidator,
                &funding,
                &f.banks[1].1,
                (300, 200, 50, 0),
            )]];
            let BHold::Exact(x) = with(&jit, Ok(None)) else {
                panic!("jit");
            };
            let flow = |reserve, residual_max| AssetFlow {
                reserve,
                residual_max,
            };
            assert_eq!(x.flows[&funding.to_string()], flow(300, Some(0)));
            assert_eq!(x.flows[&f.banks[1].1.to_string()], flow(301, None));
            // The whole seizure stays; the buy spends its 500 capital for the 500 repaid.
            assert_eq!(x.inventory_usd_micro, 1_100);
        }

        /// N7 stage 1 through the production `plans()` path: the ZERO_PREFUND plan (in-TX ExactIn
        /// swap of the whole withdrawal) holds the full seized value like the inventory classes;
        /// its swap is not proven to leave nothing behind.
        #[test]
        fn n7_zero_prefund_plan_holds_the_full_seized_value() {
            let f = PlansFixture::new();
            let swap_program = Pubkey::new_unique();
            let route = Route {
                instructions: vec![Instruction {
                    program_id: swap_program,
                    accounts: vec![],
                    data: vec![1],
                }],
                lookup_tables: vec![],
                swap_program,
                amount_in: 1_000,
                quoted_out: 600,
                minimum_executable_out: Some(550),
                enforced_slippage_bps: None,
            };
            let price = |l: u64| Some(l);
            let plans = f.plans(
                &f.state(Pubkey::default(), true),
                &price,
                &TestRoutes(Some(route)),
                true,
            );
            let holds: Vec<_> = plans
                .iter()
                .map(|p| (p.input.funding, p.inventory_hold_usd_micro))
                .collect();
            assert_eq!(
                holds,
                [
                    (FundingClass::NoSwapInventory, 1_100),
                    (FundingClass::PreheldInventory, 1_100),
                    (FundingClass::ZeroPrefundSameTx, 1_100),
                ]
            );
        }

        /// FE-02: the ZERO_PREFUND plan carries the slippage its signed swap enforces (when
        /// larger than the configured one); a floor below the repay builds no ZERO_PREFUND plan
        /// and the cause stays a route diagnostic while the inventory plans remain.
        #[test]
        fn fe02_zero_prefund_slippage_and_floor_rejection_stay_visible() {
            let f = PlansFixture::new();
            let swap_program = Pubkey::new_unique();
            let route = |floor: u64, slippage: Option<u16>| Route {
                instructions: vec![Instruction {
                    program_id: swap_program,
                    accounts: vec![],
                    data: vec![1],
                }],
                lookup_tables: vec![],
                swap_program,
                amount_in: 1_000,
                quoted_out: 600,
                minimum_executable_out: Some(floor),
                enforced_slippage_bps: slippage,
            };
            let price = |l: u64| Some(l);
            let state = f.state(Pubkey::default(), true);
            let zp = |plans: &[ReceivershipPlan]| {
                plans
                    .iter()
                    .filter(|p| p.input.funding == FundingClass::ZeroPrefundSameTx)
                    .map(|p| p.slippage_bps)
                    .collect::<Vec<_>>()
            };
            let _ = take_route_diagnostics();
            let plans = f.plans(
                &state,
                &price,
                &TestRoutes(Some(route(550, Some(9_000)))),
                true,
            );
            assert_eq!(zp(&plans), [9_000], "the enforced slippage");
            let plans = f.plans(&state, &price, &TestRoutes(Some(route(550, None))), true);
            let configured = zp(&plans);
            assert_eq!(configured.len(), 1);
            assert!(configured[0] < 9_000);
            let _ = take_route_diagnostics();
            let plans = f.plans(&state, &price, &TestRoutes(Some(route(10, Some(50)))), true);
            assert!(zp(&plans).is_empty(), "floor below the repay: no plan");
            assert!(!plans.is_empty(), "the inventory plans remain");
            let (lines, _) = take_route_diagnostics();
            assert!(
                lines
                    .iter()
                    .any(|l| l.starts_with("route_rejected:")
                        && l.contains("ROUTE_FLOOR_BELOW_REPAY")),
                "{lines:?}"
            );
        }

        /// FE-02: a liability mint whose transfer the economics do not model (here a Token-2022
        /// mint that does not decode) refuses ZERO_PREFUND before any quote; the cause stays a
        /// route diagnostic and the inventory plans remain.
        #[test]
        fn fe02_unmodelled_mint_transfer_refuses_zero_prefund_before_quoting() {
            let f = PlansFixture::new();
            let price = |l: u64| Some(l);
            let mut state = f.state(Pubkey::default(), true);
            let t22: Pubkey = darkbot_project0::protocol::TOKEN_2022_PROGRAM_ID
                .parse()
                .unwrap();
            state.0.insert(f.banks[1].1, (t22, vec![0; 82]));
            let route = Route {
                instructions: vec![],
                lookup_tables: vec![],
                swap_program: Pubkey::new_unique(),
                amount_in: 1_000,
                quoted_out: 600,
                minimum_executable_out: Some(550),
                enforced_slippage_bps: None,
            };
            let _ = take_route_diagnostics();
            let plans = f.plans(&state, &price, &TestRoutes(Some(route)), true);
            assert!(plans
                .iter()
                .all(|p| p.input.funding != FundingClass::ZeroPrefundSameTx));
            let (lines, _) = take_route_diagnostics();
            assert!(
                lines.iter().any(|l| l.contains("MINT_STATE_UNDECODABLE")),
                "{lines:?}"
            );
        }

        /// FE-02: a ZERO_PREFUND swap realizes the repay it funds plus the surplus tokens at the
        /// low-biased price (rounded down); without a low price the plan's economics are UNKNOWN,
        /// never valued at the unbiased price.
        #[test]
        fn fe02_zero_prefund_surplus_is_valued_at_the_low_price() {
            let mut sz = sizing(1_000, 2_000);
            sz.repay_low_value = Some(1_900);
            // 500 surplus tokens at 1.9 USD-micro each, rounded down.
            assert_eq!(
                zero_prefund_realizable_usd_micro(&sz, 1_500),
                Ok(Some(2_950))
            );
            assert_eq!(
                zero_prefund_realizable_usd_micro(&sz, 1_000),
                Ok(Some(2_000))
            );
            assert!(zero_prefund_realizable_usd_micro(&sz, 999).is_err());
            sz.repay_low_value = None;
            assert_eq!(zero_prefund_realizable_usd_micro(&sz, 1_500), Ok(None));
            let route = Route {
                instructions: vec![],
                lookup_tables: vec![],
                swap_program: Pubkey::new_unique(),
                amount_in: 10,
                quoted_out: 1_600,
                minimum_executable_out: Some(1_500),
                enforced_slippage_bps: None,
            };
            let input = zero_prefund_input(&sz, &route, 0, 0, Path::Rpc).unwrap();
            assert_eq!(
                input.gross_seized_realizable_usd_micro, None,
                "UNKNOWN, not equity"
            );
        }

        /// P4A-JIT-02: one opportunity, four funding classes, deterministic executable net and
        /// capital through the production economics. ZERO_PREFUND realizes at its on-chain
        /// floor with no capital; NO_SWAP and PREHELD bind the full liability value, JIT only the
        /// funding its buy spends; without a proven exit the inventory classes stay UNKNOWN,
        /// never an invented profit. With an exit, the ranking is net, then capital efficiency —
        /// never the class.
        #[test]
        fn p4a_jit_02_classes_compare_deterministically() {
            use darkbot_project0::economics::{
                compare_evaluations, evaluate, Evaluation, FailedCostMatrix,
            };
            let mut sz = sizing(500, 1_000);
            sz.flat_sol_fee = 10;
            sz.liability_inventory_value = 400;
            sz.funding_bank = Some(Pubkey::new_unique());
            sz.funding_available_value = 2_000;
            let ctx = FundingContext {
                liability_mint: Pubkey::new_unique(),
                liability_needed: 500,
                liability_inventory: 200,
                liability_needed_value: 1_000,
                liability_inventory_value: 400,
                funding_bank: sz.funding_bank,
                funding_available_value: 2_000,
                zero_prefund_qualified: true,
                route_available: true,
            };
            let failed = FailedCostMatrix {
                jito_bundle_not_landed: Some(7),
                jito_bundle_failed: Some(3),
                ..Default::default()
            };
            let route = Route {
                instructions: vec![],
                lookup_tables: vec![],
                swap_program: Pubkey::new_unique(),
                amount_in: 1_000,
                quoted_out: 640,
                minimum_executable_out: Some(600),
                enforced_slippage_bps: None,
            };
            let known = |mut c: CandidateInput| {
                c.transaction_fee_usd_micro = Some(5);
                evaluate(&c, &failed)
            };
            let zp = zero_prefund_input(
                &sz,
                &route,
                funding::capital_required(FundingClass::ZeroPrefundSameTx, &ctx),
                20,
                Path::JitoBundle,
            )
            .unwrap();
            // 640 quoted, 600 enforced: realizable at the floor, 600 * 1000 / 500 = 1200.
            assert_eq!(zp.gross_seized_realizable_usd_micro, Some(1_200));
            // Costs 1000 repay + 10 SOL fee + 5 TX fee + 20 tip + 7 failed = 1042.
            let zp_eval = known(zp);
            assert_eq!(
                zp_eval,
                Evaluation::Known {
                    net: 158,
                    profit_per_capital_bps: i128::MAX
                }
            );
            let jit_capital = jit_capital_usd_micro(
                funding::capital_required(FundingClass::JitShortfall, &ctx),
                630,
                2_000,
                2_000,
            )
            .unwrap();
            let classes = [
                (FundingClass::ZeroPrefundSameTx, 0),
                (FundingClass::JitShortfall, jit_capital),
                (
                    FundingClass::NoSwapInventory,
                    funding::capital_required(FundingClass::NoSwapInventory, &ctx),
                ),
                (
                    FundingClass::PreheldInventory,
                    funding::capital_required(FundingClass::PreheldInventory, &ctx),
                ),
            ];
            assert_eq!(
                classes.map(|c| c.1),
                [0, 630, 1_000, 1_000],
                "capital per class"
            );
            for (class, capital) in &classes[1..] {
                let c = candidate_input(&sz, *class, None, *capital, 20, Path::JitoBundle);
                assert_eq!(known(c), Evaluation::Unknown, "{class:?}: no proven exit");
            }
            // A proven exit at the same value: the JIT buy spends 630 for a shortfall worth 600,
            // so its net carries the 30 spread (158 - 30), never the ZERO_PREFUND net.
            let shortfall_value = funding::capital_required(FundingClass::JitShortfall, &ctx);
            assert_eq!(shortfall_value, 600);
            let jit = known(jit_candidate_input(
                &sz,
                Some(1_200),
                jit_capital,
                shortfall_value,
                20,
                Path::JitoBundle,
            ));
            assert_eq!(
                jit,
                Evaluation::Known {
                    net: 128,
                    profit_per_capital_bps: 128 * 10_000 / 630
                }
            );
            assert_eq!(
                compare_evaluations(zp_eval, jit),
                std::cmp::Ordering::Greater
            );
            assert_eq!(known(zp), zp_eval, "deterministic");
        }

        include!("builder_n2_tests.rs");

        /// B15: the inventory hold of a plan is the seized collateral of its own sizing for
        /// every class that keeps it, and nothing for the in-TX swap/flash classes.
        #[test]
        fn b15_inventory_hold_is_the_sized_seized_collateral() {
            let mut sz = sizing(950, 950);
            sz.seized_equity_value = 1_234_567;
            sz.seized_hold_value = 1_300_000;
            for class in [
                FundingClass::NoSwapInventory,
                FundingClass::PreheldInventory,
                FundingClass::JitShortfall,
            ] {
                assert_eq!(inventory_hold_usd_micro(class, &sz), 1_300_000, "{class:?}");
            }
            // N7 stage 1: the in-TX classes are not proven to leave nothing behind: full hold.
            for class in [FundingClass::ZeroPrefundSameTx, FundingClass::FlashFunded] {
                assert_eq!(inventory_hold_usd_micro(class, &sz), 1_300_000, "{class:?}");
            }
            // The conservative value, never below equity; an unknown price fits no limit.
            assert_eq!(conservative_hold_usd_micro(1_000, Ok(1_200)), 1_200);
            assert_eq!(conservative_hold_usd_micro(1_000, Ok(999)), 1_000);
            assert_eq!(
                conservative_hold_usd_micro(1_000, Err("stale".into())),
                u64::MAX
            );
        }

        #[test]
        fn p1c_zero_prefund_feasibility_uses_floor_not_quote() {
            use ZeroPrefundFloor::*;
            // quote >= repay, floor < repay -> NOT executable
            assert_eq!(
                classify_zero_prefund_floor(&floor_route(1_000, Some(900)), 950),
                BelowRepay {
                    floor: 900,
                    repay: 950
                }
            );
            assert!(zero_prefund_rejection(&floor_route(1_000, Some(900)), 950).is_some());
            // floor == repay / floor > repay / quote == floor -> executable at the floor
            assert_eq!(
                classify_zero_prefund_floor(&floor_route(1_000, Some(950)), 950),
                Executable(950)
            );
            assert_eq!(
                classify_zero_prefund_floor(&floor_route(1_000, Some(990)), 950),
                Executable(990)
            );
            assert_eq!(
                classify_zero_prefund_floor(&floor_route(990, Some(990)), 950),
                Executable(990)
            );
            // missing floor -> Unknown (not rejected, not the quote)
            assert_eq!(
                classify_zero_prefund_floor(&floor_route(1_000, None), 1),
                Unknown
            );
            assert!(zero_prefund_rejection(&floor_route(1_000, None), 1).is_none());
            // floor > quote -> provider/schema invalid -> rejected
            assert_eq!(
                classify_zero_prefund_floor(&floor_route(1_000, Some(1_001)), 1),
                Invalid {
                    floor: 1_001,
                    quoted_out: 1_000
                }
            );
            assert!(zero_prefund_rejection(&floor_route(1_000, Some(1_001)), 1).is_some());
        }

        #[test]
        fn p1c_quote_profitable_floor_unprofitable_is_not_actionable() {
            use darkbot_project0::economics::{
                actionable, evaluate, rpc_message_failure_costs, Evaluation,
            };
            let sz = sizing(1_000, 1_000_000); // 1_000 native units repay = 1.00 USD
            let quote_value = realizable_value_usd_micro(&sz, 1_200).unwrap();
            let floor_value = realizable_value_usd_micro(&sz, 1_050).unwrap();
            assert_eq!((quote_value, floor_value), (1_200_000, 1_050_000));
            let input = |gross: u64| CandidateInput {
                gross_seized_realizable_usd_micro: Some(gross),
                repay_usd_micro: sz.repay_value,
                protocol_fees_usd_micro: sz.protocol_fees,
                flat_sol_fee_usd_micro: sz.flat_sol_fee,
                swap_fees_and_impact_usd_micro: 0,
                transaction_fee_usd_micro: Some(1_000),
                jito_tip_usd_micro: 0,
                rent_and_token_usd_micro: Some(0),
                required_capital_usd_micro: 0,
                path: Path::Rpc,
                funding: FundingClass::ZeroPrefundSameTx,
            };
            let failed = rpc_message_failure_costs(Some(1_000));
            let min_profit: i128 = 10_000;
            let at_quote = evaluate(&input(quote_value), &failed);
            let at_floor = evaluate(&input(floor_value), &failed);
            assert!(matches!(at_quote, Evaluation::Known { net, .. } if net == 48_000));
            assert!(matches!(at_floor, Evaluation::Known { net, .. } if net == -102_000));
            // Old (quote-based) semantics would have admitted it; floor semantics must not.
            assert!(actionable(at_quote, min_profit, true, true, true));
            assert!(!actionable(at_floor, min_profit, true, true, true));
            // Floor safely above threshold -> actionable.
            let rich = realizable_value_usd_micro(&sz, 1_200).unwrap();
            assert!(actionable(
                evaluate(&input(rich), &failed),
                min_profit,
                true,
                true,
                true
            ));
        }

        /// [P1-C] Production path: the builder's ZERO_PREFUND mapping (`zero_prefund_input`, used by
        /// `plans()`) feeds the pipeline's economic gate (`plan_economic_gate`, used by
        /// `Pipeline::evaluate`). A missing floor must end as `Evaluation::Unknown`: never actionable,
        /// in Shadow at most a technical probe, never the quote.
        #[test]
        fn missing_floor_production_path_is_unknown_and_never_actionable() {
            use crate::execution::pipeline::{plan_economic_gate, Mode};
            use darkbot_project0::economics::{rpc_message_failure_costs, Evaluation};
            let sz = sizing(1_000, 1_000_000);
            let failed = rpc_message_failure_costs(Some(1_000));
            // Very profitable quote, but no executable floor.
            let mut input =
                zero_prefund_input(&sz, &floor_route(5_000, None), 0, 0, Path::Rpc).unwrap();
            assert_eq!(input.gross_seized_realizable_usd_micro, None);
            assert_eq!(input.funding, FundingClass::ZeroPrefundSameTx);
            input.transaction_fee_usd_micro = Some(1_000); // as the pipeline sets it from the fee quote
            let (eval, actionable, probe) =
                plan_economic_gate(&input, &failed, 0, Mode::Live, true, true);
            assert_eq!(
                (eval, actionable, probe),
                (Evaluation::Unknown, false, false)
            );
            let (eval, actionable, probe) =
                plan_economic_gate(&input, &failed, 0, Mode::Shadow, true, true);
            assert_eq!(
                (eval, actionable, probe),
                (Evaluation::Unknown, false, true)
            );
            // Same route with a profitable floor -> Known and actionable (P1-C unchanged).
            let mut known =
                zero_prefund_input(&sz, &floor_route(5_000, Some(1_200)), 0, 0, Path::Rpc).unwrap();
            known.transaction_fee_usd_micro = Some(1_000);
            let (eval, actionable, _) =
                plan_economic_gate(&known, &failed, 10_000, Mode::Live, true, true);
            assert!(matches!(eval, Evaluation::Known { net, .. } if net == 48_000));
            assert!(actionable);
            // Floor below repay or invalid -> no plan at all.
            assert!(
                zero_prefund_input(&sz, &floor_route(5_000, Some(900)), 0, 0, Path::Rpc).is_err()
            );
            assert!(
                zero_prefund_input(&sz, &floor_route(1_000, Some(1_001)), 0, 0, Path::Rpc).is_err()
            );
        }

        /// FE-02/B4 through the production `plans()` path: every plan carries the SOL it debits
        /// besides fee and tip — the flat protocol fee and, only when it creates the liquidation
        /// record, the record rent in lamports (the intent's later `rent_lamports`); the plan's
        /// full SOL debit adds the exact message fee and the tip, and is UNKNOWN for an unknown
        /// fee, an unknown Rent sysvar or an overflow.
        #[test]
        fn fe02_plans_carry_their_exact_sol_debit() {
            let mut f = PlansFixture::new();
            f.sizing.0.flat_sol_fee_lamports = 5_000;
            let price = |l: u64| Some(l);
            let routes = TestRoutes(None);
            let unlinked = f.plans(&f.state(Pubkey::default(), true), &price, &routes, false);
            assert!(!unlinked.is_empty());
            for p in &unlinked {
                assert_eq!(p.sol_costs.flat_fee_lamports, 5_000);
                assert_eq!(p.sol_costs.record_rent_lamports, Some(4_510_080));
                assert_eq!(
                    p.sol_debit_lamports(Some(7_000)),
                    Some(7_000 + 5_000 + 4_510_080)
                );
                assert_eq!(p.sol_debit_lamports(None), None, "unknown fee quote");
            }
            let pda = darkbot_project0::protocol::liquidation_record_pda(&f.program, &f.account);
            for p in f.plans(&f.state(pda, true), &price, &routes, false) {
                assert_eq!(p.sol_costs.record_rent_lamports, Some(0), "record exists");
                assert_eq!(p.sol_debit_lamports(Some(7_000)), Some(12_000));
            }
            for mut p in f.plans(&f.state(Pubkey::default(), false), &price, &routes, false) {
                assert_eq!(
                    p.sol_costs.record_rent_lamports, None,
                    "Rent sysvar unknown"
                );
                assert_eq!(p.sol_debit_lamports(Some(7_000)), None);
                p.sol_costs.record_rent_lamports = Some(0);
                p.tip = u64::MAX;
                assert_eq!(p.sol_debit_lamports(Some(1)), None, "overflow");
            }
        }

        /// FE-02: a positive executable net needs every cost and executability proof — the
        /// executable floor (or a proven exit), the exact message fee, rent/token costs and the
        /// path's failure cost. Missing any one of them is UNKNOWN: never actionable in LIVE,
        /// never a net, at most a Shadow technical probe.
        #[test]
        fn fe02_positive_net_requires_every_cost_and_executability_proof() {
            use crate::execution::pipeline::{plan_economic_gate, Mode};
            use darkbot_project0::economics::{
                rpc_message_failure_costs, Evaluation, FailedCostMatrix,
            };
            let sz = sizing(1_000, 1_000_000);
            let mut complete =
                zero_prefund_input(&sz, &floor_route(5_000, Some(1_200)), 0, 0, Path::Rpc).unwrap();
            complete.transaction_fee_usd_micro = Some(1_000);
            let failed = rpc_message_failure_costs(Some(1_000));
            let (eval, actionable, _) =
                plan_economic_gate(&complete, &failed, 0, Mode::Live, true, true);
            assert!(matches!(eval, Evaluation::Known { net, .. } if net > 0) && actionable);
            type Gap = fn(&mut darkbot_project0::economics::CandidateInput);
            let gaps: [(&str, Gap); 3] = [
                ("no executable floor / exit", |i| {
                    i.gross_seized_realizable_usd_micro = None
                }),
                ("fee quote unknown", |i| i.transaction_fee_usd_micro = None),
                ("rent unknown", |i| i.rent_and_token_usd_micro = None),
            ];
            for (gap, remove) in gaps {
                let mut input = complete;
                remove(&mut input);
                let live = plan_economic_gate(&input, &failed, 0, Mode::Live, true, true);
                assert_eq!(live, (Evaluation::Unknown, false, false), "{gap}");
                let shadow = plan_economic_gate(&input, &failed, 0, Mode::Shadow, true, true);
                assert_eq!(shadow, (Evaluation::Unknown, false, true), "{gap}");
            }
            let unmeasured = FailedCostMatrix::default();
            assert_eq!(
                plan_economic_gate(&complete, &unmeasured, 0, Mode::Live, true, true),
                (Evaluation::Unknown, false, false),
                "failure cost unknown"
            );
            // Known but state or safety invalid: never actionable either.
            assert!(!plan_economic_gate(&complete, &failed, 0, Mode::Live, false, true).1);
            assert!(!plan_economic_gate(&complete, &failed, 0, Mode::Live, true, false).1);
        }

        fn route_for_budget(cap: u8, data_len: usize, accounts: usize) -> Route {
            Route {
                instructions: vec![Instruction {
                    program_id: Pubkey::new_unique(),
                    accounts: (0..accounts)
                        .map(|_| AccountMeta::new(Pubkey::new_unique(), false))
                        .collect(),
                    data: vec![7; data_len],
                }],
                amount_in: 100,
                quoted_out: cap as u64 * 100,
                minimum_executable_out: Some(cap as u64 * 100),
                enforced_slippage_bps: None,
                ..Route::default()
            }
        }

        fn compile_route(route: &Route, limits: TxLimits) -> Result<tx::BuiltTx, BuildError> {
            tx::build(
                &TxPlan {
                    version: BuildVersion::V0,
                    payer: Pubkey::new_unique(),
                    instructions: route.instructions.clone(),
                    lookup_tables: route.lookup_tables.clone().into(),
                    resources: Resources {
                        compute_unit_limit: 1_400_000,
                        priority_micro_lamports_per_cu: 0,
                        loaded_accounts_data_size: None,
                    },
                },
                Hash::default(),
                &limits,
            )
        }

        #[test]
        fn route_requotes_until_actual_serialized_transaction_fits() {
            let mut caps = Vec::new();
            let limits = TxLimits {
                max_bytes_v0: 500,
                ..TxLimits::default()
            };
            let (route, built) = quote_fitting_route(
                64,
                Instant::now() + std::time::Duration::from_secs(60),
                |cap| {
                    caps.push(cap);
                    Ok(Some(route_for_budget(cap, cap as usize * 20, 0)))
                },
                |route| compile_route(route, limits),
            )
            .unwrap()
            .unwrap();
            assert!(caps.len() > 1 && caps.len() <= 6);
            assert!(caps.windows(2).all(|v| v[1] < v[0]));
            assert!(built.serialized_len <= 500);
            // Return the new quote's economics, never the original larger route's output.
            assert_eq!(route.quoted_out, *caps.last().unwrap() as u64 * 100);
        }

        #[test]
        fn route_budget_counts_payer_and_resource_accounts_too() {
            let mut calls = 0;
            let limits = TxLimits {
                max_locked_accounts: 10,
                max_bytes_v0: 5_000,
                ..TxLimits::default()
            };
            let (_, built) = quote_fitting_route(
                64,
                Instant::now() + std::time::Duration::from_secs(60),
                |cap| {
                    calls += 1;
                    Ok(Some(route_for_budget(cap, 0, cap as usize)))
                },
                |route| compile_route(route, limits),
            )
            .unwrap()
            .unwrap();
            assert!(calls > 1 && calls <= 6);
            assert!(built.locked_accounts <= 10);
        }

        #[test]
        fn impossible_route_exhausts_bounded_budget_and_fails_closed() {
            let mut calls = 0;
            let result = quote_fitting_route(
                64,
                Instant::now() + std::time::Duration::from_secs(60),
                |cap| {
                    calls += 1;
                    Ok(Some(route_for_budget(cap, 2_000, 0)))
                },
                |route| compile_route(route, TxLimits::default()),
            );
            assert!(result.unwrap_err().contains("TX_BUDGET_EXHAUSTED"));
            assert!(calls > 1 && calls <= 6);
        }

        #[test]
        fn non_budget_errors_are_not_retried() {
            let mut calls = 0;
            let result = quote_fitting_route(
                64,
                Instant::now() + std::time::Duration::from_secs(60),
                |cap| {
                    calls += 1;
                    Ok(Some(route_for_budget(cap, 0, 0)))
                },
                |_| Err::<(), _>(BuildError::Compile("bad account layout".into())),
            );
            assert!(result.unwrap_err().contains("bad account layout"));
            assert_eq!(calls, 1);
        }

        /// P6-MT.14 #31 / MT4 (b): a hinted start cap is only where the search begins. A hint
        /// the real build rejects shrinks exactly like the uncached search and ends in the same
        /// route; a hint at the proven fit cap ends in that route after one request. The trace
        /// carries cap numbers only.
        #[test]
        fn mt4_hinted_cap_is_validated_by_the_real_build_and_equals_the_uncached_result() {
            let limits = TxLimits {
                max_bytes_v0: 500,
                ..TxLimits::default()
            };
            let search = |start: u8| {
                let mut caps = Vec::new();
                let (route, built, trace) = quote_fitting_route_traced(
                    start,
                    Instant::now() + std::time::Duration::from_secs(60),
                    |cap| {
                        caps.push(cap);
                        Ok(Some(route_for_budget(cap, cap as usize * 20, 0)))
                    },
                    |route| compile_route(route, limits),
                )
                .unwrap()
                .unwrap();
                (route.quoted_out, built.serialized_len, trace, caps)
            };
            let (out, len, trace, caps) = search(64);
            assert!(caps.len() > 1, "{caps:?}");
            assert_eq!(trace.fit_cap, *caps.last().unwrap());
            assert_eq!(trace.rejected_above, Some(caps[caps.len() - 2]));
            assert_eq!(trace.attempts as usize, caps.len());
            // A hint the build rejects (the smallest rejected cap): shrinks on, same end.
            let (out2, len2, trace2, caps2) = search(trace.rejected_above.unwrap());
            assert_eq!((out2, len2, trace2.fit_cap), (out, len, trace.fit_cap));
            assert_eq!(caps2, caps[caps.len() - 2..].to_vec());
            // A hint at the proven fit: one request, the same route, nothing rejected above.
            let (out3, len3, trace3, caps3) = search(trace.fit_cap);
            assert_eq!((out3, len3, trace3.fit_cap), (out, len, trace.fit_cap));
            assert_eq!(caps3, vec![trace.fit_cap]);
            assert_eq!(trace3.rejected_above, None);
        }

        /// A router whose ExactIn route grows with the cap (one account and 20 data bytes per
        /// cap unit, deterministic keys), counting every request; `fail` answers an error.
        struct CapMarket {
            calls: std::cell::Cell<u32>,
            fail: std::cell::Cell<bool>,
            swap_program: Pubkey,
        }
        impl RouteProvider for CapMarket {
            fn quote_exact_in(
                &self,
                from: &Pubkey,
                to: &Pubkey,
                amount_in: u64,
                atas: &[PrewarmedAta],
                deadline: Instant,
            ) -> Result<Option<Route>, String> {
                self.quote_exact_in_with_budget(from, to, amount_in, atas, 64, deadline)
            }
            fn quote_exact_in_with_budget(
                &self,
                _: &Pubkey,
                _: &Pubkey,
                amount_in: u64,
                _: &[PrewarmedAta],
                cap: u8,
                _: Instant,
            ) -> Result<Option<Route>, String> {
                self.calls.set(self.calls.get() + 1);
                if self.fail.get() {
                    return Err("provider down".into());
                }
                Ok(Some(Route {
                    instructions: vec![Instruction {
                        program_id: self.swap_program,
                        accounts: (1..=cap)
                            .map(|i| AccountMeta::new(Pubkey::new_from_array([i; 32]), false))
                            .collect(),
                        data: vec![1; usize::from(cap) * 20],
                    }],
                    lookup_tables: vec![],
                    swap_program: self.swap_program,
                    amount_in,
                    quoted_out: 600,
                    minimum_executable_out: Some(550),
                    enforced_slippage_bps: None,
                }))
            }
        }

        /// MT4 (a)/(c) through the production `plans()` path: the first fit of a key is a miss
        /// and the full search; the second starts at the proven cap and needs one request for
        /// the same route (same instructions, same slippage) as the uncached search. A provider
        /// error leaves the inventory plans and the remembered cap untouched; the cache hands
        /// out a cap number only.
        #[test]
        fn mt4_second_route_fit_of_the_same_key_needs_fewer_quotes_and_the_same_route() {
            use crate::execution::route_cap::Stats;
            use std::cell::{Cell, RefCell};
            let f = PlansFixture::new();
            let price = |l: u64| Some(l);
            let state = f.state(Pubkey::default(), true);
            let cache = RefCell::new(RouteCapCache::new(16, 64));
            let market = CapMarket {
                calls: Cell::new(0),
                fail: Cell::new(false),
                swap_program: Pubkey::new_unique(),
            };
            let zp = |plans: &[ReceivershipPlan]| {
                plans
                    .iter()
                    .filter(|p| p.input.funding == FundingClass::ZeroPrefundSameTx)
                    .map(|p| (p.instructions.clone(), p.slippage_bps, p.tx_version))
                    .collect::<Vec<_>>()
            };
            let uncached = f.plans_with(&state, &price, &market, true, None, None);
            let full = market.calls.take();
            assert!(!zp(&uncached).is_empty());
            assert!(full > 1, "the fixture's route needs shrinking: {full}");

            let _ = take_route_diagnostics();
            let miss = f.plans_with(&state, &price, &market, true, None, Some(&cache));
            assert_eq!(
                market.calls.take(),
                full,
                "first use of the key: the full search"
            );
            assert_eq!(zp(&miss), zp(&uncached));
            assert_eq!(cache.borrow().stats.misses, 1);
            assert_eq!(cache.borrow().len(), 1);
            assert!(!take_route_diagnostics()
                .0
                .iter()
                .any(|l| l.starts_with("route_cap_hint:")));

            let hit = f.plans_with(&state, &price, &market, true, None, Some(&cache));
            assert_eq!(market.calls.take(), 1, "the proven cap fits at once");
            assert_eq!(zp(&hit), zp(&uncached), "the same route as uncached");
            let (lines, _) = take_route_diagnostics();
            assert!(
                lines
                    .iter()
                    .any(|l| l.starts_with("route_cap_hint: start=")),
                "{lines:?}"
            );
            assert_eq!(
                cache.borrow().stats,
                Stats {
                    hits: 1,
                    misses: 1,
                    ..Stats::default()
                }
            );

            // (c) a provider error: no ZERO_PREFUND plan, inventory plans remain, the
            // remembered cap and the entry count are unchanged.
            let remembered = |cache: &RefCell<RouteCapCache>| {
                let c = cache.borrow();
                (c.len(), c.stats.hits, c.stats.misses)
            };
            let before = cache.borrow().stats;
            market.fail.set(true);
            let failed = f.plans_with(&state, &price, &market, true, None, Some(&cache));
            market.fail.set(false);
            assert_eq!(market.calls.take(), 1);
            assert!(zp(&failed).is_empty());
            assert!(!failed.is_empty(), "the inventory plans remain");
            assert_eq!(remembered(&cache), (1, before.hits + 1, before.misses));
            let again = f.plans_with(&state, &price, &market, true, None, Some(&cache));
            assert_eq!(market.calls.take(), 1, "the hint survived the error");
            assert_eq!(zp(&again), zp(&uncached));
        }

        #[test]
        fn repay_all_removes_closed_bank_only_from_end_layout() {
            let a = Pubkey::new_unique();
            let liability = Pubkey::new_unique();
            let b = Pubkey::new_unique();
            let banks = vec![a, liability, b];
            assert_eq!(
                end_banks_after_repay(banks.clone(), liability, false),
                banks
            );
            assert_eq!(end_banks_after_repay(banks, liability, true), vec![a, b]);
        }

        #[test]
        fn liquidation_record_link_controls_init() {
            let expected = Pubkey::new_unique();
            assert_eq!(
                liquidation_record_init_decision(Pubkey::default(), expected),
                Ok(true)
            );
            assert_eq!(
                liquidation_record_init_decision(expected, expected),
                Ok(false)
            );
            assert!(liquidation_record_init_decision(Pubkey::new_unique(), expected).is_err());
        }
    }
}
