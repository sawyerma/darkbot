//! The bounded transport interface, the binding of a qualified transport,
//! and the clock of the runner.
//!
//! A [`Transport`] is one warm connection to one adapter. Establishing it
//! (DNS, TCP, TLS) happens elsewhere and never on the entry path: the send
//! path only asks for the [`ConnectionState`] and skips an adapter that is
//! not [`ConnectionState::Ready`]. It never connects, reconnects or
//! retries.
//!
//! Any `Transport` may be used for a diagnostic dispatch. A release
//! dispatch needs a [`QualifiedTransport`]: a transport bound to one
//! adapter, its kind, a capability generation, the handoff boundary whose
//! time it was qualified to report as the send time, and the
//! [`TransportQualification`] it was qualified under: an implementation id
//! and a qualification generation.
//!
//! A transport's qualification is not trusted by itself. A release
//! dispatch also takes the runner's [`TransportQualifications`]: the
//! qualification currently valid for each adapter. A transport qualified
//! under another implementation or an older generation does not match and
//! is refused before anything is consumed. Rotating a qualification
//! therefore retires every transport made under the previous one.
//!
//! Neither a qualification nor a qualified transport has a public
//! constructor; the only constructors are compiled for this crate's unit
//! tests alone. Without them the set of current qualifications can only be
//! empty, and every release dispatch is refused. A qualified TLS transport
//! will add the first production constructors, from the concrete transport
//! type and the evidence of its qualification, never from a `dyn
//! Transport`.

use std::io;
use std::time::Instant;

use darkbot_core::inclusion::capability::{AdapterKind, HandoffBoundary};
use darkbot_core::intent::{AdapterId, MAX_ADAPTERS};

/// State of an adapter's connection, as its owner last established it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    /// Connected, handshaken, nothing outstanding: a request may be sent.
    Ready,
    /// A request was handed off and its response is not complete yet. No
    /// further request may be sent on this connection until it is.
    AwaitingResponse,
    /// Not established yet.
    NotReady,
    /// Established, but no longer trusted: idle too long, or a response on
    /// it could not be understood.
    Stale,
    /// Closed or failed.
    Disconnected,
}

/// One warm connection to one adapter.
///
/// `write`, `flush` and `read` have the meaning of `std::io`. `read`
/// reports "nothing available yet" as `WouldBlock` or `TimedOut`; it must
/// not wait for a response that has not arrived.
///
/// # Sending a request in two stages
///
/// A dispatch hands a request over in two stages, so that its last
/// deadline check lies immediately before the first byte goes to the wire:
///
/// 1. [`Transport::prepare`] takes the complete request and does whatever
///    has to happen before the wire can be written, such as encrypting it
///    into TLS records, without any I/O;
/// 2. the dispatch reads the clock and checks its deadline;
/// 3. `write`, called with the same request, starts with the first write
///    to the wire, and `flush` completes the handoff.
///
/// A transport that writes the request as it is has nothing to prepare;
/// the default does nothing, and `write` and `flush` keep their plain
/// meaning.
pub trait Transport {
    /// Current state of the connection. Must not perform I/O.
    fn state(&self) -> ConnectionState;

    /// Writes some bytes of `buf` and returns how many were accepted. After
    /// a successful `prepare` of `buf`, its first action is the first write
    /// to the wire.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize>;

    /// Flushes buffers this layer owns.
    fn flush(&mut self) -> io::Result<()>;

    /// Reads response bytes that are already available.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;

    /// Records the connection's new state. Never connects.
    fn mark(&mut self, state: ConnectionState);

    /// Prepares the complete `request` for the wire without any I/O. Called
    /// at most once per request, before its first `write`, which is then
    /// given the same bytes. An error means nothing reached the wire.
    ///
    /// The default has nothing to prepare.
    fn prepare(&mut self, request: &[u8]) -> io::Result<()> {
        let _ = request;
        Ok(())
    }

    /// Largest request, in bytes, this transport takes. A dispatch refuses
    /// a larger one before anything is consumed.
    ///
    /// The default sets no limit.
    fn max_request_len(&self) -> usize {
        usize::MAX
    }
}

/// A [`Transport`] over any `std::io` stream, for diagnostics only.
///
/// It forwards to the stream and records a state. It does not make the
/// stream non-blocking, does not know whether the stream is a socket, a
/// TLS session or something else, and says nothing about where its
/// writes end up. It is never qualified and cannot be used for a release.
#[derive(Debug)]
pub struct DiagnosticStreamTransport<S> {
    stream: S,
    state: ConnectionState,
}

impl<S> DiagnosticStreamTransport<S> {
    /// Wraps a stream its owner considers connected and ready.
    pub fn ready(stream: S) -> Self {
        Self {
            stream,
            state: ConnectionState::Ready,
        }
    }

