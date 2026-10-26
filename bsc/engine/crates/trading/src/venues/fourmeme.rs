//! Four.meme, the production decoder (Phase E) of the frozen Four Classic
//! (TokenManager2) and OpenFour (Core) surfaces on BSC mainnet (G0.1 Four
//! freeze at block 129842711,
//! `bsc/native/evidence/g01/four_freeze_20261026T053659Z/`).
//!
//! What is decoded is what the freeze proves (selectors in the executing
//! dispatcher, the published ABIs re-hashed, canonical fixtures, the CREATE2
//! derivation reproduced on every located launch):
//!
//! - [`recognize`]: pending calls. Launch (`createToken(bytes,bytes)` of
//!   both families) and the BUY of each family that has a canonical fixture
//!   (Classic `buyTokenAMAP(address,uint256,uint256)`, OpenFour
//!   `buyByBudget`) are decoded; every other launch/BUY/SELL selector of the
//!   published ABIs is explicitly unsupported.
//! - [`events`]: canonical logs. The events with a witnessed live log are
//!   decoded (Classic `TokenCreate`, `TokenPurchase`; OpenFour
//!   `TokenCreated`, the BUY form of `TradeExecuted`, `PhaseTransition`);
//!   SELL, liquidity/graduation and migration events have no witnessed log
//!   and are explicitly unsupported.
//! - [`deployment`]: the frozen addresses, fingerprints and derivation
//!   inputs, and the pre-execution token address where the calldata selects
//!   proven inputs ([`recognize::PendingToken`]).
//!
//! The verified (source-matched) ABI of both implementations is UNKNOWN:
//! they are not on Sourcify and BscScan needs a key. Field names come from
//! the published ABIs (four-meme-community docs); the positions the freeze
//! checked against events are facts, the economic meaning of every amount
//! is UNKNOWN until Phase G. No mode (TaxToken, X, AntiSniper) is inferred.
//!
//! This is the single Four.meme decoder of capture (the canonical follower),
//! replay (decoded from the same bytes) and any later PAPER/LIVE path. It
//! performs no I/O, does not allocate and copies nothing; oversized or
//! malformed input fails closed. The EVM address derivation (EIP-1014,
//! EIP-1167) is the one of `flap::address`; it holds no protocol
//! assumption.

pub mod abi;
pub mod deployment;
pub mod events;
pub mod recognize;

#[cfg(test)]
mod decoder_tests;
