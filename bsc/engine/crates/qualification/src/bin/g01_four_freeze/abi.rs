//! Selectors, topics, bytecode facts and calldata/event decoders of the
//! freeze. Every selector and topic is alloy's (`sol!`: `SolCall::SELECTOR`,
//! `SolEvent::SIGNATURE_HASH`), every hash alloy's `keccak256`, every
//! CREATE2 address alloy's `Address::create2`; nothing here hashes by hand.
//! The Solidity declarations are the exact signatures of the archived
//! published ABIs (TokenManager2.lite.json, OpenFour*.json); the freeze
//! re-hashes the archived signatures and compares.

use alloy_primitives::{Address, B256, U256};
use alloy_sol_types::{SolCall, SolEvent, SolType};

pub use crate::common::bytecode::{
    CodeScan, addr, eip1167_target, eip1967_implementation_slot, hex_bytes, predict_clone,
};

use crate::contracts::{CORE, Family, HELPER3, REGISTRY, TOKEN_MANAGER2, TOOLS};

/// The Solidity declarations. alloy generates a constructor with one
/// argument per Solidity parameter, so the argument-count lint is off here.
#[allow(clippy::too_many_arguments)]
mod declarations {
    use alloy_sol_types::sol;

    sol! {
        /// Four Classic TokenManager2 (archived `sources/TokenManager2.lite.json`).
        interface IClassicTokenManager2 {
            event TokenCreate(address creator, address token, uint256 requestId, string name, string symbol, uint256 totalSupply, uint256 launchTime, uint256 launchFee);
            event TokenPurchase(address token, address account, uint256 price, uint256 amount, uint256 cost, uint256 fee, uint256 offers, uint256 funds);
            event TokenSale(address token, address account, uint256 price, uint256 amount, uint256 cost, uint256 fee, uint256 offers, uint256 funds);
            event LiquidityAdded(address base, uint256 offers, address quote, uint256 funds);
            event TradeStop(address token);

            struct TokenInfo {
                address base;
                address quote;
                uint256 template;
                uint256 totalSupply;
                uint256 maxOffers;
                uint256 maxRaising;
                uint256 launchTime;
                uint256 offers;
                uint256 funds;
                uint256 lastPrice;
                uint256 K;
                uint256 T;
                uint256 status;
            }

            function createToken(bytes args, bytes signature) external payable;

            function buyTokenAMAP(address token, uint256 funds, uint256 minAmount) external payable;
            function buyTokenAMAP(address token, address to, uint256 funds, uint256 minAmount) external payable;
            function buyTokenAMAP(uint256 origin, address token, uint256 funds, uint256 minAmount) external payable;
            function buyTokenAMAP(uint256 origin, address token, address to, uint256 funds, uint256 minAmount) external payable;
            function buyToken(address token, uint256 amount, uint256 maxFunds) external payable;
            function buyToken(address token, address to, uint256 amount, uint256 maxFunds) external payable;
            function buyToken(uint256 origin, address token, uint256 amount, uint256 maxFunds) external payable;
            function buyToken(uint256 origin, address token, address to, uint256 amount, uint256 maxFunds) external payable;

            function sellToken(address token, uint256 amount) external;
            function sellToken(address token, uint256 amount, uint256 minFunds) external;
            function sellToken(uint256 origin, address token, uint256 amount) external;
            function sellToken(uint256 origin, address token, uint256 amount, uint256 minFunds) external;
            function sellToken(uint256 origin, address token, uint256 amount, uint256 minFunds, uint256 feeRate, address feeRecipient) external;
            function sellToken(uint256 origin, address token, address from, address to, uint256 amount, uint256 minFunds) external;
            function sellToken(uint256 origin, address token, address from, uint256 amount, uint256 minFunds, uint256 feeRate, address feeRecipient) external;

            function _tokenInfos(address token) external view returns (address base, address quote, uint256 template, uint256 totalSupply, uint256 maxOffers, uint256 maxRaising, uint256 launchTime, uint256 offers, uint256 funds, uint256 lastPrice, uint256 K, uint256 T, uint256 status);
            function _tokenInfoExs(address token) external view returns (address creator, address founder, uint256 reserves);
            function _tokenInfoEx1s(address token) external view returns (uint256 launchFee, uint256 pcFee, uint256 feeSetting, uint256 blockNumber, uint256 extraFee);
            function calcBuyAmount(TokenInfo ti, uint256 funds) external pure returns (uint256);
            function calcBuyCost(TokenInfo ti, uint256 amount) external pure returns (uint256);
            function calcSellCost(TokenInfo ti, uint256 amount) external pure returns (uint256);
            function calcTradingFee(TokenInfo ti, uint256 funds) external view returns (uint256);
            function calcLastPrice(TokenInfo ti) external pure returns (uint256);
            function calcInitialPrice(uint256 maxRaising, uint256 totalSupply, uint256 offers, uint256 reserves) external pure returns (uint256);
        }

        /// Four Classic TokenManagerHelper3: validation only. No archived ABI;
        /// the signatures are checked against the implementation dispatcher.
        interface IClassicHelper3 {
            function getTokenInfo(address token) external view;
            function tryBuy(address token, uint256 amount, uint256 funds) external view;
            function trySell(address token, uint256 amount) external view;
        }

        /// OpenFour Core (archived `openfour/OpenFourCore.json`).
        interface IOpenFourCore {
            event TokenCreated(uint256 requestId, uint256 indexed presetId, address indexed creator, address indexed token, string name, string symbol, uint256 maxSupply, uint256 saleAmount, uint256 raiseAmount, uint256 initialPrice, address quoteAsset, address vault, address curveModule, address tradeModule, address migrateModule, address customData, address tokenModule, string tokenMetaUri, uint256 flags, bytes encodedTags);
            event TradeExecuted(address indexed token, address indexed trader, uint256 indexed presetId, bool isBuy, address quoteAsset, address vault, uint256 requestedTokenAmount, uint256 tokenAmount, uint256 curveQuoteAmount, uint256 traderQuoteAmount, uint256 lastPrice, uint256 slippageLimit, uint256 protocolFee, uint256 taxFee, uint256 devFee, uint256 antiSniperFee, uint256 totalRaised, uint256 remainingForSale);
            event PhaseTransition(address indexed token, uint8 from, uint8 to, address operator);
            event MigrateExecuted(uint256 indexed presetId, address indexed caller, address indexed token, address vault, address migrateModule, bytes8 migrateTagId, uint256 totalRaised, uint256 remainingForSale, bytes32 hookDataHash, uint8 migratedDataVersion, bytes encodedMigratedData);

            function createToken(bytes createArgs, bytes signature) external payable returns (address token);
            function buy(address token, uint256 amount, uint256 maxPayAmount, uint256 options, bytes proof) external payable;
            function buyByBudget(address token, uint256 maxPayAmount, uint256 minAmountOut, uint256 options, bytes proof) external payable;
            function sell(address token, uint256 amount, uint256 minReceiveAmount, uint256 options, bytes proof) external;
            function tokens(address token) external view;
            function version() external view returns (uint32);
            function paused() external view returns (bool);
            function feeRouter() external view returns (address);
            function wrappedNative() external view returns (address);
        }

        /// OpenFour Tools (archived `openfour/OpenFourTools.json`).
        interface IOpenFourTools {
            struct TokenInitParams {
                string name;
                string symbol;
                string tokenUri;
                uint256 maxSupply;
                uint256 saleAmount;
                uint256 raiseAmount;
                address quoteAsset;
                bytes32 tokenSalt;
                bytes tokenParams;
                bytes vaultParams;
                bytes curveParams;
                bytes tradeParams;
                bytes migrateParams;
                bytes customDataParams;
            }

            function estimateBuy(address token, address trader, uint256 amount, uint256 options, bytes proof) external;
            function estimateBuyByBudget(address token, address trader, uint256 maxPayAmount, uint256 options, bytes proof) external;
            function estimateSell(address token, address trader, uint256 amount, uint256 options, bytes proof) external;
            function getCurveLiquiditySnapshot(address token) external view;
            function getCreateTokenSigningPayload(address caller, uint256 requestId, uint256 presetId, uint256 validTimestamp, uint256 createFee, uint256 presaleQuote, bool antiSniperEnabled, TokenInitParams initParams) external view returns (bytes);
        }

        /// OpenFour Registry (archived `openfour/OpenFourRegistry.json`).
        interface IOpenFourRegistry {
            function openFourCore() external view returns (address);
            function openFourTool() external view returns (address);
            function createDeployer() external view returns (address);
            function getPresetTokenImpl(uint256 presetId) external view returns (address);
            function isPresetActive(uint256 presetId) external view returns (bool);
            function isPresetCreateEnabled(uint256 presetId) external view returns (bool);
        }
    }
}

