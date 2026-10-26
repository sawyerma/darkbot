//! Selectors, topics and decoders of the Flap Portal for the freeze. Every
//! selector and topic is alloy's (`sol!`); the declarations are the
//! signatures of the archived official IPortal (enums as `uint8`), and the
//! freeze re-derives every signature from the archived sources with
//! [`solidity`] and compares. Nothing here hashes by hand.

use alloy_primitives::{Address, B256};
use alloy_sol_types::{SolCall, SolEvent};

/// The Solidity declarations. alloy generates a constructor with one
/// argument per Solidity parameter, so the argument-count lint is off here.
#[allow(clippy::too_many_arguments)]
mod declarations {
    use alloy_sol_types::sol;

    sol! {
        /// Flap Portal (archived flap-sh/FlapVaultExample@5949cc7e
        /// `src/flap/IPortal.sol`), launch, trade and event surface.
        interface IFlapPortal {
            struct NewTokenV2Params {
                string name; string symbol; string meta; uint8 dexThresh; bytes32 salt;
                uint16 taxRate; uint8 migratorType; address quoteToken; uint256 quoteAmt;
                address beneficiary; bytes permitData;
            }
            struct NewTokenV3Params {
                string name; string symbol; string meta; uint8 dexThresh; bytes32 salt;
                uint16 taxRate; uint8 migratorType; address quoteToken; uint256 quoteAmt;
                address beneficiary; bytes permitData; bytes32 extensionID; bytes extensionData;
            }
            struct NewTokenV4Params {
                string name; string symbol; string meta; uint8 dexThresh; bytes32 salt;
                uint16 taxRate; uint8 migratorType; address quoteToken; uint256 quoteAmt;
                address beneficiary; bytes permitData; bytes32 extensionID; bytes extensionData;
                uint8 dexId; uint8 lpFeeProfile;
            }
            struct NewTokenV5Params {
                string name; string symbol; string meta; uint8 dexThresh; bytes32 salt;
                uint16 taxRate; uint8 migratorType; address quoteToken; uint256 quoteAmt;
                address beneficiary; bytes permitData; bytes32 extensionID; bytes extensionData;
                uint8 dexId; uint8 lpFeeProfile; uint64 taxDuration; uint64 antiFarmerDuration;
                uint16 mktBps; uint16 deflationBps; uint16 dividendBps; uint16 lpBps;
                uint256 minimumShareBalance;
            }
            struct NewTokenV6Params {
                string name; string symbol; string meta; uint8 dexThresh; bytes32 salt;
                uint8 migratorType; address quoteToken; uint256 quoteAmt; address beneficiary;
                bytes permitData; bytes32 extensionID; bytes extensionData; uint8 dexId;
                uint8 lpFeeProfile; uint16 buyTaxRate; uint16 sellTaxRate; uint64 taxDuration;
                uint64 antiFarmerDuration; uint16 mktBps; uint16 deflationBps; uint16 dividendBps;
                uint16 lpBps; uint256 minimumShareBalance; address dividendToken;
                address commissionReceiver; uint8 tokenVersion;
            }
            struct FeeConfig {
                uint8 feeType; uint16 bps; address marketingAddress; address dividendToken;
                uint256 minimumShareBalance;
            }
            struct NewTokenV7Params {
                string name; string symbol; string meta; uint8 dexThresh; bytes32 salt;
                uint8 migratorType; address quoteToken; uint256 quoteAmt; bytes permitData;
                bytes32 extensionID; bytes extensionData; uint8 dexId; uint16 buyTaxRate;
                uint16 sellTaxRate; uint64 taxDuration; uint64 antiFarmerDuration;
                address commissionReceiver; uint8 tokenVersion; FeeConfig[4] feeConfigs;
            }
            struct StageNewTokenV5Params {
                uint8 dexThresh; bytes32 salt; bool isTaxToken; uint8 migratorType;
                address quoteToken; uint8 dexId;
            }
            struct CommitNewTokenV5Params {
                bytes32 salt; uint16 taxRate; string name; string symbol; string meta;
                uint256 quoteAmt; address beneficiary; bytes permitData; uint64 taxDuration;
                uint64 antiFarmerDuration; uint16 mktBps; uint16 deflationBps; uint16 dividendBps;
                uint16 lpBps; uint256 minimumShareBalance;
            }
            struct ExactInputParams {
                address inputToken; address outputToken; uint256 inputAmount;
                uint256 minOutputAmount; bytes permitData;
            }
            struct ExactInputV3Params {
                address inputToken; address outputToken; uint256 inputAmount;
                uint256 minOutputAmount; bytes permitData; bytes extensionData;
            }

            function newTokenV2(NewTokenV2Params params) external payable returns (address token);
            function newTokenV3(NewTokenV3Params params) external payable returns (address token);
            function newTokenV4(NewTokenV4Params params) external payable returns (address token);
            function newTokenV5(NewTokenV5Params params) external payable returns (address token);
            function newTokenV6(NewTokenV6Params params) external payable returns (address token);
            function newTokenV7(NewTokenV7Params params) external payable returns (address token);
            function stageNewTokenV5(StageNewTokenV5Params params) external returns (address token);
            function commitNewTokenV5(CommitNewTokenV5Params params) external payable;
            function buyOnCreation(address token, address recipient, uint256 inputAmount) external payable returns (uint256 amount);
            function buy(address token, address recipient, uint256 minAmount) external payable returns (uint256 amount);
            function sell(address token, uint256 amount, uint256 minEth) external returns (uint256 eth);
            function swapExactInput(ExactInputParams params) external payable returns (uint256 outputAmount);
            function swapExactInputV3(ExactInputV3Params params) external payable returns (uint256 outputAmount);
            function version() external view returns (string);

            event TokenCreated(uint256 ts, address creator, uint256 nonce, address token, string name, string symbol, string meta);
            event TokenBought(uint256 ts, address token, address buyer, uint256 amount, uint256 eth, uint256 fee, uint256 postPrice);
            event TokenSold(uint256 ts, address token, address seller, uint256 amount, uint256 eth, uint256 fee, uint256 postPrice);
            event LaunchedToDEX(address token, address pool, uint256 amount, uint256 eth);
            event TokenVersionSet(address token, uint8 version);
            event TokenCurveSet(address token, address curve, uint256 curveParameter);
            event TokenCurveSetV2(address token, uint256 r, uint256 h, uint256 k);
            event TokenQuoteSet(address token, address quoteToken);
            event TokenMigratorSet(address token, uint8 migratorType);
            event TokenDexSupplyThreshSet(address token, uint256 dexSupplyThresh);
            event FlapTokenTaxSet(address token, uint256 tax);
            event FlapTokenAsymmetricTaxSet(address token, uint256 buyTax, uint256 sellTax);
            event VanityTokenCreated(address token, address creator, address beneficiary);
            event FlapTokenStaged(uint256 ts, address creator, address token);
        }
    }
}

