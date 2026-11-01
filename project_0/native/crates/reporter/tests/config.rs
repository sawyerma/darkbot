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
        "missing required env: P0_WS_URL (the selected WSS provider: p0ctl provider select)"
    );
}
/// DEC-18 §1.3.2: a key in the environment never makes a capture endpoint (before: the Helius
/// endpoint with the key in its query); the endpoint is the operational configuration's export.
#[test]
fn logs_without_a_derived_endpoint() {
    let env = [
        ("P0_PROGRAM_ID", ID),
        ("HELIUS_API_KEY", "TEST"),
        ("P0_MARKET_SOURCE", "logs"),
    ];
    assert!(MarketConfig::from_lookup(&get(&env), 1700000000)
        .unwrap_err()
        .starts_with("missing required env: P0_WS_URL"));
    let c = MarketConfig::from_lookup(
        &get(&[
            ("P0_PROGRAM_ID", ID),
            ("P0_WS_URL", "wss://mainnet.helius-rpc.com"),
            ("P0_MARKET_SOURCE", "logs"),
        ]),
        1700000000,
    )
    .unwrap();
    assert_eq!(c.source, MarketSource::Logs);
    assert_eq!(c.ws_url, "wss://mainnet.helius-rpc.com");
}
