//! BSC network cost (Economics gate §9/§10): the chain layer's one model,
//! shared by every venue.
//!
//! - Realized cost of an included transaction: `gas_used ×
//!   effective_gas_price` of its receipt, charged for a success and for a
//!   revert alike (BSC has no other chain-level charge in the evidence).
//! - One nonce lifecycle (intent → attempts → at most one inclusion): the
//!   included attempt is charged once; an attempt that never landed
//!   (dropped, replaced, abandoned) consumed no chain gas and costs nothing;
//!   two inclusions of one nonce are impossible and refused, never summed.
//! - Counterfactual (replay) cost: never a "typical gas" constant. A frozen
//!   evidence set of real receipts per path class gives the largest observed
//!   gas used of that class (a documented adverse bound, labeled as an
//!   assumption), priced at the replay block's own gas-price evidence.
//!
//! Exact integer arithmetic; overflow is an error.

use alloy_primitives::U256;

use crate::chain_state::canonical::ExecutionStatus;

/// Why a cost cannot be given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CostError {
    Overflow,
    /// A lifecycle without any attempt.
    NoAttempt,
    /// The same attempt twice in one lifecycle.
    DuplicateAttempt([u8; 32]),
    /// Two included attempts of one nonce: impossible on one chain.
    DoubleInclusion([u8; 32], [u8; 32]),
    /// No evidence of this path class (fail closed, never zero).
    NoEvidence(PathClass),
}

/// The gas facts of one included transaction, from its receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IncludedCost {
    pub gas_used: u64,
    pub effective_gas_price_wei: U256,
    pub status: ExecutionStatus,
}

impl IncludedCost {
    /// `gas_used × effective_gas_price`.
    pub fn fee_wei(&self) -> Result<U256, CostError> {
        self.effective_gas_price_wei
            .checked_mul(U256::from(self.gas_used))
            .ok_or(CostError::Overflow)
    }
}

/// What became of one attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttemptOutcome {
    Included(IncludedCost),
    /// Dropped, replaced or never landed: no chain gas.
    NotIncluded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attempt {
    pub tx_hash: [u8; 32],
    pub outcome: AttemptOutcome,
}

/// The network cost of one nonce lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LifecycleCost {
    /// The included attempt and how it executed; `None` if none landed.
    pub included: Option<([u8; 32], ExecutionStatus)>,
    pub fee_wei: U256,
    pub attempts: u32,
    pub not_included: u32,
}

/// The network cost of the attempts of one nonce, each attempt once.
pub fn lifecycle_cost(attempts: &[Attempt]) -> Result<LifecycleCost, CostError> {
    if attempts.is_empty() {
        return Err(CostError::NoAttempt);
    }
    let mut out = LifecycleCost {
        included: None,
        fee_wei: U256::ZERO,
        attempts: 0,
        not_included: 0,
    };
    for (i, a) in attempts.iter().enumerate() {
        if attempts[..i].iter().any(|b| b.tx_hash == a.tx_hash) {
            return Err(CostError::DuplicateAttempt(a.tx_hash));
        }
        out.attempts = out.attempts.checked_add(1).ok_or(CostError::Overflow)?;
        match a.outcome {
            AttemptOutcome::NotIncluded => {
                out.not_included = out.not_included.checked_add(1).ok_or(CostError::Overflow)?;
            }
            AttemptOutcome::Included(cost) => {
                if let Some((first, _)) = out.included {
                    return Err(CostError::DoubleInclusion(first, a.tx_hash));
                }
                out.included = Some((a.tx_hash, cost.status));
                out.fee_wei = cost.fee_wei()?;
            }
        }
    }
    Ok(out)
}

/// The venue a path belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Venue {
    Flap,
    FourClassic,
    OpenFour,
}

/// What a transaction does on a venue, for its gas evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    Launch,
    Buy,
    Sell,
    /// `approve` of the venue as spender (the sell path's prerequisite).
    Approve,
    /// An included call to the venue that reverted.
    Reverted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathClass {
    pub venue: Venue,
    pub action: Action,
}

/// The receipts of one path class in a frozen evidence set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassEvidence {
    pub class: PathClass,
    pub receipts: u32,
    pub gas_used_min: u64,
    pub gas_used_max: u64,
}

/// A frozen set of real receipts by path class (Economics gate §9: the
/// counterfactual cost policy; its provenance is the snapshot's).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GasEvidence {
    classes: Vec<ClassEvidence>,
}

impl GasEvidence {
    /// Adds one included receipt of `class`.
    pub fn add(&mut self, class: PathClass, gas_used: u64) -> Result<(), CostError> {
        match self.classes.iter_mut().find(|c| c.class == class) {
            Some(c) => {
                c.receipts = c.receipts.checked_add(1).ok_or(CostError::Overflow)?;
                c.gas_used_min = c.gas_used_min.min(gas_used);
                c.gas_used_max = c.gas_used_max.max(gas_used);
            }
            None => {
                self.classes.push(ClassEvidence {
                    class,
                    receipts: 1,
                    gas_used_min: gas_used,
                    gas_used_max: gas_used,
                });
                self.classes.sort_by_key(|c| c.class);
            }
        }
        Ok(())
    }

    pub fn class(&self, class: PathClass) -> Option<&ClassEvidence> {
        self.classes.iter().find(|c| c.class == class)
    }

    pub fn classes(&self) -> &[ClassEvidence] {
        &self.classes
    }

    /// The adverse counterfactual fee of one transaction of `class` at
    /// `gas_price_wei` (the replay block's evidence): the largest observed
    /// gas used of that class. An assumption bound, not a measurement of
    /// the counterfactual transaction.
    pub fn counterfactual_fee_wei(
        &self,
        class: PathClass,
        gas_price_wei: U256,
    ) -> Result<U256, CostError> {
        let c = self.class(class).ok_or(CostError::NoEvidence(class))?;
        gas_price_wei
            .checked_mul(U256::from(c.gas_used_max))
            .ok_or(CostError::Overflow)
    }
}

#[cfg(test)]
mod tests;