use IFlapPortal as P;
pub use declarations::IFlapPortal;

pub use crate::common::bytecode::{addr, hex_bytes};

pub const SELECTOR_DERIVATION: &str =
    "alloy-sol-types 1.7.3 sol!: SolCall::SELECTOR = keccak256(SIGNATURE)[0..4]";
pub const TOPIC_DERIVATION: &str =
    "alloy-sol-types 1.7.3 sol!: SolEvent::SIGNATURE_HASH = keccak256(SIGNATURE)";

/// `FUNCTION` or `EVENT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecKind {
    Function,
    Event,
}

/// One frozen selector or topic.
#[derive(Clone, Debug)]
pub struct Spec {
    pub kind: SpecKind,
    pub role: &'static str,
    /// The Portal function name of a call (`newTokenV6`, ...).
    pub name: &'static str,
    pub signature: &'static str,
    pub value: Vec<u8>,
}

fn function<T: SolCall>(role: &'static str, name: &'static str) -> Spec {
    Spec {
        kind: SpecKind::Function,
        role,
        name,
        signature: T::SIGNATURE,
        value: T::SELECTOR.to_vec(),
    }
}

fn event<T: SolEvent>(name: &'static str) -> Spec {
    Spec {
        kind: SpecKind::Event,
        role: "EVENT",
        name,
        signature: T::SIGNATURE,
        value: T::SIGNATURE_HASH.to_vec(),
    }
}

