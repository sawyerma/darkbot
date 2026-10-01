use alloy_genesis::Genesis;
use alloy_primitives::{BlockHash, U256};
use reth_chainspec::{
    hardfork, make_genesis_header, BaseFeeParams, BaseFeeParamsKind, Chain, ChainHardforks,
    ChainSpec, EthereumHardfork, ForkCondition, Hardfork, Head,
};
use reth_network_peers::NodeRecord;
use reth_primitives_traits::SealedHeader;
use std::{str::FromStr, sync::Arc, time::{SystemTime, UNIX_EPOCH}};

hardfork!(
    BscHardfork {
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
        Pasteur,
    }
);

impl BscHardfork {
    fn testnet() -> ChainHardforks {
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
            (Self::Ramanujan.boxed(), ForkCondition::Block(1_010_000)),
            (Self::Niels.boxed(), ForkCondition::Block(1_014_369)),
            (Self::MirrorSync.boxed(), ForkCondition::Block(5_582_500)),
            (Self::Bruno.boxed(), ForkCondition::Block(13_837_000)),
            (Self::Euler.boxed(), ForkCondition::Block(19_203_503)),
            (Self::Gibbs.boxed(), ForkCondition::Block(22_800_220)),
            (Self::Nano.boxed(), ForkCondition::Block(23_482_428)),
            (Self::Moran.boxed(), ForkCondition::Block(23_603_940)),
            (Self::Planck.boxed(), ForkCondition::Block(28_196_022)),
            (Self::Luban.boxed(), ForkCondition::Block(29_295_050)),
            (Self::Plato.boxed(), ForkCondition::Block(29_861_024)),
            (EthereumHardfork::Berlin.boxed(), ForkCondition::Block(31_103_030)),
            (EthereumHardfork::London.boxed(), ForkCondition::Block(31_103_030)),
            (Self::Hertz.boxed(), ForkCondition::Block(31_103_030)),
            (Self::HertzFix.boxed(), ForkCondition::Block(35_682_300)),
            (EthereumHardfork::Shanghai.boxed(), ForkCondition::Timestamp(1_702_972_800)),
            (Self::Kepler.boxed(), ForkCondition::Timestamp(1_702_972_800)),
            (Self::Feynman.boxed(), ForkCondition::Timestamp(1_710_136_800)),
            (Self::FeynmanFix.boxed(), ForkCondition::Timestamp(1_711_342_800)),
            (EthereumHardfork::Cancun.boxed(), ForkCondition::Timestamp(1_713_330_442)),
            (Self::Cancun.boxed(), ForkCondition::Timestamp(1_713_330_442)),
            (Self::Tycho.boxed(), ForkCondition::Timestamp(1_713_330_442)),
            (Self::Haber.boxed(), ForkCondition::Timestamp(1_716_962_820)),
            (Self::HaberFix.boxed(), ForkCondition::Timestamp(1_719_986_788)),
            (Self::Bohr.boxed(), ForkCondition::Timestamp(1_724_116_996)),
            (EthereumHardfork::Prague.boxed(), ForkCondition::Timestamp(1_740_452_880)),
            (Self::Pascal.boxed(), ForkCondition::Timestamp(1_740_452_880)),
            (Self::Lorentz.boxed(), ForkCondition::Timestamp(1_744_097_580)),
            (Self::Maxwell.boxed(), ForkCondition::Timestamp(1_748_243_100)),
            (Self::Fermi.boxed(), ForkCondition::Timestamp(1_762_741_500)),
            (Self::Osaka.boxed(), ForkCondition::Timestamp(1_774_319_400)),
            (Self::Mendel.boxed(), ForkCondition::Timestamp(1_774_319_400)),
            (Self::Pasteur.boxed(), ForkCondition::Timestamp(1_784_601_000)),
        ])
    }
}

pub fn chain_spec() -> Arc<ChainSpec> {
    // Only fork-id/network compatibility is needed. No state DB or genesis allocation is loaded.
    let genesis = Genesis::default().with_timestamp(0x5e9da7ce);
    let hardforks = BscHardfork::testnet();
    ChainSpec {
        chain: Chain::from_id(97),
        genesis: genesis.clone(),
        paris_block_and_final_difficulty: Some((0, U256::ZERO)),
        hardforks: hardforks.clone(),
        deposit_contract: None,
        base_fee_params: BaseFeeParamsKind::Constant(BaseFeeParams::new(1, 1)),
        prune_delete_limit: 3500,
        genesis_header: SealedHeader::new(
            make_genesis_header(&genesis, &hardforks),
            BlockHash::from_str(
                "0x6d3c66c5357ec91d5c43af47e234a939b22557cbb552dc45bebbceeed90fbe34",
            )
            .unwrap(),
        ),
        ..Default::default()
    }
    .into()
}

pub fn head() -> Head {
    let number = std::env::var("DARKBOT_BSC_P2P_HEAD_NUMBER")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(140_000_000);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before unix epoch")
        .as_secs();
    Head { number, timestamp, ..Default::default() }
}

pub fn boot_nodes() -> Vec<NodeRecord> {
    BOOTNODES.iter().map(|s| s.parse().unwrap()).collect()
}

static BOOTNODES: &[&str] = &[
    "enode://0637d1e62026e0c8685b1db0ca1c767c78c95c3fab64abc468d1a64b12ca4b530b46b8f80c915aec96f74f7ffc5999e8ad6d1484476f420f0c10e3d42361914b@52.199.214.252:30311",
    "enode://df1e8eb59e42cad3c4551b2a53e31a7e55a2fdde1287babd1e94b0836550b489ba16c40932e4dacb16cba346bd442c432265a299c4aca63ee7bb0f832b9f45eb@52.51.80.128:30311",
    "enode://ecd664250ca19b1074dcfbfb48576a487cc18d052064222a363adacd2650f8e08fb3db9de7a7aecb48afa410eaeb3285e92e516ead01fb62598553aed91ee15e@3.209.122.123:30311",
    "enode://665cf77ca26a8421cfe61a52ac312958308d4912e78ce8e0f61d6902e4494d4cc38f9b0dd1b23a427a7a5734e27e5d9729231426b06bb9c73b56a142f83f6b68@52.72.123.113:30311",
];
