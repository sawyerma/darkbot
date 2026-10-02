//! [FIX-2] `P0ReceivershipBuilder`: erzeugt pro machbarer FundingClass einen eigenen Plan.
//!
//! Zahlen (Beträge, realisierbarer Wert, Kosten) kommen aus einem `Sizing`-Provider (P4A-03);
//! ohne Sizing gibt es **keinen** Plan (fail closed), nie geschätzte Beträge.
//! Routen kommen aus einem `RouteProvider` (allowlisted Instructions); ohne Route keine
//! ZERO_PREFUND-/JIT-Pläne.
//!
//! Nur mit Feature `crate_health`. Status: UNTESTED / VERIFY.

#[cfg(feature = "crate_health")]
pub mod p0 {
    use crate::pipeline::{ReceivershipBuilder, ReceivershipPlan};
    use darkbot_project0::economics::{CandidateInput, Path};
    use darkbot_project0::funding::{self, FundingClass, FundingContext};
    use darkbot_project0::health::{AccountSource, HealthEngine};
    use darkbot_project0::health::crate_impl::CrateHealth;
    use darkbot_project0::planner::{RouteKind, Step};
    use darkbot_project0::receivership::{builders, ReceivershipParams};
    use darkbot_solana::tx::BuildVersion;
    use marginfi_type_crate::types::Bank;
    use solana_message::AddressLookupTableAccount /* [FIX-13] */;
    use solana_sdk::instruction::Instruction;
    use solana_sdk::pubkey::Pubkey;

    /// Protocol-Sizing für ein Asset×Liability-Paar. Markt-Realisierbarkeit gehört nicht hierher.
    #[derive(Clone, Debug)]
    pub struct Sizing {
        pub asset_bank: Pubkey,
        pub liab_bank: Pubkey,
        pub withdraw_amount: u64,
        pub repay_amount: u64,
        pub seized_equity_value: u64,    // USD-Micro, Protocol/EMA only
        pub repay_value: u64,            // USD-Micro
        pub liability_needed_value: u64, // USD-Micro
        pub liability_inventory_value: u64,
        pub funding_bank: Option<Pubkey>,
        pub funding_available_value: u64,
        pub protocol_fees: u64,          // USD-Micro
        pub flat_sol_fee: u64,           // USD-Micro after explicit SOL conversion
        pub rent_and_token: u64,         // USD-Micro
        pub premium_bps: u64,
        pub slippage_bps: u64,
    }

    pub trait SizingProvider {
        fn sizes(&self, account: &Pubkey, now_unix_s: u64) -> Result<Vec<Sizing>, String>;
    }

    /// Allowlisted Swap-Instructions (Jupiter/Titan) für collateral→liability.
    #[derive(Clone, Debug, Default)]
    pub struct Route {
        pub instructions: Vec<Instruction>,
        pub lookup_tables: Vec<AddressLookupTableAccount>,
        pub swap_program: Pubkey,
        pub fees_and_impact: u64,
        /// [FIX-60] Tatsächlich über einen ausführbaren Marktpfad realisierbarer Wert, USD-Micro.
        pub realizable_value: u64,
        /// Vom Router bestimmt: Input in Funding-Token-Basis (ExactOut) / Output (ExactIn).
        pub amount_in: u64,
        pub amount_out: u64,
    }

    pub trait RouteProvider {
        /// ExactIn: ZERO_PREFUND (seized asset → liability) mit `withdraw_amount` als Input.
        fn quote_exact_in(&self, from_bank: &Pubkey, to_bank: &Pubkey, amount_in: u64) -> Result<Option<Route>, String>;
        /// [FIX-29] ExactOut: JIT (Funding-Asset → Liability) mit dem Liability-Fehlbetrag als **Output**;
        /// der Router bestimmt den Funding-Input (andere Decimals!).
        fn quote_exact_out(&self, from_bank: &Pubkey, to_bank: &Pubkey, amount_out: u64) -> Result<Option<Route>, String>;
    }