/// Every frozen selector and topic of the Portal.
pub fn specs() -> Vec<Spec> {
    vec![
        function::<P::newTokenV2Call>("LAUNCH", "newTokenV2"),
        function::<P::newTokenV3Call>("LAUNCH", "newTokenV3"),
        function::<P::newTokenV4Call>("LAUNCH", "newTokenV4"),
        function::<P::newTokenV5Call>("LAUNCH", "newTokenV5"),
        function::<P::newTokenV6Call>("LAUNCH", "newTokenV6"),
        function::<P::newTokenV7Call>("LAUNCH", "newTokenV7"),
        function::<P::stageNewTokenV5Call>("LAUNCH_TWO_STEP", "stageNewTokenV5"),
        function::<P::commitNewTokenV5Call>("LAUNCH_TWO_STEP", "commitNewTokenV5"),
        function::<P::swapExactInputCall>("BUY", "swapExactInput"),
        function::<P::swapExactInputV3Call>("BUY", "swapExactInputV3"),
        function::<P::buyCall>("BUY", "buy"),
        function::<P::buyOnCreationCall>("BUY", "buyOnCreation"),
        function::<P::sellCall>("SELL", "sell"),
        function::<P::versionCall>("STATE", "version"),
        event::<P::TokenCreated>("TokenCreated"),
        event::<P::TokenBought>("TokenBought"),
        event::<P::TokenSold>("TokenSold"),
        event::<P::LaunchedToDEX>("LaunchedToDEX"),
        event::<P::TokenVersionSet>("TokenVersionSet"),
        event::<P::TokenCurveSet>("TokenCurveSet"),
        event::<P::TokenCurveSetV2>("TokenCurveSetV2"),
        event::<P::TokenQuoteSet>("TokenQuoteSet"),
        event::<P::TokenMigratorSet>("TokenMigratorSet"),
        event::<P::TokenDexSupplyThreshSet>("TokenDexSupplyThreshSet"),
        event::<P::FlapTokenTaxSet>("FlapTokenTaxSet"),
        event::<P::FlapTokenAsymmetricTaxSet>("FlapTokenAsymmetricTaxSet"),
        event::<P::VanityTokenCreated>("VanityTokenCreated"),
        event::<P::FlapTokenStaged>("FlapTokenStaged"),
    ]
}

pub fn token_created_topic() -> B256 {
    P::TokenCreated::SIGNATURE_HASH
}

pub fn token_bought_topic() -> B256 {
    P::TokenBought::SIGNATURE_HASH
}

pub fn token_sold_topic() -> B256 {
    P::TokenSold::SIGNATURE_HASH
}

pub fn launched_to_dex_topic() -> B256 {
    P::LaunchedToDEX::SIGNATURE_HASH
}

pub fn version_call() -> Vec<u8> {
    P::versionCall {}.abi_encode()
}

pub fn version_return(output: &[u8]) -> Result<String, String> {
    P::versionCall::abi_decode_returns(output).map_err(|e| format!("version(): {e}"))
}

/// The Portal function of a selector, if it is a frozen function.
pub fn function_name(selector: &[u8]) -> Option<&'static str> {
    specs()
        .into_iter()
        .find(|s| s.kind == SpecKind::Function && s.value == selector)
        .map(|s| s.name)
}

// ---- launch calldata ---------------------------------------------------------------

/// A decoded direct Portal launch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchCall {
    pub function: &'static str,
    pub salt: B256,
    /// `tokenVersion` of the V6/V7 structs.
    pub token_version: Option<u8>,
    pub quote_token: Option<Address>,
    pub params: Vec<(String, String)>,
}

fn p(name: &str, value: impl ToString) -> (String, String) {
    (name.to_string(), value.to_string())
}

