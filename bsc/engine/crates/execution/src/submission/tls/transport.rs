//! The TLS transport: the runner's `Transport` over one warm TLS
//! connection.
//!
//! # What each call does
//!
//! - `prepare` hands the complete request to rustls, which encrypts it
//!   into TLS records in its own outgoing buffer. No socket I/O happens.
//!   This is where the encryption and its allocations take place: before
//!   the dispatch's last deadline check.
//! - `write`, called with the prepared request, makes exactly one call of
//!   `write_tls` to the non-blocking socket: the first write to the wire.
//!   If it fails, including with `WouldBlock`, the error is returned as it
//!   is.
//! - `flush` drains what is left with `write_tls` until nothing is
//!   pending. A `WouldBlock`, a write of zero bytes or any other error is
//!   returned as it is: nothing is waited for and nothing is retried,
//!   except an interrupted call, which is continued.
//! - `read` returns plaintext of the response that is already there,
//!   reading and decrypting what the socket has, and `WouldBlock` when
//!   nothing is.
//! - `state` and `mark` perform no I/O. `state` reads the monotonic clock
//!   to apply the idle limit.
//!
//! After a permit, every error of `prepare`, `write` or `flush` makes the
//! submission unknown in the runner.
//!
//! So the boundary at which `flush` returns is: every ciphertext byte of
//! the request was accepted by the kernel's `send`. That is the
//! `TlsUserspaceWriteFlushReturned` boundary of the capability records,
//! and it says nothing about the NIC or the wire. [`TlsTransport::wire_bytes`]
//! counts the bytes the kernel accepted on the current connection.
//!
//! A `write` without a `prepare` takes the bytes into rustls and writes
//! nothing; `flush` then sends them. The dispatch never does that.
//!
//! # Eligibility
//!
//! The transport reports `Ready` only with a connection, nothing pending
//! from an earlier write or preparation, and an idle time within the
//! policy. A connection with pending ciphertext reports `Stale`: a new
//! request would queue behind bytes of another one, so it is skipped
//! before any permit, and nothing is disclosed. That includes a request
//! that was prepared and then not sent because the deadline had passed:
//! its records are never sent, the owner replaces the connection.
//!
//! # Size
//!
//! The transport takes requests up to the policy's
//! `max_plaintext_request_len`; a dispatch refuses a larger one before
//! anything is consumed. rustls' outgoing buffer is sized above that by
//! [`tls_output_capacity`](super::tls_output_capacity), so that a request
//! within the limit is always taken whole.
//!
//! # Allocation
//!
//! rustls' buffered API allocates when it encrypts, in `prepare`: each TLS
//! record becomes a buffer of its own. Nothing here claims an
//! allocation-free path; it has not been measured.

use std::fmt;
use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

use super::connection::{ConnectionFacts, WarmTlsConnection};
use crate::submission::transport::{ConnectionState, Transport};

/// How often an interrupted system call is continued. Continuing the same
/// call is not a retry of the submission.
const MAX_INTERRUPTS: u8 = 4;

/// How many socket reads one `read` call makes at most before it reports
/// that nothing is available yet.
const MAX_READ_ROUNDS: u8 = 8;

/// The runner's transport over one warm TLS connection. Owned exclusively
/// by a [`TlsConnectionOwner`](super::TlsConnectionOwner), which gives it
/// out by `&mut` for a dispatch: no lock, no channel and no thread lie
/// between the runner and the socket.
pub struct TlsTransport {
    connection: Option<WarmTlsConnection>,
    state: ConnectionState,
    max_idle: Duration,
    max_request_len: usize,
    last_activity: Instant,
    /// Length of the request prepared and not yet started; zero if none.
    prepared: usize,
    /// Bytes the kernel accepted on the current connection.
    wire_bytes: u64,
}

impl TlsTransport {
    pub(crate) fn new(max_idle: Duration, max_request_len: usize) -> Self {
        Self {
            connection: None,
            state: ConnectionState::NotReady,
            max_idle,
            max_request_len,
            last_activity: Instant::now(),
            prepared: 0,
            wire_bytes: 0,
        }
    }

    /// Takes over a newly established connection: ready.
    pub(crate) fn install(&mut self, connection: WarmTlsConnection) {
        self.last_activity = connection.established;
        self.connection = Some(connection);
        self.state = ConnectionState::Ready;
        self.prepared = 0;
        self.wire_bytes = 0;
    }

