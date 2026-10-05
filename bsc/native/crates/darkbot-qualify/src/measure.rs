//! The Q5A measurement over the frozen production transport.
//!
//! Per provider: one connection is established before anything is
//! measured. Then, with the pacing between samples:
//!
//! 1. the transport must be `Ready`, else the sample is skipped and
//!    nothing is repaired;
//! 2. the probe request is framed completely;
//! 3. `Transport::prepare` takes it (TLS: encrypted into records);
//! 4. the clock is read: `t_write_check`;
//! 5. `Transport::write`, whose first action is the first socket write;
//! 6. `Transport::flush`, which drains the rest;
//! 7. the clock is read: `t_boundary`;
//! 8. the response is read without blocking and classified;
//! 9. the transport is marked by the response.
//!
//! No name resolution, connect, handshake or reconnect happens between
//! `prepare` and `t_boundary`, nor anywhere in the series: a failed handoff
//! is recorded as unknown and the sample is not retried. After the series,
//! the connection is replaced explicitly a few times, each new one probed.
//!
//! The clock is the runner's monotonic clock.
//!
//! A complete HTTP response of any status leaves the connection ready for
//! the next probe: it was read to its end, and the probe is expected to be
//! refused. Only a response that cannot be understood, a timeout or a
//! closed connection end its usability, as in the runner.

use std::io;
use std::thread;
use std::time::{Duration, Instant};

use darkbot_runner::inclusion::response::{ProtocolFailure, ResponseBuffer, ResponseOutcome};
use darkbot_runner::inclusion::tls::{
    AuthorizationToken, TlsConnectionOwner, TlsEndpoint, TlsPolicy, TlsSetup,
};
use darkbot_runner::inclusion::transport::{Clock, ConnectionState, MonotonicClock, Transport};

use crate::evidence::{
    AuthScope, Phase, ProviderRun, ReconnectCycle, ResponseClass, Sample, SampleOutcome, Stage,
    Workload,
};
use crate::identity::now_rfc3339;
use crate::probe::{frame_request, probe_body, sha256_hex};

/// The transport policy of a Q5A run.
pub const QUALIFICATION_POLICY: TlsPolicy = TlsPolicy {
    connect_timeout: Duration::from_secs(5),
    handshake_timeout: Duration::from_secs(5),
    max_idle: Duration::from_secs(10),
    response_timeout: Duration::from_secs(5),
    max_plaintext_request_len: 64 * 1024,
};

/// Storage for one response.
const RESPONSE_CAPACITY: usize = 64 * 1024;

/// How often an interrupted call is continued within one step.
const MAX_INTERRUPTS: u8 = 4;

/// Pause between two reads while a response is awaited.
const RESPONSE_POLL: Duration = Duration::from_micros(50);

/// The authorization of a provider.
pub enum Auth {
    /// The provider takes none.
    NotRequired,
    /// The configured authorization.
    Token(AuthorizationToken),
    /// Needed and not configured.
    Missing,
    /// Configured and not usable.
    Invalid,
}

impl Auth {
    fn scope(&self) -> AuthScope {
        match self {
            Self::NotRequired => AuthScope::NotRequired,
            Self::Token(_) => AuthScope::Present,
            Self::Missing => AuthScope::Missing,
            Self::Invalid => AuthScope::Invalid,
        }
    }
}

/// One provider to measure.
pub struct Provider {
    /// Name in evidence: club48 or blockrazor.
    pub name: &'static str,
    /// Where to connect.
    pub endpoint: TlsEndpoint,
    /// Its authorization.
    pub auth: Auth,
}