/// Decodes a direct Portal launch call (`newTokenV2..V7`, the two-step
/// V5 calls).
pub fn launch_call(input: &[u8]) -> Result<LaunchCall, String> {
    let selector: [u8; 4] = input
        .get(..4)
        .and_then(|s| s.try_into().ok())
        .ok_or("calldata shorter than a selector")?;
    let e = |e: alloy_sol_types::Error| e.to_string();
    macro_rules! common {
        ($x:expr) => {
            vec![
                p("name", &$x.name),
                p("symbol", &$x.symbol),
                p("meta", &$x.meta),
                p("dexThresh", $x.dexThresh),
                p("salt", $x.salt),
                p("migratorType", $x.migratorType),
                p("quoteToken", addr($x.quoteToken)),
                p("quoteAmt", $x.quoteAmt),
            ]
        };
    }
    Ok(match selector {
        P::newTokenV2Call::SELECTOR => {
            let x = P::newTokenV2Call::abi_decode(input).map_err(e)?.params;
            let mut params = common!(x);
            params.push(p("taxRate", x.taxRate));
            LaunchCall {
                function: "newTokenV2",
                salt: x.salt,
                token_version: None,
                quote_token: Some(x.quoteToken),
                params,
            }
        }
        P::newTokenV3Call::SELECTOR => {
            let x = P::newTokenV3Call::abi_decode(input).map_err(e)?.params;
            let mut params = common!(x);
            params.extend([
                p("taxRate", x.taxRate),
                p("extensionID", x.extensionID),
                p("extensionData.len", x.extensionData.len()),
            ]);
            LaunchCall {
                function: "newTokenV3",
                salt: x.salt,
                token_version: None,
                quote_token: Some(x.quoteToken),
                params,
            }
        }
        P::newTokenV4Call::SELECTOR => {
            let x = P::newTokenV4Call::abi_decode(input).map_err(e)?.params;
            let mut params = common!(x);
            params.extend([
                p("taxRate", x.taxRate),
                p("extensionID", x.extensionID),
                p("extensionData.len", x.extensionData.len()),
                p("dexId", x.dexId),
                p("lpFeeProfile", x.lpFeeProfile),
            ]);
            LaunchCall {
                function: "newTokenV4",
                salt: x.salt,
                token_version: None,
                quote_token: Some(x.quoteToken),
                params,
            }
        }
        P::newTokenV5Call::SELECTOR => {
            let x = P::newTokenV5Call::abi_decode(input).map_err(e)?.params;
            let mut params = common!(x);
            params.extend([
                p("taxRate", x.taxRate),
                p("extensionID", x.extensionID),
                p("extensionData.len", x.extensionData.len()),
                p("dexId", x.dexId),
                p("lpFeeProfile", x.lpFeeProfile),
                p("taxDuration", x.taxDuration),
                p("antiFarmerDuration", x.antiFarmerDuration),
            ]);
            LaunchCall {
                function: "newTokenV5",
                salt: x.salt,
                token_version: None,
                quote_token: Some(x.quoteToken),
                params,
            }
        }
        P::newTokenV6Call::SELECTOR => {
            let x = P::newTokenV6Call::abi_decode(input).map_err(e)?.params;
            let mut params = common!(x);
            params.extend([
                p("beneficiary", addr(x.beneficiary)),
                p("extensionID", x.extensionID),
                p("extensionData.len", x.extensionData.len()),
                p("dexId", x.dexId),
                p("lpFeeProfile", x.lpFeeProfile),
                p("buyTaxRate", x.buyTaxRate),
                p("sellTaxRate", x.sellTaxRate),
                p("taxDuration", x.taxDuration),
                p("antiFarmerDuration", x.antiFarmerDuration),
                p("mktBps", x.mktBps),
                p("deflationBps", x.deflationBps),
                p("dividendBps", x.dividendBps),
                p("lpBps", x.lpBps),
                p("minimumShareBalance", x.minimumShareBalance),
                p("dividendToken", addr(x.dividendToken)),
                p("commissionReceiver", addr(x.commissionReceiver)),
                p("tokenVersion", x.tokenVersion),
            ]);
            LaunchCall {
                function: "newTokenV6",
                salt: x.salt,
                token_version: Some(x.tokenVersion),
                quote_token: Some(x.quoteToken),
                params,
            }
        }
        P::newTokenV7Call::SELECTOR => {
            let x = P::newTokenV7Call::abi_decode(input).map_err(e)?.params;
            let mut params = common!(x);
            params.extend([
                p("extensionID", x.extensionID),
                p("extensionData.len", x.extensionData.len()),
                p("dexId", x.dexId),
                p("buyTaxRate", x.buyTaxRate),
                p("sellTaxRate", x.sellTaxRate),
                p("taxDuration", x.taxDuration),
                p("antiFarmerDuration", x.antiFarmerDuration),
                p("commissionReceiver", addr(x.commissionReceiver)),
                p("tokenVersion", x.tokenVersion),
            ]);
            for (i, f) in x.feeConfigs.iter().enumerate() {
                params.push(p(
                    &format!("feeConfigs[{i}]"),
                    format!(
                        "type {} bps {} marketing {} dividend {}",
                        f.feeType,
                        f.bps,
                        addr(f.marketingAddress),
                        addr(f.dividendToken)
                    ),
                ));
            }
            LaunchCall {
                function: "newTokenV7",
                salt: x.salt,
                token_version: Some(x.tokenVersion),
                quote_token: Some(x.quoteToken),
                params,
            }
        }
        P::stageNewTokenV5Call::SELECTOR => {
            let x = P::stageNewTokenV5Call::abi_decode(input).map_err(e)?.params;
            LaunchCall {
                function: "stageNewTokenV5",
                salt: x.salt,
                token_version: None,
                quote_token: Some(x.quoteToken),
                params: vec![
                    p("salt", x.salt),
                    p("isTaxToken", x.isTaxToken),
                    p("quoteToken", addr(x.quoteToken)),
                    p("dexId", x.dexId),
                ],
            }
        }
        P::commitNewTokenV5Call::SELECTOR => {
            let x = P::commitNewTokenV5Call::abi_decode(input)
                .map_err(e)?
                .params;
            LaunchCall {
                function: "commitNewTokenV5",
                salt: x.salt,
                token_version: None,
                quote_token: None,
                params: vec![
                    p("salt", x.salt),
                    p("taxRate", x.taxRate),
                    p("name", &x.name),
                    p("symbol", &x.symbol),
                    p("quoteAmt", x.quoteAmt),
                ],
            }
        }
        other => {
            return Err(format!(
                "selector {} is not a Portal launch",
                hex_bytes(&other)
            ));
        }
    })
}

