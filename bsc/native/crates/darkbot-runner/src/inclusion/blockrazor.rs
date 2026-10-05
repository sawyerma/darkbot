//! BlockRazor block builder, submitted to directly: framing.
//!
//! `eth_sendBundle` over HTTPS with `[launch, BUY]` and an `Authorization`
//! header. The options written are exactly those of the plan's envelope
//! for this adapter: the plan's `maxBlockNumber`, `noMerge` only when the
//! plan requests it, and never `positionFirst`, which no plan derives for
//! this builder.
//!
//! This is the direct builder. The BlockRazor RPC is another capability
//! with other recipients; it has no framing here and cannot stand in.

use darkbot_core::inclusion::capability::{AdapterKind, AdapterTable};
use darkbot_core::inclusion::plan::SubmissionPlan;
use darkbot_core::intent::AdapterId;

use super::frame::{Endpoint, Frame, FrameError, bundle_frame};

/// Builds the request of `adapter`, which must be the BlockRazor builder,
/// for `plan`. `launch_raw` are the launch bytes the plan refers to;
/// `capacity` bounds the request. The endpoint must carry the
/// authorization.
pub fn frame(
    plan: &SubmissionPlan,
    table: &AdapterTable,
    adapter: AdapterId,
    endpoint: &Endpoint<'_>,
    launch_raw: &[u8],
    capacity: usize,
) -> Result<Frame, FrameError> {
    if endpoint.authorization.is_none() {
        return Err(FrameError::MissingAuthorization);
    }
    let kind = AdapterKind::BlockRazorBuilder;
    bundle_frame(plan, table, adapter, kind, endpoint, launch_raw, capacity)
}
