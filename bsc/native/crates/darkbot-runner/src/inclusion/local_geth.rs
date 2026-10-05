//! Local pinned geth node: the adapter boundary.
//!
//! What this adapter submits is the one signed BUY, as it is. Its frame is
//! therefore just the slot for those bytes.
//!
//! What it submits *through* does not exist yet. The outbound handoff of
//! the pinned node is built in phase C1; this module defines no JSON-RPC
//! call, no peer protocol and no second implementation of either. Until
//! then the frame can be written to an injected [`Transport`] for tests
//! and diagnostics, which qualifies nothing: the adapter's boundary is
//! recorded as deferred, its send time is unknown, and no response is read
//! for it.
//!
//! [`Transport`]: super::transport::Transport

use darkbot_core::inclusion::capability::{AdapterKind, AdapterTable};
use darkbot_core::inclusion::plan::SubmissionPlan;
use darkbot_core::intent::AdapterId;

use super::frame::{Frame, FrameError, raw_buy_frame};

/// Builds the frame of `adapter`, which must be the local node, for a plan
/// that submits the BUY alone. `capacity` bounds the frame.
pub fn frame(
    plan: &SubmissionPlan,
    table: &AdapterTable,
    adapter: AdapterId,
    capacity: usize,
) -> Result<Frame, FrameError> {
    raw_buy_frame(plan, table, adapter, AdapterKind::LocalGeth, capacity)
}