/// Measures one provider: connect, warmup, measured series, reconnect
/// cycles.
pub fn run_provider(
    provider: &Provider,
    setup: &TlsSetup,
    policy: TlsPolicy,
    workload: Workload,
    clock: &MonotonicClock,
) -> ProviderRun {
    let mut run = ProviderRun {
        provider: provider.name,
        host: provider.endpoint.host().to_owned(),
        port: provider.endpoint.port(),
        auth: provider.auth.scope(),
        policy,
        workload,
        connect: Err("not attempted".to_owned()),
        started_at: now_rfc3339(),
        ended_at: String::new(),
        samples: Vec::new(),
        reconnects: Vec::new(),
    };
    let mut owner = match TlsConnectionOwner::new(provider.endpoint.clone(), setup.clone(), policy)
    {
        Ok(owner) => owner,
        Err(error) => {
            run.connect = Err(error.to_string());
            run.ended_at = now_rfc3339();
            return run;
        }
    };
    run.connect = owner.connect().map_err(|error| error.to_string());
    let authorization = match &provider.auth {
        Auth::NotRequired => None,
        Auth::Token(token) => Some(token.expose()),
        // Without a usable authorization only the handshake is measured.
        Auth::Missing | Auth::Invalid => {
            run.ended_at = now_rfc3339();
            return run;
        }
    };
    if run.connect.is_err() {
        run.ended_at = now_rfc3339();
        return run;
    }

    let mut buffer = ResponseBuffer::new(RESPONSE_CAPACITY);
    let mut prober = Prober {
        provider: provider.name,
        host: &run.host,
        authorization,
        clock,
        response_timeout: policy.response_timeout,
        buffer: &mut buffer,
    };
    let total = workload.warmup + workload.measured;
    let mut samples = Vec::with_capacity(total as usize);
    for step in 0..total {
        if step > 0 {
            thread::sleep(workload.pacing);
        }
        let (phase, index) = if step < workload.warmup {
            (Phase::Warmup, step)
        } else {
            (Phase::Measured, step - workload.warmup)
        };
        samples.push(prober.sample(&mut owner, phase, index));
    }

    let mut reconnects = Vec::with_capacity(workload.reconnect_cycles as usize);
    for cycle in 0..workload.reconnect_cycles {
        thread::sleep(workload.pacing);
        let previous = owner.transport().facts();
        let result = owner.connect().map_err(|error| error.to_string());
        let previous_still_current = previous
            .as_ref()
            .is_some_and(|facts| owner.is_current(facts));
        let probe = result
            .is_ok()
            .then(|| prober.sample(&mut owner, Phase::Reconnect, cycle));
        reconnects.push(ReconnectCycle {
            cycle,
            previous,
            result,
            previous_still_current,
            probe,
        });
    }
    run.samples = samples;
    run.reconnects = reconnects;
    run.ended_at = now_rfc3339();
    run
}

/// What every sample of one provider shares.
struct Prober<'a> {
    provider: &'static str,
    host: &'a str,
    authorization: Option<&'a str>,
    clock: &'a MonotonicClock,
    response_timeout: Duration,
    buffer: &'a mut ResponseBuffer,
}

