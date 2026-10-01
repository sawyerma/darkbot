//! Minimal BSC Testnet P2P ingress for Gate 0.
//!
//! Standalone Reth network only. No full node, no database, no state sync,
//! no transaction pool and no JSON/WSS race ingress.
//!
//! The race boundary is the raw ETH transaction channel emitted by NetworkManager.
//! Full Transactions messages are actionable immediately. Hash-only announcements are
//! timestamped separately, then the exact transaction is requested from the announcing peer.

use alloy_consensus::Transaction as _;
use alloy_primitives::{Address, BlockHash, Bytes, TxKind, B256};
use alloy_rlp::{Decodable, Encodable};
use bytes::{Buf, BufMut, BytesMut};
use futures::SinkExt;
use reth_chainspec::{
    hardfork, make_genesis_header, BaseFeeParams, BaseFeeParamsKind, Chain, ChainHardforks,
    ChainSpec, EthereumHardfork, ForkCondition, Hardfork, Head, NamedChain,
};
use reth_discv4::{Discv4ConfigBuilder, NodeRecord};
use reth_eth_wire::{
    errors::{EthHandshakeError, EthStreamError},
    handshake::{EthRlpxHandshake, EthereumEthHandshake, UnauthEth},
    GetPooledTransactions, UnifiedStatus,
};
use reth_eth_wire_types::{DisconnectReason, EthVersion};
use reth_ethereum_forks::ForkFilter;
use reth_metrics::common::mpsc::memory_bounded_channel;
use reth_network::{
    config::rng_secret_key,
    transactions::NetworkTransactionEvent,
    EthNetworkPrimitives,
    NetworkConfig,
    NetworkManager,
    PeerRequest,
    Peers,
    PeersConfig,
    PeersInfo,
};
use reth_primitives_traits::SealedHeader;
use reth_storage_api::noop::NoopProvider;
use reth_tasks::Runtime;
use std::{
    future::Future,
    net::{Ipv4Addr, SocketAddr},
    pin::Pin,
    str::FromStr,
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};
use tokio::{sync::oneshot, time::timeout};
use tokio_stream::StreamExt;

const UPGRADE_STATUS_MESSAGE_ID: u8 = 0x0b;
const TX_EVENT_CHANNEL_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct UpgradeStatus {
    extension: UpgradeStatusExtension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UpgradeStatusExtension {
    disable_peer_tx_broadcast: bool,
}

impl Encodable for UpgradeStatusExtension {
    fn encode(&self, out: &mut dyn BufMut) {
        vec![self.disable_peer_tx_broadcast].encode(out);
    }
}

impl Decodable for UpgradeStatusExtension {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        if buf.first().copied() == Some(0x80) {
            buf.advance(1);
            return Ok(Self { disable_peer_tx_broadcast: false });
        }
        let vals = Vec::<bool>::decode(buf)?;
        if vals.len() != 1 {
            return Err(alloy_rlp::Error::Custom("invalid BSC UpgradeStatus bool length"));
        }
        Ok(Self { disable_peer_tx_broadcast: vals[0] })
    }
}

impl Encodable for UpgradeStatus {
    fn encode(&self, out: &mut dyn BufMut) {
        UPGRADE_STATUS_MESSAGE_ID.encode(out);
        vec![&self.extension].encode(out);
    }
}

impl Decodable for UpgradeStatus {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let message_id = u8::decode(buf)?;
        if message_id != UPGRADE_STATUS_MESSAGE_ID {
            return Err(alloy_rlp::Error::Custom("invalid BSC UpgradeStatus message id"));
        }
        let extension = Vec::<UpgradeStatusExtension>::decode(buf)?;
        if extension.len() != 1 {
            return Err(alloy_rlp::Error::Custom("invalid BSC UpgradeStatus extension length"));
        }
        Ok(Self { extension: extension[0] })
    }
}