    /// The wrapped stream.
    pub fn stream(&self) -> &S {
        &self.stream
    }
}

impl<S: io::Read + io::Write> Transport for DiagnosticStreamTransport<S> {
    fn state(&self) -> ConnectionState {
        self.state
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }

    fn mark(&mut self, state: ConnectionState) {
        self.state = state;
    }
}

/// The qualification of a transport implementation: which implementation,
/// and which generation of its qualification. Opaque: there is no public
/// constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransportQualification {
    implementation: &'static str,
    generation: u64,
}

impl TransportQualification {
    /// Id of the qualified implementation.
    pub fn implementation(&self) -> &'static str {
        self.implementation
    }

    /// Generation of the qualification. A new qualification run of the
    /// same implementation gets a new generation.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// A qualification for the unit tests of the release path only.
    #[cfg(test)]
    pub(crate) fn for_tests(implementation: &'static str, generation: u64) -> Self {
        Self {
            implementation,
            generation,
        }
    }
}

/// The transport qualification currently valid for each adapter, as the
/// runner holds it. A release dispatch admits a transport only if its
/// qualification is exactly the current one of its adapter.
///
/// It can be built freely, but only from [`TransportQualification`]s,
/// which nothing outside the unit tests can make: in a production build it
/// is empty.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransportQualifications {
    current: [Option<TransportQualification>; MAX_ADAPTERS],
}

impl TransportQualifications {
    /// The same set with `qualification` as the current one of `adapter`,
    /// replacing an earlier one.
    pub fn with(mut self, adapter: AdapterId, qualification: TransportQualification) -> Self {
        self.current[adapter.index()] = Some(qualification);
        self
    }

    /// The current qualification of `adapter`, if any.
    pub fn current(&self, adapter: AdapterId) -> Option<TransportQualification> {
        self.current[adapter.index()]
    }
}

/// What a qualified transport is bound to. Compared with the plan, the
/// capability table and the current qualifications before a release
/// dispatch consumes anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransportBinding {
    adapter: AdapterId,
    kind: AdapterKind,
    capability_generation: u64,
    boundary: HandoffBoundary,
    qualification: TransportQualification,
}

impl TransportBinding {
    /// The adapter index the transport is bound to.
    pub fn adapter(&self) -> AdapterId {
        self.adapter
    }

    /// The adapter kind the transport is bound to.
    pub fn kind(&self) -> AdapterKind {
        self.kind
    }

    /// The capability generation the qualification holds for.
    pub fn capability_generation(&self) -> u64 {
        self.capability_generation
    }

    /// The boundary whose time this transport reports as the send time.
    pub fn boundary(&self) -> HandoffBoundary {
        self.boundary
    }

    /// The qualification the transport was made under.
    pub fn qualification(&self) -> TransportQualification {
        self.qualification
    }
}

/// A transport bound to one adapter and qualified to report its send time
/// at one boundary. Opaque: there is no public constructor.
pub struct QualifiedTransport<'a> {
    transport: &'a mut dyn Transport,
    binding: TransportBinding,
}

impl<'a> QualifiedTransport<'a> {
    /// What the transport is bound to.
    pub fn binding(&self) -> TransportBinding {
        self.binding
    }

    pub(crate) fn transport(&self) -> &dyn Transport {
        &*self.transport
    }

    pub(crate) fn transport_mut(&mut self) -> &mut dyn Transport {
        &mut *self.transport
    }

    /// A test double bound as qualified under `qualification`. Unit tests
    /// of the release path only.
    #[cfg(test)]
    pub(crate) fn for_tests(
        transport: &'a mut dyn Transport,
        adapter: AdapterId,
        kind: AdapterKind,
        capability_generation: u64,
        boundary: HandoffBoundary,
        qualification: TransportQualification,
    ) -> Self {
        Self {
            transport,
            binding: TransportBinding {
                adapter,
                kind,
                capability_generation,
                boundary,
                qualification,
            },
        }
    }
}

/// A monotonic clock with an identity. Times of different clocks are never
/// compared.
pub trait Clock {
    /// Identity of this clock.
    fn id(&self) -> u32;

    /// Nanoseconds on this clock. Never decreases.
    fn now_ns(&self) -> u64;
}

/// The process's monotonic clock: nanoseconds since this value was made,
/// read from `std::time::Instant`.
///
/// It is a clock of this process only. Relating it to the clock of another
/// process or machine needs a measured mapping, which is not done here.
#[derive(Clone, Copy, Debug)]
pub struct MonotonicClock {
    id: u32,
    origin: Instant,
}

impl MonotonicClock {
    /// A clock with identity `id` whose zero is now.
    pub fn start(id: u32) -> Self {
        Self {
            id,
            origin: Instant::now(),
        }
    }
}

impl Clock for MonotonicClock {
    fn id(&self) -> u32 {
        self.id
    }

    fn now_ns(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
}