/// A decoded direct Portal trade: its function and parameters.
pub type TradeCall = (&'static str, Vec<(String, String)>);

/// Decodes a direct Portal BUY/SELL call into its parameters.
pub fn trade_call(input: &[u8]) -> Result<TradeCall, String> {
    let selector: [u8; 4] = input
        .get(..4)
        .and_then(|s| s.try_into().ok())
        .ok_or("calldata shorter than a selector")?;
    let e = |e: alloy_sol_types::Error| e.to_string();
    Ok(match selector {
        P::swapExactInputCall::SELECTOR => {
            let x = P::swapExactInputCall::abi_decode(input).map_err(e)?.params;
            (
                "swapExactInput",
                vec![
                    p("inputToken", addr(x.inputToken)),
                    p("outputToken", addr(x.outputToken)),
                    p("inputAmount", x.inputAmount),
                    p("minOutputAmount", x.minOutputAmount),
                    p("permitData.len", x.permitData.len()),
                ],
            )
        }
        P::swapExactInputV3Call::SELECTOR => {
            let x = P::swapExactInputV3Call::abi_decode(input)
                .map_err(e)?
                .params;
            (
                "swapExactInputV3",
                vec![
                    p("inputToken", addr(x.inputToken)),
                    p("outputToken", addr(x.outputToken)),
                    p("inputAmount", x.inputAmount),
                    p("minOutputAmount", x.minOutputAmount),
                    p("permitData.len", x.permitData.len()),
                    p("extensionData.len", x.extensionData.len()),
                ],
            )
        }
        P::buyCall::SELECTOR => {
            let x = P::buyCall::abi_decode(input).map_err(e)?;
            (
                "buy",
                vec![
                    p("token", addr(x.token)),
                    p("recipient", addr(x.recipient)),
                    p("minAmount", x.minAmount),
                ],
            )
        }
        P::buyOnCreationCall::SELECTOR => {
            let x = P::buyOnCreationCall::abi_decode(input).map_err(e)?;
            (
                "buyOnCreation",
                vec![
                    p("token", addr(x.token)),
                    p("recipient", addr(x.recipient)),
                    p("inputAmount", x.inputAmount),
                ],
            )
        }
        P::sellCall::SELECTOR => {
            let x = P::sellCall::abi_decode(input).map_err(e)?;
            (
                "sell",
                vec![
                    p("token", addr(x.token)),
                    p("amount", x.amount),
                    p("minEth", x.minEth),
                ],
            )
        }
        other => {
            return Err(format!(
                "selector {} is not a Portal trade",
                hex_bytes(&other)
            ));
        }
    })
}

// ---- events ---------------------------------------------------------------------------

/// A decoded frozen Portal event: its signature, the token it names and its
/// fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub signature: &'static str,
    pub token: Option<Address>,
    pub params: Vec<(String, String)>,
}

