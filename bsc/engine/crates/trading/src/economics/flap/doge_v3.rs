//! Direct BUY of the historical DOGE-quoted launch (G0.3 fixture):
//! arithmetic for its single witnessed state, disabled at runtime.
//!
//! Port of the Gate-0 `g03` computation with identical operation order and
//! typed errors instead of assertions. The buyer pays native BNB; the Portal
//! converts it to DOGE through one PancakeSwap-V3 segment and buys on the
//! token's bonding curve, whose `r` and `k` derive from the launch's oracle
//! price.
//!
//! ```text
//! net        = floor(native * 997500 / 1000000)
//! sqrt_next  = sqrt + floor(net * 2^96 / liquidity)
//! quote      = floor(floor((liquidity << 96) * (sqrt_next - sqrt) / sqrt_next) / sqrt)
//! fees       = split(quote, 100, 100)
//! r          = floor(3837e18 * 1e18 / oracle_price)
//! k          = floor(r * (1e27 + 107036751e18) / 1e18)
//! creator    = quote_amt - split(quote_amt, 100, 100).combined
//! output     = supply((creator + quote - fees.combined) * 1e10)
//!              - supply(creator * 1e10)
//! ```
//!
//! # Why this profile cannot run
//!
//! [`quote_doge_fixture`] requires an [`OracleAdmissionProof`]. That type
//! stands for a verified oracle signature and signer role. No Rust
//! implementation of that verification exists (in Gate 0 it ran only inside
//! the EVM verifier), so the type has no constructor outside this crate's
//! tests and the function cannot be called by a runtime.
//!
//! # Scope: one state, one amount
//!
//! The evidence covers exactly one execution: the launch of block
//! 124415357, caller `0xb6864c01…4c90`, 0.15 BNB, parent state
//! `0x854055a2…1104`, one V3 segment. It does not qualify other amounts,
//! callers, pool states or launches. [`quote_doge_fixture`] therefore
//! rejects every input that differs from that execution, including a
//! smaller amount inside the same segment.
//!
//! # Input contract (master plan §5)
//!
//! | Input | Source | Witness | Invalidated by |
//! |---|---|---|---|
//! | launch fields, oracle price, creator amount | launch calldata | G0.3 sequential EVM execution | launch replacement, reorg |
//! | oracle signature and signer role | extension tuple, role storage | EVM verifier only | **not verifiable in Rust: profile disabled** |
//! | caller | sender policy | G0.3 (historical direct caller) | sender or exemption change |
//! | pool identity, fee tier | converter configuration | G0.2 buy-path analysis (pool `0xac109c80…017f`, 2500 ppm) | converter or registry update |
//! | pool price, liquidity, segment bound | V3 pool state | G0.3 parent state | any swap or liquidity change, reorg |
//! | parent block | canonical chain | G0.3 `actual_parent_block_hash` | reorg |
//!
//! The recorded fee tier was read from latest state, not from the
//! historical pre-state; the arithmetic's 0.25 % pool fee is consistent
//! with it.

use alloy_primitives::{Address, B256, U256};

use super::QuoteError;
use super::curve::{Curve, TOTAL_SUPPLY, WAD, supply};
use crate::economics::arith::{add, mul, mul_div_down, narrow, sub, wide};
use crate::economics::fees::split_fee;
use crate::venues::flap::abi::{ExtensionTuple, LaunchParamsV6, TOKEN_VERSION_TAXED_V3};
use crate::venues::flap::profile::{DOGE_EXTENSION_ID, DOGE_TOKEN};

/// Buy and sell tax of the fixture launch, in basis points.
const TAX_BPS: u16 = 100;
/// Protocol fee of the fixture, in basis points.
const PROTOCOL_FEE_BPS: u16 = 100;
const POOL_FEE_NUMERATOR: u64 = 997_500;
const POOL_FEE_DENOMINATOR: u64 = 1_000_000;
const CURVE_R_NUMERATOR: u128 = 3_837_000_000_000_000_000_000;
const CURVE_H: u128 = 107_036_751_000_000_000_000_000_000;
/// DOGE has 8 decimals; the curve reserve is in 18-decimal units.
const QUOTE_SCALE: u64 = 10_000_000_000;

