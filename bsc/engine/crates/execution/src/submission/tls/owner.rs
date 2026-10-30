//! The owner of one builder's connection.
//!
//! It establishes the connection, keeps it warm, notices when it went bad
//! and replaces it, all outside any dispatch. A dispatch borrows its
//! transport by `&mut` and finds it ready or skips it; nothing a dispatch
//! does can make the owner connect.
//!
//! BSC3-025: the establishment itself (resolve, connect, handshake, within
//! the policy's limits) runs on a thread of its own, so the owner's caller
//! (the runtime's decision loop) never waits for it: [`begin_connect`]
//! starts it, [`poll_connect`] installs what it established. Until then
//! the transport is not eligible, as any transport that is not ready.
//!
//! [`begin_connect`]: TlsConnectionOwner::begin_connect
//! [`poll_connect`]: TlsConnectionOwner::poll_connect

use std::fmt;
use std::io::{self, Read};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread;

use super::config::{PolicyError, TlsEndpoint, TlsPolicy, TlsSetup};
use super::connection::{
    ConnectError, ConnectionFacts, HttpVersion, TLS_TRANSPORT_IMPLEMENTATION, WarmTlsConnection,
};
use super::transport::TlsTransport;
use crate::submission::transport::{ConnectionState, Transport};

/// What one maintenance pass found or did.
#[derive(Clone, Debug, PartialEq)]
pub enum Maintenance {
    /// The connection is ready and was found alive.
    Warm,
    /// A response is outstanding and still within its time.
    Awaiting,
    /// A new connection was established and is ready.
    Connected(ConnectionFacts),
    /// A new connection is being established (BSC3-025); the transport is
    /// not eligible until a later pass finds it ready.
    Connecting,
    /// No connection could be established; the transport is not eligible.
    Failed(ConnectError),
}

/// What a probe of an idle connection found.
enum Probe {
    /// Nothing arrived: alive.
    Alive,
    /// The peer closed the connection.
    Closed,
    /// Data arrived that no request asked for.
    Unsolicited,
}

/// What a qualification of this transport would have to bind, as far as
/// the transport knows it: the implementation, the endpoint, the
/// negotiated connection, the policy and the platform. Adapter, adapter
/// kind, capability generation, handoff boundary and qualification
/// generation are bound by the qualification itself.
#[derive(Clone, Debug, PartialEq)]
pub struct TlsProfile {
    /// [`TLS_TRANSPORT_IMPLEMENTATION`].
    pub implementation: &'static str,
    /// The endpoint's DNS name.
    pub host: String,
    /// The endpoint's port.
    pub port: u16,
    /// The current connection.
    pub connection: ConnectionFacts,
    /// The HTTP version spoken.
    pub http: HttpVersion,
    /// The timing and size rules.
    pub policy: TlsPolicy,
    /// Operating system and architecture of the build.
    pub target: (&'static str, &'static str),
}

/// The owner of one builder's connection: endpoint, TLS setup, policy, the
/// transport, and the generation of its connections.
pub struct TlsConnectionOwner {
    endpoint: TlsEndpoint,
    setup: TlsSetup,
    policy: TlsPolicy,
    transport: TlsTransport,
    generation: u64,
    /// The connection being established, and its generation.
    pending: Option<(u64, Receiver<Result<WarmTlsConnection, ConnectError>>)>,
}

impl TlsConnectionOwner {
    /// An owner with no connection yet. Performs no I/O.
    pub fn new(
        endpoint: TlsEndpoint,
        setup: TlsSetup,
        policy: TlsPolicy,
    ) -> Result<Self, PolicyError> {
        policy.validate()?;
        Ok(Self {
            endpoint,
            setup,
            policy,
            transport: TlsTransport::new(policy.max_idle, policy.max_plaintext_request_len),
            generation: 0,
            pending: None,
        })
    }

    /// Replaces the connection with a new one: resolve, connect, handshake,
    /// waiting for it. For startup and tools; the runtime's loop uses
    /// [`Self::begin_connect`]. Never call it while a dispatch could use the
    /// transport.
    ///
    /// The old connection is dropped first. On success the transport is
    /// ready and the generation is one higher; on failure there is no
    /// connection and the transport is not eligible.
    pub fn connect(&mut self) -> Result<ConnectionFacts, ConnectError> {
        self.begin_connect()?;
        let Some((_, result)) = &self.pending else {
            return Err(ConnectError::Closed);
        };
        let established = result.recv().unwrap_or(Err(ConnectError::Closed));
        self.finish(established)
    }

    /// Starts replacing the connection with a new one on a thread of its
    /// own and returns at once (BSC3-025): the old connection is dropped,
    /// the transport is not eligible until [`Self::poll_connect`] installs
    /// the new one. One that is already being established goes on.
    pub fn begin_connect(&mut self) -> Result<(), ConnectError> {
        if self.pending.is_some() {
            return Ok(());
        }
        self.transport.discard();
        let generation = self.generation + 1;
        let (tx, result) = channel();
        let (endpoint, policy, config) = (self.endpoint.clone(), self.policy, self.setup.config());
        thread::Builder::new()
            .name(format!("bsc-tls-{}", self.endpoint.host()))
            .spawn(move || {
                // A dropped owner no longer waits for it: the connection goes.
                let _ = tx.send(WarmTlsConnection::establish(
                    &endpoint, &policy, config, generation,
                ));
            })
            .map_err(|e| ConnectError::Io(e.kind()))?;
        self.pending = Some((generation, result));
        Ok(())
    }

