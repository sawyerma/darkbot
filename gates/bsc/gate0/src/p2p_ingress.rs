//! Minimal BSC Testnet P2P ingress for Gate 0.
//!
//! Uses BNB Chain's reth-bsc handshake/chainspec and upstream Reth network+txpool
//! as a standalone network component. It does NOT start a full node, database,
//! state sync, RPC server, or JSON/WSS ingress.
//!
//! T_signal for this adapter is taken when a complete transaction has entered
//! the in-memory Reth transaction pool. Hash-only gossip may therefore incur an
//! ETH GetPooledTransactions round trip to the announcing peer, but never a
//! JSON-RPC lookup.

use alloy_consensus::Transaction as _;
use alloy_primitives::{Address, Bytes, TxKind, B256};
use reth_bsc::{
    chainspec::{bsc_testnet, BscChainSpec},
    node::network::handshake::BscHandshake,
};
use reth_chainspec::EthChainSpec;
use reth_discv4::Discv4ConfigBuilder;
use reth_network::{config::rng_secret_key, NetworkConfig, NetworkManager, PeersInfo};
use reth_provider::noop::NoopProvider;
use reth_tasks::Runtime;
use reth_transaction_pool::{
    blobstore::InMemoryBlobStore,
    test_utils::OkValidator,
    CoinbaseTipOrdering,
    EthPooledTransaction,
    Pool,
    TransactionListenerKind,
    TransactionPool,
};
use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct P2pSignal {
    pub observed: Instant,
    pub hash: B256,
    pub input: Bytes,
    pub peers: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct P2pReady {
    pub peers: usize,
}

pub struct P2pSensor {
    pub ready: mpsc::Receiver<P2pReady>,
    pub signals: mpsc::Receiver<P2pSignal>,
}

pub fn spawn_testnet_flap_sensor(
    portal: Address,
    launch_selector: [u8; 4],
    min_peers: usize,
) -> P2pSensor {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (signal_tx, signal_rx) = mpsc::sync_channel(1024);

    thread::Builder::new()
        .name("darkbot-bsc-p2p".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .thread_name("darkbot-bsc-p2p-rt")
                .build()
                .expect("failed to build BSC P2P runtime");

            rt.block_on(async move {
                // Reth's standalone BSC P2P example uses a normal Ethereum wire
                // network plus the BSC handshake. Keep the raw ChainSpec in the
                // NoopProvider and use the BscChainSpec wrapper only for BSC
                // bootnodes/head selection.
                let raw_chain_spec = Arc::new(bsc_testnet());
                let bsc_chain_spec = BscChainSpec::from((*raw_chain_spec).clone());
                let boot_nodes = bsc_chain_spec.bootnodes().unwrap_or_default();
                assert!(!boot_nodes.is_empty(), "reth-bsc returned no BSC Testnet bootnodes");

                let pool: Pool<
                    OkValidator<EthPooledTransaction>,
                    CoinbaseTipOrdering<EthPooledTransaction>,
                    InMemoryBlobStore,
                > = Pool::new(
                    OkValidator::default(),
                    CoinbaseTipOrdering::default(),
                    InMemoryBlobStore::default(),
                    Default::default(),
                );

                let local_addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0);

                let net_cfg = NetworkConfig::builder(rng_secret_key(), Runtime::test())
                    .boot_nodes(boot_nodes.clone())
                    .set_head(bsc_chain_spec.head())
                    .with_pow()
                    .listener_addr(local_addr)
                    .eth_rlpx_handshake(Arc::new(BscHandshake::default()))
                    .build(NoopProvider::eth(raw_chain_spec));

                let net_cfg = net_cfg.set_discovery_v4(
                    Discv4ConfigBuilder::default()
                        .add_boot_nodes(boot_nodes)
                        .lookup_interval(Duration::from_millis(250))
                        .build(),
                );

                let transactions_manager_config = net_cfg.transactions_manager_config.clone();
                let (handle, network, txpool, _) = NetworkManager::builder(net_cfg)
                    .await
                    .expect("failed to create standalone BSC P2P network")
                    .transactions(pool.clone(), transactions_manager_config)
                    .split_with_handle();

                tokio::spawn(network);
                tokio::spawn(txpool);

                let mut txs =
                    pool.new_transactions_listener_for(TransactionListenerKind::All);
                let mut ready_sent = false;
                let mut readiness_tick = tokio::time::interval(Duration::from_millis(100));
                readiness_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                loop {
                    tokio::select! {
                        _ = readiness_tick.tick() => {
                            let peers = handle.num_connected_peers();
                            if !ready_sent && peers >= min_peers {
                                let _ = ready_tx.send(P2pReady { peers });
                                ready_sent = true;
                            }
                        }
                        event = txs.recv() => {
                            let Some(event) = event else {
                                panic!("BSC P2P transaction listener closed");
                            };

                            // Timestamp immediately when the full transaction becomes locally
                            // actionable. No JSON parsing or synchronous RPC lookup occurs here.
                            let observed = Instant::now();
                            let pool_tx = &event.transaction.transaction;

                            if pool_tx.kind() != TxKind::Call(portal) {
                                continue;
                            }

                            let input = pool_tx.input();
                            if input.as_ref().get(..4) != Some(launch_selector.as_slice()) {
                                continue;
                            }

                            let signal = P2pSignal {
                                observed,
                                hash: *event.transaction.hash(),
                                input: input.clone(),
                                peers: handle.num_connected_peers(),
                            };

                            // Never block the P2P task behind the Gate-0 consumer.
                            let _ = signal_tx.try_send(signal);
                        }
                    }
                }
            });
        })
        .expect("failed to spawn BSC P2P sensor thread");

    P2pSensor {
        ready: ready_rx,
        signals: signal_rx,
    }
}