impl Prober<'_> {
    /// One probe over the owner's current connection.
    fn sample(&mut self, owner: &mut TlsConnectionOwner, phase: Phase, index: u32) -> Sample {
        let id = format!("q5a-{}-{}-{index:03}", self.provider, phase.tag());
        let body = probe_body(&id);
        let request = frame_request(self.host, self.authorization, &body);
        let transport = owner.transport();
        let facts = transport.facts();
        let state_before = transport.state();
        let mut sample = Sample {
            phase,
            index,
            generation: facts.map(|facts| facts.generation),
            peer: facts.map(|facts| facts.peer),
            state_before,
            request_bytes: request.len(),
            body_bytes: body.len(),
            body_sha256: sha256_hex(body.as_bytes()),
            outcome: SampleOutcome::Skipped(state_before),
            prepare_ns: None,
            t_write_check_ns: None,
            t_boundary_ns: None,
            write_check_to_boundary_ns: None,
            wire_bytes: None,
            response: None,
            response_latency_ns: None,
            state_after: state_before,
        };
        if state_before != ConnectionState::Ready {
            return sample;
        }

        let wire_before = transport.wire_bytes();
        let t_prepare = self.clock.now_ns();
        if let Err(error) = transport.prepare(&request) {
            sample.outcome = SampleOutcome::Unknown {
                stage: Stage::Prepare,
                error: error.kind(),
            };
            sample.state_after = transport.state();
            return sample;
        }
        let t_write_check = self.clock.now_ns();
        let handoff = hand_off(transport, &request);
        let t_boundary = self.clock.now_ns();
        sample.prepare_ns = Some(t_write_check.saturating_sub(t_prepare));
        sample.t_write_check_ns = Some(t_write_check);
        sample.wire_bytes = Some(transport.wire_bytes().saturating_sub(wire_before));
        if let Err((stage, error)) = handoff {
            transport.mark(ConnectionState::Disconnected);
            sample.outcome = SampleOutcome::Unknown { stage, error };
            sample.state_after = transport.state();
            return sample;
        }
        sample.outcome = SampleOutcome::HandedOff;
        sample.t_boundary_ns = Some(t_boundary);
        sample.write_check_to_boundary_ns = Some(t_boundary.saturating_sub(t_write_check));

        transport.mark(ConnectionState::AwaitingResponse);
        let (class, t_response) = self.await_response(transport, &id);
        sample.response = Some(class);
        sample.response_latency_ns = t_response.map(|t| t.saturating_sub(t_boundary));
        transport.mark(state_after_response(class));
        sample.state_after = transport.state();
        sample
    }

    /// Reads the response to request `id` without blocking, until it is
    /// complete or the response timeout has passed.
    fn await_response(
        &mut self,
        transport: &mut dyn Transport,
        id: &str,
    ) -> (ResponseClass, Option<u64>) {
        self.buffer.reset();
        let started = Instant::now();
        let mut interrupts = 0u8;
        loop {
            match transport.read(self.buffer.spare()) {
                Ok(0) => return (ResponseClass::Connection, Some(self.clock.now_ns())),
                Ok(count) => {
                    if let Some(outcome) = self.buffer.advance(count, id.as_bytes()) {
                        let at = self.clock.now_ns();
                        self.buffer.reset();
                        return (classify(outcome), Some(at));
                    }
                }
                Err(error) => match error.kind() {
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => {
                        if started.elapsed() >= self.response_timeout {
                            return (ResponseClass::Timeout, None);
                        }
                        thread::sleep(RESPONSE_POLL);
                    }
                    io::ErrorKind::Interrupted if interrupts < MAX_INTERRUPTS => interrupts += 1,
                    _ => return (ResponseClass::Connection, Some(self.clock.now_ns())),
                },
            }
        }
    }
}

/// Write and flush, as the runner does: an interrupted call is continued,
/// anything else ends the handoff. Never retried as a whole.
fn hand_off(transport: &mut dyn Transport, request: &[u8]) -> Result<(), (Stage, io::ErrorKind)> {
    let mut accepted = 0usize;
    let mut interrupts = 0u8;
    while accepted < request.len() {
        match transport.write(&request[accepted..]) {
            Ok(0) => return Err((Stage::Write, io::ErrorKind::WriteZero)),
            Ok(count) => accepted += count.min(request.len() - accepted),
            Err(error)
                if error.kind() == io::ErrorKind::Interrupted && interrupts < MAX_INTERRUPTS =>
            {
                interrupts += 1;
            }
            Err(error) => return Err((Stage::Write, error.kind())),
        }
    }
    loop {
        match transport.flush() {
            Ok(()) => return Ok(()),
            Err(error)
                if error.kind() == io::ErrorKind::Interrupted && interrupts < MAX_INTERRUPTS =>
            {
                interrupts += 1;
            }
            Err(error) => return Err((Stage::Flush, error.kind())),
        }
    }
}

/// The class of a complete response.
pub fn classify(outcome: ResponseOutcome) -> ResponseClass {
    match outcome {
        ResponseOutcome::Ack => ResponseClass::Ack,
        ResponseOutcome::Reject { code } => ResponseClass::Reject { code },
        ResponseOutcome::RateLimited => ResponseClass::RateLimited,
        ResponseOutcome::Protocol(ProtocolFailure::HttpStatus(status)) => {
            ResponseClass::HttpStatus(status)
        }
        ResponseOutcome::Protocol(failure) => ResponseClass::Malformed(failure),
        ResponseOutcome::Connection => ResponseClass::Connection,
    }
}

/// The state of the connection after a response of `class`.
pub fn state_after_response(class: ResponseClass) -> ConnectionState {
    match class {
        ResponseClass::Ack
        | ResponseClass::Reject { .. }
        | ResponseClass::RateLimited
        | ResponseClass::HttpStatus(_) => ConnectionState::Ready,
        ResponseClass::Malformed(_) | ResponseClass::Timeout => ConnectionState::Stale,
        ResponseClass::Connection => ConnectionState::Disconnected,
    }
}