pub use declarations::{
    IClassicHelper3, IClassicTokenManager2, IOpenFourCore, IOpenFourRegistry, IOpenFourTools,
};

use IClassicTokenManager2 as C;
use IOpenFourCore as OC;
use IOpenFourRegistry as OR;
use IOpenFourTools as OT;

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
    pub family: Family,
    pub kind: SpecKind,
    pub role: &'static str,
    /// The contract role whose code is checked.
    pub contract: &'static str,
    pub signature: &'static str,
    pub value: Vec<u8>,
}

fn function<T: SolCall>(family: Family, role: &'static str, contract: &'static str) -> Spec {
    Spec {
        family,
        kind: SpecKind::Function,
        role,
        contract,
        signature: T::SIGNATURE,
        value: T::SELECTOR.to_vec(),
    }
}

fn event<T: SolEvent>(family: Family, role: &'static str, contract: &'static str) -> Spec {
    Spec {
        family,
        kind: SpecKind::Event,
        role,
        contract,
        signature: T::SIGNATURE,
        value: T::SIGNATURE_HASH.to_vec(),
    }
}

/// Every frozen selector and topic of both families.
pub fn specs() -> Vec<Spec> {
    use Family::{Classic as K, OpenFour as O};
    vec![
        function::<C::createTokenCall>(K, "LAUNCH", TOKEN_MANAGER2),
        function::<C::buyTokenAMAP_0Call>(K, "BUY", TOKEN_MANAGER2),
        function::<C::buyTokenAMAP_1Call>(K, "BUY", TOKEN_MANAGER2),
        function::<C::buyTokenAMAP_2Call>(K, "BUY", TOKEN_MANAGER2),
        function::<C::buyTokenAMAP_3Call>(K, "BUY", TOKEN_MANAGER2),
        function::<C::buyToken_0Call>(K, "BUY", TOKEN_MANAGER2),
        function::<C::buyToken_1Call>(K, "BUY", TOKEN_MANAGER2),
        function::<C::buyToken_2Call>(K, "BUY", TOKEN_MANAGER2),
        function::<C::buyToken_3Call>(K, "BUY", TOKEN_MANAGER2),
        function::<C::sellToken_0Call>(K, "SELL", TOKEN_MANAGER2),
        function::<C::sellToken_1Call>(K, "SELL", TOKEN_MANAGER2),
        function::<C::sellToken_2Call>(K, "SELL", TOKEN_MANAGER2),
        function::<C::sellToken_3Call>(K, "SELL", TOKEN_MANAGER2),
        function::<C::sellToken_4Call>(K, "SELL", TOKEN_MANAGER2),
        function::<C::sellToken_5Call>(K, "SELL", TOKEN_MANAGER2),
        function::<C::sellToken_6Call>(K, "SELL", TOKEN_MANAGER2),
        function::<C::_tokenInfosCall>(K, "STATE", TOKEN_MANAGER2),
        function::<C::_tokenInfoExsCall>(K, "STATE", TOKEN_MANAGER2),
        function::<C::_tokenInfoEx1sCall>(K, "STATE", TOKEN_MANAGER2),
        function::<C::calcBuyAmountCall>(K, "ESTIMATE", TOKEN_MANAGER2),
        function::<C::calcBuyCostCall>(K, "ESTIMATE", TOKEN_MANAGER2),
        function::<C::calcSellCostCall>(K, "ESTIMATE", TOKEN_MANAGER2),
        function::<C::calcTradingFeeCall>(K, "ESTIMATE", TOKEN_MANAGER2),
        function::<C::calcLastPriceCall>(K, "ESTIMATE", TOKEN_MANAGER2),
        function::<C::calcInitialPriceCall>(K, "ESTIMATE", TOKEN_MANAGER2),
        function::<IClassicHelper3::getTokenInfoCall>(K, "VALIDATION", HELPER3),
        function::<IClassicHelper3::tryBuyCall>(K, "VALIDATION", HELPER3),
        function::<IClassicHelper3::trySellCall>(K, "VALIDATION", HELPER3),
        event::<C::TokenCreate>(K, "LAUNCH_EVENT", TOKEN_MANAGER2),
        event::<C::TokenPurchase>(K, "BUY_EVENT", TOKEN_MANAGER2),
        event::<C::TokenSale>(K, "SELL_EVENT", TOKEN_MANAGER2),
        event::<C::LiquidityAdded>(K, "LIFECYCLE_EVENT", TOKEN_MANAGER2),
        event::<C::TradeStop>(K, "LIFECYCLE_EVENT", TOKEN_MANAGER2),
        function::<OC::createTokenCall>(O, "LAUNCH", CORE),
        function::<OC::buyCall>(O, "BUY", CORE),
        function::<OC::buyByBudgetCall>(O, "BUY", CORE),
        function::<OC::sellCall>(O, "SELL", CORE),
        function::<OC::tokensCall>(O, "STATE", CORE),
        function::<OC::versionCall>(O, "STATE", CORE),
        function::<OC::pausedCall>(O, "STATE", CORE),
        function::<OC::feeRouterCall>(O, "GETTER", CORE),
        function::<OC::wrappedNativeCall>(O, "GETTER", CORE),
        function::<OT::estimateBuyCall>(O, "ESTIMATE", TOOLS),
        function::<OT::estimateBuyByBudgetCall>(O, "ESTIMATE", TOOLS),
        function::<OT::estimateSellCall>(O, "ESTIMATE", TOOLS),
        function::<OT::getCurveLiquiditySnapshotCall>(O, "STATE", TOOLS),
        function::<OT::getCreateTokenSigningPayloadCall>(O, "CREATE_HELPER", TOOLS),
        function::<OR::openFourCoreCall>(O, "GETTER", REGISTRY),
        function::<OR::openFourToolCall>(O, "GETTER", REGISTRY),
        function::<OR::createDeployerCall>(O, "GETTER", REGISTRY),
        function::<OR::getPresetTokenImplCall>(O, "GETTER", REGISTRY),
        function::<OR::isPresetActiveCall>(O, "STATE", REGISTRY),
        function::<OR::isPresetCreateEnabledCall>(O, "STATE", REGISTRY),
        event::<OC::TokenCreated>(O, "LAUNCH_EVENT", CORE),
        event::<OC::TradeExecuted>(O, "BUY_EVENT", CORE),
        event::<OC::PhaseTransition>(O, "LIFECYCLE_EVENT", CORE),
        event::<OC::MigrateExecuted>(O, "LIFECYCLE_EVENT", CORE),
    ]
}

