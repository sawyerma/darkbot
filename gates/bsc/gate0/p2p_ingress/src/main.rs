use alloy_primitives::Address;
use darkbot_bsc_p2p_ingress::spawn_testnet_launch_ingress;
use std::{str::FromStr, time::Duration};

fn parse_selector(s: &str) -> [u8; 4] {
    let raw = s.trim_start_matches("0x");
    assert_eq!(raw.len(), 8, "selector must be exactly 4 bytes");
    let bytes = alloy_primitives::hex::decode(raw).expect("invalid selector hex");
    bytes.try_into().unwrap()
}

fn main() {
    let portal = Address::from_str(
        &std::env::var("DARKBOT_BSC_P2P_PORTAL").expect("DARKBOT_BSC_P2P_PORTAL"),
    )
    .expect("invalid portal");
    let selector = parse_selector(
        &std::env::var("DARKBOT_BSC_P2P_SELECTOR").expect("DARKBOT_BSC_P2P_SELECTOR"),
    );
    let head_number = std::env::var("DARKBOT_BSC_P2P_HEAD_NUMBER")
        .expect("DARKBOT_BSC_P2P_HEAD_NUMBER")
        .parse()
        .expect("invalid head number");
    let head_timestamp = std::env::var("DARKBOT_BSC_P2P_HEAD_TIMESTAMP")
        .expect("DARKBOT_BSC_P2P_HEAD_TIMESTAMP")
        .parse()
        .expect("invalid head timestamp");

    let ingress =
        spawn_testnet_launch_ingress(portal, selector, head_number, head_timestamp);

    let ready = ingress
        .ready
        .recv_timeout(Duration::from_secs(20))
        .expect("no BSC P2P peer connected within 20s");
    eprintln!("P2P_READY peers={}", ready.connected_peers);

    loop {
        let signal = ingress.launches.recv().expect("P2P ingress stopped");
        println!(
            "P2P_LAUNCH tx_hash={} source={:?} raw_bytes={} input_bytes={} fetch_ns={:?}",
            signal.tx_hash,
            signal.source,
            signal.raw_tx.len(),
            signal.input.len(),
            signal.fetch_ns,
        );
    }
}
