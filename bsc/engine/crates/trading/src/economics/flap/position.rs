//! Entry and exit cost semantics of one native curve-20 Flap position
//! (Economics gate §19: entry and exit paths with complete cost semantics).
//!
//! Pure composition of the exact quotes and the network model: a BUY
//! spends its input plus its network fee and yields the quoted tokens; a
//! SELL of exactly those tokens returns the quoted native amount less the
//! approval (unless already given) and SELL network fees. Never a guessed
//! percentage: whatever a quote refuses (holding cap, DEX threshold,
//! unwitnessed branch) or a path class without gas evidence fails closed.
//! The network fee is the class's largest observed gas used (an assumption) at the
//! given gas price (`network::GasEvidence`), an assumption labeled as such,
//! not a measurement of the trade.

use alloy_primitives::U256;

use super::QuoteError;
use super::native_curve20::{
    NativeBuyInputs, NativeSellInputs, quote_native_buy, quote_native_sell,
};
use crate::economics::arith::{ArithError, add};
use crate::economics::network::{Action, CostError, GasEvidence, PathClass, Venue};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositionError {
    Quote(QuoteError),
    Cost(CostError),
    Arith(ArithError),
    /// The SELL is not of the entry's tokens.
    NotTheEntrysTokens,
}

impl From<QuoteError> for PositionError {
    fn from(e: QuoteError) -> Self {
        Self::Quote(e)
    }
}

impl From<CostError> for PositionError {
    fn from(e: CostError) -> Self {
        Self::Cost(e)
    }
}

impl From<ArithError> for PositionError {
    fn from(e: ArithError) -> Self {
        Self::Arith(e)
    }
}

const fn flap(action: Action) -> PathClass {
    PathClass {
        venue: Venue::Flap,
        action,
    }
}

/// What a BUY spends and yields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub tokens: U256,
    pub input_wei: U256,
    pub network_fee_wei: U256,
    /// Input plus network fee.
    pub spent_wei: U256,
}

/// The BUY of `buy` at `gas_price_wei`.
pub fn entry(
    buy: &NativeBuyInputs,
    gas: &GasEvidence,
    gas_price_wei: U256,
) -> Result<Entry, PositionError> {
    let quote = quote_native_buy(buy)?;
    let fee = gas.counterfactual_fee_wei(flap(Action::Buy), gas_price_wei)?;
    Ok(Entry {
        tokens: quote.output_amount,
        input_wei: buy.input_amount,
        network_fee_wei: fee,
        spent_wei: add(buy.input_amount, fee)?,
    })
}

/// What the SELL of an entry's tokens returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Exit {
    /// The quoted native amount received.
    pub proceeds_wei: U256,
    /// The approval's (unless already given) and the SELL's network fees.
    pub network_fee_wei: U256,
}

/// The SELL of `sell` (exactly the entry's tokens) at `gas_price_wei`;
/// `approved` when the Portal may already spend them.
pub fn exit(
    entry: &Entry,
    sell: &NativeSellInputs,
    approved: bool,
    gas: &GasEvidence,
    gas_price_wei: U256,
) -> Result<Exit, PositionError> {
    if sell.input_amount != entry.tokens {
        return Err(PositionError::NotTheEntrysTokens);
    }
    let quote = quote_native_sell(sell)?;
    let mut fee = gas.counterfactual_fee_wei(flap(Action::Sell), gas_price_wei)?;
    if !approved {
        fee = add(
            fee,
            gas.counterfactual_fee_wei(flap(Action::Approve), gas_price_wei)?,
        )?;
    }
    Ok(Exit {
        proceeds_wei: quote.output_amount,
        network_fee_wei: fee,
    })
}

/// The native result of a round trip, every cost included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Gain(U256),
    Loss(U256),
}

pub fn outcome(entry: &Entry, exit: &Exit) -> Result<Outcome, PositionError> {
    let cost = add(entry.spent_wei, exit.network_fee_wei)?;
    Ok(if exit.proceeds_wei >= cost {
        Outcome::Gain(exit.proceeds_wei - cost)
    } else {
        Outcome::Loss(cost - exit.proceeds_wei)
    })
}

#[cfg(test)]
mod tests {
    use super::super::native_curve20::{
        FRESH_LAUNCH_HOLDING_CAP, PricedToken, QUALIFIED_MAINNET_INPUT_WEI, TokenTerms,
        WITNESSED_MAINNET_TERMS, profile_terms,
    };
    use super::*;

    fn priced(buy: u16, sell: u16) -> PricedToken {
        PricedToken::new(&profile_terms(buy, sell)).unwrap()
    }