/// The launch selector and topic and the BUY topic of a family.
pub fn launch_selector(family: Family) -> [u8; 4] {
    match family {
        Family::Classic => C::createTokenCall::SELECTOR,
        Family::OpenFour => OC::createTokenCall::SELECTOR,
    }
}

pub fn launch_topic(family: Family) -> B256 {
    match family {
        Family::Classic => C::TokenCreate::SIGNATURE_HASH,
        Family::OpenFour => OC::TokenCreated::SIGNATURE_HASH,
    }
}

pub fn buy_topic(family: Family) -> B256 {
    match family {
        Family::Classic => C::TokenPurchase::SIGNATURE_HASH,
        Family::OpenFour => OC::TradeExecuted::SIGNATURE_HASH,
    }
}

// ---- calldata --------------------------------------------------------------------

fn word(bytes: &[u8], index: usize) -> Option<B256> {
    bytes.get(index * 32..index * 32 + 32).map(B256::from_slice)
}

fn u256_word(bytes: &[u8], index: usize) -> Option<U256> {
    word(bytes, index).map(|w| U256::from_be_bytes(w.0))
}

/// A decoded create call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    pub request_id: U256,
    /// OpenFour only.
    pub preset_id: Option<U256>,
    pub salt: B256,
    pub salt_location: &'static str,
    pub params: Vec<(String, String)>,
    /// OpenFour `TokenInitParams` (name, symbol, tokenUri, maxSupply,
    /// saleAmount, raiseAmount, quoteAsset).
    pub init: Option<InitView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitView {
    pub name: String,
    pub symbol: String,
    pub token_uri: String,
    pub max_supply: U256,
    pub sale_amount: U256,
    pub raise_amount: U256,
    pub quote_asset: Address,
}