// Witnessed launch mode (decoded launch of block 124415357).
const LAUNCH_DEX_THRESH: u8 = 1;
const LAUNCH_MIGRATOR_TYPE: u8 = 1;
const LAUNCH_TAX_DURATION: u64 = 3_153_600_000;
const LAUNCH_ANTI_FARMER_DURATION: u64 = 31_536_000;
const LAUNCH_DIVIDEND_BPS: u16 = 10_000;
const LAUNCH_MINIMUM_SHARE_BALANCE: u128 = 10_000_000_000_000_000_000_000;
const LAUNCH_CREATOR_QUOTE_AMOUNT: u64 = 80_574_062_404;
const LAUNCH_ORACLE_PRICE: u64 = 96_028_517_023_029_920;

// Witnessed execution state (G0.3).
const NATIVE_INPUT_WEI: u64 = 150_000_000_000_000_000;
const POOL_FEE_PIPS: u32 = 2_500;
const POOL_SQRT_PRICE_X96: u128 = 88_144_784_934_153_146_812_079_208_718_554;
const POOL_LIQUIDITY: u128 = 3_883_217_927_027_149_621;
/// Price reached by the witnessed execution inside one segment.
const SEGMENT_SQRT_PRICE_LIMIT_X96: u128 = 88_147_837_689_378_056_938_624_643_760_236;
const CALLER: [u8; 20] = [
    0xb6, 0x86, 0x4c, 0x01, 0x7e, 0x9d, 0x9d, 0x89, 0x01, 0x2d, 0x4f, 0x90, 0xee, 0x38, 0x2b, 0xf1,
    0x46, 0x1e, 0x4c, 0x90,
];
const POOL: [u8; 20] = [
    0xac, 0x10, 0x9c, 0x80, 0x25, 0xf2, 0x72, 0x41, 0x4f, 0xd9, 0xe2, 0xfa, 0xa8, 0x05, 0xa5, 0x83,
    0x70, 0x8a, 0x01, 0x7f,
];
const PARENT_BLOCK_HASH: [u8; 32] = [
    0x85, 0x40, 0x55, 0xa2, 0x93, 0xdb, 0x2a, 0x4c, 0x69, 0xd4, 0x82, 0x7e, 0x00, 0x65, 0x4b, 0xc3,
    0xbe, 0x75, 0xd0, 0x48, 0x4b, 0x1f, 0x42, 0x58, 0xa8, 0x05, 0x63, 0x80, 0x67, 0x2d, 0x11, 0x04,
];

/// Stands for a verified oracle signature and signer role of a DOGE-quoted
/// launch. That verification is not implemented, so no constructor exists
/// outside this crate's tests.
#[derive(Debug)]
pub struct OracleAdmissionProof {
    _sealed: (),
}

#[cfg(test)]
impl OracleAdmissionProof {
    /// Test-only stand-in for the EVM verifier's result on the fixture.
    pub(crate) fn fixture_only() -> Self {
        Self { _sealed: () }
    }
}

/// Reason a launch is outside the DOGE fixture scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DogeScopeError {
    /// Not token version 6.
    TokenVersion,
    /// A mode field (`dexThresh`, `migratorType`, `dexId`, `lpFeeProfile`,
    /// `permit`) differs from the fixture launch.
    Mode,
    /// Buy or sell tax, a duration or the distribution differs.
    Tax,
    /// Quote token is not DOGE.
    QuoteToken,
    /// Dividend token, minimum share balance or commission receiver differs.
    Dividend,
    /// The creator purchase differs from the fixture launch.
    CreatorPurchase,
    /// Another extension id.
    ExtensionId,
    /// Extension mode is not 0, or the oracle price differs.
    Extension,
    /// The extension deadline is before the latest admissible timestamp.
    Deadline,
}

impl core::fmt::Display for DogeScopeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::TokenVersion => "doge scope: token version is not 6",
            Self::Mode => "doge scope: launch mode differs from the fixture",
            Self::Tax => "doge scope: tax settings differ from the fixture",
            Self::QuoteToken => "doge scope: quote token is not DOGE",
            Self::Dividend => "doge scope: dividend settings differ from the fixture",
            Self::CreatorPurchase => "doge scope: creator purchase differs from the fixture",
            Self::ExtensionId => "doge scope: unexpected extension id",
            Self::Extension => "doge scope: extension mode or price differs",
            Self::Deadline => "doge scope: extension deadline too early",
        })
    }
}