impl UpgradeStatus {
    fn into_rlpx(self) -> bytes::Bytes {
        let mut out = BytesMut::new();
        self.encode(&mut out);
        out.freeze()
    }
}

#[derive(Debug, Default)]
struct BscHandshake;

impl EthRlpxHandshake for BscHandshake {
    fn handshake<'a>(
        &'a self,
        unauth: &'a mut dyn UnauthEth,
        status: UnifiedStatus,
        fork_filter: ForkFilter,
        timeout_limit: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedStatus, EthStreamError>> + 'a + Send>> {
        Box::pin(async move {
            let fut = async {
                let negotiated =
                    EthereumEthHandshake(unauth).eth_handshake(status, fork_filter).await?;

                if negotiated.version > EthVersion::Eth66 {
                    let msg = UpgradeStatus {
                        extension: UpgradeStatusExtension {
                            disable_peer_tx_broadcast: false,
                        },
                    };
                    unauth.start_send_unpin(msg.into_rlpx())?;

                    let their_msg = match unauth.next().await {
                        Some(Ok(msg)) => msg,
                        Some(Err(e)) => return Err(EthStreamError::from(e)),
                        None => {
                            unauth
                                .disconnect(DisconnectReason::DisconnectRequested)
                                .await?;
                            return Err(EthStreamError::EthHandshakeError(
                                EthHandshakeError::NoResponse,
                            ));
                        }
                    };

                    UpgradeStatus::decode(&mut their_msg.as_ref())
                        .map_err(|e| EthStreamError::InvalidMessage(e.into()))?;
                }

                Ok(negotiated)
            };

            timeout(timeout_limit, fut)
                .await
                .map_err(|_| EthStreamError::StreamTimeout)?
        })
    }
}

hardfork!(
    #[derive(Default)]
    BscHardfork {
        Frontier,
        Ramanujan,
        Niels,
        MirrorSync,
        Bruno,
        Euler,
        Nano,
        Moran,
        Gibbs,
        Planck,
        Luban,
        Plato,
        Hertz,
        HertzFix,
        Kepler,
        Feynman,
        FeynmanFix,
        Cancun,
        Haber,
        HaberFix,
        Bohr,
        Tycho,
        Pascal,
        Lorentz,
        Maxwell,
        Fermi,
        Osaka,
        Mendel,
        #[default]
        Pasteur,
    }
);

