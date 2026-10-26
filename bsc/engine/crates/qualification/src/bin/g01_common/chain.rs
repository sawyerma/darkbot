//! Canonical header chains and their log blooms, shared by the G0.1 freeze
//! binaries (free endpoints refuse `eth_getLogs`).

use super::edge::{Endpoint, Header};
use alloy_primitives::{Address, B256, BloomInput};

/// Whether the bloom of `header` can hold a log of `contract` with `topic`.
pub fn bloom_has(header: &Header, contract: Address, topic: B256) -> bool {
    header
        .logs_bloom
        .contains_input(BloomInput::Raw(contract.as_slice()))
        && header
            .logs_bloom
            .contains_input(BloomInput::Raw(topic.as_slice()))
}

/// Headers `from..=to`, batched, contiguous and parent-linked.
pub fn linked_headers(
    rpc: &Endpoint,
    from: u64,
    to: u64,
    batch: u64,
) -> Result<Vec<Header>, String> {
    let mut out: Vec<Header> = Vec::new();
    let mut start = from;
    while start <= to {
        let end = to.min(start + batch - 1);
        for h in rpc.headers(start, end)? {
            if let Some(prev) = out.last()
                && h.parent_hash != prev.hash
            {
                return Err(format!(
                    "header {} does not link to {}",
                    h.number, prev.number
                ));
            }
            out.push(h);
        }
        start = end + 1;
    }
    Ok(out)
}