impl std::error::Error for DogeScopeError {}

/// Reason the DOGE fixture quote is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DogeError {
    /// The launch is not the fixture launch.
    Scope(DogeScopeError),
    /// Caller, amount, pool or parent state differs from the witnessed
    /// execution.
    OutsideWitnessedExecution,
    /// The arithmetic failed.
    Quote(QuoteError),
}

impl From<DogeScopeError> for DogeError {
    fn from(e: DogeScopeError) -> Self {
        Self::Scope(e)
    }
}

impl From<QuoteError> for DogeError {
    fn from(e: QuoteError) -> Self {
        Self::Quote(e)
    }
}

impl core::fmt::Display for DogeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Scope(e) => write!(f, "doge fixture: {e}"),
            Self::OutsideWitnessedExecution => {
                f.write_str("doge fixture: inputs differ from the witnessed execution")
            }
            Self::Quote(e) => write!(f, "doge fixture: {e}"),
        }
    }
}

impl std::error::Error for DogeError {}

/// Checks every behaviour-relevant launch field against the fixture launch.
///
/// `latest_admissible_timestamp` is the latest block timestamp at which the
/// caller still accepts execution; the extension deadline must not precede
/// it. Equality passes, as witnessed.
fn check_doge_scope(
    launch: &LaunchParamsV6<'_>,
    extension: &ExtensionTuple<'_>,
    latest_admissible_timestamp: u64,
) -> Result<(), DogeScopeError> {
    let doge = Address::from_slice(&DOGE_TOKEN);
    if launch.token_version != TOKEN_VERSION_TAXED_V3 {
        return Err(DogeScopeError::TokenVersion);
    }
    if launch.dex_thresh != LAUNCH_DEX_THRESH
        || launch.migrator_type != LAUNCH_MIGRATOR_TYPE
        || launch.dex_id != 0
        || launch.lp_fee_profile != 0
        || !launch.permit.is_empty()
    {
        return Err(DogeScopeError::Mode);
    }
    if launch.buy_tax != TAX_BPS
        || launch.sell_tax != TAX_BPS
        || launch.tax_duration != LAUNCH_TAX_DURATION
        || launch.anti_farmer_duration != LAUNCH_ANTI_FARMER_DURATION
        || launch.mkt_bps != 0
        || launch.deflation_bps != 0
        || launch.dividend_bps != LAUNCH_DIVIDEND_BPS
        || launch.lp_bps != 0
    {
        return Err(DogeScopeError::Tax);
    }
    if launch.quote_token != doge {
        return Err(DogeScopeError::QuoteToken);
    }
    if launch.dividend_token != doge
        || launch.minimum_share_balance != U256::from(LAUNCH_MINIMUM_SHARE_BALANCE)
        || launch.commission_receiver != Address::ZERO
    {
        return Err(DogeScopeError::Dividend);
    }
    if launch.quote_amt != U256::from(LAUNCH_CREATOR_QUOTE_AMOUNT) {
        return Err(DogeScopeError::CreatorPurchase);
    }
    if launch.extension_id != B256::from_slice(&DOGE_EXTENSION_ID) {
        return Err(DogeScopeError::ExtensionId);
    }
    if !extension.mode.is_zero() || extension.price != U256::from(LAUNCH_ORACLE_PRICE) {
        return Err(DogeScopeError::Extension);
    }
    if U256::from(latest_admissible_timestamp) > extension.deadline {
        return Err(DogeScopeError::Deadline);
    }
    Ok(())
}

/// State and policy inputs of the DOGE fixture execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DogeExecutionState {
    /// Caller of the Portal; also the recipient.
    pub caller: Address,
    /// Native amount to spend, in wei.
    pub native_input: U256,
    /// Conversion pool.
    pub pool: Address,
    /// Fee tier of the conversion pool, in hundredths of a basis point.
    pub pool_fee_pips: u32,
    /// Current `sqrtPriceX96` of the conversion pool.
    pub pool_sqrt_price_x96: U256,
    /// Active liquidity of the conversion pool.
    pub pool_liquidity: U256,
    /// Highest `sqrtPriceX96` up to which `pool_liquidity` is known to stay
    /// constant.
    pub segment_sqrt_price_limit_x96: U256,
    /// Canonical parent block the state was read at.
    pub parent_block_hash: B256,
}