fn bsc_testnet_hardforks() -> ChainHardforks {
    ChainHardforks::new(vec![
        (EthereumHardfork::Frontier.boxed(), ForkCondition::Block(0)),
        (EthereumHardfork::Homestead.boxed(), ForkCondition::Block(0)),
        (EthereumHardfork::Tangerine.boxed(), ForkCondition::Block(0)),
        (EthereumHardfork::SpuriousDragon.boxed(), ForkCondition::Block(0)),
        (EthereumHardfork::Byzantium.boxed(), ForkCondition::Block(0)),
        (EthereumHardfork::Constantinople.boxed(), ForkCondition::Block(0)),
        (EthereumHardfork::Petersburg.boxed(), ForkCondition::Block(0)),
        (EthereumHardfork::Istanbul.boxed(), ForkCondition::Block(0)),
        (EthereumHardfork::MuirGlacier.boxed(), ForkCondition::Block(0)),
        (BscHardfork::Ramanujan.boxed(), ForkCondition::Block(1_010_000)),
        (BscHardfork::Niels.boxed(), ForkCondition::Block(1_014_369)),
        (BscHardfork::MirrorSync.boxed(), ForkCondition::Block(5_582_500)),
        (BscHardfork::Bruno.boxed(), ForkCondition::Block(13_837_000)),
        (BscHardfork::Euler.boxed(), ForkCondition::Block(19_203_503)),
        (BscHardfork::Gibbs.boxed(), ForkCondition::Block(22_800_220)),
        (BscHardfork::Nano.boxed(), ForkCondition::Block(23_482_428)),
        (BscHardfork::Moran.boxed(), ForkCondition::Block(23_603_940)),
        (BscHardfork::Planck.boxed(), ForkCondition::Block(28_196_022)),
        (BscHardfork::Luban.boxed(), ForkCondition::Block(29_295_050)),
        (BscHardfork::Plato.boxed(), ForkCondition::Block(29_861_024)),
        (EthereumHardfork::Berlin.boxed(), ForkCondition::Block(31_103_030)),
        (EthereumHardfork::London.boxed(), ForkCondition::Block(31_103_030)),
        (BscHardfork::Hertz.boxed(), ForkCondition::Block(31_103_030)),
        (BscHardfork::HertzFix.boxed(), ForkCondition::Block(35_682_300)),
        (EthereumHardfork::Shanghai.boxed(), ForkCondition::Timestamp(1_702_972_800)),
        (BscHardfork::Kepler.boxed(), ForkCondition::Timestamp(1_702_972_800)),
        (BscHardfork::Feynman.boxed(), ForkCondition::Timestamp(1_710_136_800)),
        (BscHardfork::FeynmanFix.boxed(), ForkCondition::Timestamp(1_711_342_800)),
        (EthereumHardfork::Cancun.boxed(), ForkCondition::Timestamp(1_713_330_442)),
        (BscHardfork::Cancun.boxed(), ForkCondition::Timestamp(1_713_330_442)),
        (BscHardfork::Tycho.boxed(), ForkCondition::Timestamp(1_713_330_442)),
        (BscHardfork::Haber.boxed(), ForkCondition::Timestamp(1_716_962_820)),
        (BscHardfork::HaberFix.boxed(), ForkCondition::Timestamp(1_719_986_788)),
        (BscHardfork::Bohr.boxed(), ForkCondition::Timestamp(1_724_116_996)),
        (EthereumHardfork::Prague.boxed(), ForkCondition::Timestamp(1_740_452_880)),
        (BscHardfork::Pascal.boxed(), ForkCondition::Timestamp(1_740_452_880)),
        (BscHardfork::Lorentz.boxed(), ForkCondition::Timestamp(1_744_097_580)),
        (BscHardfork::Maxwell.boxed(), ForkCondition::Timestamp(1_748_243_100)),
        (BscHardfork::Fermi.boxed(), ForkCondition::Timestamp(1_762_741_500)),
        (BscHardfork::Osaka.boxed(), ForkCondition::Timestamp(1_774_319_400)),
        (BscHardfork::Mendel.boxed(), ForkCondition::Timestamp(1_774_319_400)),
        (BscHardfork::Pasteur.boxed(), ForkCondition::Timestamp(1_784_601_000)),
    ])
}

fn bsc_testnet_chain_spec() -> Arc<ChainSpec> {
    let genesis = Default::default();
    let hardforks = bsc_testnet_hardforks();
    ChainSpec {
        chain: Chain::from_named(NamedChain::BinanceSmartChainTestnet),
        genesis,
        hardforks: hardforks.clone(),
        deposit_contract: None,
        base_fee_params: BaseFeeParamsKind::Constant(BaseFeeParams::new(1, 1)),
        prune_delete_limit: 3500,
        genesis_header: SealedHeader::new(
            make_genesis_header(&Default::default(), &hardforks),
            BlockHash::from_str(
                "0x6d3c66c5357ec91d5c43af47e234a939b22557cbb552dc45bebbceeed90fbe34",
            )
            .unwrap(),
        ),
        ..Default::default()
    }
    .into()
}

