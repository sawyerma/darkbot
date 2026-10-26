//! Execution of one submission plan over warm transports.
//!
//! Two entry points, separated by type:
//!
//! - [`dispatch_diagnostic`] takes a plain `SubmissionPlan` and adapters
//!   over any [`Transport`]. It is for building, testing and qualifying
//!   adapters. Its report is marked [`EvidenceClass::Diagnostic`] and never
//!   reports a qualified send time.
//! - [`dispatch_release`] takes a `QualifiedReleasePlan`, adapters over
//!   [`QualifiedTransport`]s and the runner's current
//!   [`TransportQualifications`], and nothing else. No flag or option lets
//!   a plain plan or an unqualified transport through. Today neither a
//!   qualified plan (no frozen record is release-eligible) nor a qualified
//!   transport or qualification (no production constructor) can exist
//!   outside tests.
//!
//! # Order of one dispatch
//!
//! Before anything is consumed: the deadline belongs to the plan's
//! profile, the armed adapters are exactly the plan's, every frame was
//! built for this plan and adapter, holds no bytes and is no larger than
//! its transport takes, a release
//! transport's binding matches the plan, the capability table and the
//! qualification currently valid for its adapter, and the plan's own final
//! check passes. If the deadline has already passed, or
//! no adapter is ready, the dispatch is a skip: the economic admission is
//! not consumed and the intent stays prepared.
//!
//! Then the one economic admission, `IntentLane::begin_send`, and the
//! plan's binding to exactly that dispatch.
//!
//! Then, per adapter, in the order given:
//!
//! 1. the connection must be [`ConnectionState::Ready`], else skip;
//! 2. pre-permit check: the clock is read and the transport start deadline
//!    checked. A miss is a skip; no permit is consumed;
//! 3. `IntentLane::start_write` consumes the permit and releases the BUY;
//! 4. the BUY is encoded into the frame's slot;
//! 5. the transport prepares the complete request for the wire without
//!    any I/O (`Transport::prepare`): for TLS, it is encrypted into records
//!    here. A failure now means no byte is written, but the permit is
//!    consumed, so the submission is unknown;
//! 6. write check: the clock is read again and the deadline checked again.
//!    This read is `t_write_check`. A miss now means no byte is written,
//!    but the permit is consumed, so the submission is unknown;
//! 7. the call of the first transport write, whose first action is the
//!    first write to the wire, then the rest and the flush.
//!
//! Between the clock read of step 6 and the call of the first write there
//! is only the deadline check: no preparation, encryption or framing. No
//! lock, log, file, name resolution, connect, read or sleep anywhere from
//! step 2 to step 7; what a transport allocates in step 5 is its own.
//!
//! No adapter waits for another one's response. Responses are read
//! afterwards, without blocking, by [`poll_response`] and
//! [`poll_release_response`].
//!
//! # After the permit
//!
//! Once `start_write` succeeded, every failure is unknown, whether the
//! transport accepted bytes or none: the permit is consumed and the lane
//! has no way back. The lane stops, every permit not started is
//! suppressed, nothing is retried. "Certainly not disclosed" exists only
//! before the permit.
//!
//! # No permit outlives a dispatch
//!
//! If the admission was consumed and any selected adapter was not started
//! (not ready, or late at the pre-permit check), the dispatch stops the
//! lane once the fan-out is complete. That suppresses every permit that
//! was not started, so none can be used later. Adapters that were sent
//! stay as they are. The report keeps the original skips visible.
//!
//! # What is measured
//!
//! Times are read on the given clock: `t_select` once after the
//! pre-checks; per adapter `t_prepermit_check`, `t_write_check` (the read
//! of the final deadline check, taken once the transport prepared the
//! request and immediately before the first `Transport::write` is called,
//! whose first action is the first write to the wire; not a time inside
//! that call and not a send boundary), `t_boundary` after write and flush
//! returned, and `t_response`. The send time is reported only if the
//! adapter's record qualifies its boundary *and* the dispatch went over a
//! qualified transport bound to that boundary: see [`SendTime`]. These
//! adapters have no queue, so there is no queue-commit time.

use std::io;

use darkbot_bsc_trading::chain_state::SnapshotId;
use darkbot_bsc_trading::intent::{
    AdapterId, HaltReason, IntentLane, LaneError, MAX_ADAPTERS, SubmissionState, Ticket,
};
use darkbot_bsc_trading::submission_policy::capability::{
    AdapterKind, AdapterTable, HandoffBoundary,
};
use darkbot_bsc_trading::submission_policy::plan::{
    ChainHead, PlanError, QualifiedReleasePlan, SubmissionPlan, TransportDeadlineError,
    TransportStartDeadline,
};