/// The one witnessed execution state (G0.3).
fn witnessed_execution_state() -> DogeExecutionState {
    DogeExecutionState {
        caller: Address::from_slice(&CALLER),
        native_input: U256::from(NATIVE_INPUT_WEI),
        pool: Address::from_slice(&POOL),
        pool_fee_pips: POOL_FEE_PIPS,
        pool_sqrt_price_x96: U256::from(POOL_SQRT_PRICE_X96),
        pool_liquidity: U256::from(POOL_LIQUIDITY),
        segment_sqrt_price_limit_x96: U256::from(SEGMENT_SQRT_PRICE_LIMIT_X96),
        parent_block_hash: B256::from_slice(&PARENT_BLOCK_HASH),
    }
}

/// Result of the DOGE fixture quote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DogeBuyQuote {
    /// `sqrtPriceX96` of the pool after the conversion.
    pub sqrt_price_next_x96: U256,
    /// DOGE obtained from the conversion.
    pub quote_input: U256,
    /// Protocol fee, in DOGE units.
    pub protocol_fee: U256,
    /// Tax, in DOGE units.
    pub tax: U256,
    /// Curve reserve after the creator purchase, in DOGE units.
    pub creator_reserve: U256,
    /// Circulating supply after the creator purchase.
    pub creator_supply: U256,
    /// Tokens received; strictly positive.
    pub output_amount: U256,
}

/// Quote of the witnessed DOGE fixture execution.
///
/// Requires an [`OracleAdmissionProof`], which no runtime can obtain, and
/// refuses every launch and state other than the witnessed one.
pub fn quote_doge_fixture(
    _proof: &OracleAdmissionProof,
    launch: &LaunchParamsV6<'_>,
    extension: &ExtensionTuple<'_>,
    latest_admissible_timestamp: u64,
    state: &DogeExecutionState,
) -> Result<DogeBuyQuote, DogeError> {
    check_doge_scope(launch, extension, latest_admissible_timestamp)?;
    if *state != witnessed_execution_state() {
        return Err(DogeError::OutsideWitnessedExecution);
    }
    Ok(doge_arithmetic(state, extension.price, launch.quote_amt)?)
}

fn max_bits(bits: usize) -> U256 {
    (U256::from(1u64) << bits) - U256::from(1u64)
}