    /// Drops the connection, closing its socket. Prepared records that
    /// were not sent are dropped with it.
    pub(crate) fn discard(&mut self) {
        self.connection = None;
        self.state = ConnectionState::NotReady;
        self.prepared = 0;
        self.wire_bytes = 0;
    }

    pub(crate) fn connection_mut(&mut self) -> Option<&mut WarmTlsConnection> {
        self.connection.as_mut()
    }

    /// When the connection was last made ready or handed a request.
    pub(crate) fn last_activity(&self) -> Instant {
        self.last_activity
    }

    /// What the current connection was established as, if there is one.
    pub fn facts(&self) -> Option<ConnectionFacts> {
        self.connection.as_ref().map(WarmTlsConnection::facts)
    }

    /// Bytes the kernel accepted on the current connection, by `write_tls`
    /// in `write` and `flush`: ciphertext, records and all. Zero for a
    /// connection that never wrote.
    pub fn wire_bytes(&self) -> u64 {
        self.wire_bytes
    }
}

impl Transport for TlsTransport {
    fn state(&self) -> ConnectionState {
        let Some(connection) = &self.connection else {
            return match self.state {
                ConnectionState::Ready | ConnectionState::AwaitingResponse => {
                    ConnectionState::NotReady
                }
                other => other,
            };
        };
        if self.state == ConnectionState::Ready
            && (connection.tls.wants_write()
                || self.prepared != 0
                || self.last_activity.elapsed() > self.max_idle)
        {
            return ConnectionState::Stale;
        }
        self.state
    }

    fn prepare(&mut self, request: &[u8]) -> io::Result<()> {
        let Some(connection) = self.connection.as_mut() else {
            return Err(io::Error::from(io::ErrorKind::NotConnected));
        };
        if self.prepared != 0 || connection.tls.wants_write() {
            // Bytes of another request are still pending.
            return Err(io::Error::from(io::ErrorKind::ResourceBusy));
        }
        let mut taken = 0usize;
        while taken < request.len() {
            match connection.tls.writer().write(&request[taken..]) {
                Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero)),
                Ok(count) => taken += count,
                Err(error) => return Err(error),
            }
        }
        self.prepared = request.len();
        Ok(())
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let Some(connection) = self.connection.as_mut() else {
            return Err(io::Error::from(io::ErrorKind::NotConnected));
        };
        if self.prepared == 0 {
            // Not prepared: take the bytes; `flush` sends them.
            return connection.tls.writer().write(buf);
        }
        if buf.len() != self.prepared {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        // The first write to the wire.
        let written = connection.tls.write_tls(&mut connection.socket)?;
        if written == 0 {
            return Err(io::Error::from(io::ErrorKind::WriteZero));
        }
        self.wire_bytes = self.wire_bytes.saturating_add(written as u64);
        self.prepared = 0;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let Some(connection) = self.connection.as_mut() else {
            return Err(io::Error::from(io::ErrorKind::NotConnected));
        };
        let mut interrupts = 0u8;
        while connection.tls.wants_write() {
            match connection.tls.write_tls(&mut connection.socket) {
                Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero)),
                Ok(count) => self.wire_bytes = self.wire_bytes.saturating_add(count as u64),
                Err(error)
                    if error.kind() == io::ErrorKind::Interrupted
                        && interrupts < MAX_INTERRUPTS =>
                {
                    interrupts += 1;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let Some(connection) = self.connection.as_mut() else {
            return Err(io::Error::from(io::ErrorKind::NotConnected));
        };
        for _ in 0..MAX_READ_ROUNDS {
            // Plaintext already decrypted, or the peer's close_notify.
            match connection.tls.reader().read(buf) {
                Ok(count) => return Ok(count),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
            // More from the socket, without waiting.
            match connection.tls.read_tls(&mut connection.socket) {
                // The peer closed the TCP stream.
                Ok(0) => return Ok(0),
                Ok(_) => {
                    connection
                        .tls
                        .process_new_packets()
                        .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
                }
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::from(io::ErrorKind::WouldBlock))
    }

    fn mark(&mut self, state: ConnectionState) {
        self.state = state;
        if matches!(
            state,
            ConnectionState::Ready | ConnectionState::AwaitingResponse
        ) {
            self.last_activity = Instant::now();
        }
    }

    fn max_request_len(&self) -> usize {
        self.max_request_len
    }
}

impl fmt::Debug for TlsTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsTransport")
            .field("state", &self.state())
            .field("facts", &self.facts())
            .field("max_idle", &self.max_idle)
            .field("max_request_len", &self.max_request_len)
            .field("wire_bytes", &self.wire_bytes)
            .finish()
    }
}
