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
    use std::str::FromStr;

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
        fn sizes(&self, account: &Pubkey, now_unix_s: u64, slot: u64) -> Result<Vec<Sizing>, String>;
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
        pub amount_out: u64,
    }

    pub trait RouteProvider {
        /// ExactIn: ZERO_PREFUND (seized asset → liability) mit `withdraw_amount` als Input.
        fn quote_exact_in(&self, from_mint: &Pubkey, to_mint: &Pubkey, amount_in: u64) -> Result<Option<Route>, String>;
        /// ExactOut bleibt Provider-Capability. Ein Provider ohne belegten ExactOut-Pfad liefert None.
        fn quote_exact_out(&self, from_mint: &Pubkey, to_mint: &Pubkey, amount_out: u64) -> Result<Option<Route>, String>;
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
        pub ata: &'a dyn Fn(&Pubkey) -> Option<Pubkey>,
        /// Economics cost after explicit conversion, never raw lamports. [FIX-61]
        pub priority_fee_usd_micro: u64,
        pub jito_tip_usd_micro: u64,
        /// Execution amount for Jito tip transaction.
        pub jito_tip_lamports: u64,
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
            let asset_ata = (self.ata)(&asset_mint).ok_or_else(|| format!("missing/unsupported asset ATA for {}", asset_mint))?;
            let liab_ata = (self.ata)(&liab_mint).ok_or_else(|| format!("missing/unsupported liability ATA for {}", liab_mint))?;
            let withdraw = builders::withdraw(&self.program, &self.group, account, &self.liquidator, &sz.asset_bank,
                &asset_ata, &asset_vault, &asset_tp, sz.withdraw_amount, None, obs);
            let repay = builders::repay(&self.program, &self.group, account, &self.liquidator, &sz.liab_bank,
                &liab_ata, &liab_vault, &liab_tp, sz.repay_amount, None);
            let route_steps: Vec<Step> = swaps.iter().map(|i| {
                if i.program_id == Pubkey::from_str(darkbot_project0::protocol::JUP_SWAP_PROGRAM_ID).expect("static") {
                    Step::Swap(i.program_id)
                } else {
                    Step::Other(i.program_id)
                }
            }).collect();
            let init_record = self.state.account(&darkbot_project0::protocol::liquidation_record_pda(&self.program, account)).is_none();
            let ixs = builders::build_receivership(ReceivershipParams {
                program: &self.program, group: &self.group, marginfi_account: account, liquidator: &self.liquidator,
                global_fee_wallet: &self.global_fee_wallet, observation: obs, init_record,
                pre_refreshes: vec![], withdraws: vec![withdraw], swaps, repays: vec![repay],
            });
            let mut steps = vec![Step::StartLiquidation, Step::Withdraw];
            steps.extend(route_steps);
            steps.push(if repay_from_inventory { Step::RepayFromInventory } else { Step::Repay });
            steps.push(Step::EndLiquidation);
            Ok((ixs, steps))
        }

        fn realizable_value(&self, sz: &Sizing, liability_out: u64) -> Result<u64, String> {
            if sz.repay_amount == 0 { return Err("repay_amount=0".into()); }
            let v = (liability_out as u128)
                .checked_mul(sz.repay_value as u128)
                .ok_or("realizable value overflow")?
                / sz.repay_amount as u128;
            u64::try_from(v).map_err(|_| "realizable value > u64".into())
        }

        fn input(&self, sz: &Sizing, funding: FundingClass, realizable_value_usd_micro: u64, capital_usd_micro: u64) -> CandidateInput {
            CandidateInput {
                gross_seized_realizable_usd_micro: realizable_value_usd_micro,
                repay_usd_micro: sz.repay_value,
                protocol_fees_usd_micro: sz.protocol_fees,
                flat_sol_fee_usd_micro: sz.flat_sol_fee,
                swap_fees_and_impact_usd_micro: 0,
                priority_fee_usd_micro: self.priority_fee_usd_micro,
                jito_tip_usd_micro: self.jito_tip_usd_micro,
                rent_and_token_usd_micro: sz.rent_and_token,
                required_capital_usd_micro: capital_usd_micro,
                path: self.path,
                funding,
            }
        }
    }

    impl<'a, S: AccountSource, Z: SizingProvider, R: RouteProvider> ReceivershipBuilder for P0ReceivershipBuilder<'a, S, Z, R> {
        fn plans(&self, account: &Pubkey, now_unix_s: u64, slot: u64) -> Result<Vec<ReceivershipPlan>, String> {
            let sizings = self.sizing.sizes(account, now_unix_s, slot)?;
            if sizings.is_empty() {
                return Ok(vec![]);
            }
            let health = CrateHealth { state: self.state, group: self.group };
            let obs = health.observation_accounts(account).map_err(|e| e.to_string())?;
            let mut out = Vec::new();

            for sz in sizings {
                let (_, liab_mint, _) = self.bank(&sz.liab_bank)?;
                let (_, asset_mint, _) = self.bank(&sz.asset_bank)?;
                let inv = (self.inventory)(&sz.liab_bank);
                // [FIX-60] Ohne executable Marktquote kein Realizable-Value; Oracle-Wert wird nicht eingesetzt.
                let market = self.routes.quote_exact_in(&asset_mint, &liab_mint, sz.withdraw_amount)?;
                let ctx = FundingContext {
                    liability_mint: liab_mint,
                    liability_needed: sz.repay_amount,
                    liability_inventory: inv,
                    liability_needed_value: sz.liability_needed_value,
                    liability_inventory_value: sz.liability_inventory_value,
                    funding_bank: sz.funding_bank,
                    funding_available_value: sz.funding_available_value,
                    zero_prefund_qualified: self.zero_prefund_qualified,
                    route_available: market.is_some(),
                };

                for class in funding::feasible_classes(&ctx) {
                    let capital = funding::capital_required(class, &ctx);
                    let Some(mark) = market.clone() else { continue };
                    let plan = match class {
                        FundingClass::ZeroPrefundSameTx => {
                            if mark.amount_out < sz.repay_amount { continue; }
                            let (ixs, steps) = self.receivership(account, &obs, &sz, mark.instructions.clone(), false)?;
                            out.push(ReceivershipPlan {
                                steps: steps.clone(),
                                instructions: ixs.clone(),
                                route: RouteKind::FreePublicAllowed,
                                input: self.input(&sz, class, self.realizable_value(&sz, mark.amount_out)?, capital),
                                premium_bps: sz.premium_bps,
                                exposure: capital,
                                tip: self.jito_tip_lamports,
                                slippage_bps: sz.slippage_bps,
                                assets: vec![asset_mint, liab_mint],
                                tx_version: BuildVersion::V1,
                                lookup_tables: vec![],
                                prerequisite_txs: vec![],
                            });
                            ReceivershipPlan {
                                steps,
                                instructions: ixs,
                                route: RouteKind::FreePublicAllowed,
                                input: self.input(&sz, class, self.realizable_value(&sz, mark.amount_out)?, capital),
                                premium_bps: sz.premium_bps,
                                exposure: capital,
                                tip: self.jito_tip_lamports,
                                slippage_bps: sz.slippage_bps,
                                assets: vec![asset_mint, liab_mint],
                                tx_version: BuildVersion::V0,
                                lookup_tables: mark.lookup_tables,
                                prerequisite_txs: vec![],
                            }
                        }
                        FundingClass::NoSwapInventory | FundingClass::PreheldInventory => {
                            let (ixs, steps) = self.receivership(account, &obs, &sz, vec![], true)?;
                            ReceivershipPlan {
                                steps,
                                instructions: ixs,
                                route: RouteKind::NoSwap,
                                input: self.input(&sz, class, self.realizable_value(&sz, mark.amount_out)?, capital),
                                premium_bps: sz.premium_bps,
                                exposure: capital,
                                tip: self.jito_tip_lamports,
                                slippage_bps: 0,
                                assets: vec![asset_mint, liab_mint],
                                tx_version: BuildVersion::V0,
                                lookup_tables: vec![],
                                prerequisite_txs: vec![],
                            }
                        }
                        FundingClass::JitShortfall => {
                            let Some(fb) = sz.funding_bank else { continue };
                            let (_, funding_mint, _) = self.bank(&fb)?;
                            let shortfall = funding::shortfall_token_amount(&ctx);
                            let Some(buy) = self.routes.quote_exact_out(&funding_mint, &liab_mint, shortfall)? else { continue };
                            let (ixs, steps) = self.receivership(account, &obs, &sz, vec![], true)?;
                            ReceivershipPlan {
                                steps,
                                instructions: ixs,
                                route: RouteKind::NoSwap,
                                input: self.input(&sz, class, self.realizable_value(&sz, mark.amount_out)?, capital),
                                premium_bps: sz.premium_bps,
                                exposure: capital,
                                tip: self.jito_tip_lamports,
                                slippage_bps: sz.slippage_bps,
                                assets: vec![asset_mint, liab_mint],
                                tx_version: BuildVersion::V0,
                                lookup_tables: buy.lookup_tables,
                                prerequisite_txs: vec![buy.instructions],
                            }
                        }
                        FundingClass::FlashFunded => continue,
                    };
                    out.push(plan);
                }
            }
            Ok(out)
        }
    }
}
