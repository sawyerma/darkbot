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
    use crate::execution::pipeline::{ReceivershipBuilder, ReceivershipPlan};
    use darkbot_project0::economics::{CandidateInput, Path};
    use darkbot_project0::funding::{self, FundingClass, FundingContext};
    use darkbot_project0::health::crate_impl::CrateHealth;
    use darkbot_project0::health::{AccountSource, HealthEngine};
    use darkbot_project0::planner::{InstructionTable, RouteKind};
    use darkbot_project0::receivership::{builders, ReceivershipParams};
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

    /// B15: collateral a plan leaves in the wallet, valued like the seized amount itself
    /// (`Sizing::seized_equity_value`: protocol equity price of the exact `withdraw_amount`).
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
            | FundingClass::FlashFunded => sz.seized_equity_value,
        }
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
        /// Actual economic value of the SPL amount transferred for repayment.
        pub repay_value: u64, // USD-Micro
        pub liability_needed_value: u64, // USD-Micro
        pub liability_inventory_value: u64,
        pub funding_bank: Option<Pubkey>,
        pub funding_available_value: u64,
        pub protocol_fees: u64,  // USD-Micro
        pub flat_sol_fee: u64,   // USD-Micro after explicit SOL conversion
        pub rent_and_token: u64, // USD-Micro
        pub premium_bps: u64,
        pub slippage_bps: u64,
    }

    pub trait SizingProvider {
        fn sizes(
            &self,
            account: &Pubkey,
            now_unix_s: u64,
            slot: u64,
        ) -> Result<Vec<Sizing>, String>;
        fn no_size_reason(&self, _account: &Pubkey) -> Result<String, String> {
            Ok("NO_VALID_PROTOCOL_PAIR".into())
        }
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
    }

    impl<'a, S: AccountSource + ?Sized> ProtocolSizingProvider<'a, S> {
        pub fn no_size_reason(&self, account: &Pubkey) -> Result<String, String> {
            darkbot_project0::sizing::ProtocolSizer {
                state: self.state,
                program: self.program,
                group: self.group,
                headroom_bps: darkbot_project0::sizing::DEFAULT_HEADROOM_BPS,
            }
            .no_size_reason(account)
        }
    }

    impl<'a, S: AccountSource + ?Sized> SizingProvider for ProtocolSizingProvider<'a, S> {
        fn sizes(
            &self,
            account: &Pubkey,
            now_unix_s: u64,
            slot: u64,
        ) -> Result<Vec<Sizing>, String> {
            let sizer = darkbot_project0::sizing::ProtocolSizer {
                state: self.state,
                program: self.program,
                group: self.group,
                headroom_bps: darkbot_project0::sizing::DEFAULT_HEADROOM_BPS,
            };
            let raw = sizer.sizes(account, now_unix_s as i64, slot)?;
            let mut out = Vec::with_capacity(raw.len());
            for sz in raw {
                let inv_native = (self.inventory)(&sz.liab_bank).min(sz.repay_amount);
                let inv_value = sizer.native_amount_value_usd_micro(
                    &sz.liab_bank,
                    inv_native,
                    now_unix_s as i64,
                    slot,
                )?;
                let flat_sol_fee = sizer.native_amount_value_usd_micro(
                    &self.sol_bank,
                    sz.flat_sol_fee_lamports,
                    now_unix_s as i64,
                    slot,
                )?;
                out.push(Sizing {
                    asset_bank: sz.asset_bank,
                    liab_bank: sz.liab_bank,
                    withdraw_amount: sz.withdraw_amount,
                    repay_amount: sz.repay_amount,
                    repay_all: sz.repay_all,
                    seized_equity_value: sz.seized_equity_value_micro,
                    // [FIX-128] Economics/funding pay the actual SPL transfer. EndLiquidation
                    // premium/health parity is already encoded in premium_bps by ProtocolSizer.
                    repay_value: sz.repay_transfer_value_micro,
                    liability_needed_value: sz.repay_transfer_value_micro,
                    liability_inventory_value: inv_value,
                    // ExactOut/JIT is intentionally disabled until a documented provider path exists.
                    funding_bank: None,
                    funding_available_value: 0,
                    // EndLiquidation premium is already represented by seized-vs-repaid value.
                    protocol_fees: 0,
                    flat_sol_fee,
                    // [FIX-102] Receivership uses prewarmed ATAs created outside the liquidation
                    // transaction. Historical ATA rent is not charged to every opportunity.
                    rent_and_token: 0,
                    premium_bps: sz.premium_bps,
                    slippage_bps: self.slippage_bps,
                });
            }
            Ok(out)
        }

        fn no_size_reason(&self, account: &Pubkey) -> Result<String, String> {
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
        /// [P1-C] Minimum output the swap instruction enforces on-chain (Jupiter
        /// `otherAmountThreshold` for ExactIn). `None` = unknown -> fail closed.
        pub minimum_executable_out: Option<u64>,
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
            ZeroPrefundFloor::Executable(floor) => Some(realizable_value_usd_micro(sz, floor)?),
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

    /// Economic value of `liability_out` native units, normalised by the protocol-exact repay
    /// value/amount ratio. [P1-C] Callers pass the executable floor, never the quote.
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
        /// ExactOut bleibt Provider-Capability. Ein Provider ohne belegten ExactOut-Pfad liefert None.
        fn quote_exact_out(
            &self,
            from_mint: &Pubkey,
            to_mint: &Pubkey,
            amount_out: u64,
            deadline: Instant,
        ) -> Result<Option<Route>, String>;
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

    /// [FIX-123][R7/R6] Router caps are hints; only the complete compiled TX proves fit.
    /// Bound remote work to six requests. Every changed quote is revalidated and repriced.
    fn quote_fitting_route<T>(
        initial_cap: u8,
        deadline: Instant,
        mut quote: impl FnMut(u8) -> Result<Option<Route>, String>,
        mut compile: impl FnMut(&Route) -> Result<T, BuildError>,
    ) -> Result<Option<(Route, T)>, String> {
        let mut cap = initial_cap.clamp(1, 64);
        let mut rejected = Vec::new();
        darkbot_common::mt0::MT0.add(darkbot_common::mt0::Counter::RouteFitCalls, 1); // [MT0]
        for attempt in 0..6 {
            // N2: no re-quote after the candidate deadline.
            if Instant::now() >= deadline {
                return Err(format!("ROUTE_FIT_DEADLINE after {attempt} attempt(s)"));
            }
            darkbot_common::mt0::MT0.add(darkbot_common::mt0::Counter::RouteFitAttempts, 1); // [MT0]
            let Some(route) = quote(cap)? else {
                return Ok(None);
            };
            match compile(&route) {
                Ok(compiled) => return Ok(Some((route, compiled))),
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
    fn required_fee_wallet(fee: &Result<Pubkey, String>) -> Result<&Pubkey, String> {
        fee.as_ref()
            .map_err(|e| format!("{}:{e}", super::FEE_STATE_UNAVAILABLE))
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

        /// [FIX-100] MarginfiAccount itself is the authoritative source for whether its
        /// liquidation-record PDA exists. The protocol writes this field when the record is
        /// created and resets it to Pubkey::default() when the record is closed. This avoids
        /// both hot-path RPC and the invalid assumption that "not present in our RAM state"
        /// means "not present on-chain".
        fn should_init_liquidation_record(&self, account: &Pubkey) -> Result<bool, String> {
            let (owner, _, data) = self
                .state
                .account(account)
                .ok_or("marginfi account not in state")?;
            if owner != self.program {
                return Err(format!("UNKNOWN_LAYOUT: marginfi account owner {}", owner));
            }
            // B2: strict pinned layout (discriminator, exact length), never a prefix decode.
            let a = darkbot_project0::protocol::decode_marginfi_account(data)
                .map_err(|e| e.to_string())?;
            let expected =
                darkbot_project0::protocol::liquidation_record_pda(&self.program, account);
            liquidation_record_init_decision(a.liquidation_record, expected)
                .map_err(|e| format!("{} account={}", e, account))
        }

        /// [FIX-107] Exact bank-only layout used by pinned p0-cli for EndLiquidation.
        /// Active balance order is normalized descending by raw pubkey bytes, matching
        /// load_observation_bank_only_metas().
        fn active_bank_keys(&self, account: &Pubkey) -> Result<Vec<Pubkey>, String> {
            let (owner, _, data) = self
                .state
                .account(account)
                .ok_or("marginfi account not in state")?;
            if owner != self.program
                || darkbot_project0::protocol::classify(data)
                    != darkbot_project0::protocol::AccountType::MarginfiAccount
            {
                return Err(
                    "UNKNOWN_LAYOUT: invalid MarginfiAccount for observation layout".into(),
                );
            }
            // B2: strict pinned layout, never a prefix decode.
            let a = darkbot_project0::protocol::decode_marginfi_account(data)
                .map_err(|e| e.to_string())?;
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

        fn receivership(
            &self,
            account: &Pubkey,
            obs: &[Pubkey],
            sz: &Sizing,
            swaps: Vec<Instruction>,
        ) -> Result<Vec<Instruction>, String> {
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
            let active_banks_before = self.active_bank_keys(account)?;
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

            // [FIX-109][R6] Refresh every active integration balance before StartLiquidation.
            // The planner is deterministic from canonical RAM state and fails closed on any
            // missing/unknown integration layout.
            let pre_refreshes = darkbot_project0::integration::refresh_plan_for_account(
                self.state,
                &self.program,
                account,
            )?;
            let init_record = self.should_init_liquidation_record(account)?;
            let ixs = builders::build_receivership(ReceivershipParams {
                program: &self.program,
                group: &self.group,
                marginfi_account: account,
                liquidator: &self.liquidator,
                global_fee_wallet: required_fee_wallet(&self.global_fee_wallet)?,
                start_observation,
                end_banks,
                init_record,
                pre_refreshes,
                withdraws: vec![withdraw],
                swaps,
                repays: vec![repay],
            });
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

        fn plans(
            &self,
            account: &Pubkey,
            now_unix_s: u64,
            slot: u64,
            deadline: Instant,
        ) -> Result<Vec<ReceivershipPlan>, String> {
            required_fee_wallet(&self.global_fee_wallet)?;
            let sizings = self.sizing.sizes(account, now_unix_s, slot)?;
            if sizings.is_empty() {
                return Err(format!("SIZING:{}", self.sizing.no_size_reason(account)?));
            }
            let protocol_luts =
                crate::execution::lookup_tables::from_state(self.state, &self.group, slot);
            let health = CrateHealth {
                state: self.state,
                group: self.group,
            };
            let obs = health
                .observation_accounts(account)
                .map_err(|e| e.to_string())?;
            let mut out = Vec::new();

            let mut route_errors = Vec::new();
            let mut ata_errors = Vec::new();

            // Z12: an error of one asset/liability pair skips that pair only; account-wide
            // checks (fee wallet, sizing, observation, record link) stay fatal above and below.
            let mut pair_errors: Vec<String> = Vec::new();
            macro_rules! pair_try {
                ($pair:lifetime, $e:expr) => {
                    match $e {
                        Ok(v) => v,
                        Err(e) => {
                            pair_errors.push(e);
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

                for class in funding::feasible_classes(&base_ctx) {
                    let capital = funding::capital_required(class, &base_ctx);
                    let plan = match class {
                        // route_available=false above, therefore ZERO_PREFUND is handled
                        // separately below after its own ExactIn quote.
                        FundingClass::ZeroPrefundSameTx => continue,
                        FundingClass::NoSwapInventory | FundingClass::PreheldInventory => {
                            let ixs =
                                pair_try!('pairs, self.receivership(account, &obs, &sz, vec![]));
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
                                exposure: capital,
                                inventory_hold_usd_micro: inventory_hold_usd_micro(class, &sz),
                                tip: self.jito_tip_lamports,
                                slippage_bps: 0,
                                assets: vec![asset_mint, liab_mint],
                                tx_version: BuildVersion::V0,
                                lookup_tables: protocol_luts.clone(),
                                prerequisite_txs: vec![],
                            }
                        }
                        FundingClass::JitShortfall => {
                            let Some(fb) = sz.funding_bank else { continue };
                            let (_, funding_mint, _) = pair_try!('pairs, self.bank(&fb));
                            let shortfall = funding::shortfall_token_amount(&base_ctx);
                            let buy = match self.routes.quote_exact_out(
                                &funding_mint,
                                &liab_mint,
                                shortfall,
                                deadline,
                            ) {
                                Ok(Some(route)) => route,
                                Ok(None) => continue,
                                Err(e) => {
                                    route_errors.push(format!(
                                        "exact-out {}→{}: {}",
                                        funding_mint, liab_mint, e
                                    ));
                                    continue;
                                }
                            };
                            let ixs =
                                pair_try!('pairs, self.receivership(account, &obs, &sz, vec![]));
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
                                exposure: capital,
                                inventory_hold_usd_micro: inventory_hold_usd_micro(class, &sz),
                                tip: self.jito_tip_lamports,
                                slippage_bps: sz.slippage_bps,
                                assets: vec![asset_mint, liab_mint],
                                tx_version: BuildVersion::V0,
                                lookup_tables: crate::execution::lookup_tables::merge(
                                    &protocol_luts,
                                    &buy.lookup_tables,
                                ),
                                prerequisite_txs: vec![buy.instructions],
                            }
                        }
                        FundingClass::FlashFunded => continue,
                    };
                    out.push(plan);
                }

                // ZERO_PREFUND has a real collateral→liability swap in the receivership TX,
                // so this class alone requires ExactIn market evidence. A provider error or
                // missing route must not invalidate already-built inventory/JIT candidates.
                if self.zero_prefund_qualified {
                    let asset_bank = pair_try!('pairs, self.bank_full(&sz.asset_bank));
                    let route_input_amount =
                        pair_try!('pairs, darkbot_project0::integration::execution_amount_for_asset(
                            self.state,
                            &asset_bank,
                            sz.withdraw_amount,
                        ))
                        .map(|x| x.underlying_out)
                        .unwrap_or(sz.withdraw_amount);
                    let fitted = quote_fitting_route(
                        self.routes.max_accounts(),
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
                                .receivership(account, &obs, &sz, route.instructions.clone())
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
                        Ok(v) => v,
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
                                            exposure: capital,
                                            inventory_hold_usd_micro: inventory_hold_usd_micro(
                                                FundingClass::ZeroPrefundSameTx,
                                                &sz,
                                            ),
                                            tip: self.jito_tip_lamports,
                                            slippage_bps: sz.slippage_bps,
                                            assets: vec![asset_mint, liab_mint],
                                            tx_version: BuildVersion::V1,
                                            lookup_tables: vec![],
                                            prerequisite_txs: vec![],
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
                                        exposure: capital,
                                        inventory_hold_usd_micro: inventory_hold_usd_micro(
                                            FundingClass::ZeroPrefundSameTx,
                                            &sz,
                                        ),
                                        tip: self.jito_tip_lamports,
                                        slippage_bps: sz.slippage_bps,
                                        assets: vec![asset_mint, liab_mint],
                                        tx_version: BuildVersion::V0,
                                        lookup_tables: crate::execution::lookup_tables::merge(
                                            &protocol_luts,
                                            &mark.lookup_tables,
                                        ),
                                        prerequisite_txs: vec![],
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
            if out.is_empty() && !ata_errors.is_empty() {
                ata_errors.sort();
                ata_errors.dedup();
                let routing = if route_errors.is_empty() {
                    String::new()
                } else {
                    format!("; other pairs ROUTING: {}", route_errors.join(" | "))
                };
                return Err(format!(
                    "ATA:PREWARM_REQUIRED: {}{}",
                    ata_errors.join(" | "),
                    routing
                ));
            }
            if out.is_empty() && !route_errors.is_empty() {
                return Err(format!("ROUTING:{}", route_errors.join(" | ")));
            }
            if self.should_init_liquidation_record(account)? {
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
            assert_eq!(err, "FEE_STATE_UNAVAILABLE:FeeState account missing");
            let wallet = Pubkey::new_unique();
            assert_eq!(required_fee_wallet(&Ok(wallet)), Ok(&wallet));
        }

        fn floor_route(quoted_out: u64, floor: Option<u64>) -> Route {
            Route {
                amount_in: 1_000,
                quoted_out,
                minimum_executable_out: floor,
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
                repay_value,
                liability_needed_value: repay_value,
                liability_inventory_value: 0,
                funding_bank: None,
                funding_available_value: 0,
                protocol_fees: 0,
                flat_sol_fee: 150_000,
                rent_and_token: 0,
                premium_bps: 0,
                slippage_bps: 50,
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
        }
        struct FixtureSource(std::collections::HashMap<Pubkey, (Pubkey, Vec<u8>)>);
        impl AccountSource for FixtureSource {
            fn account(&self, k: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
                self.0.get(k).map(|(o, d)| (*o, 1, d.as_slice()))
            }
        }
        struct OneSizing(Sizing);
        impl SizingProvider for OneSizing {
            fn sizes(&self, _: &Pubkey, _: u64, _: u64) -> Result<Vec<Sizing>, String> {
                Ok(vec![self.0.clone()])
            }
        }
        /// The ExactIn route a router would return (or none).
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
            fn quote_exact_out(
                &self,
                _: &Pubkey,
                _: &Pubkey,
                _: u64,
                _: Instant,
            ) -> Result<Option<Route>, String> {
                Ok(None)
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
                        repay_value: 500,
                        liability_needed_value: 500,
                        liability_inventory_value: 10_000,
                        funding_bank: None,
                        funding_available_value: 0,
                        protocol_fees: 0,
                        flat_sol_fee: 0,
                        rent_and_token: 7,
                        premium_bps: 500,
                        slippage_bps: 0,
                    }),
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
                let inventory = |_: &Pubkey| 10_000u64;
                let ata = |mint: &Pubkey| {
                    Some(get_associated_token_address_with_program_id(
                        &self.liquidator,
                        mint,
                        &self.token_program,
                    ))
                };
                let lamports = |l: u64| Some(l);
                P0ReceivershipBuilder {
                    state: src,
                    program: self.program,
                    group: self.group,
                    liquidator: self.liquidator,
                    global_fee_wallet: Ok(Pubkey::new_unique()),
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

        /// Z12: an error of one asset/liability pair (here: the liability bank's mint is not in
        /// state) no longer discards the valid plans of the account's other pairs; when every pair
        /// fails, the first pair error comes back unchanged (same classification as before).
        #[test]
        fn z12_one_invalid_pair_keeps_the_valid_pairs_plans() {
            struct TwoSizings(Vec<Sizing>);
            impl SizingProvider for TwoSizings {
                fn sizes(&self, _: &Pubkey, _: u64, _: u64) -> Result<Vec<Sizing>, String> {
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
                plans_with(vec![invalid.clone()]).err().as_deref(),
                Some("mint not in state")
            );
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
                    (FundingClass::NoSwapInventory, 1_050),
                    (FundingClass::PreheldInventory, 1_050),
                    (FundingClass::ZeroPrefundSameTx, 1_050),
                ]
            );
        }

        include!("builder_n2_tests.rs");

        /// B15: the inventory hold of a plan is the seized collateral of its own sizing for
        /// every class that keeps it, and nothing for the in-TX swap/flash classes.
        #[test]
        fn b15_inventory_hold_is_the_sized_seized_collateral() {
            let mut sz = sizing(950, 950);
            sz.seized_equity_value = 1_234_567;
            for class in [
                FundingClass::NoSwapInventory,
                FundingClass::PreheldInventory,
                FundingClass::JitShortfall,
            ] {
                assert_eq!(inventory_hold_usd_micro(class, &sz), 1_234_567, "{class:?}");
            }
            // N7 stage 1: the in-TX classes are not proven to leave nothing behind: full hold.
            for class in [FundingClass::ZeroPrefundSameTx, FundingClass::FlashFunded] {
                assert_eq!(inventory_hold_usd_micro(class, &sz), 1_234_567, "{class:?}");
            }
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
                ..Route::default()
            }
        }

        fn compile_route(route: &Route, limits: TxLimits) -> Result<tx::BuiltTx, BuildError> {
            tx::build(
                &TxPlan {
                    version: BuildVersion::V0,
                    payer: Pubkey::new_unique(),
                    instructions: route.instructions.clone(),
                    lookup_tables: route.lookup_tables.clone(),
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
