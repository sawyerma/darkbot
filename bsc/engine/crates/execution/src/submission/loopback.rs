//! The loopback adapter (plan 28.10.): the LIVE release path ending at a
//! builder stand-in on this host. It connects to 127.0.0.1 only and checks
//! the connected peer before it is qualified, so nothing written to it
//! leaves the host. Synthetic LIVE runs and an operator's non-sending LIVE
//! check use it in place of the builders' TLS connections; the frames are
//! the builders' own, the qualification names this implementation (it can
//! never pass for a builder's TLS transport).

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use darkbot_bsc_trading::intent::AdapterId;
use darkbot_bsc_trading::submission_policy::capability::AdapterTable;

use super::transport::{
    ConnectionState, QualifiedTransport, Transport, TransportQualification, bind_loopback,
};

pub const LOOPBACK_TRANSPORT_IMPLEMENTATION: &str = "darkbot-runner/loopback-v1 tcp 127.0.0.1";
pub const LOOPBACK_QUALIFICATION_GENERATION: u64 = 1;

/// Proof that the connected peer is this host: only [`qualify_loopback`]
/// makes one.
pub(crate) struct LoopbackPeer {
    _sealed: (),
}

/// One connection to a builder stand-in on 127.0.0.1.
#[derive(Debug)]
pub struct LoopbackTransport {
    port: u16,
    stream: Option<TcpStream>,
    state: ConnectionState,
}

impl LoopbackTransport {
    /// A transport to 127.0.0.1:`port`, not connected yet. No I/O.
    pub fn new(port: u16) -> Self {
        LoopbackTransport {
            port,
            stream: None,
            state: ConnectionState::NotReady,
        }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Connects to 127.0.0.1 within `timeout` (startup and maintenance,
    /// never in a dispatch); refuses a peer that is not this host.
    pub fn connect(&mut self, timeout: Duration) -> io::Result<()> {
        self.stream = None;
        self.state = ConnectionState::NotReady;
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, self.port));
        let stream = TcpStream::connect_timeout(&addr, timeout)?;
        if !stream.peer_addr()?.ip().is_loopback() {
            return Err(io::Error::other(
                "the loopback adapter reached another host",
            ));
        }
        stream.set_nodelay(true)?;
        stream.set_nonblocking(true)?;
        self.stream = Some(stream);
        self.state = ConnectionState::Ready;
        Ok(())
    }

    fn stream(&mut self) -> io::Result<&mut TcpStream> {
        self.stream
            .as_mut()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))
    }
}

impl Transport for LoopbackTransport {
    fn state(&self) -> ConnectionState {
        self.state
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream()?.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream()?.flush()
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream()?.read(buf)
    }

    fn mark(&mut self, state: ConnectionState) {
        self.state = state;
    }
}

/// Why a loopback transport was not qualified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopbackQualificationError {
    UnboundAdapter,
    NotReady(ConnectionState),
    NotThisHost,
}

/// The loopback transport as the qualified transport of `adapter`: only
/// while it is connected and its peer is 127.0.0.1, bound to the adapter's
/// record in `table`.
pub fn qualify_loopback<'a>(
    transport: &'a mut LoopbackTransport,
    table: &AdapterTable,
    adapter: AdapterId,
) -> Result<(QualifiedTransport<'a>, TransportQualification), LoopbackQualificationError> {
    loopback_in(transport, table, adapter, ConnectionState::Ready)
}

/// The same loopback connection of `adapter`, to read the response to the
/// request it carries (BSC3-051): only while it awaits that response.
pub fn loopback_response<'a>(
    transport: &'a mut LoopbackTransport,
    table: &AdapterTable,
    adapter: AdapterId,
) -> Result<(QualifiedTransport<'a>, TransportQualification), LoopbackQualificationError> {
    loopback_in(transport, table, adapter, ConnectionState::AwaitingResponse)
}

fn loopback_in<'a>(
    transport: &'a mut LoopbackTransport,
    table: &AdapterTable,
    adapter: AdapterId,
    expected: ConnectionState,
) -> Result<(QualifiedTransport<'a>, TransportQualification), LoopbackQualificationError> {
    let record = table
        .record(adapter)
        .ok_or(LoopbackQualificationError::UnboundAdapter)?;
    if transport.state != expected {
        return Err(LoopbackQualificationError::NotReady(transport.state));
    }
    let here = transport
        .stream
        .as_ref()
        .and_then(|s| s.peer_addr().ok())
        .is_some_and(|peer| peer.ip().is_loopback() && peer.port() == transport.port);
    if !here {
        return Err(LoopbackQualificationError::NotThisHost);
    }
    Ok(bind_loopback(
        transport,
        adapter,
        record.kind(),
        table.generation(),
        record.handoff_boundary(),
        LoopbackPeer { _sealed: () },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkbot_bsc_trading::submission_policy::capability::{AdapterBinding, CapabilityId};
    use std::net::TcpListener;

    fn table() -> AdapterTable {
        AdapterTable::new(
            56,
            1,
            &[AdapterBinding {
                adapter: AdapterId::new(0).unwrap(),
                capability: CapabilityId::Club48BuilderV1,
            }],
        )
        .unwrap()
    }

    /// Plan 28.10.: the loopback adapter reaches this host only; it is
    /// qualified under its own implementation, never as the builder's TLS,
    /// and only while connected.
    #[test]
    fn the_loopback_adapter_stays_on_this_host() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let adapter = AdapterId::new(0).unwrap();
        let mut t = LoopbackTransport::new(port);
        assert_eq!(
            qualify_loopback(&mut t, &table(), adapter).err(),
            Some(LoopbackQualificationError::NotReady(
                ConnectionState::NotReady
            ))
        );
        t.connect(Duration::from_secs(1)).unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        let (qualified, q) = qualify_loopback(&mut t, &table(), adapter).unwrap();
        assert_eq!(q.implementation(), LOOPBACK_TRANSPORT_IMPLEMENTATION);
        assert_eq!(qualified.binding().adapter(), adapter);
        assert_ne!(
            q.implementation(),
            crate::submission::tls::TLS_TRANSPORT_IMPLEMENTATION
        );
        assert_eq!(t.write(b"request").unwrap(), 7);
        t.flush().unwrap();
        let mut got = [0u8; 7];
        peer.read_exact(&mut got).unwrap();
        assert_eq!(&got, b"request");
        let mut buf = [0u8; 4];
        assert_eq!(
            t.read(&mut buf).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(
            qualify_loopback(&mut t, &table(), AdapterId::new(1).unwrap()).err(),
            Some(LoopbackQualificationError::UnboundAdapter)
        );
        // Nothing listening: no connection, nothing qualified.
        drop(listener);
        let mut closed = LoopbackTransport::new(port);
        assert!(closed.connect(Duration::from_millis(200)).is_err());
        assert_eq!(closed.state(), ConnectionState::NotReady);
    }
}