use super::frame::Frame;
use super::response::{ResponseBuffer, ResponseOutcome};
use super::transport::{
    Clock, ConnectionState, QualifiedTransport, Transport, TransportBinding,
    TransportQualifications,
};

/// One adapter armed for a diagnostic dispatch: any transport.
pub struct Armed<'a> {
    /// The adapter index of the intent lane.
    pub adapter: AdapterId,
    /// The adapter's warm connection.
    pub transport: &'a mut dyn Transport,
    /// The frame built for this adapter and the plan.
    pub frame: &'a mut Frame,
}

/// One adapter armed for a release dispatch: a qualified transport, which
/// also names the adapter.
pub struct ReleaseArmed<'a> {
    /// The adapter's qualified, bound transport.
    pub transport: QualifiedTransport<'a>,
    /// The frame built for this adapter and the plan.
    pub frame: &'a mut Frame,
}

/// What the dispatch needs of an armed adapter, whichever entry point.
trait Slot {
    fn adapter(&self) -> AdapterId;
    fn state(&self) -> ConnectionState;
    fn max_request_len(&self) -> usize;
    fn frame(&self) -> &Frame;
    fn parts(&mut self) -> (&mut dyn Transport, &mut Frame);
    fn binding(&self) -> Option<TransportBinding>;
}

impl Slot for Armed<'_> {
    fn adapter(&self) -> AdapterId {
        self.adapter
    }

    fn state(&self) -> ConnectionState {
        self.transport.state()
    }

    fn max_request_len(&self) -> usize {
        self.transport.max_request_len()
    }

    fn frame(&self) -> &Frame {
        &*self.frame
    }

    fn parts(&mut self) -> (&mut dyn Transport, &mut Frame) {
        (&mut *self.transport, &mut *self.frame)
    }

    fn binding(&self) -> Option<TransportBinding> {
        None
    }
}

impl Slot for ReleaseArmed<'_> {
    fn adapter(&self) -> AdapterId {
        self.transport.binding().adapter()
    }

    fn state(&self) -> ConnectionState {
        self.transport.transport().state()
    }

    fn max_request_len(&self) -> usize {
        self.transport.transport().max_request_len()
    }

    fn frame(&self) -> &Frame {
        &*self.frame
    }

    fn parts(&mut self) -> (&mut dyn Transport, &mut Frame) {
        (self.transport.transport_mut(), &mut *self.frame)
    }

    fn binding(&self) -> Option<TransportBinding> {
        Some(self.transport.binding())
    }
}

/// What the plan's final check is made against.
#[derive(Clone, Copy)]
pub struct DispatchContext<'a> {
    /// The capability table the plan was formed against.
    pub table: &'a AdapterTable,
    /// The current snapshot identity.
    pub current: &'a SnapshotId,
    /// The current chain head.
    pub head: ChainHead,
}

/// What a dispatch may be evidence for. Set by the entry point that was
/// called, never by an argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceClass {
    /// From a plain plan over any transport: development and qualification
    /// runs.
    Diagnostic,
    /// From a qualified release plan over qualified transports.
    Release,
}

/// Why a submission is unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownCause {
    /// The BUY did not fit the frame's slot. Nothing was written.
    BuyEncoding,
    /// The transport could not prepare the request after the permit.
    /// Nothing was written to the wire.
    Prepare,
    /// The write check after the permit missed the deadline. Nothing was
    /// written.
    WriteDeadline(TransportDeadlineError),
    /// A write failed.
    Write,
    /// The flush failed after every byte was accepted.
    Flush,
}

/// What happened to one adapter of a dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendOutcome {
    /// Write and flush returned. Says nothing about the provider.
    HandedOff,
    /// Skipped before the permit: the connection was not ready.
    SkippedConnection(ConnectionState),
    /// Skipped before the permit: the pre-permit check missed the deadline.
    SkippedDeadline(TransportDeadlineError),
    /// The lane refused the permit. Nothing was written.
    PermitRefused(LaneError),
    /// Not started because the dispatch had stopped. Nothing was written.
    Suppressed,
    /// The permit was consumed and the handoff did not complete. The bytes
    /// may have left. `bytes_accepted` is what the transport took; it
    /// proves nothing either way.
    Unknown {
        /// Bytes the transport accepted before the failure.
        bytes_accepted: usize,
        /// What went wrong.
        cause: UnknownCause,
    },
}

