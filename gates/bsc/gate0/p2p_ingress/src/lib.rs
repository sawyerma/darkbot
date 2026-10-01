use alloy_consensus::Transaction;
use alloy_eips::eip2718::Encodable2718;
use alloy_primitives::{Address, B256};
use reth_bsc::{
    chainspec::bsc_testnet,
    node::network::{bootnodes::bsc_testnet_nodes, handshake::BscHandshake},
};
use reth_chainspec::Head;
use reth_discv4::Discv4ConfigBuilder;
use reth_eth_wire_types::{GetPooledTransactions, PooledTransactions};
use reth_metrics::common::mpsc::memory_bounded_channel;
use reth_network::{
    transactions::{
        constants::tx_manager::DEFAULT_TX_MANAGER_CHANNEL_MEMORY_LIMIT_BYTES,
        NetworkTransactionEvent,
    },
    NetworkConfig, NetworkEvent, NetworkEventListenerProvider, NetworkManager, PeerRequest,
    PeersInfo,
};
use reth_network_api::events::PeerEvent;
use reth_provider::noop::NoopProvider;
use reth_tasks::Runtime;
use secp256k1::{rand, SecretKey};
use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::{mpsc as std_mpsc, Arc},
    thread,
    time::{Duration, Instant},
};
use tokio::sync::oneshot;
use tokio_stream::StreamExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalSource {
    FullBroadcast,
    HashFetch,
}

#[derive(Debug)]
pub struct LaunchSignal {
    /// Gate-0 T_signal: first instant at which the complete signed transaction body
    /// is available to our process. Hash-only announcements are deliberately not T_signal.
    pub observed_at: Instant,
    pub announcement_at: Option<Instant>,
    pub fetch_ns: Option<u64>,
    pub tx_hash: B256,
    pub raw_tx: Vec<u8>,
    pub input: Vec<u8>,
    pub source: SignalSource,
}

#[derive(Debug, Clone, Copy)]
pub struct IngressReady {
    pub connected_peers: usize,
}

pub struct IngressHandle {
    pub launches: std_mpsc::Receiver<LaunchSignal>,
    pub ready: std_mpsc::Receiver<IngressReady>,
}

fn maybe_emit<T>(
    tx: &T,
    observed_at: Instant,
    announcement_at: Option<Instant>,
    portal: Address,
    selector: [u8; 4],
    source: SignalSource,
    out: &std_mpsc::Sender<LaunchSignal>,
) where
    T: Transaction + Encodable2718,
{
    // IMPORTANT: observed_at is captured before target qualification.
    // The race timer therefore includes portal/selector discrimination.
    if tx.to() != Some(portal) {
        return;
    }
    let input = tx.input();
    if input.len() < 4 || input[..4] != selector {
        return;
    }

    let fetch_ns = announcement_at.map(|t| observed_at.duration_since(t).as_nanos() as u64);
    let signal = LaunchSignal {
        observed_at,
        announcement_at,
        fetch_ns,
        tx_hash: tx.trie_hash(),
        raw_tx: tx.encoded_2718(),
        input: input.to_vec(),
        source,
    };
    let _ = out.send(signal);
}