/// Classic `createToken(bytes args, bytes signature)`: `args` is one ABI
/// tuple (word 0 = its offset 0x20); its head starts with `requestId` and the
/// CREATE2 salt. Only these two positions are claimed: requestId is checked
/// against the event, the salt by reproducing the token address.
pub const CLASSIC_SALT_LOCATION: &str =
    "createToken.args word 2 (tuple head field 1, after requestId)";

pub fn classic_launch(input: &[u8]) -> Result<Launch, String> {
    let call = C::createTokenCall::abi_decode(input)
        .map_err(|e| format!("not createToken(bytes,bytes): {e}"))?;
    let args = call.args.as_ref();
    if u256_word(args, 0) != Some(U256::from(32u8)) {
        return Err("createToken.args does not start with the tuple offset 0x20".into());
    }
    let request_id = u256_word(args, 1).ok_or("createToken.args too short")?;
    let salt = word(args, 2).ok_or("createToken.args too short")?;
    Ok(Launch {
        request_id,
        preset_id: None,
        salt,
        salt_location: CLASSIC_SALT_LOCATION,
        params: vec![
            ("args.len".into(), args.len().to_string()),
            ("args.requestId".into(), request_id.to_string()),
            ("args.salt".into(), salt.to_string()),
            ("signature.len".into(), call.signature.len().to_string()),
        ],
        init: None,
    })
}

/// OpenFour `createToken(bytes createArgs, bytes signature)`: `createArgs`
/// is one ABI tuple whose head holds seven static words (requestId and
/// presetId first) and the offset of the published `TokenInitParams`
/// (OpenFourTools.getCreateTokenSigningPayload), whose `tokenSalt` is the
/// CREATE2 salt.
pub const OPENFOUR_ARGS_HEAD_WORDS: usize = 8;
pub const OPENFOUR_SALT_LOCATION: &str =
    "createToken.createArgs -> TokenInitParams.tokenSalt (head word 8 = offset 0x100)";