/// The Gate-0 `g03` computation. A pure function; scope is enforced by
/// [`quote_doge_fixture`].
fn doge_arithmetic(
    state: &DogeExecutionState,
    oracle_price: U256,
    creator_quote_amount: U256,
) -> Result<DogeBuyQuote, QuoteError> {
    if state.native_input.is_zero() {
        return Err(QuoteError::ZeroInput);
    }
    let sqrt = state.pool_sqrt_price_x96;
    let liquidity = state.pool_liquidity;
    let limit = state.segment_sqrt_price_limit_x96;
    // Uniswap-V3 widths: uint160 prices, uint128 liquidity. They also bound
    // the 512-bit product below to 384 bits.
    if sqrt.is_zero()
        || sqrt > max_bits(160)
        || limit > max_bits(160)
        || liquidity.is_zero()
        || liquidity > max_bits(128)
        || oracle_price.is_zero()
    {
        return Err(QuoteError::InvalidState);
    }

    // Native -> DOGE through one constant-liquidity segment.
    let net = mul_div_down(
        state.native_input,
        U256::from(POOL_FEE_NUMERATOR),
        U256::from(POOL_FEE_DENOMINATOR),
    )?;
    let next = add(sqrt, mul_div_down(net, U256::from(1u64) << 96, liquidity)?)?;
    if next > limit {
        return Err(QuoteError::OutsideQualifiedSegment);
    }
    // Two successive floor divisions, as witnessed.
    let scaled = (wide(liquidity) << 96) * wide(next - sqrt);
    let quote_input = narrow(scaled / wide(next) / wide(sqrt))?;

    // Fees on the DOGE amount, then the bonding curve.
    let fees = split_fee(quote_input, PROTOCOL_FEE_BPS, TAX_BPS)?;
    let wad = U256::from(WAD);
    let r = mul_div_down(U256::from(CURVE_R_NUMERATOR), wad, oracle_price)?;
    let h = U256::from(CURVE_H);
    let k = mul_div_down(r, add(U256::from(TOTAL_SUPPLY), h)?, wad)?;
    let curve = Curve { r, h, k };
    let scale = U256::from(QUOTE_SCALE);

    let creator_fees = split_fee(creator_quote_amount, PROTOCOL_FEE_BPS, TAX_BPS)?;
    let creator_reserve = sub(creator_quote_amount, creator_fees.combined)?;
    let creator_supply = supply(curve, mul(creator_reserve, scale)?)?;

    let reserve_after = add(creator_reserve, sub(quote_input, fees.combined)?)?;
    let supply_after = supply(curve, mul(reserve_after, scale)?)?;
    if supply_after < creator_supply {
        return Err(QuoteError::InvalidCurveState);
    }
    let output_amount = supply_after - creator_supply;
    if output_amount.is_zero() {
        return Err(QuoteError::ZeroOutput);
    }

    Ok(DogeBuyQuote {
        sqrt_price_next_x96: next,
        quote_input,
        protocol_fee: fees.protocol,
        tax: fees.tax,
        creator_reserve,
        creator_supply,
        output_amount,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economics::slippage::min_output;
    use crate::transactions::legacy::testdata::unhex;
    use crate::venues::flap::abi::{
        decode_extension_tuple, decode_new_token_v6, encode_swap_exact_input_native,
    };
    use crate::venues::flap::deployment::bsc_mainnet_v5_24_0;

    const LAUNCH_HEX: &str =
        include_str!("../../../fixtures/flap/launch_v6_doge_block_124415357.hex");
    /// Timestamp of the witnessed execution (`g03_rust_inputs.json`).
    const FIXTURE_TIMESTAMP: u64 = 1_790_549_254;
    /// Deadline of the fixture launch's extension tuple.
    const FIXTURE_DEADLINE: u64 = 1_790_549_433;

    fn dec(s: &str) -> U256 {
        s.parse::<U256>().unwrap()
    }

    /// Runs the public entry on the fixture launch with one state change.
    fn quote_with(
        timestamp: u64,
        change: impl FnOnce(&mut DogeExecutionState),
    ) -> Result<DogeBuyQuote, DogeError> {
        let calldata = unhex(LAUNCH_HEX.trim());
        let launch = decode_new_token_v6(&calldata, calldata.len()).unwrap();
        let extension = decode_extension_tuple(launch.extension_data).unwrap();
        let mut state = witnessed_execution_state();
        change(&mut state);
        let proof = OracleAdmissionProof::fixture_only();
        quote_doge_fixture(&proof, &launch, &extension, timestamp, &state)
    }

    #[test]
    fn g03_fixture_reproduces_every_recorded_value() {
        let quote = quote_with(FIXTURE_TIMESTAMP, |_| ()).unwrap();
        assert_eq!(
            quote.sqrt_price_next_x96,
            dec("88147837689378056938624643760236")
        );
        assert_eq!(quote.quote_input, U256::from(120_880_165_276u64));
        assert_eq!(quote.protocol_fee, U256::from(1_208_801_652u64));
        assert_eq!(quote.tax, U256::from(1_208_801_653u64));
        assert_eq!(quote.creator_reserve, U256::from(78_962_581_156u64));
        // Equals the creator purchase recorded on chain in the launch.
        assert_eq!(quote.creator_supply, dec("21453245135239692933310825"));
        // g03_result.json: `acquired`, `expected_output_from_rust`.
        assert_eq!(quote.output_amount, dec("30669575743216741084405716"));

        // Final BUY calldata of `g03_rust_output.json`, byte for byte.
        let minimum = min_output(quote.output_amount, 100).unwrap();
        assert_eq!(minimum, dec("30362879985784573673561658"));
        let token = bsc_mainnet_v5_24_0().taxed_v3_token(B256::from_slice(&unhex(
            "0c5ae0f6ffd0a19acfb501ad8eb95542ce1ee50305987db5b6d388537a128e2c",
        )));
        let buy =
            encode_swap_exact_input_native(token, U256::from(NATIVE_INPUT_WEI), minimum).unwrap();
        let expected = unhex(concat!(
            "ef7ec2e7",
            "0000000000000000000000000000000000000000000000000000000000000020",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "000000000000000000000000abe1fe16cf6bc48c9524713054efe6e954b17777",
            "0000000000000000000000000000000000000000000000000214e8348c4f0000",
            "000000000000000000000000000000000000000000191d9705ad465a377b5e3a",
            "00000000000000000000000000000000000000000000000000000000000000a0",
            "0000000000000000000000000000000000000000000000000000000000000000",
        ));
        assert_eq!(buy.as_slice(), expected.as_slice());
    }

    #[test]
    fn every_deviation_from_the_witnessed_execution_is_refused() {
        let refused = Err(DogeError::OutsideWitnessedExecution);
        // A smaller amount inside the same segment is not qualified.
        assert_eq!(
            quote_with(FIXTURE_TIMESTAMP, |s| {
                s.native_input = U256::from(1_000_000u64)
            }),
            refused
        );
        assert_eq!(
            quote_with(FIXTURE_TIMESTAMP, |s| {
                s.native_input = U256::from(NATIVE_INPUT_WEI + 1)
            }),
            refused
        );
        assert_eq!(
            quote_with(FIXTURE_TIMESTAMP, |s| s.caller = Address::ZERO),
            refused
        );
        assert_eq!(
            quote_with(FIXTURE_TIMESTAMP, |s| s.pool = Address::ZERO),
            refused
        );
        assert_eq!(
            quote_with(FIXTURE_TIMESTAMP, |s| s.pool_fee_pips = 500),
            refused
        );
        assert_eq!(
            quote_with(FIXTURE_TIMESTAMP, |s| {
                s.pool_sqrt_price_x96 = U256::from(1u64)
            }),
            refused
        );
        assert_eq!(
            quote_with(FIXTURE_TIMESTAMP, |s| s.pool_liquidity = U256::from(1u64)),
            refused
        );
        assert_eq!(
            quote_with(FIXTURE_TIMESTAMP, |s| {
                s.segment_sqrt_price_limit_x96 = U256::from(SEGMENT_SQRT_PRICE_LIMIT_X96 + 1)
            }),
            refused
        );
        assert_eq!(
            quote_with(FIXTURE_TIMESTAMP, |s| s.parent_block_hash = B256::ZERO),
            refused
        );
    }

    #[test]
    fn deadline_equality_passes_and_one_second_later_is_rejected() {
        assert!(quote_with(FIXTURE_DEADLINE, |_| ()).is_ok());
        assert_eq!(
            quote_with(FIXTURE_DEADLINE + 1, |_| ()),
            Err(DogeError::Scope(DogeScopeError::Deadline))
        );
    }

    /// Scope check of the fixture launch after one field was changed.
    fn scope_after(change: impl FnOnce(&mut LaunchParamsV6<'_>)) -> Result<(), DogeScopeError> {
        let calldata = unhex(LAUNCH_HEX.trim());
        let mut launch = decode_new_token_v6(&calldata, calldata.len()).unwrap();
        let extension = decode_extension_tuple(launch.extension_data).unwrap();
        change(&mut launch);
        check_doge_scope(&launch, &extension, FIXTURE_TIMESTAMP)
    }

    #[test]
    fn scope_check_binds_every_fixture_field() {
        use DogeScopeError::*;
        assert_eq!(scope_after(|_| ()), Ok(()));
        assert_eq!(scope_after(|l| l.token_version = 7), Err(TokenVersion));
        assert_eq!(scope_after(|l| l.dex_thresh = 2), Err(Mode));
        assert_eq!(scope_after(|l| l.migrator_type = 0), Err(Mode));
        assert_eq!(scope_after(|l| l.dex_id = 1), Err(Mode));
        assert_eq!(scope_after(|l| l.lp_fee_profile = 1), Err(Mode));
        assert_eq!(scope_after(|l| l.buy_tax = 0), Err(Tax));
        assert_eq!(scope_after(|l| l.sell_tax = 200), Err(Tax));
        assert_eq!(scope_after(|l| l.tax_duration = 1), Err(Tax));
        assert_eq!(scope_after(|l| l.anti_farmer_duration = 1), Err(Tax));
        assert_eq!(scope_after(|l| l.dividend_bps = 9_999), Err(Tax));
        assert_eq!(
            scope_after(|l| l.quote_token = Address::ZERO),
            Err(QuoteToken)
        );
        assert_eq!(
            scope_after(|l| l.dividend_token = Address::ZERO),
            Err(Dividend)
        );
        assert_eq!(
            scope_after(|l| l.minimum_share_balance = U256::ZERO),
            Err(Dividend)
        );
        let other = Address::from_slice(&CALLER);
        assert_eq!(
            scope_after(|l| l.commission_receiver = other),
            Err(Dividend)
        );
        assert_eq!(
            scope_after(|l| l.quote_amt = U256::from(1u64)),
            Err(CreatorPurchase)
        );
        assert_eq!(
            scope_after(|l| l.extension_id = B256::ZERO),
            Err(ExtensionId)
        );
    }

    #[test]
    fn scope_check_binds_extension_mode_and_price() {
        let calldata = unhex(LAUNCH_HEX.trim());
        let launch = decode_new_token_v6(&calldata, calldata.len()).unwrap();
        let extension = decode_extension_tuple(launch.extension_data).unwrap();
        assert_eq!(extension.deadline, U256::from(FIXTURE_DEADLINE));
        assert_eq!(extension.price, U256::from(LAUNCH_ORACLE_PRICE));

        let mut other = extension;
        other.mode = U256::from(1u64);
        assert_eq!(
            check_doge_scope(&launch, &other, FIXTURE_TIMESTAMP),
            Err(DogeScopeError::Extension)
        );
        let mut other = extension;
        other.price = U256::from(1u64);
        assert_eq!(
            check_doge_scope(&launch, &other, FIXTURE_TIMESTAMP),
            Err(DogeScopeError::Extension)
        );
    }

    #[test]
    fn arithmetic_rejects_bad_state_without_panic() {
        let price = U256::from(LAUNCH_ORACLE_PRICE);
        let creator = U256::from(LAUNCH_CREATOR_QUOTE_AMOUNT);
        let good = witnessed_execution_state();

        let mut state = good;
        state.segment_sqrt_price_limit_x96 = U256::from(SEGMENT_SQRT_PRICE_LIMIT_X96 - 1);
        assert_eq!(
            doge_arithmetic(&state, price, creator),
            Err(QuoteError::OutsideQualifiedSegment)
        );

        let mut state = good;
        state.native_input = U256::ZERO;
        assert_eq!(
            doge_arithmetic(&state, price, creator),
            Err(QuoteError::ZeroInput)
        );

        let mut state = good;
        state.pool_sqrt_price_x96 = U256::ZERO;
        assert_eq!(
            doge_arithmetic(&state, price, creator),
            Err(QuoteError::InvalidState)
        );

        let mut state = good;
        state.pool_sqrt_price_x96 = U256::from(1u64) << 160;
        assert_eq!(
            doge_arithmetic(&state, price, creator),
            Err(QuoteError::InvalidState)
        );

        let mut state = good;
        state.segment_sqrt_price_limit_x96 = U256::MAX;
        assert_eq!(
            doge_arithmetic(&state, price, creator),
            Err(QuoteError::InvalidState)
        );

        let mut state = good;
        state.pool_liquidity = U256::ZERO;
        assert_eq!(
            doge_arithmetic(&state, price, creator),
            Err(QuoteError::InvalidState)
        );

        let mut state = good;
        state.pool_liquidity = U256::from(1u64) << 128;
        assert_eq!(
            doge_arithmetic(&state, price, creator),
            Err(QuoteError::InvalidState)
        );

        assert_eq!(
            doge_arithmetic(&good, U256::ZERO, creator),
            Err(QuoteError::InvalidState)
        );

        // Extreme but well-formed inputs return an error, never panic.
        let mut state = good;
        state.native_input = U256::MAX;
        state.segment_sqrt_price_limit_x96 = max_bits(160);
        assert!(doge_arithmetic(&state, price, creator).is_err());
        assert!(doge_arithmetic(&good, price, U256::MAX).is_err());
        let _ = doge_arithmetic(&good, U256::from(1u64), creator);
    }
}