    pub struct P0ReceivershipBuilder<'a, S: AccountSource, Z: SizingProvider, R: RouteProvider> {
        pub state: &'a S,
        pub program: Pubkey,
        pub group: Pubkey,
        pub liquidator: Pubkey,
        pub global_fee_wallet: Pubkey,
        pub sizing: &'a Z,
        pub routes: &'a R,
        /// Inventar je Liability-Bank (Token-Basis), aus P8-Reconciliation gepflegt.
        pub inventory: &'a dyn Fn(&Pubkey) -> u64,
        /// ATA des Liquidators je Mint.
        pub ata: &'a dyn Fn(&Pubkey) -> Pubkey,
        pub priority_fee: u64,
        pub jito_tip: u64,
        pub zero_prefund_qualified: bool,
        pub path: Path,
    }

    impl<'a, S: AccountSource, Z: SizingProvider, R: RouteProvider> P0ReceivershipBuilder<'a, S, Z, R> {
        /// [FIX-19] fail closed: Owner, Discriminator, Länge prüfen — kein Index-Panic bei fremdem Layout.
        fn bank(&self, k: &Pubkey) -> Result<(Pubkey, Pubkey, Pubkey), String> {
            let (owner, _, data) = self.state.account(k).ok_or("bank not in state")?;
            if owner != self.program { return Err(format!("UNKNOWN_LAYOUT: bank owner {}", owner)); }
            if darkbot_project0::protocol::classify(data) != darkbot_project0::protocol::AccountType::Bank { return Err("UNKNOWN_LAYOUT: not a Bank discriminator".into()); }
            let need = 8 + std::mem::size_of::<Bank>();
            if data.len() < need { return Err(format!("UNKNOWN_LAYOUT: bank len {} < {}", data.len(), need)); }
            let b: &Bank = bytemuck::try_from_bytes(&data[8..need]).map_err(|e| format!("{:?}", e))?;
            let (mint_owner, _, _) = self.state.account(&b.mint).ok_or("mint not in state")?;
            Ok((b.liquidity_vault, b.mint, mint_owner))
        }

        fn receivership(&self, account: &Pubkey, obs: &[Pubkey], sz: &Sizing, swaps: Vec<Instruction>, repay_from_inventory: bool) -> Result<(Vec<Instruction>, Vec<Step>), String> {
            let (asset_vault, asset_mint, asset_tp) = self.bank(&sz.asset_bank)?;
            let (liab_vault, liab_mint, liab_tp) = self.bank(&sz.liab_bank)?;
            let withdraw = builders::withdraw(&self.program, &self.group, account, &self.liquidator, &sz.asset_bank,
                &(self.ata)(&asset_mint), &asset_vault, &asset_tp, sz.withdraw_amount, None, obs);
            let repay = builders::repay(&self.program, &self.group, account, &self.liquidator, &sz.liab_bank,
                &(self.ata)(&liab_mint), &liab_vault, &liab_tp, sz.repay_amount, None);
            let swap_programs: Vec<Pubkey> = swaps.iter().map(|i| i.program_id).collect();
            let init_record = self.state.account(&darkbot_project0::protocol::liquidation_record_pda(&self.program, account)).is_none();
            let ixs = builders::build_receivership(ReceivershipParams {
                program: &self.program, group: &self.group, marginfi_account: account, liquidator: &self.liquidator,
                global_fee_wallet: &self.global_fee_wallet, observation: obs, init_record,
                pre_refreshes: vec![], withdraws: vec![withdraw], swaps, repays: vec![repay],
            });
            let mut steps = vec![Step::StartLiquidation, Step::Withdraw];
            steps.extend(swap_programs.into_iter().map(Step::Swap));
            steps.push(if repay_from_inventory { Step::RepayFromInventory } else { Step::Repay });
            steps.push(Step::EndLiquidation);
            Ok((ixs, steps))
        }

        fn input(&self, sz: &Sizing, funding: FundingClass, realizable_value: u64, swap_cost: u64, capital: u64) -> CandidateInput {
            CandidateInput {
                gross_seized_realizable: realizable_value,
                repay: sz.repay_value,
                protocol_fees: sz.protocol_fees,
                flat_sol_fee: sz.flat_sol_fee,
                swap_fees_and_impact: swap_cost,
                priority_fee: self.priority_fee,
                jito_tip: self.jito_tip,
                rent_and_token: sz.rent_and_token,
                required_capital: capital,
                path: self.path,
                funding,
            }
        }
    }

    impl<'a, S: AccountSource, Z: SizingProvider, R: RouteProvider> ReceivershipBuilder for P0ReceivershipBuilder<'a, S, Z, R> {
        fn plans(&self, account: &Pubkey, now_unix_s: u64) -> Result<Vec<ReceivershipPlan>, String> {
            let Some(sz) = self.sizing.size(account, now_unix_s)? else { return Ok(vec![]) };
            let health = CrateHealth { state: self.state, group: self.group };
            let obs = health.observation_accounts(account).map_err(|e| e.to_string())?;
            let (_, liab_mint, _) = self.bank(&sz.liab_bank)?;
            let (_, asset_mint, _) = self.bank(&sz.asset_bank)?;
            let inv = (self.inventory)(&sz.liab_bank);
            let route = self.routes.quote_exact_in(&sz.asset_bank, &sz.liab_bank, sz.withdraw_amount)?;
            let ctx = FundingContext {
                liability_mint: liab_mint,
                liability_needed: sz.repay_amount,
                liability_inventory: inv,
                liability_needed_value: sz.liability_needed_value,
                liability_inventory_value: sz.liability_inventory_value,
                funding_bank: sz.funding_bank,
                funding_available_value: sz.funding_available_value,
                zero_prefund_qualified: self.zero_prefund_qualified,
                route_available: route.is_some(),
            };
            let mut out = Vec::new();
            for class in funding::feasible_classes(&ctx) {
                let capital = funding::capital_required(class, &ctx);
                let plan = match class {
                    FundingClass::ZeroPrefundSameTx => {
                        // [FIX-20] dieselbe Route als v0- UND v1-Kandidat; passt v0 nicht in 1232 B, bleibt v1 erhalten.
                        let r = route.clone().ok_or("route")?;
                        let (ixs, steps) = self.receivership(account, &obs, &sz, r.instructions.clone(), false)?;
                        out.push(ReceivershipPlan { steps: steps.clone(), instructions: ixs.clone(), route: RouteKind::FreePublicAllowed, input: self.input(&sz, class, r.fees_and_impact, capital),
                            premium_bps: sz.premium_bps, exposure: capital, tip: self.jito_tip, slippage_bps: sz.slippage_bps, assets: vec![asset_mint, liab_mint],
                            tx_version: BuildVersion::V1, lookup_tables: vec![], prerequisite_txs: vec![] });
                        ReceivershipPlan { steps, instructions: ixs, route: RouteKind::FreePublicAllowed, input: self.input(&sz, class, r.fees_and_impact, capital),
                            premium_bps: sz.premium_bps, exposure: capital, tip: self.jito_tip, slippage_bps: sz.slippage_bps, assets: vec![asset_mint, liab_mint],
                            tx_version: BuildVersion::V0, lookup_tables: r.lookup_tables, prerequisite_txs: vec![] }
                    }
                    FundingClass::NoSwapInventory | FundingClass::PreheldInventory => {
                        let (ixs, steps) = self.receivership(account, &obs, &sz, vec![], true)?;
                        ReceivershipPlan { steps, instructions: ixs, route: RouteKind::NoSwap, input: self.input(&sz, class, 0, capital),
                            premium_bps: sz.premium_bps, exposure: capital, tip: self.jito_tip, slippage_bps: 0, assets: vec![asset_mint, liab_mint],
                            tx_version: BuildVersion::V0, lookup_tables: vec![], prerequisite_txs: vec![] }
                    }
                    FundingClass::JitShortfall => {
                        // [FIX-17] Buy-TX: Funding-Bank (Base-Inventar) → Liability, nur der Token-Fehlbetrag.
                        let Some(fb) = sz.funding_bank else { continue };
                        let shortfall = funding::shortfall_token_amount(&ctx);
                        let Some(buy) = self.routes.quote_exact_out(&fb, &sz.liab_bank, shortfall)? else { continue }; // [FIX-29] ExactOut
                        let (ixs, steps) = self.receivership(account, &obs, &sz, vec![], true)?;
                        ReceivershipPlan { steps, instructions: ixs, route: RouteKind::NoSwap, input: self.input(&sz, class, buy.fees_and_impact, capital),
                            premium_bps: sz.premium_bps, exposure: capital, tip: self.jito_tip, slippage_bps: sz.slippage_bps, assets: vec![asset_mint, liab_mint],
                            tx_version: BuildVersion::V0, lookup_tables: buy.lookup_tables, prerequisite_txs: vec![buy.instructions] }
                    }
                    FundingClass::FlashFunded => continue,
                };
                out.push(plan);
            }
            Ok(out)
        }
    }
}