pub fn openfour_launch(input: &[u8]) -> Result<Launch, String> {
    let call = OC::createTokenCall::abi_decode(input)
        .map_err(|e| format!("not createToken(bytes,bytes): {e}"))?;
    let args = call.createArgs.as_ref();
    if u256_word(args, 0) != Some(U256::from(32u8)) {
        return Err("createArgs does not start with the tuple offset 0x20".into());
    }
    let head = &args[32..];
    let request_id = u256_word(head, 0).ok_or("createArgs too short")?;
    let preset_id = u256_word(head, 1).ok_or("createArgs too short")?;
    let offset = u256_word(head, OPENFOUR_ARGS_HEAD_WORDS - 1).ok_or("createArgs too short")?;
    if offset != U256::from(OPENFOUR_ARGS_HEAD_WORDS * 32) {
        return Err(format!(
            "TokenInitParams offset {offset} is not {} (other head layout)",
            OPENFOUR_ARGS_HEAD_WORDS * 32
        ));
    }
    let init =
        <OT::TokenInitParams as SolType>::abi_decode_params(&head[OPENFOUR_ARGS_HEAD_WORDS * 32..])
            .map_err(|e| format!("TokenInitParams: {e}"))?;
    let salt = init.tokenSalt;
    let mut params = vec![
        ("createArgs.len".to_string(), args.len().to_string()),
        ("createArgs.requestId".into(), request_id.to_string()),
        ("createArgs.presetId".into(), preset_id.to_string()),
    ];
    for i in 2..OPENFOUR_ARGS_HEAD_WORDS - 1 {
        if let Some(w) = word(head, i) {
            params.push((format!("createArgs.head[{i}] (unnamed)"), w.to_string()));
        }
    }
    params.extend([
        ("initParams.name".into(), init.name.clone()),
        ("initParams.symbol".into(), init.symbol.clone()),
        ("initParams.tokenUri".into(), init.tokenUri.clone()),
        ("initParams.maxSupply".into(), init.maxSupply.to_string()),
        ("initParams.saleAmount".into(), init.saleAmount.to_string()),
        (
            "initParams.raiseAmount".into(),
            init.raiseAmount.to_string(),
        ),
        ("initParams.quoteAsset".into(), addr(init.quoteAsset)),
        ("initParams.tokenSalt".into(), salt.to_string()),
        ("signature.len".into(), call.signature.len().to_string()),
    ]);
    Ok(Launch {
        request_id,
        preset_id: Some(preset_id),
        salt,
        salt_location: OPENFOUR_SALT_LOCATION,
        params,
        init: Some(InitView {
            name: init.name,
            symbol: init.symbol,
            token_uri: init.tokenUri,
            max_supply: init.maxSupply,
            sale_amount: init.saleAmount,
            raise_amount: init.raiseAmount,
            quote_asset: init.quoteAsset,
        }),
    })
}

/// A decoded BUY call: its signature, parameters, token and explicit
/// recipient (`to`), if the variant has one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Buy {
    pub signature: &'static str,
    pub token: Address,
    pub recipient: Option<Address>,
    pub params: Vec<(String, String)>,
}

fn p(name: &str, value: impl ToString) -> (String, String) {
    (name.to_string(), value.to_string())
}

pub fn classic_buy(input: &[u8]) -> Result<Buy, String> {
    let selector: [u8; 4] = input
        .get(..4)
        .and_then(|s| s.try_into().ok())
        .ok_or("calldata shorter than a selector")?;
    let e = |e: alloy_sol_types::Error| e.to_string();
    Ok(match selector {
        C::buyTokenAMAP_0Call::SELECTOR => {
            let c = C::buyTokenAMAP_0Call::abi_decode(input).map_err(e)?;
            Buy {
                signature: C::buyTokenAMAP_0Call::SIGNATURE,
                token: c.token,
                recipient: None,
                params: vec![
                    p("token", addr(c.token)),
                    p("funds", c.funds),
                    p("minAmount", c.minAmount),
                ],
            }
        }
        C::buyTokenAMAP_1Call::SELECTOR => {
            let c = C::buyTokenAMAP_1Call::abi_decode(input).map_err(e)?;
            Buy {
                signature: C::buyTokenAMAP_1Call::SIGNATURE,
                token: c.token,
                recipient: Some(c.to),
                params: vec![
                    p("token", addr(c.token)),
                    p("to", addr(c.to)),
                    p("funds", c.funds),
                    p("minAmount", c.minAmount),
                ],
            }
        }
        C::buyTokenAMAP_2Call::SELECTOR => {
            let c = C::buyTokenAMAP_2Call::abi_decode(input).map_err(e)?;
            Buy {
                signature: C::buyTokenAMAP_2Call::SIGNATURE,
                token: c.token,
                recipient: None,
                params: vec![
                    p("origin", c.origin),
                    p("token", addr(c.token)),
                    p("funds", c.funds),
                    p("minAmount", c.minAmount),
                ],
            }
        }
        C::buyTokenAMAP_3Call::SELECTOR => {
            let c = C::buyTokenAMAP_3Call::abi_decode(input).map_err(e)?;
            Buy {
                signature: C::buyTokenAMAP_3Call::SIGNATURE,
                token: c.token,
                recipient: Some(c.to),
                params: vec![
                    p("origin", c.origin),
                    p("token", addr(c.token)),
                    p("to", addr(c.to)),
                    p("funds", c.funds),
                    p("minAmount", c.minAmount),
                ],
            }
        }
        C::buyToken_0Call::SELECTOR => {
            let c = C::buyToken_0Call::abi_decode(input).map_err(e)?;
            Buy {
                signature: C::buyToken_0Call::SIGNATURE,
                token: c.token,
                recipient: None,
                params: vec![
                    p("token", addr(c.token)),
                    p("amount", c.amount),
                    p("maxFunds", c.maxFunds),
                ],
            }
        }
        C::buyToken_1Call::SELECTOR => {
            let c = C::buyToken_1Call::abi_decode(input).map_err(e)?;
            Buy {
                signature: C::buyToken_1Call::SIGNATURE,
                token: c.token,
                recipient: Some(c.to),
                params: vec![
                    p("token", addr(c.token)),
                    p("to", addr(c.to)),
                    p("amount", c.amount),
                    p("maxFunds", c.maxFunds),
                ],
            }
        }
        C::buyToken_2Call::SELECTOR => {
            let c = C::buyToken_2Call::abi_decode(input).map_err(e)?;
            Buy {
                signature: C::buyToken_2Call::SIGNATURE,
                token: c.token,
                recipient: None,
                params: vec![
                    p("origin", c.origin),
                    p("token", addr(c.token)),
                    p("amount", c.amount),
                    p("maxFunds", c.maxFunds),
                ],
            }
        }
        C::buyToken_3Call::SELECTOR => {
            let c = C::buyToken_3Call::abi_decode(input).map_err(e)?;
            Buy {
                signature: C::buyToken_3Call::SIGNATURE,
                token: c.token,
                recipient: Some(c.to),
                params: vec![
                    p("origin", c.origin),
                    p("token", addr(c.token)),
                    p("to", addr(c.to)),
                    p("amount", c.amount),
                    p("maxFunds", c.maxFunds),
                ],
            }
        }
        other => {
            return Err(format!(
                "selector {} is not a Classic BUY",
                hex_bytes(&other)
            ));
        }
    })
}

