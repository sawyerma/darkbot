// Pinned from bnb-chain/reth-bsc @ 98ef6c5b2d8b191f9b10bf1406dce341475c30ce.
// Gate-0 needs only fork-id compatibility + bootnodes; no state DB/full sync.
use alloy_genesis::Genesis;
use alloy_primitives::{BlockHash, U256};
use reth_chainspec::{
    hardfork, make_genesis_header, BaseFeeParams, BaseFeeParamsKind, Chain, ChainHardforks,
    ChainSpec, EthereumHardfork, ForkCondition, Hardfork, NamedChain,
};
use reth_network_peers::NodeRecord;
use reth_primitives_traits::SealedHeader;
use std::{str::FromStr, sync::Arc};

hardfork!(
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
        Pasteur,
    }
);

fn testnet_hardforks() -> ChainHardforks {
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
        (BscHardfork::Haber.boxed(), ForkCondition::Timestamp(1_716_962_820)),
        (BscHardfork::HaberFix.boxed(), ForkCondition::Timestamp(1_719_986_788)),
        (BscHardfork::Bohr.boxed(), ForkCondition::Timestamp(1_724_116_996)),
        (BscHardfork::Tycho.boxed(), ForkCondition::Timestamp(1_713_330_442)),
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

pub fn chain_spec() -> Arc<ChainSpec> {
    let genesis = Genesis::default().with_timestamp(0x5e9da7ce);
    let hardforks = testnet_hardforks();
    Arc::new(ChainSpec {
        chain: Chain::from_named(NamedChain::BinanceSmartChainTestnet),
        genesis: genesis.clone(),
        hardforks: hardforks.clone(),
        deposit_contract: None,
        base_fee_params: BaseFeeParamsKind::Constant(BaseFeeParams::new(1, 1)),
        prune_delete_limit: 3500,
        genesis_header: SealedHeader::new(
            make_genesis_header(&genesis, &hardforks),
            BlockHash::from_str(
                "0x6d3c66c5357ec91d5c43af47e234a939b22557cbb552dc45bebbceeed90fbe34",
            )
            .expect("BSC testnet genesis hash"),
        ),
        ..Default::default()
    })
}

pub fn boot_nodes() -> Vec<NodeRecord> {
    BOOTNODES.iter().map(|s| s.parse().expect("valid BSC bootnode")).collect()
}

static BOOTNODES: &[&str] = &[
    "enode://0637d1e62026e0c8685b1db0ca1c767c78c95c3fab64abc468d1a64b12ca4b530b46b8f80c915aec96f74f7ffc5999e8ad6d1484476f420f0c10e3d42361914b@52.199.214.252:30311",
    "enode://df1e8eb59e42cad3c4551b2a53e31a7e55a2fdde1287babd1e94b0836550b489ba16c40932e4dacb16cba346bd442c432265a299c4aca63ee7bb0f832b9f45eb@52.51.80.128:30311",
    "enode://ecd664250ca19b1074dcfbfb48576a487cc18d052064222a363adacd2650f8e08fb3db9de7a7aecb48afa410eaeb3285e92e516ead01fb62598553aed91ee15e@3.209.122.123:30311",
    "enode://665cf77ca26a8421cfe61a52ac312958308d4912e78ce8e0f61d6902e4494d4cc38f9b0dd1b23a427a7a5734e27e5d9729231426b06bb9c73b56a142f83f6b68@52.72.123.113:30311",
];