/// The send time of one adapter, as far as it is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendTime {
    /// The adapter did not reach its boundary.
    NotReached,
    /// The boundary was reached, the adapter's record qualifies it as the
    /// send time, and the dispatch went over a qualified transport bound
    /// to it: this is the send time, in nanoseconds on the clock.
    Qualified(u64),
    /// The boundary was reached and measured, but the adapter's record
    /// does not qualify it as the send time. Unknown.
    BoundaryUnqualified,
    /// The record qualifies the boundary, but the transport the dispatch
    /// used is not a qualified one. Unknown.
    TransportUnqualified,
    /// The adapter's boundary is the native outbound handoff of the pinned
    /// node, which phase C1 builds. Tracked, deferred: unknown until then.
    TrackedDeferredC1,
}

/// The record of one adapter of a dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdapterReport {
    /// The adapter.
    pub adapter: AdapterId,
    /// What happened.
    pub outcome: SendOutcome,
    /// The local boundary `t_boundary_ns` is read at, per the adapter's
    /// capability record.
    pub boundary: HandoffBoundary,
    /// Whether the adapter's record qualifies that boundary as the send
    /// time.
    pub record_qualifies_boundary: bool,
    /// Whether the dispatch went over a qualified transport bound to this
    /// adapter and boundary. Never true for a diagnostic dispatch.
    pub transport_qualified: bool,
    /// Clock read of the pre-permit check.
    pub t_prepermit_check_ns: Option<u64>,
    /// Elapsed time on the deadline at the pre-permit check, if it passed.
    pub elapsed_at_prepermit_ns: Option<u64>,
    /// Clock read of the final deadline check, the write check, taken once
    /// the transport prepared the request and immediately before the first
    /// `Transport::write` is called, whose first action is the first write
    /// to the wire. Set only if that call followed. Not a time inside the
    /// write, and not a send boundary.
    pub t_write_check_ns: Option<u64>,
    /// Elapsed time on the deadline at the write check, if it passed.
    pub elapsed_at_write_check_ns: Option<u64>,
    /// Clock read after write and flush returned.
    pub t_boundary_ns: Option<u64>,
    /// Clock read when a complete response was classified.
    pub t_response_ns: Option<u64>,
    /// The classified response, once there is one.
    pub response: Option<ResponseOutcome>,
    /// An error of the lane when a submission state was recorded. Never
    /// expected; kept instead of being dropped.
    pub lane_error: Option<LaneError>,
    /// Clock read when an egress transport's commit returned (`T_queue`);
    /// `None` for every wire transport.
    pub t_queue_ns: Option<u64>,
}

impl AdapterReport {
    /// Whether bytes may have left the process through this adapter.
    pub fn bytes_may_have_left(&self) -> bool {
        matches!(
            self.outcome,
            SendOutcome::HandedOff | SendOutcome::Unknown { .. }
        )
    }

    /// Queue-commit time: the local node's egress lane commit (Phase I
    /// gate M4); `None` for an adapter without a queue. Not a send.
    pub fn t_queue_commit_ns(&self) -> Option<u64> {
        self.t_queue_ns
    }

    /// The send time, as far as it is known.
    pub fn send_time(&self) -> SendTime {
        let Some(t_boundary) = self.t_boundary_ns else {
            return SendTime::NotReached;
        };
        if self.boundary == HandoffBoundary::DeferredToNativeGeth {
            return SendTime::TrackedDeferredC1;
        }
        if !self.record_qualifies_boundary {
            return SendTime::BoundaryUnqualified;
        }
        if !self.transport_qualified {
            return SendTime::TransportUnqualified;
        }
        SendTime::Qualified(t_boundary)
    }

    /// The send time, if it is qualified. No other time stands in for it.
    pub fn t_send_ns(&self) -> Option<u64> {
        match self.send_time() {
            SendTime::Qualified(t_send) => Some(t_send),
            _ => None,
        }
    }

    /// The time of an acknowledgement, if the response was one.
    pub fn t_ack_ns(&self) -> Option<u64> {
        match self.response {
            Some(ResponseOutcome::Ack) => self.t_response_ns,
            _ => None,
        }
    }

    /// When the first write to the wire started, as closely as this clock
    /// can say: the write check, after which only the deadline comparison
    /// and the call come. The same read as `t_write_check_ns`.
    pub fn t_socket_write_start_ns(&self) -> Option<u64> {
        self.t_write_check_ns
    }

    /// From the write check to the local boundary. A diagnostic duration;
    /// a send latency only if the send time is qualified.
    pub fn write_check_to_boundary_ns(&self) -> Option<u64> {
        self.t_boundary_ns?.checked_sub(self.t_write_check_ns?)
    }

    /// From the write check to the qualified send time.
    pub fn write_check_to_send_ns(&self) -> Option<u64> {
        self.t_send_ns()?.checked_sub(self.t_write_check_ns?)
    }