pub fn openfour_buy(input: &[u8]) -> Result<Buy, String> {
    let selector: [u8; 4] = input
        .get(..4)
        .and_then(|s| s.try_into().ok())
        .ok_or("calldata shorter than a selector")?;
    let e = |e: alloy_sol_types::Error| e.to_string();
    Ok(match selector {
        OC::buyCall::SELECTOR => {
            let c = OC::buyCall::abi_decode(input).map_err(e)?;
            Buy {
                signature: OC::buyCall::SIGNATURE,
                token: c.token,
                recipient: None,
                params: vec![
                    p("token", addr(c.token)),
                    p("amount", c.amount),
                    p("maxPayAmount", c.maxPayAmount),
                    p("options", c.options),
                    p("proof.len", c.proof.len()),
                ],
            }
        }
        OC::buyByBudgetCall::SELECTOR => {
            let c = OC::buyByBudgetCall::abi_decode(input).map_err(e)?;
            Buy {
                signature: OC::buyByBudgetCall::SIGNATURE,
                token: c.token,
                recipient: None,
                params: vec![
                    p("token", addr(c.token)),
                    p("maxPayAmount", c.maxPayAmount),
                    p("minAmountOut", c.minAmountOut),
                    p("options", c.options),
                    p("proof.len", c.proof.len()),
                ],
            }
        }
        other => {
            return Err(format!(
                "selector {} is not an OpenFour BUY",
                hex_bytes(&other)
            ));
        }
    })
}

// ---- events ----------------------------------------------------------------------

/// The facts of a create event the freeze checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Created {
    pub signature: &'static str,
    pub creator: Address,
    pub token: Address,
    pub request_id: U256,
    pub preset_id: Option<U256>,
    pub name: String,
    pub symbol: String,
    /// OpenFour: maxSupply, saleAmount, raiseAmount, quoteAsset, tokenMetaUri.
    pub init: Option<InitView>,
}

pub fn created(family: Family, topics: &[B256], data: &[u8]) -> Result<Created, String> {
    match family {
        Family::Classic => {
            let ev = C::TokenCreate::decode_raw_log(topics.iter().copied(), data)
                .map_err(|e| format!("TokenCreate: {e}"))?;
            Ok(Created {
                signature: C::TokenCreate::SIGNATURE,
                creator: ev.creator,
                token: ev.token,
                request_id: ev.requestId,
                preset_id: None,
                name: ev.name,
                symbol: ev.symbol,
                init: None,
            })
        }
        Family::OpenFour => {
            let ev = OC::TokenCreated::decode_raw_log(topics.iter().copied(), data)
                .map_err(|e| format!("TokenCreated: {e}"))?;
            Ok(Created {
                signature: OC::TokenCreated::SIGNATURE,
                creator: ev.creator,
                token: ev.token,
                request_id: ev.requestId,
                preset_id: Some(ev.presetId),
                name: ev.name.clone(),
                symbol: ev.symbol.clone(),
                init: Some(InitView {
                    name: ev.name,
                    symbol: ev.symbol,
                    token_uri: ev.tokenMetaUri,
                    max_supply: ev.maxSupply,
                    sale_amount: ev.saleAmount,
                    raise_amount: ev.raiseAmount,
                    quote_asset: ev.quoteAsset,
                }),
            })
        }
    }
}

/// A BUY event: token, buyer and whether it is a buy at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bought {
    pub signature: &'static str,
    pub token: Address,
    pub account: Address,
    pub is_buy: bool,
}

pub fn bought(family: Family, topics: &[B256], data: &[u8]) -> Result<Bought, String> {
    match family {
        Family::Classic => {
            let ev = C::TokenPurchase::decode_raw_log(topics.iter().copied(), data)
                .map_err(|e| format!("TokenPurchase: {e}"))?;
            Ok(Bought {
                signature: C::TokenPurchase::SIGNATURE,
                token: ev.token,
                account: ev.account,
                is_buy: true,
            })
        }
        Family::OpenFour => {
            let ev = OC::TradeExecuted::decode_raw_log(topics.iter().copied(), data)
                .map_err(|e| format!("TradeExecuted: {e}"))?;
            Ok(Bought {
                signature: OC::TradeExecuted::SIGNATURE,
                token: ev.token,
                account: ev.trader,
                is_buy: ev.isBuy,
            })
        }
    }
}