fn bsc_testnet_bootnodes() -> Vec<NodeRecord> {
    const NODES: &[&str] = &[
        "enode://0637d1e62026e0c8685b1db0ca1c767c78c95c3fab64abc468d1a64b12ca4b530b46b8f80c915aec96f74f7ffc5999e8ad6d1484476f420f0c10e3d42361914b@52.199.214.252:30311",
        "enode://df1e8eb59e42cad3c4551b2a53e31a7e55a2fdde1287babd1e94b0836550b489ba16c40932e4dacb16cba346bd442c432265a299c4aca63ee7bb0f832b9f45eb@52.51.80.128:30311",
        "enode://ecd664250ca19b1074dcfbfb48576a487cc18d052064222a363adacd2650f8e08fb3db9de7a7aecb48afa410eaeb3285e92e516ead01fb62598553aed91ee15e@3.209.122.123:30311",
        "enode://665cf77ca26a8421cfe61a52ac312958308d4912e78ce8e0f61d6902e4494d4cc38f9b0dd1b23a427a7a5734e27e5d9729231426b06bb9c73b56a142f83f6b68@52.72.123.113:30311",
    ];
    NODES.iter().map(|s| s.parse().unwrap()).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum P2pSignalSource {
    FullBroadcast,
    HashFetch,
}

impl P2pSignalSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FullBroadcast => "BSC_P2P_FULL_BROADCAST",
            Self::HashFetch => "BSC_P2P_HASH_FETCH",
        }
    }
}

#[derive(Debug, Clone)]
pub struct P2pSignal {
    pub observed: Instant,
    pub hash_observed: Option<Instant>,
    pub hash: B256,
    pub input: Bytes,
    pub peers: usize,
    pub source: P2pSignalSource,
}

#[derive(Debug, Clone, Copy)]
pub struct P2pReady {
    pub peers: usize,
}

pub struct P2pSensor {
    pub ready: mpsc::Receiver<P2pReady>,
    pub signals: mpsc::Receiver<P2pSignal>,
    pub hashes: mpsc::Receiver<Instant>,
}

fn tx_matches(tx: &impl alloy_consensus::Transaction, portal: Address, selector: [u8; 4]) -> bool {
    tx.kind() == TxKind::Call(portal) &&
        tx.input().as_ref().get(..4) == Some(selector.as_slice())
}