    /// From the qualified send time to the acknowledgement.
    pub fn send_to_ack_ns(&self) -> Option<u64> {
        self.t_ack_ns()?.checked_sub(self.t_send_ns()?)
    }
}

/// The record of one dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FanoutReport {
    /// What this dispatch may be evidence for.
    pub evidence: EvidenceClass,
    /// The clock all times were read on.
    pub clock_id: u32,
    /// Clock read after the pre-checks, when the frames were selected.
    pub t_select_ns: u64,
    /// The dispatched intent, once the admission was consumed.
    pub ticket: Option<Ticket>,
    /// Whether the admission was consumed and then no adapter started.
    pub admitted_without_disclosure: bool,
    /// How many adapters were skipped or made unknown by the deadline, at
    /// either check.
    pub deadline_misses: u8,
    /// How many permits were not started when the fan-out ended and were
    /// suppressed by stopping the lane.
    pub permits_suppressed_at_end: u8,
    /// Why the lane is stopped after this dispatch, if it is.
    pub lane_halt: Option<HaltReason>,
    adapters: [Option<AdapterReport>; MAX_ADAPTERS],
}

impl FanoutReport {
    /// Whether the economic admission of the intent was consumed.
    pub fn admitted(&self) -> bool {
        self.ticket.is_some()
    }

    /// The record of `adapter`, if it was part of the dispatch.
    pub fn adapter(&self, adapter: AdapterId) -> Option<&AdapterReport> {
        self.adapters[adapter.index()].as_ref()
    }

    /// From frame selection to the qualified send time of `adapter`.
    pub fn select_to_send_ns(&self, adapter: AdapterId) -> Option<u64> {
        self.adapter(adapter)?
            .t_send_ns()?
            .checked_sub(self.t_select_ns)
    }

    fn set(&mut self, entry: AdapterReport) {
        self.adapters[entry.adapter.index()] = Some(entry);
    }

    fn miss(&mut self) {
        self.deadline_misses = self.deadline_misses.saturating_add(1);
    }
}

/// Reason a dispatch was refused before anything was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchError {
    /// The deadline was made for another release profile than the plan's.
    ProfileMismatch,
    /// The armed adapters are not exactly the plan's adapters, each once.
    ArmedSetMismatch,
    /// A frame was built for another plan or adapter, for another adapter
    /// kind than the table binds, or still holds bytes.
    FrameMismatch,
    /// A frame is larger than its transport takes.
    RequestTooLarge,
    /// A qualified transport is bound to another adapter kind, capability
    /// generation or boundary than the plan and the table have, or was
    /// qualified under another implementation or generation than the one
    /// currently valid for its adapter.
    TransportBindingMismatch,
    /// The plan's final check failed.
    Plan(PlanError),
    /// The lane refused the economic admission.
    Lane(LaneError),
    /// The lane's dispatch is not the one the plan describes. The admission
    /// was consumed and nothing was written; the lane is stopped.
    DispatchBinding(PlanError),
}

impl core::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ProfileMismatch => f.write_str("dispatch: deadline of another profile"),
            Self::ArmedSetMismatch => f.write_str("dispatch: armed adapters differ from the plan"),
            Self::FrameMismatch => f.write_str("dispatch: frame of another plan or adapter"),
            Self::RequestTooLarge => {
                f.write_str("dispatch: request larger than its transport takes")
            }
            Self::TransportBindingMismatch => {
                f.write_str("dispatch: qualified transport bound to something else")
            }
            Self::Plan(error) => write!(f, "dispatch: {error}"),
            Self::Lane(error) => write!(f, "dispatch: {error}"),
            Self::DispatchBinding(error) => write!(f, "dispatch binding: {error}"),
        }
    }
}

impl std::error::Error for DispatchError {}

/// Which entry point a dispatch came through, with what it brings.
#[derive(Clone, Copy)]
enum Mode<'q> {
    Diagnostic,
    Release(&'q TransportQualifications),
}

impl Mode<'_> {
    fn evidence(self) -> EvidenceClass {
        match self {
            Self::Diagnostic => EvidenceClass::Diagnostic,
            Self::Release(_) => EvidenceClass::Release,
        }
    }
}