// ---- read-only calls -------------------------------------------------------------

/// One state read the freeze lists, with the calldata of its probe (none
/// when the probe needs inputs the freeze does not have).
#[derive(Clone, Debug)]
pub struct Read {
    pub contract: &'static str,
    pub signature: &'static str,
    pub selector: [u8; 4],
    pub purpose: &'static str,
    pub probe: Option<Vec<u8>>,
    pub semantics: &'static str,
}

fn read<T: SolCall>(
    contract: &'static str,
    purpose: &'static str,
    probe: Option<T>,
    semantics: &'static str,
) -> Read {
    Read {
        contract,
        signature: T::SIGNATURE,
        selector: T::SELECTOR,
        purpose,
        probe: probe.map(|c| c.abi_encode()),
        semantics,
    }
}

const ECONOMICS_UNKNOWN: &str =
    "read surface only; economic meaning of the output UNKNOWN (Phase G)";
const STATE_RAW: &str = "raw state words; field meaning from the published ABI names only";

/// Classic: TokenManager2 state/quote reads and the Helper3 validation
/// reads, probed with `token` and `funds` (`ti` from a `_tokenInfos` probe).
pub fn classic_reads(token: Option<Address>, funds: U256, ti: Option<C::TokenInfo>) -> Vec<Read> {
    let t = token;
    vec![
        read(
            TOKEN_MANAGER2,
            "STATE",
            t.map(|token| C::_tokenInfosCall { token }),
            STATE_RAW,
        ),
        read(
            TOKEN_MANAGER2,
            "STATE",
            t.map(|token| C::_tokenInfoExsCall { token }),
            STATE_RAW,
        ),
        read(
            TOKEN_MANAGER2,
            "STATE",
            t.map(|token| C::_tokenInfoEx1sCall { token }),
            STATE_RAW,
        ),
        read(
            TOKEN_MANAGER2,
            "QUOTE",
            ti.clone().map(|ti| C::calcBuyAmountCall { ti, funds }),
            ECONOMICS_UNKNOWN,
        ),
        read::<C::calcBuyCostCall>(TOKEN_MANAGER2, "QUOTE", None, ECONOMICS_UNKNOWN),
        read::<C::calcSellCostCall>(TOKEN_MANAGER2, "QUOTE", None, ECONOMICS_UNKNOWN),
        read(
            TOKEN_MANAGER2,
            "QUOTE",
            ti.clone().map(|ti| C::calcTradingFeeCall { ti, funds }),
            ECONOMICS_UNKNOWN,
        ),
        read(
            TOKEN_MANAGER2,
            "QUOTE",
            ti.map(|ti| C::calcLastPriceCall { ti }),
            ECONOMICS_UNKNOWN,
        ),
        read::<C::calcInitialPriceCall>(TOKEN_MANAGER2, "QUOTE", None, ECONOMICS_UNKNOWN),
        read(
            HELPER3,
            "VALIDATION",
            t.map(|token| IClassicHelper3::getTokenInfoCall { token }),
            "validation only: word 0 version, word 1 token manager (published Helper3 docs)",
        ),
        read(
            HELPER3,
            "VALIDATION",
            t.map(|token| IClassicHelper3::tryBuyCall {
                token,
                amount: U256::ZERO,
                funds,
            }),
            "validation only, never a hot-path dependency; output meaning UNKNOWN",
        ),
    ]
}

/// OpenFour: Core/Tools quote and state reads and the Registry mode reads.
pub fn openfour_reads(token: Option<Address>, preset: Option<U256>, funds: U256) -> Vec<Read> {
    vec![
        read(
            CORE,
            "STATE",
            token.map(|token| OC::tokensCall { token }),
            STATE_RAW,
        ),
        read(
            CORE,
            "STATE",
            Some(OC::versionCall {}),
            "uint32 version label of Core",
        ),
        read(CORE, "STATE", Some(OC::pausedCall {}), "bool: Core paused"),
        read(
            TOOLS,
            "QUOTE",
            token.map(|token| OT::getCurveLiquiditySnapshotCall { token }),
            ECONOMICS_UNKNOWN,
        ),
        read::<OT::estimateBuyCall>(TOOLS, "QUOTE", None, ECONOMICS_UNKNOWN),
        read(
            TOOLS,
            "QUOTE",
            token.map(|token| OT::estimateBuyByBudgetCall {
                token,
                trader: Address::ZERO,
                maxPayAmount: funds,
                options: U256::ZERO,
                proof: Default::default(),
            }),
            "nonpayable estimate, called statically; economic meaning UNKNOWN (Phase G)",
        ),
        read::<OT::estimateSellCall>(TOOLS, "QUOTE", None, ECONOMICS_UNKNOWN),
        read(
            REGISTRY,
            "MODE",
            preset.map(|p| OR::isPresetActiveCall { presetId: p }),
            "bool: preset active",
        ),
        read(
            REGISTRY,
            "MODE",
            preset.map(|p| OR::isPresetCreateEnabledCall { presetId: p }),
            "bool: preset create enabled",
        ),
        read(
            REGISTRY,
            "DERIVATION",
            preset.map(|p| OR::getPresetTokenImplCall { presetId: p }),
            "address: token implementation the preset clones",
        ),
        read(
            REGISTRY,
            "DERIVATION",
            Some(OR::createDeployerCall {}),
            "address: create deployer",
        ),
    ]
}