    /// The Flap BUY, approval and SELL classes of the receipts fixture.
    fn evidence() -> GasEvidence {
        let mut e = GasEvidence::default();
        for l in include_str!("../../../fixtures/bsc/receipts_20261026.txt")
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        {
            let f: Vec<&str> = l.split_whitespace().collect();
            let action = match (f[0], f[1], f[5]) {
                ("FLAP", "BUY", "1") => Action::Buy,
                ("FLAP", "SELL", "1") => Action::Sell,
                ("FLAP", "APPROVE", "1") => Action::Approve,
                _ => continue,
            };
            e.add(flap(action), f[7].parse().unwrap()).unwrap();
        }
        e
    }

    fn fresh_buy() -> NativeBuyInputs {
        NativeBuyInputs {
            token: priced(100, 100),
            reserve_before: U256::ZERO,
            circulating_before: U256::ZERO,
            input_amount: U256::from(QUALIFIED_MAINNET_INPUT_WEI),
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
            holding_cap: FRESH_LAUNCH_HOLDING_CAP,
        }
    }

    /// A BUY and the immediate SELL of its tokens: every component is the
    /// quote's or the evidence's, and the loss is exactly their sum's
    /// difference (fees and taxes both ways plus three network fees).
    #[test]
    fn a_round_trip_is_the_sum_of_its_quoted_and_network_parts() {
        let gas = evidence();
        let price = U256::from(100_000_000u64);
        let buy = fresh_buy();
        let quote = quote_native_buy(&buy).unwrap();
        let e = entry(&buy, &gas, price).unwrap();
        let buy_fee = gas
            .counterfactual_fee_wei(flap(Action::Buy), price)
            .unwrap();
        assert_eq!(e.tokens, quote.output_amount);
        assert_eq!(e.spent_wei, buy.input_amount + buy_fee);
        let sell = NativeSellInputs {
            token: priced(100, 100),
            reserve_before: quote.reserve_after,
            circulating_before: quote.supply_after,
            input_amount: e.tokens,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_sell_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
        };
        let x = exit(&e, &sell, false, &gas, price).unwrap();
        let sold = quote_native_sell(&sell).unwrap();
        let exit_fees = gas
            .counterfactual_fee_wei(flap(Action::Sell), price)
            .unwrap()
            + gas
                .counterfactual_fee_wei(flap(Action::Approve), price)
                .unwrap();
        assert_eq!(
            x,
            Exit {
                proceeds_wei: sold.output_amount,
                network_fee_wei: exit_fees,
            }
        );
        assert_eq!(
            outcome(&e, &x).unwrap(),
            Outcome::Loss(e.spent_wei + exit_fees - sold.output_amount)
        );
        // Already approved: one network fee less.
        let approved = exit(&e, &sell, true, &gas, price).unwrap();
        assert!(approved.network_fee_wei < x.network_fee_wei);
        // Only the entry's tokens are its exit.
        let mut other = sell;
        other.input_amount = e.tokens - U256::from(1u8);
        assert_eq!(
            exit(&e, &other, false, &gas, price),
            Err(PositionError::NotTheEntrysTokens)
        );
    }

    /// Phase G gate H1: a position whose token left the curve has no exit
    /// price (`QuoteError::Migrated`), never a curve quote.
    #[test]
    fn a_migrated_token_has_no_exit() {
        let gas = evidence();
        let price = U256::from(100_000_000u64);
        let e = entry(&fresh_buy(), &gas, price).unwrap();
        let threshold = PricedToken::new(&profile_terms(100, 100))
            .unwrap()
            .dex_supply_threshold();
        let sell = NativeSellInputs {
            token: priced(100, 100),
            reserve_before: U256::from(16u64) * U256::from(10u64).pow(U256::from(18u64)),
            circulating_before: threshold + U256::from(1_000u64),
            input_amount: e.tokens,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_sell_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
        };
        assert_eq!(
            exit(&e, &sell, false, &gas, price),
            Err(PositionError::Quote(QuoteError::Migrated))
        );
    }

    /// A refused quote or a class without evidence fails the path closed.
    #[test]
    fn a_refused_quote_or_missing_gas_evidence_fails_closed() {
        let gas = evidence();
        let price = U256::from(100_000_000u64);
        let mut buy = fresh_buy();
        buy.token = PricedToken::new(&TokenTerms {
            dex_supply_threshold: U256::from(1u8),
            ..profile_terms(100, 100)
        })
        .unwrap();
        assert_eq!(
            entry(&buy, &gas, price),
            Err(PositionError::Quote(QuoteError::ReachesDexThreshold))
        );
        assert_eq!(
            entry(&fresh_buy(), &GasEvidence::default(), price),
            Err(PositionError::Cost(CostError::NoEvidence(flap(
                Action::Buy
            ))))
        );
    }
}
