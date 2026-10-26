//! The local node's adapter transport over the FastLane egress lane
//! (C1.7; Phase I gate M4): the dispatch hands it the permit's released
//! transaction (`IntentLane::start_write`), which it commits to the egress
//! lane with the write check's clock read. That commit is `T_queue`: a
//! local queue, read by the pinned node's outbound coordinator, not a
//! send, an inclusion or a purchase. It owns no socket; it writes no frame
//! and refuses one. The PAPER path never uses it (its sink has no Mainnet
//! write capability); tests and measurements run it on an isolated arena.

use std::io;
use std::path::Path;

use darkbot_bsc_geth_ipc::outbound::EgressProducerV2;
use darkbot_bsc_geth_ipc::protocol_v2::V2Error;
use darkbot_bsc_trading::intent::EgressWrite;

use super::transport::{ConnectionState, Transport};

pub struct EgressTransport {
    producer: EgressProducerV2,
    state: ConnectionState,
    last_sequence: Option<u64>,
}

impl EgressTransport {
    /// The egress lane of the arena at `path`, ready.
    pub fn open(path: &Path, generation: u64) -> Result<Self, V2Error> {
        Ok(Self {
            producer: EgressProducerV2::open(path, generation)?,
            state: ConnectionState::Ready,
            last_sequence: None,
        })
    }

    /// The egress lane sequence of the last commit (the node's evidence
    /// carries it).
    pub fn last_sequence(&self) -> Option<u64> {
        self.last_sequence
    }
}

impl Transport for EgressTransport {
    fn state(&self) -> ConnectionState {
        self.state
    }

    /// The egress lane takes no frame: only a permit's write.
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::Unsupported.into())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    /// The node answers on evidence lanes, never here.
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::ErrorKind::WouldBlock.into())
    }

    fn mark(&mut self, state: ConnectionState) {
        self.state = state;
    }

    fn is_egress(&self) -> bool {
        true
    }

    fn commit_egress(&mut self, write: EgressWrite<'_>, commit_ns: u64) -> io::Result<()> {
        let sequence = self
            .producer
            .publish_buy(write, commit_ns)
            .map_err(|e| io::Error::other(format!("{e:?}")))?;
        self.last_sequence = Some(sequence);
        Ok(())
    }
}