pub fn token_infos_returns(output: &[u8]) -> Option<C::TokenInfo> {
    let r = C::_tokenInfosCall::abi_decode_returns(output).ok()?;
    Some(C::TokenInfo {
        base: r.base,
        quote: r.quote,
        template: r.template,
        totalSupply: r.totalSupply,
        maxOffers: r.maxOffers,
        maxRaising: r.maxRaising,
        launchTime: r.launchTime,
        offers: r.offers,
        funds: r.funds,
        lastPrice: r.lastPrice,
        K: r.K,
        T: r.T,
        status: r.status,
    })
}

/// Zero-argument address getters (the Registry resolution).
pub fn getter_call(signature: &str) -> Option<Vec<u8>> {
    Some(match signature {
        "openFourCore()" => OR::openFourCoreCall {}.abi_encode(),
        "openFourTool()" => OR::openFourToolCall {}.abi_encode(),
        "createDeployer()" => OR::createDeployerCall {}.abi_encode(),
        _ => return None,
    })
}

pub fn preset_token_impl_call(preset: U256) -> Vec<u8> {
    OR::getPresetTokenImplCall { presetId: preset }.abi_encode()
}

/// A returned ABI address word (upper 12 bytes zero).
pub fn address_return(output: &[u8]) -> Result<Address, String> {
    if output.len() != 32 || output[..12] != [0u8; 12] {
        return Err(format!("not one address word ({} bytes)", output.len()));
    }
    Ok(Address::from_slice(&output[12..]))
}

pub fn u256_return(output: &[u8], index: usize) -> Option<U256> {
    u256_word(output, index)
}

pub fn version_return(output: &[u8]) -> Result<u32, String> {
    OC::versionCall::abi_decode_returns(output).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{hex, keccak256};
    use std::collections::BTreeSet;

    #[test]
    fn selectors_and_topics_equal_the_independently_known_values() {
        let known: [(&str, &str); 9] = [
            ("createToken(bytes,bytes)", "519ebb10"),
            ("buyTokenAMAP(address,uint256,uint256)", "87f27655"),
            ("buyTokenAMAP(address,address,uint256,uint256)", "7f79f6df"),
            ("buyToken(address,uint256,uint256)", "e671499b"),
            ("buyToken(address,address,uint256,uint256)", "7d17ff3d"),
            // Archived web3_sha3 reference (keccak_comparison.json).
            (
                "TokenCreate(address,address,uint256,string,string,uint256,uint256,uint256)",
                "396d5e902b675b032348d3d2e9517ee8f0c4a926603fbc075d3d282ff00cad20",
            ),
            (
                "TokenPurchase(address,address,uint256,uint256,uint256,uint256,uint256,uint256)",
                "7db52723",
            ),
            // Topics of real canonical OpenFour logs (tx 0x94f56a88..., block
            // 129837633, log indices 584 and 596).
            (
                "TokenCreated(uint256,uint256,address,address,string,string,uint256,uint256,uint256,uint256,address,address,address,address,address,address,address,string,uint256,bytes)",
                "7dae4dcd1b20eb0445e7cc1d02b3e2e7f32fa63081a0e764732c269f2b8f228f",
            ),
            (
                "TradeExecuted(address,address,uint256,bool,address,address,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256)",
                "8728ba77a8dc0cbf185f23fe65b61fc658a37975e027062c3114394222a10f3f",
            ),
        ];
        let specs = specs();
        for (signature, prefix) in known {
            let spec = specs
                .iter()
                .find(|s| s.signature == signature)
                .unwrap_or_else(|| panic!("{signature} not frozen"));
            assert!(hex::encode(&spec.value).starts_with(prefix), "{signature}");
            // The sol! constant is alloy's keccak256 of the same string.
            let full = keccak256(signature.as_bytes());
            assert_eq!(spec.value[..], full[..spec.value.len()], "{signature}");
        }
        assert_eq!(
            launch_selector(Family::Classic),
            launch_selector(Family::OpenFour)
        );
        assert_eq!(
            eip1967_implementation_slot().to_string(),
            "0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc"
        );
        // Selectors are unique per contract.
        let mut seen = BTreeSet::new();
        for s in &specs {
            assert!(
                seen.insert((s.contract, s.value.clone())),
                "{}",
                s.signature
            );
        }
    }

    #[test]
    fn calldata_that_is_not_a_create_call_is_refused() {
        assert!(classic_launch(&hex!("519ebb10")).is_err());
        assert!(openfour_launch(&hex!("87f27655")).is_err());
        assert!(classic_buy(&hex!("519ebb10")).is_err());
        assert!(openfour_buy(&[0x01]).is_err());
    }
}