/// Dispatches a plain plan over any transports. For development and
/// qualification runs; the report is diagnostic and never has a qualified
/// send time.
pub fn dispatch_diagnostic(
    lane: &mut IntentLane,
    plan: &SubmissionPlan,
    context: &DispatchContext<'_>,
    clock: &dyn Clock,
    deadline: &TransportStartDeadline,
    armed: &mut [Armed<'_>],
) -> Result<FanoutReport, DispatchError> {
    run(
        Mode::Diagnostic,
        lane,
        plan,
        context,
        clock,
        deadline,
        armed,
    )
}

/// Dispatches a qualified release plan over qualified transports. The
/// only way to a release dispatch.
///
/// `qualifications` is the runner's current transport qualification per
/// adapter. Every transport must have been qualified under exactly the
/// current one of its adapter; a transport of another implementation or
/// an older generation is refused before anything is consumed.
///
/// A plain plan does not fit:
///
/// ```compile_fail
/// # use darkbot_bsc_trading::submission_policy::plan::{SubmissionPlan, TransportStartDeadline};
/// # use darkbot_bsc_trading::intent::IntentLane;
/// # use darkbot_bsc_execution::submission::send::{dispatch_release, DispatchContext, ReleaseArmed};
/// # use darkbot_bsc_execution::submission::transport::{Clock, TransportQualifications};
/// fn release_from_a_plain_plan(
///     lane: &mut IntentLane,
///     plan: &SubmissionPlan,
///     context: &DispatchContext<'_>,
///     clock: &dyn Clock,
///     deadline: &TransportStartDeadline,
///     qualifications: &TransportQualifications,
///     armed: &mut [ReleaseArmed<'_>],
/// ) {
///     let _ = dispatch_release(lane, plan, context, clock, deadline, qualifications, armed);
/// }
/// ```
///
/// Nor do adapters over unqualified transports:
///
/// ```compile_fail
/// # use darkbot_bsc_trading::submission_policy::plan::{QualifiedReleasePlan, TransportStartDeadline};
/// # use darkbot_bsc_trading::intent::IntentLane;
/// # use darkbot_bsc_execution::submission::send::{dispatch_release, Armed, DispatchContext};
/// # use darkbot_bsc_execution::submission::transport::{Clock, TransportQualifications};
/// fn release_over_unqualified_transports(
///     lane: &mut IntentLane,
///     plan: &QualifiedReleasePlan,
///     context: &DispatchContext<'_>,
///     clock: &dyn Clock,
///     deadline: &TransportStartDeadline,
///     qualifications: &TransportQualifications,
///     armed: &mut [Armed<'_>],
/// ) {
///     let _ = dispatch_release(lane, plan, context, clock, deadline, qualifications, armed);
/// }
/// ```
///
/// With both qualified it compiles, so the two refusals above are about
/// those types and nothing else:
///
/// ```no_run
/// # use darkbot_bsc_trading::submission_policy::plan::{QualifiedReleasePlan, TransportStartDeadline};
/// # use darkbot_bsc_trading::intent::IntentLane;
/// # use darkbot_bsc_execution::submission::send::{dispatch_release, DispatchContext, ReleaseArmed};
/// # use darkbot_bsc_execution::submission::transport::{Clock, TransportQualifications};
/// fn release(
///     lane: &mut IntentLane,
///     plan: &QualifiedReleasePlan,
///     context: &DispatchContext<'_>,
///     clock: &dyn Clock,
///     deadline: &TransportStartDeadline,
///     qualifications: &TransportQualifications,
///     armed: &mut [ReleaseArmed<'_>],
/// ) {
///     let _ = dispatch_release(lane, plan, context, clock, deadline, qualifications, armed);
/// }
/// ```
pub fn dispatch_release(
    lane: &mut IntentLane,
    plan: &QualifiedReleasePlan,
    context: &DispatchContext<'_>,
    clock: &dyn Clock,
    deadline: &TransportStartDeadline,
    qualifications: &TransportQualifications,
    armed: &mut [ReleaseArmed<'_>],
) -> Result<FanoutReport, DispatchError> {
    let mode = Mode::Release(qualifications);
    run(mode, lane, plan.plan(), context, clock, deadline, armed)
}

fn run<S: Slot>(
    mode: Mode<'_>,
    lane: &mut IntentLane,
    plan: &SubmissionPlan,
    context: &DispatchContext<'_>,
    clock: &dyn Clock,
    deadline: &TransportStartDeadline,
    armed: &mut [S],
) -> Result<FanoutReport, DispatchError> {
    let evidence = mode.evidence();

    // ---- Before anything is consumed.
    if deadline.profile() != plan.profile() {
        return Err(DispatchError::ProfileMismatch);
    }
    let mut seen = [false; MAX_ADAPTERS];
    for slot in armed.iter() {
        let adapter = slot.adapter();
        if seen[adapter.index()] || !plan.adapters().contains(adapter) {
            return Err(DispatchError::ArmedSetMismatch);
        }
        seen[adapter.index()] = true;
        let record = context.table.record(adapter);
        let frame = slot.frame();
        if frame.adapter() != adapter
            || frame.plan() != plan
            || frame.is_filled()
            || record.map(|r| r.kind()) != Some(frame.kind())
        {
            return Err(DispatchError::FrameMismatch);
        }
        if frame.len() > slot.max_request_len() {
            return Err(DispatchError::RequestTooLarge);
        }
        if let Mode::Release(qualifications) = mode {
            let current = qualifications.current(adapter);
            let bound = slot.binding().is_some_and(|binding| {
                record.is_some_and(|r| {
                    binding.adapter() == adapter
                        && binding.kind() == r.kind()
                        && binding.capability_generation() == plan.capability_generation()
                        && binding.boundary() == r.handoff_boundary()
                        && current == Some(binding.qualification())
                })
            });
            if !bound {
                return Err(DispatchError::TransportBindingMismatch);
            }
        }
    }
    if armed.len() != plan.adapters().count() {
        return Err(DispatchError::ArmedSetMismatch);
    }
    plan.check_dispatch(context.table, context.current, context.head)
        .map_err(DispatchError::Plan)?;
    // Only a release dispatch, over transports whose binding and current
    // qualification were just verified, can report a qualified send time.
    let transport_qualified = evidence == EvidenceClass::Release;

    let clock_id = clock.id();
    let t_select_ns = clock.now_ns();
    let mut report = FanoutReport {
        evidence,
        clock_id,
        t_select_ns,
        ticket: None,
        admitted_without_disclosure: false,
        deadline_misses: 0,
        permits_suppressed_at_end: 0,
        lane_halt: None,
        adapters: [None; MAX_ADAPTERS],
    };

    // If no adapter could start now, the admission is not consumed: the
    // intent stays prepared and nothing was disclosed.
    let early = deadline.check(clock_id, t_select_ns);
    let any_ready = armed
        .iter()
        .any(|slot| slot.state() == ConnectionState::Ready);
    if early.is_err() || !any_ready {
        for slot in armed.iter() {
            let outcome = match early {
                Err(error) => {
                    report.miss();
                    SendOutcome::SkippedDeadline(error)
                }
                Ok(_) => SendOutcome::SkippedConnection(slot.state()),
            };
            report.set(entry(context.table, slot.adapter(), outcome, false));
        }
        report.lane_halt = lane.halt_reason();
        return Ok(report);
    }

    // ---- The one economic admission of this intent.
    let dispatch = lane
        .begin_send(context.current, plan.adapters(), &plan.costs())
        .map_err(DispatchError::Lane)?;
    report.ticket = Some(dispatch.ticket);
    let bound = match plan.bind_dispatch(&dispatch) {
        Ok(bound) => bound,
        Err(error) => {
            lane.halt(HaltReason::External);
            return Err(DispatchError::DispatchBinding(error));
        }
    };
    let ticket = bound.ticket();
    let buy_hash = bound.buy().hash();

    // ---- Fan-out. No adapter waits for another one's response.
    let mut stopped = false;
    let mut started = false;
    for slot in armed.iter_mut() {
        let adapter = slot.adapter();
        let mut record = entry(
            context.table,
            adapter,
            SendOutcome::Suppressed,
            transport_qualified,
        );
        if stopped {
            report.set(record);
            continue;
        }
        let state = slot.state();
        if state != ConnectionState::Ready {
            record.outcome = SendOutcome::SkippedConnection(state);
            report.set(record);
            continue;
        }

        // Pre-permit check.
        let t_prepermit = clock.now_ns();
        record.t_prepermit_check_ns = Some(t_prepermit);
        match deadline.check(clock_id, t_prepermit) {
            Ok(elapsed) => record.elapsed_at_prepermit_ns = Some(elapsed),
            Err(error) => {
                report.miss();
                record.outcome = SendOutcome::SkippedDeadline(error);
                report.set(record);
                continue;
            }
        }

        // The permit. From here on every failure is unknown.
        let (transport, frame) = slot.parts();
        let egress = transport.is_egress();
        let written = match lane.start_write(ticket, adapter, buy_hash) {
            // The local node's egress lane takes the permit's write itself
            // (no frame, no wire), after the same write check.
            Ok(write) if egress => {
                let t_write = clock.now_ns();
                match deadline.check(clock_id, t_write) {
                    Ok(elapsed) => {
                        record.t_write_check_ns = Some(t_write);
                        record.elapsed_at_write_check_ns = Some(elapsed);
                        match transport.commit_egress(write, t_write) {
                            Ok(()) => Written::Complete,
                            Err(_) => Written::Failed {
                                accepted: 0,
                                cause: UnknownCause::Write,
                            },
                        }
                    }
                    Err(error) => {
                        report.miss();
                        Written::Failed {
                            accepted: 0,
                            cause: UnknownCause::WriteDeadline(error),
                        }
                    }
                }
            }
            Ok(write) => match frame.fill_buy(write.bytes()) {
                // Everything the transport must do before the wire, such
                // as encrypting the request into TLS records, happens now,
                // before the last clock read.
                Ok(()) => match transport.prepare(frame.bytes()) {
                    Ok(()) => {
                        // Write check: the last clock read before the first
                        // write to the wire. Nothing lies between it and
                        // the write but the comparison.
                        let t_write = clock.now_ns();
                        match deadline.check(clock_id, t_write) {
                            Ok(elapsed) => {
                                record.t_write_check_ns = Some(t_write);
                                record.elapsed_at_write_check_ns = Some(elapsed);
                                write_all(transport, frame.bytes())
                            }
                            Err(error) => {
                                report.miss();
                                Written::Failed {
                                    accepted: 0,
                                    cause: UnknownCause::WriteDeadline(error),
                                }
                            }
                        }
                    }
                    Err(_) => Written::Failed {
                        accepted: 0,
                        cause: UnknownCause::Prepare,
                    },
                },
                Err(_) => Written::Failed {
                    accepted: 0,
                    cause: UnknownCause::BuyEncoding,
                },
            },
            Err(error) => {
                record.outcome = SendOutcome::PermitRefused(error);
                report.set(record);
                stopped = true;
                continue;
            }
        };
        let after_ns = clock.now_ns();
        frame.clear_buy();
        started = true;
        match written {
            Written::Complete => {
                record.t_boundary_ns = Some(after_ns);
                if egress {
                    record.t_queue_ns = Some(after_ns);
                }
                record.outcome = SendOutcome::HandedOff;
                record.lane_error = lane.write_completed(ticket, adapter).err();
                if frame.kind() != AdapterKind::LocalGeth {
                    transport.mark(ConnectionState::AwaitingResponse);
                }
            }
            Written::Failed { accepted, cause } => {
                record.outcome = SendOutcome::Unknown {
                    bytes_accepted: accepted,
                    cause,
                };
                record.lane_error = lane.write_failed(ticket, adapter).err();
                if accepted > 0 || matches!(cause, UnknownCause::Write | UnknownCause::Flush) {
                    transport.mark(ConnectionState::Disconnected);
                }
                stopped = true;
            }
        }
        if record.lane_error.is_some() {
            stopped = true;
        }
        report.set(record);
    }

    // ---- No permit outlives the dispatch.
    let mut unstarted = 0u8;
    for slot in armed.iter() {
        if lane.submission(ticket, slot.adapter()) == Ok(SubmissionState::NotStarted) {
            unstarted = unstarted.saturating_add(1);
        }
    }
    report.admitted_without_disclosure = !started;
    if (stopped || unstarted > 0 || !started) && lane.halt_reason().is_none() {
        lane.halt(HaltReason::External);
    }
    report.permits_suppressed_at_end = unstarted;
    report.lane_halt = lane.halt_reason();
    Ok(report)
}

/// A report entry with what the capability record says about the adapter's
/// boundary.
fn entry(
    table: &AdapterTable,
    adapter: AdapterId,
    outcome: SendOutcome,
    transport_qualified: bool,
) -> AdapterReport {
    let record = table.record(adapter);
    AdapterReport {
        adapter,
        outcome,
        boundary: record.map_or(HandoffBoundary::NotDefined, |r| r.handoff_boundary()),
        record_qualifies_boundary: record.is_some_and(|r| r.qualified_as_t_send()),
        transport_qualified,
        t_prepermit_check_ns: None,
        elapsed_at_prepermit_ns: None,
        t_write_check_ns: None,
        elapsed_at_write_check_ns: None,
        t_boundary_ns: None,
        t_response_ns: None,
        response: None,
        lane_error: None,
        t_queue_ns: None,
    }
}

enum Written {
    Complete,
    Failed {
        accepted: usize,
        cause: UnknownCause,
    },
}

/// How often an interrupted system call is continued. Continuing the same
/// call is not a retry of the submission.
const MAX_INTERRUPTS: u8 = 4;

/// Writes all of `bytes` and flushes. Its first action is the first
/// write. Any error, a transport that accepts nothing, or one that would
/// block is a failure; nothing is waited for.
fn write_all(transport: &mut dyn Transport, bytes: &[u8]) -> Written {
    let mut accepted = 0usize;
    let mut interrupts = 0u8;
    while accepted < bytes.len() {
        match transport.write(&bytes[accepted..]) {
            Ok(0) => {
                return Written::Failed {
                    accepted,
                    cause: UnknownCause::Write,
                };
            }
            Ok(count) => accepted += count.min(bytes.len() - accepted),
            Err(error)
                if error.kind() == io::ErrorKind::Interrupted && interrupts < MAX_INTERRUPTS =>
            {
                interrupts += 1;
            }
            Err(_) => {
                return Written::Failed {
                    accepted,
                    cause: UnknownCause::Write,
                };
            }
        }
    }
    loop {
        match transport.flush() {
            Ok(()) => return Written::Complete,
            Err(error)
                if error.kind() == io::ErrorKind::Interrupted && interrupts < MAX_INTERRUPTS =>
            {
                interrupts += 1;
            }
            Err(_) => {
                return Written::Failed {
                    accepted,
                    cause: UnknownCause::Flush,
                };
            }
        }
    }
}

/// Progress of reading one adapter's response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Poll {
    /// Nothing complete yet. Nothing was waited for.
    Pending,
    /// A response was classified and recorded.
    Done(ResponseOutcome),
    /// No response is read for this adapter: its write did not complete,
    /// its response is already recorded, it has no response protocol, or
    /// its connection is not awaiting a response.
    NotExpected,
}

/// Reads what is available of a diagnostic adapter's response and, once
/// it is complete, records it. See [`poll_release_response`] for the
/// rules; they are the same.
pub fn poll_response(
    lane: &mut IntentLane,
    slot: &mut Armed<'_>,
    buffer: &mut ResponseBuffer,
    clock: &dyn Clock,
    report: &mut FanoutReport,
) -> Poll {
    poll(lane, slot, buffer, clock, report)
}

/// Reads what is available of a release adapter's response and, once it
/// is complete, records it: in the report, and as the adapter's
/// submission state in the lane.
///
/// The response is bound to the request in two layers (see the
/// `response` module): by the connection, which carries one request at a
/// time, and, for an HTTP 200, by the JSON-RPC body, whose `jsonrpc` must
/// be `"2.0"` and whose `id` must be the request's. An HTTP 429 is bound by
/// the connection alone.
///
/// An acknowledgement (a bundle hash) moves the submission to
/// acknowledged, a JSON-RPC error or a 429 to rejected, and the connection
/// is ready again. Neither changes the intent: it stays sending until the
/// chain-state owner proves what happened on chain. A response that cannot
/// be understood or answers another request changes no submission state
/// and marks the connection stale; a connection that ended is marked
/// disconnected. The buffer is reset once a response is recorded.
pub fn poll_release_response(
    lane: &mut IntentLane,
    slot: &mut ReleaseArmed<'_>,
    buffer: &mut ResponseBuffer,
    clock: &dyn Clock,
    report: &mut FanoutReport,
) -> Poll {
    poll(lane, slot, buffer, clock, report)
}

fn poll<S: Slot>(
    lane: &mut IntentLane,
    slot: &mut S,
    buffer: &mut ResponseBuffer,
    clock: &dyn Clock,
    report: &mut FanoutReport,
) -> Poll {
    let Some(ticket) = report.ticket else {
        return Poll::NotExpected;
    };
    let adapter = slot.adapter();
    let Some(record) = report.adapters[adapter.index()].as_mut() else {
        return Poll::NotExpected;
    };
    if record.outcome != SendOutcome::HandedOff
        || record.response.is_some()
        || slot.frame().kind() == AdapterKind::LocalGeth
        || slot.state() != ConnectionState::AwaitingResponse
    {
        return Poll::NotExpected;
    }
    let (transport, frame) = slot.parts();
    let request_id = frame.request_id();
    let mut interrupts = 0u8;
    let outcome = loop {
        match transport.read(buffer.spare()) {
            Ok(0) => break ResponseOutcome::Connection,
            Ok(count) => {
                if let Some(outcome) = buffer.advance(count, request_id) {
                    break outcome;
                }
            }
            Err(error) => match error.kind() {
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => return Poll::Pending,
                io::ErrorKind::Interrupted if interrupts < MAX_INTERRUPTS => interrupts += 1,
                _ => break ResponseOutcome::Connection,
            },
        }
    };
    buffer.reset();
    record.t_response_ns = Some(clock.now_ns());
    record.response = Some(outcome);
    match outcome {
        ResponseOutcome::Ack => {
            record.lane_error = lane.acked(ticket, adapter).err();
            transport.mark(ConnectionState::Ready);
        }
        ResponseOutcome::Reject { .. } | ResponseOutcome::RateLimited => {
            record.lane_error = lane.rejected(ticket, adapter).err();
            transport.mark(ConnectionState::Ready);
        }
        ResponseOutcome::Protocol(_) => transport.mark(ConnectionState::Stale),
        ResponseOutcome::Connection => transport.mark(ConnectionState::Disconnected),
    }
    Poll::Done(outcome)
}