/// Decodes `topics`/`data` if topic 0 is a frozen Portal event.
pub fn event_of(topics: &[B256], data: &[u8]) -> Option<Result<Event, String>> {
    let t = topics.first()?;
    let raw = topics.iter().copied();
    macro_rules! ev {
        ($ty:ty, $token:expr, |$x:ident| $params:expr) => {
            Some(
                <$ty>::decode_raw_log(raw, data)
                    .map(|$x| Event {
                        signature: <$ty>::SIGNATURE,
                        token: $token,
                        params: $params,
                    })
                    .map_err(|e| format!("{}: {e}", <$ty>::SIGNATURE)),
            )
        };
    }
    match *t {
        P::TokenCreated::SIGNATURE_HASH => {
            let decoded = P::TokenCreated::decode_raw_log(raw, data);
            Some(
                decoded
                    .map(|x| Event {
                        signature: P::TokenCreated::SIGNATURE,
                        token: Some(x.token),
                        params: vec![
                            p("ts", x.ts),
                            p("creator", addr(x.creator)),
                            p("nonce", x.nonce),
                            p("token", addr(x.token)),
                            p("name", &x.name),
                            p("symbol", &x.symbol),
                            p("meta", &x.meta),
                        ],
                    })
                    .map_err(|e| format!("TokenCreated: {e}")),
            )
        }
        P::TokenBought::SIGNATURE_HASH => {
            let decoded = P::TokenBought::decode_raw_log(raw, data);
            Some(
                decoded
                    .map(|x| Event {
                        signature: P::TokenBought::SIGNATURE,
                        token: Some(x.token),
                        params: vec![
                            p("ts", x.ts),
                            p("token", addr(x.token)),
                            p("buyer", addr(x.buyer)),
                            p("amount", x.amount),
                            p("eth", x.eth),
                            p("fee", x.fee),
                            p("postPrice", x.postPrice),
                        ],
                    })
                    .map_err(|e| format!("TokenBought: {e}")),
            )
        }
        P::TokenSold::SIGNATURE_HASH => {
            let decoded = P::TokenSold::decode_raw_log(raw, data);
            Some(
                decoded
                    .map(|x| Event {
                        signature: P::TokenSold::SIGNATURE,
                        token: Some(x.token),
                        params: vec![
                            p("ts", x.ts),
                            p("token", addr(x.token)),
                            p("seller", addr(x.seller)),
                            p("amount", x.amount),
                            p("eth", x.eth),
                            p("fee", x.fee),
                            p("postPrice", x.postPrice),
                        ],
                    })
                    .map_err(|e| format!("TokenSold: {e}")),
            )
        }
        P::LaunchedToDEX::SIGNATURE_HASH => {
            let decoded = P::LaunchedToDEX::decode_raw_log(raw, data);
            Some(
                decoded
                    .map(|x| Event {
                        signature: P::LaunchedToDEX::SIGNATURE,
                        token: Some(x.token),
                        params: vec![
                            p("token", addr(x.token)),
                            p("pool", addr(x.pool)),
                            p("amount", x.amount),
                            p("eth", x.eth),
                        ],
                    })
                    .map_err(|e| format!("LaunchedToDEX: {e}")),
            )
        }
        P::TokenVersionSet::SIGNATURE_HASH => ev!(P::TokenVersionSet, None, |x| vec![
            p("token", addr(x.token)),
            p("version", x.version)
        ]),
        P::TokenCurveSet::SIGNATURE_HASH => ev!(P::TokenCurveSet, None, |x| vec![
            p("token", addr(x.token)),
            p("curve", addr(x.curve)),
            p("curveParameter", x.curveParameter)
        ]),
        P::TokenCurveSetV2::SIGNATURE_HASH => ev!(P::TokenCurveSetV2, None, |x| vec![
            p("token", addr(x.token)),
            p("r", x.r),
            p("h", x.h),
            p("k", x.k)
        ]),
        P::TokenQuoteSet::SIGNATURE_HASH => ev!(P::TokenQuoteSet, None, |x| vec![
            p("token", addr(x.token)),
            p("quoteToken", addr(x.quoteToken))
        ]),
        P::TokenMigratorSet::SIGNATURE_HASH => ev!(P::TokenMigratorSet, None, |x| vec![
            p("token", addr(x.token)),
            p("migratorType", x.migratorType)
        ]),
        P::TokenDexSupplyThreshSet::SIGNATURE_HASH => {
            ev!(P::TokenDexSupplyThreshSet, None, |x| vec![
                p("token", addr(x.token)),
                p("dexSupplyThresh", x.dexSupplyThresh)
            ])
        }
        P::FlapTokenTaxSet::SIGNATURE_HASH => ev!(P::FlapTokenTaxSet, None, |x| vec![
            p("token", addr(x.token)),
            p("tax", x.tax)
        ]),
        P::FlapTokenAsymmetricTaxSet::SIGNATURE_HASH => {
            ev!(P::FlapTokenAsymmetricTaxSet, None, |x| vec![
                p("token", addr(x.token)),
                p("buyTax", x.buyTax),
                p("sellTax", x.sellTax)
            ])
        }
        P::VanityTokenCreated::SIGNATURE_HASH => ev!(P::VanityTokenCreated, None, |x| vec![
            p("token", addr(x.token)),
            p("creator", addr(x.creator)),
            p("beneficiary", addr(x.beneficiary))
        ]),
        P::FlapTokenStaged::SIGNATURE_HASH => ev!(P::FlapTokenStaged, None, |x| vec![
            p("ts", x.ts),
            p("creator", addr(x.creator)),
            p("token", addr(x.token))
        ]),
        _ => None,
    }
}

/// `(token, version)` of a `TokenVersionSet` log.
pub fn version_set(topics: &[B256], data: &[u8]) -> Option<(Address, u8)> {
    (topics.first() == Some(&P::TokenVersionSet::SIGNATURE_HASH))
        .then(|| P::TokenVersionSet::decode_raw_log(topics.iter().copied(), data).ok())
        .flatten()
        .map(|x| (x.token, x.version))
}

/// `(token, quote)` of a `TokenQuoteSet` log.
pub fn quote_set(topics: &[B256], data: &[u8]) -> Option<(Address, Address)> {
    (topics.first() == Some(&P::TokenQuoteSet::SIGNATURE_HASH))
        .then(|| P::TokenQuoteSet::decode_raw_log(topics.iter().copied(), data).ok())
        .flatten()
        .map(|x| (x.token, x.quoteToken))
}

