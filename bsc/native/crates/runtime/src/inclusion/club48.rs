//! 48Club puissant builder: framing.
//!
//! `eth_sendBundle` over HTTPS with `[launch, BUY]`. The options written
//! are exactly those of the plan's envelope for this adapter; under the
//! birth-entry profile that is `noMerge` and `positionFirst`, with the
//! plan's `maxBlockNumber`. The reference documents no authorization
//! header for this endpoint, so one is refused.

use darkbot_core::inclusion::capability::{AdapterKind, AdapterTable};
use darkbot_core::inclusion::plan::SubmissionPlan;
use darkbot_core::intent::AdapterId;

use super::frame::{Endpoint, Frame, FrameError, bundle_frame};

/// Builds the request of `adapter`, which must be the 48Club builder, for
/// `plan`. `launch_raw` are the launch bytes the plan refers to; `capacity`
/// bounds the request.
pub fn frame(
    plan: &SubmissionPlan,
    table: &AdapterTable,
    adapter: AdapterId,
    endpoint: &Endpoint<'_>,
    launch_raw: &[u8],
    capacity: usize,
) -> Result<Frame, FrameError> {
    if endpoint.authorization.is_some() {
        return Err(FrameError::UnexpectedAuthorization);
    }
    let kind = AdapterKind::Club48Builder;
    bundle_frame(plan, table, adapter, kind, endpoint, launch_raw, capacity)
}