pub fn spawn_testnet_flap_sensor(
    portal: Address,
    launch_selector: [u8; 4],
    expected_hash: B256,
    min_peers: usize,
    head: Head,
) -> P2pSensor {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (signal_tx, signal_rx) = mpsc::sync_channel(16);
    let (hash_tx, hash_rx) = mpsc::sync_channel(16);

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
                let chain_spec = bsc_testnet_chain_spec();
                let boot_nodes = bsc_testnet_bootnodes();
                let local_addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0);

                // Chapel/Testnet is much more deterministic when the known nodes are
                // dialled directly instead of relying on discovery to populate the
                // peer set. Keep discv4 enabled only as an additional source.
                let peer_config = PeersConfig::default()
                    .with_basic_nodes(boot_nodes.iter().cloned().collect())
                    .with_max_outbound(16)
                    .with_refill_slots_interval(Duration::from_millis(250));

                let net_cfg = NetworkConfig::<_, EthNetworkPrimitives>::builder(
                    rng_secret_key(),
                    Runtime::test(),
                )
                .peer_config(peer_config)
                .boot_nodes(boot_nodes.clone())
                .set_head(head)
                .with_pow()
                .listener_addr(local_addr)
                .eth_rlpx_handshake(Arc::new(BscHandshake))
                .build(NoopProvider::eth(chain_spec));

                let net_cfg = net_cfg.set_discovery_v4(
                    Discv4ConfigBuilder::default()
                        .add_boot_nodes(boot_nodes)
                        .lookup_interval(Duration::from_millis(250))
                        .build(),
                );

                let (network_tx, mut network_rx) =
                    memory_bounded_channel::<NetworkTransactionEvent<EthNetworkPrimitives>>(
                        TX_EVENT_CHANNEL_BYTES,
                        "darkbot_gate0_raw_tx",
                    );

                let mut network = NetworkManager::<EthNetworkPrimitives>::new(net_cfg)
                    .await
                    .expect("failed to create standalone BSC P2P network");
                network.set_transactions(network_tx);
                let handle = network.handle().clone();
                tokio::spawn(network);

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
                        event = network_rx.recv() => {
                            let Some(event) = event else {
                                panic!("BSC raw network transaction channel closed");
                            };

                            match event {
                                NetworkTransactionEvent::IncomingTransactions { msg, .. } => {
                                    let observed = Instant::now();
                                    for tx in msg.0 {
                                        if *tx.tx_hash() != expected_hash {
                                            continue;
                                        }
                                        if !tx_matches(&tx, portal, launch_selector) {
                                            continue;
                                        }
                                        let _ = signal_tx.try_send(P2pSignal {
                                            observed,
                                            hash_observed: None,
                                            hash: expected_hash,
                                            input: tx.input().clone(),
                                            peers: handle.num_connected_peers(),
                                            source: P2pSignalSource::FullBroadcast,
                                        });
                                    }
                                }
                                NetworkTransactionEvent::IncomingPooledTransactionHashes { peer_id, msg } => {
                                    if !msg.hashes().contains(&expected_hash) {
                                        continue;
                                    }

                                    let hash_observed = Instant::now();
                                    let _ = hash_tx.try_send(hash_observed);

                                    // Request immediately from the announcing peer and, in
                                    // parallel, fan out to every currently connected peer. The
                                    // first valid full transaction wins; later duplicates are
                                    // harmless evidence.
                                    let handle_fetch = handle.clone();
                                    let signal_tx_fetch = signal_tx.clone();
                                    tokio::spawn(async move {
                                        let peers = handle_fetch
                                            .get_all_peers()
                                            .await
                                            .unwrap_or_default();

                                        let mut peer_ids = Vec::with_capacity(peers.len() + 1);
                                        peer_ids.push(peer_id);
                                        for info in peers {
                                            if info.remote_id != peer_id {
                                                peer_ids.push(info.remote_id);
                                            }
                                        }

                                        for candidate in peer_ids {
                                            let handle_one = handle_fetch.clone();
                                            let signal_one = signal_tx_fetch.clone();
                                            tokio::spawn(async move {
                                                let (response_tx, response_rx) = oneshot::channel();
                                                handle_one.send_request(
                                                    candidate,
                                                    PeerRequest::GetPooledTransactions {
                                                        request: GetPooledTransactions(vec![expected_hash]),
                                                        response: response_tx,
                                                    },
                                                );

                                                let Ok(Ok(response)) = timeout(
                                                    Duration::from_millis(500),
                                                    response_rx,
                                                ).await else {
                                                    return;
                                                };
                                                let Ok(response) = response else {
                                                    return;
                                                };

                                                let observed = Instant::now();
                                                for tx in response.0 {
                                                    if *tx.tx_hash() != expected_hash {
                                                        continue;
                                                    }
                                                    if !tx_matches(&tx, portal, launch_selector) {
                                                        continue;
                                                    }
                                                    let _ = signal_one.try_send(P2pSignal {
                                                        observed,
                                                        hash_observed: Some(hash_observed),
                                                        hash: expected_hash,
                                                        input: tx.input().clone(),
                                                        peers: handle_one.num_connected_peers(),
                                                        source: P2pSignalSource::HashFetch,
                                                    });
                                                }
                                            });
                                        }
                                    });
                                }
                                NetworkTransactionEvent::GetPooledTransactions { response, .. } => {
                                    let _ = response.send(Ok(Default::default()));
                                }
                                NetworkTransactionEvent::GetTransactionsHandle(response) => {
                                    let _ = response.send(None);
                                }
                            }
                        }
                    }
                }
            });
        })
        .expect("failed to spawn BSC P2P sensor thread");

    P2pSensor {
        ready: ready_rx,
        signals: signal_rx,
        hashes: hash_rx,
    }
}