    /// The connection being established, once it is: installed and ready
    /// (or why it failed); `None` while it still runs or none was begun.
    pub fn poll_connect(&mut self) -> Option<Result<ConnectionFacts, ConnectError>> {
        let (_, result) = self.pending.as_ref()?;
        let established = match result.try_recv() {
            Err(TryRecvError::Empty) => return None,
            // The thread ended without a result (it never should).
            Err(TryRecvError::Disconnected) => Err(ConnectError::Closed),
            Ok(established) => established,
        };
        Some(self.finish(established))
    }

    /// Whether a connection is being established.
    pub fn connecting(&self) -> bool {
        self.pending.is_some()
    }

    fn finish(
        &mut self,
        established: Result<WarmTlsConnection, ConnectError>,
    ) -> Result<ConnectionFacts, ConnectError> {
        let Some((generation, _)) = self.pending.take() else {
            return Err(ConnectError::Closed);
        };
        let connection = established?;
        self.generation = generation;
        let facts = connection.facts();
        self.transport.install(connection);
        Ok(facts)
    }

    /// One maintenance pass, outside any dispatch, never waiting for a
    /// connection to be established (BSC3-025).
    ///
    /// A connection being established is installed once it is. A ready
    /// connection is probed without waiting: if the peer closed it or sent
    /// something no request asked for, it is replaced. A response
    /// outstanding beyond the policy's time gives the connection up. A
    /// connection that is stale, disconnected or missing is replaced: the
    /// new one is begun and the pass returns.
    pub fn maintain(&mut self) -> Maintenance {
        if self.pending.is_some() {
            return match self.poll_connect() {
                None => Maintenance::Connecting,
                Some(Ok(facts)) => Maintenance::Connected(facts),
                Some(Err(error)) => Maintenance::Failed(error),
            };
        }
        match self.transport.state() {
            ConnectionState::Ready => match self.probe() {
                Probe::Alive => return Maintenance::Warm,
                Probe::Closed => self.transport.mark(ConnectionState::Disconnected),
                Probe::Unsolicited => self.transport.mark(ConnectionState::Stale),
            },
            ConnectionState::AwaitingResponse => {
                if self.transport.last_activity().elapsed() <= self.policy.response_timeout {
                    return Maintenance::Awaiting;
                }
                self.transport.mark(ConnectionState::Stale);
            }
            ConnectionState::NotReady | ConnectionState::Stale | ConnectionState::Disconnected => {}
        }
        match self.begin_connect() {
            Ok(()) => Maintenance::Connecting,
            Err(error) => Maintenance::Failed(error),
        }
    }

    /// Reads what an idle connection has, without waiting.
    fn probe(&mut self) -> Probe {
        let Some(connection) = self.transport.connection_mut() else {
            return Probe::Closed;
        };
        let mut scratch = [0u8; 64];
        for _ in 0..4 {
            match connection.tls.read_tls(&mut connection.socket) {
                Ok(0) => return Probe::Closed,
                Ok(_) => {
                    if connection.tls.process_new_packets().is_err() {
                        return Probe::Closed;
                    }
                    // Session tickets and other handshake messages yield
                    // no plaintext; anything else is not ours.
                    match connection.tls.reader().read(&mut scratch) {
                        Ok(0) => return Probe::Closed,
                        Ok(_) => return Probe::Unsolicited,
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                        Err(_) => return Probe::Closed,
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Probe::Alive,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => return Probe::Closed,
            }
        }
        Probe::Alive
    }

    /// The transport, for one dispatch or one response poll. Exclusive:
    /// nothing else can use it meanwhile.
    pub fn transport(&mut self) -> &mut TlsTransport {
        &mut self.transport
    }

    /// The transport's state, as a dispatch would see it.
    pub fn state(&self) -> ConnectionState {
        self.transport.state()
    }

    /// Generation of the current connection; zero before the first one.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether `facts` describe the current connection. Facts of an
    /// earlier generation never do.
    pub fn is_current(&self, facts: &ConnectionFacts) -> bool {
        self.transport.facts().as_ref() == Some(facts)
    }

    /// The endpoint.
    pub fn endpoint(&self) -> &TlsEndpoint {
        &self.endpoint
    }

    /// What a qualification would bind, while there is a connection.
    pub fn profile(&self) -> Option<TlsProfile> {
        let connection = self.transport.facts()?;
        Some(TlsProfile {
            implementation: TLS_TRANSPORT_IMPLEMENTATION,
            host: self.endpoint.host().to_owned(),
            port: self.endpoint.port(),
            connection,
            http: connection.http,
            policy: self.policy,
            target: (std::env::consts::OS, std::env::consts::ARCH),
        })
    }
}

impl fmt::Debug for TlsConnectionOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsConnectionOwner")
            .field("endpoint", &self.endpoint)
            .field("generation", &self.generation)
            .field("transport", &self.transport)
            .finish()
    }
}
