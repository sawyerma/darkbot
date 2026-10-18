use darkbot_runner::market::observer::{MarketConfig, MarketSource};
fn get<'a>(items: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |key| {
        items
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.to_string())
    }
}
const ID: &str = "MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA";
#[test]
fn missing_program() {
    assert_eq!(
        MarketConfig::from_lookup(&get(&[("HELIUS_API_KEY", "TEST")]), 1700000000).unwrap_err(),
        "missing required env: P0_PROGRAM_ID"
    );
}
#[test]
fn missing_source() {
    assert_eq!(
        MarketConfig::from_lookup(&get(&[("P0_PROGRAM_ID", ID)]), 1700000000).unwrap_err(),
        "missing required env: P0_WS_URL (or HELIUS_API_KEY)"
    );
}
#[test]
fn logs_helius() {
    let c = MarketConfig::from_lookup(
        &get(&[
            ("P0_PROGRAM_ID", ID),
            ("HELIUS_API_KEY", "TEST"),
            ("P0_MARKET_SOURCE", "logs"),
        ]),
        1700000000,
    )
    .unwrap();
    assert_eq!(c.source, MarketSource::Logs);
    assert!(c
        .ws_url
        .starts_with("wss://mainnet.helius-rpc.com/?api-key="));
}