/// The token of a `TokenCreated` log.
pub fn created_token(topics: &[B256], data: &[u8]) -> Option<Address> {
    (topics.first() == Some(&P::TokenCreated::SIGNATURE_HASH))
        .then(|| P::TokenCreated::decode_raw_log(topics.iter().copied(), data).ok())
        .flatten()
        .map(|x| x.token)
}

/// Canonical Solidity signatures of an interface source: what the archived
/// IPortal files declare, re-derived without a compiler. Struct parameters
/// expand to tuples, enums are `uint8`, `uint`/`int` are 256 bits.
pub mod solidity {
    use std::collections::BTreeMap;

    /// The declarations of one source.
    #[derive(Debug, Default)]
    pub struct Source {
        structs: BTreeMap<String, Vec<String>>,
        pub enums: BTreeMap<String, Vec<String>>,
        /// (is event, name, canonical signature) in source order.
        pub items: Vec<(bool, String, String)>,
    }

    fn strip_comments(src: &str) -> String {
        let mut out = String::with_capacity(src.len());
        let b = src.as_bytes();
        let mut i = 0;
        let mut quote: Option<u8> = None;
        while i < b.len() {
            match (quote, b[i]) {
                (Some(q), c) => {
                    if c == b'\\' && i + 1 < b.len() {
                        out.push(c as char);
                        i += 1;
                    } else if c == q {
                        quote = None;
                    }
                    out.push(b[i] as char);
                    i += 1;
                }
                (None, b'"' | b'\'') => {
                    quote = Some(b[i]);
                    out.push(b[i] as char);
                    i += 1;
                }
                (None, b'/') if b.get(i + 1) == Some(&b'/') => {
                    while i < b.len() && b[i] != b'\n' {
                        i += 1;
                    }
                }
                (None, b'/') if b.get(i + 1) == Some(&b'*') => {
                    i += 2;
                    while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                        i += 1;
                    }
                    i += 2;
                }
                (None, _) => {
                    // Non-ASCII bytes only occur in comments and strings.
                    out.push(if b[i].is_ascii() { b[i] as char } else { ' ' });
                    i += 1;
                }
            }
        }
        out
    }

    /// `body` of every `keyword NAME { body }` (no nested braces).
    fn blocks<'a>(src: &'a str, keyword: &str) -> Vec<(&'a str, &'a str)> {
        let mut out = Vec::new();
        let mut rest = src;
        let pat = format!("{keyword} ");
        while let Some(at) = rest.find(&pat) {
            let before_ok = at == 0 || !rest.as_bytes()[at - 1].is_ascii_alphanumeric();
            let after = &rest[at + pat.len()..];
            let name_end = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(after.len());
            let name = &after[..name_end];
            let tail = after[name_end..].trim_start();
            if before_ok
                && !name.is_empty()
                && tail.starts_with('{')
                && let Some(close) = tail.find('}')
            {
                out.push((name, &tail[1..close]));
            }
            rest = &rest[at + pat.len()..];
        }
        out
    }

    impl Source {
        pub fn parse(src: &str) -> Result<Self, String> {
            let src = strip_comments(src);
            let mut s = Source::default();
            for (name, body) in blocks(&src, "struct") {
                let members = body
                    .split(';')
                    .map(str::trim)
                    .filter(|m| !m.is_empty())
                    .map(|m| m.split_whitespace().next().unwrap_or("").to_string())
                    .collect();
                s.structs.insert(name.to_string(), members);
            }
            for (name, body) in blocks(&src, "enum") {
                let members = body
                    .split(',')
                    .map(str::trim)
                    .filter(|m| !m.is_empty())
                    .map(str::to_string)
                    .collect();
                s.enums.insert(name.to_string(), members);
            }
            for keyword in ["function", "event"] {
                let pat = format!("{keyword} ");
                let mut at = 0;
                while let Some(found) = src[at..].find(&pat) {
                    let start = at + found;
                    at = start + pat.len();
                    if start > 0 && src.as_bytes()[start - 1].is_ascii_alphanumeric() {
                        continue;
                    }
                    let after = &src[at..];
                    let Some(open) = after.find('(') else { break };
                    let name = after[..open].trim();
                    if name.is_empty()
                        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    {
                        continue;
                    }
                    let close = after[open..]
                        .find(')')
                        .ok_or_else(|| format!("{keyword} {name}: unclosed parameter list"))?;
                    let params = &after[open + 1..open + close];
                    let mut types = Vec::new();
                    for param in params.split(',').map(str::trim).filter(|x| !x.is_empty()) {
                        let ty = param.split_whitespace().next().unwrap_or("");
                        types.push(s.canonical(ty, 0)?);
                    }
                    s.items.push((
                        keyword == "event",
                        name.to_string(),
                        format!("{name}({})", types.join(",")),
                    ));
                }
            }
            Ok(s)
        }

        fn canonical(&self, ty: &str, depth: usize) -> Result<String, String> {
            if depth > 8 {
                return Err(format!("{ty}: structs nested too deep"));
            }
            let split = ty.find('[').unwrap_or(ty.len());
            let (base, arrays) = ty.split_at(split);
            let base = base.rsplit('.').next().unwrap_or(base);
            let base = if let Some(members) = self.structs.get(base) {
                let parts = members
                    .iter()
                    .map(|m| self.canonical(m, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                format!("({})", parts.join(","))
            } else if self.enums.contains_key(base) {
                "uint8".into()
            } else {
                match base {
                    "uint" => "uint256".into(),
                    "int" => "int256".into(),
                    other => other.to_string(),
                }
            };
            Ok(format!("{base}{arrays}"))
        }

        /// Whether the source declares `signature` as an event (`true`) or
        /// a function.
        pub fn declares(&self, event: bool, signature: &str) -> bool {
            self.items
                .iter()
                .any(|(e, _, s)| *e == event && s == signature)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{hex, keccak256};

    #[test]
    fn selectors_and_topics_equal_the_independently_known_values() {
        // Archived web3_sha3 reference (2026-09-27 keccak_comparison.json),
        // the production constants and real canonical Portal logs.
        let known = [
            ("newTokenV6", "8cb5772c"),
            ("newTokenV7", "87ef5b30"),
            ("swapExactInput", "ef7ec2e7"),
            ("swapExactInputV3", "4f7edf58"),
            ("buyOnCreation", "fcb5c9e3"),
            ("buy", "153e66e6"),
            ("version", "54fd4d50"),
            (
                "TokenCreated",
                "504e7f360b2e5fe33cbaaae4c593bc55305328341bf79009e43e0e3b7f699603",
            ),
            (
                "TokenBought",
                "a800a2038683844fac66747f771bfdfae862eb28b16bcfa387afa9fbacce8ff7",
            ),
        ];
        let specs = specs();
        for (name, prefix) in known {
            let s = specs.iter().find(|s| s.name == name).unwrap();
            assert!(hex::encode(&s.value).starts_with(prefix), "{name}");
            assert_eq!(
                s.value[..],
                keccak256(s.signature.as_bytes())[..s.value.len()]
            );
        }
        assert_eq!(
            specs.iter().find(|s| s.name == "newTokenV6").unwrap().value,
            darkbot_bsc_trading::venues::flap::abi::NEW_TOKEN_V6_SELECTOR
        );
        assert_eq!(
            specs
                .iter()
                .find(|s| s.name == "swapExactInput")
                .unwrap()
                .value,
            darkbot_bsc_trading::venues::flap::abi::SWAP_EXACT_INPUT_SELECTOR
        );
        let mut seen = std::collections::BTreeSet::new();
        for s in &specs {
            assert!(seen.insert(s.value.clone()), "{}", s.signature);
        }
        assert_eq!(function_name(&hex!("8cb5772c")), Some("newTokenV6"));
    }

    #[test]
    fn the_signature_extractor_expands_structs_enums_and_arrays() {
        let src = r#"
            // struct Fake { uint a; }  (a comment)
            interface I {
                enum Kind { A, B }
                struct Inner { uint16 bps; address who; }
                /* struct Hidden { bool x; } */
                struct Outer { string name; Kind kind; Inner[4] inner; bytes32 salt; }
                event Done(address indexed token, Kind kind);
                function make(Outer calldata params, uint amount) external returns (address);
                function note(string memory s) external; // "function fake(uint)"
            }
        "#;
        let s = solidity::Source::parse(src).unwrap();
        assert!(s.declares(
            false,
            "make((string,uint8,(uint16,address)[4],bytes32),uint256)"
        ));
        assert!(s.declares(true, "Done(address,uint8)"));
        assert!(s.declares(false, "note(string)"));
        assert!(!s.declares(false, "fake(uint256)"));
        assert_eq!(s.enums["Kind"], ["A", "B"]);
    }

    #[test]
    fn calldata_that_is_not_a_launch_or_trade_is_refused() {
        assert!(launch_call(&hex!("8cb5772c")).is_err());
        assert!(launch_call(&hex!("ef7ec2e7")).is_err());
        assert!(trade_call(&hex!("8cb5772c")).is_err());
        assert!(event_of(&[B256::ZERO], &[]).is_none());
    }
}