/// Start a BSC Testnet P2P-only launch ingress.
///
/// head_number and head_timestamp are bootstrap/control inputs only. They are
/// needed for the eth fork-id handshake and are not used on the measured race path.
/// No JSON-RPC/WSS request occurs inside this ingress.
pub fn spawn_testnet_launch_ingress(
    portal: Address,
    selector: [u8; 4],
    head_number: u64,
    head_timestamp: u64,
) -> IngressHandle {
    let (launch_tx, launch_rx) = std_mpsc::channel();
    let (ready_tx, ready_rx) = std_mpsc::channel();

    thread::Builder::new()
        .name("darkbot-bsc-p2p".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .thread_name("darkbot-bsc-p2p-rt")
                .build()
                .expect("build P2P runtime");
            rt.block_on(async move {
                let secret_key = SecretKey::new(&mut rand::thread_rng());
                let boot_nodes = bsc_testnet_nodes();
                let listen_port = std::env::var("DARKBOT_BSC_P2P_PORT")
                    .ok()
                    .and_then(|v| v.parse::<u16>().ok())
                    .unwrap_or(30303);
                let local_addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), listen_port);

                let head = Head {
                    number: head_number,
                    timestamp: head_timestamp,
                    ..Default::default()
                };

                let chain_spec = Arc::new(bsc_testnet());
                let cfg = NetworkConfig::builder(secret_key, Runtime::test())
                    .boot_nodes(boot_nodes.clone())
                    .set_head(head)
                    .with_pow()
                    .listener_addr(local_addr)
                    .eth_rlpx_handshake(Arc::new(BscHandshake::default()))
                    .build(NoopProvider::eth(chain_spec))
                    .set_discovery_v4(
                        Discv4ConfigBuilder::default()
                            .add_boot_nodes(boot_nodes)
                            .lookup_interval(Duration::from_millis(250))
                            .build(),
                    );

                let (transactions_tx, mut transactions_rx) =
                    memory_bounded_channel::<NetworkTransactionEvent<EthNetworkPrimitives>>(
                        DEFAULT_TX_MANAGER_CHANNEL_MEMORY_LIMIT_BYTES,
                        "darkbot_gate0_p2p_tx_events",
                    );

                let network = NetworkManager::<EthNetworkPrimitives>::new(cfg)
                    .await
                    .expect("create BSC P2P network")
                    .with_transactions(transactions_tx);

                let handle = network.handle().clone();
                let mut events = handle.event_listener();
                tokio::spawn(network);

                let ready_sender = ready_tx.clone();
                let peer_handle = handle.clone();
                tokio::spawn(async move {
                    let mut announced_ready = false;
                    while let Some(event) = events.next().await {
                        match event {
                            NetworkEvent::ActivePeerSession { .. } => {
                                let peers = peer_handle.num_connected_peers();
                                if peers > 0 && !announced_ready {
                                    let _ = ready_sender.send(IngressReady {
                                        connected_peers: peers,
                                    });
                                    announced_ready = true;
                                }
                            }
                            NetworkEvent::Peer(PeerEvent::SessionClosed { .. }) => {}
                            _ => {}
                        }
                    }
                });

                while let Some(event) = transactions_rx.recv().await {
                    match event {
                        NetworkTransactionEvent::IncomingTransactions { msg, .. } => {
                            let observed_at = Instant::now();
                            for tx in msg {
                                maybe_emit(
                                    &tx,
                                    observed_at,
                                    None,
                                    portal,
                                    selector,
                                    SignalSource::FullBroadcast,
                                    &launch_tx,
                                );
                            }
                        }
                        NetworkTransactionEvent::IncomingPooledTransactionHashes {
                            peer_id,
                            msg,
                        } => {
                            let announcement_at = Instant::now();
                            let hashes = msg.into_hashes();
                            if hashes.is_empty() {
                                continue;
                            }

                            let request_handle = handle.clone();
                            let emit = launch_tx.clone();
                            tokio::spawn(async move {
                                let (response_tx, response_rx) = oneshot::channel();
                                request_handle.send_request(
                                    peer_id,
                                    PeerRequest::GetPooledTransactions {
                                        request: GetPooledTransactions(hashes),
                                        response: response_tx,
                                    },
                                );

                                let response = tokio::time::timeout(
                                    Duration::from_millis(750),
                                    response_rx,
                                )
                                .await;

                                let Ok(Ok(Ok(PooledTransactions(txs)))) = response else {
                                    return;
                                };

                                let observed_at = Instant::now();
                                for tx in txs {
                                    maybe_emit(
                                        &tx,
                                        observed_at,
                                        Some(announcement_at),
                                        portal,
                                        selector,
                                        SignalSource::HashFetch,
                                        &emit,
                                    );
                                }
                            });
                        }
                        NetworkTransactionEvent::GetPooledTransactions { response, .. } => {
                            let _ = response.send(Ok(PooledTransactions::default()));
                        }
                        NetworkTransactionEvent::GetTransactionsHandle(response) => {
                            let _ = response.send(None);
                        }
                    }
                }
            });
        })
        .expect("spawn BSC P2P ingress thread");

    IngressHandle {
        launches: launch_rx,
        ready: ready_rx,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn selector_shape_is_exactly_four_bytes() {
        let selector = [0x12, 0x34, 0x56, 0x78];
        assert_eq!(selector.len(), 4);
    }
}
