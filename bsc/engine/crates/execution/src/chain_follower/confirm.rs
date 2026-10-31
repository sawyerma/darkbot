//! BSC3-056: the second source's headers. One endpoint, headers only, the
//! follower's HTTP, JSON-RPC and backoff; the comparison with the followed
//! chain is the caller's.

use std::io::{Read, Write};
use std::sync::atomic::AtomicBool;

use super::follow::{Rpc, wait_unless_stopped};
use super::{ConfirmEvent, FollowerFault, FollowerLimits, RequestError, stopped};
use crate::wss::DisconnectReason;

/// Runs header steps until `stop`: after a completed step wait `poll`,
/// after a fault report it and wait the backoff (doubling, `backoff_max`
/// at once after a rate-limit signal).
pub(super) fn run<S, C>(
    rpc: &mut Rpc<S, C>,
    limits: &FollowerLimits,
    stop: &AtomicBool,
    sink: &mut dyn FnMut(ConfirmEvent),
) where
    S: Read + Write,
    C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
{
    let mut last: Option<u64> = None;
    let mut next_wait = limits.backoff_initial;
    while !stopped(stop) {
        let wait = match step(rpc, &mut last, limits, stop, sink) {
            Ok(()) => {
                next_wait = limits.backoff_initial;
                limits.poll
            }
            Err(FollowerFault::Request {
                error: RequestError::Transport(DisconnectReason::Stopped),
                ..
            }) => return,
            Err(fault) => {
                let wait = if matches!(
                    fault,
                    FollowerFault::Request {
                        error: RequestError::RateLimited(_),
                        ..
                    }
                ) {
                    limits.backoff_max
                } else {
                    next_wait
                };
                next_wait = wait.saturating_mul(2).min(limits.backoff_max);
                sink(ConfirmEvent::Fault(fault));
                wait
            }
        };
        if !wait_unless_stopped(wait, limits.read_tick, stop) {
            return;
        }
    }
}

/// The head, then every height after the last reported one up to it (at
/// most `max_catch_up`; further behind, from the head minus that).
fn step<S, C>(
    rpc: &mut Rpc<S, C>,
    last: &mut Option<u64>,
    limits: &FollowerLimits,
    stop: &AtomicBool,
    sink: &mut dyn FnMut(ConfirmEvent),
) -> Result<(), FollowerFault>
where
    S: Read + Write,
    C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
{
    let head = rpc.block_number(stop)?;
    let earliest = head.saturating_sub(limits.max_catch_up.saturating_sub(1));
    let from = match *last {
        Some(l) if head <= l => return Ok(()),
        Some(l) => l.saturating_add(1).max(earliest),
        None => head,
    };
    for number in from..=head {
        let h = rpc.header(number, stop)?;
        sink(ConfirmEvent::Header {
            number: h.number,
            hash: h.hash,
            parent_hash: h.parent_hash,
            block_ms: h.block_ms,
        });
        *last = Some(number);
    }
    Ok(())
}
