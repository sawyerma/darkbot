//! Fail-closed classification of decoded launches into qualified profiles.
//!
//! The ABI decoder ([`super::abi`]) is factual: it returns whatever a
//! canonical `newTokenV6` call contains, so capture and evidence keep the
//! facts of launches that are not supported. This module is the separate
//! trading-admission step. It accepts a launch only when every
//! behaviour-relevant field equals the value of a witnessed execution and
//! reports the first deviating field otherwise.
//!
//! # Qualified profile: native-BNB curve 20 without creator purchase
//!
//! Witness: the G0.6a independent EVM execution on mainnet state (launch
//! followed by BUY, exact output match). Bound fields and their witnessed
//! values:
//!
//! ```text
//! tokenVersion 6        dexThresh 1            migratorType 1
//! quoteToken 0x0        quoteAmt 0             permit empty
//! extensionId 0x0       extensionData empty    dexId 0
//! lpFeeProfile 0        buyTax 100             sellTax 100
//! taxDuration 31536000  antiFarmerDuration 86400
//! mktBps 10000          deflationBps 0         dividendBps 0
//! lpBps 0               minimumShareBalance 0  dividendToken 0x0
//! commissionReceiver 0x0
//! ```
//!
//! `name`, `symbol`, `uri` and `beneficiary` are bound to the values of that
//! witnessed launch as well. Whether they influence the BUY has not been
//! established from the pinned Portal code; two executions with different
//! values following the same formula are not a proof. Until it is proven,
//! or another launch identity is qualified by its own independent
//! execution, a launch with other values is not admissible.
//!
//! Only `salt` is free: it selects the token address through the CREATE2
//! derivation that is pinned by its own vectors.
//!
//! # Candidate identity
//!
//! A classification belongs to one launch transaction. The caller passes
//! the hash of the transaction the calldata was taken from; it is carried
//! in the result and later compared with the state snapshot, so a launch
//! can never be combined with the state of another one. That the hash
//! belongs to these bytes is established at ingress, not here.
//!
//! # Known but disabled: DOGE-quoted launch
//!
//! A launch with the DOGE quote token and the witnessed extension id is
//! recognised and rejected with its own reason. Its oracle signature and
//! signer role cannot be verified in Rust yet, so it is not admissible.

use alloy_primitives::{Address, B256, U256};

use super::abi::{LaunchParamsV6, TOKEN_VERSION_TAXED_V3};
use super::deployment::ValidatedDeployment;

/// DOGE on BSC, quote token of the historical fixture launch (8 decimals).
pub const DOGE_TOKEN: [u8; 20] = [
    0xba, 0x2a, 0xe4, 0x24, 0xd9, 0x60, 0xc2, 0x62, 0x47, 0xdd, 0x6c, 0x32, 0xed, 0xc7, 0x0b, 0x29,
    0x5c, 0x74, 0x4c, 0x43,
];

/// Extension id of the historical DOGE fixture launch.
pub const DOGE_EXTENSION_ID: [u8; 32] = [
    0x35, 0x26, 0xf6, 0xa0, 0xe7, 0x90, 0x30, 0x6d, 0x64, 0xc8, 0x96, 0xee, 0xda, 0x5e, 0xc6, 0x60,
    0x52, 0xea, 0x17, 0x88, 0x9e, 0x20, 0x63, 0x2f, 0xf0, 0xcf, 0x51, 0x29, 0x23, 0x15, 0x3d, 0xad,
];

const NATIVE_DEX_THRESH: u8 = 1;
const NATIVE_MIGRATOR_TYPE: u8 = 1;
const NATIVE_DEX_ID: u8 = 0;
const NATIVE_LP_FEE_PROFILE: u8 = 0;
const NATIVE_TAX_BPS: u16 = 100;
const NATIVE_TAX_DURATION: u64 = 31_536_000;
const NATIVE_ANTI_FARMER_DURATION: u64 = 86_400;
const NATIVE_MKT_BPS: u16 = 10_000;

// Identity of the one witnessed native launch
// (`g06a_economics_evm_crosscheck.cjs`).
const WITNESSED_NAME: &[u8] = b"DarkBot G06A Economics";
const WITNESSED_SYMBOL: &[u8] = b"DBG06E";
const WITNESSED_URI: &[u8] = b"ipfs://darkbot-g06a-economics";
/// `0x000000000000000000000000000000000000a11c`, launcher and beneficiary
/// of the witnessed execution.
const WITNESSED_BENEFICIARY: [u8; 20] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0xa1, 0x1c,
];

/// A qualified launch profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchProfile {
    /// Native-BNB quote on curve id 20, no creator purchase, 100 bps taxes.
    NativeCurve20NoCreatorPurchase,
}

/// Why a decoded launch is not admissible. Each variant names the first
/// field that left the qualified profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedLaunch {
    /// `tokenVersion` is not 6.
    TokenVersion,
    /// DOGE-quoted launch: recognised, but disabled until its oracle
    /// signature and signer role can be verified.
    DogeRuntimeDisabled,
    /// `dexThresh` differs.
    DexThresh,
    /// `migratorType` differs.
    MigratorType,
    /// The quote token is not native BNB.
    QuoteToken,
    /// The launch contains a creator purchase (`quoteAmt` is not zero).
    CreatorPurchase,
    /// `permit` is not empty.
    Permit,
    /// `extensionId` or `extensionData` is set.
    Extension,
    /// `dexId` differs.
    DexId,
    /// `lpFeeProfile` differs.
    LpFeeProfile,
    /// `buyTax` differs.
    BuyTax,
    /// `sellTax` differs.
    SellTax,
    /// `taxDuration` differs.
    TaxDuration,
    /// `antiFarmerDuration` differs.
    AntiFarmerDuration,
    /// The tax distribution (`mktBps`, `deflationBps`, `dividendBps`,
    /// `lpBps`) differs.
    Distribution,
    /// `minimumShareBalance` is not zero.
    MinimumShareBalance,
    /// `dividendToken` is set.
    DividendToken,
    /// `commissionReceiver` is set.
    CommissionReceiver,
    /// `beneficiary` is not the one of a witnessed launch.
    Beneficiary,
    /// `name`, `symbol` or `uri` is not the one of a witnessed launch.
    Metadata,
    /// The deployment is decode-only: its economics are not witnessed on
    /// its implementation.
    EconomicsNotWitnessed,
}

impl core::fmt::Display for UnsupportedLaunch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::TokenVersion => "launch profile: token version",
            Self::DogeRuntimeDisabled => "launch profile: DOGE profile is disabled at runtime",
            Self::DexThresh => "launch profile: dexThresh",
            Self::MigratorType => "launch profile: migratorType",
            Self::QuoteToken => "launch profile: quote token",
            Self::CreatorPurchase => "launch profile: creator purchase",
            Self::Permit => "launch profile: permit",
            Self::Extension => "launch profile: extension",
            Self::DexId => "launch profile: dexId",
            Self::LpFeeProfile => "launch profile: lpFeeProfile",
            Self::BuyTax => "launch profile: buy tax",
            Self::SellTax => "launch profile: sell tax",
            Self::TaxDuration => "launch profile: tax duration",
            Self::AntiFarmerDuration => "launch profile: anti-farmer duration",
            Self::Distribution => "launch profile: tax distribution",
            Self::MinimumShareBalance => "launch profile: minimum share balance",
            Self::DividendToken => "launch profile: dividend token",
            Self::CommissionReceiver => "launch profile: commission receiver",
            Self::Beneficiary => "launch profile: beneficiary is not witnessed",
            Self::Metadata => "launch profile: name, symbol or uri is not witnessed",
            Self::EconomicsNotWitnessed => {
                "launch profile: the deployment's economics are not witnessed"
            }
        })
    }
}

impl std::error::Error for UnsupportedLaunch {}

/// A launch that matched a qualified profile on a validated deployment.
/// Constructed only by [`classify_launch`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassifiedLaunch {
    profile: LaunchProfile,
    candidate: B256,
    token: Address,
    buy_tax_bps: u16,
    codehash_generation: u64,
}

impl ClassifiedLaunch {
    /// The matched profile.
    #[inline]
    pub fn profile(&self) -> LaunchProfile {
        self.profile
    }

    /// Hash of the launch transaction this classification belongs to.
    #[inline]
    pub fn candidate(&self) -> B256 {
        self.candidate
    }

    /// Address of the token the launch creates.
    #[inline]
    pub fn token(&self) -> Address {
        self.token
    }

    /// Buy tax of the token in basis points.
    #[inline]
    pub fn buy_tax_bps(&self) -> u16 {
        self.buy_tax_bps
    }

    /// Code generation of the deployment the launch was classified against.
    #[inline]
    pub fn codehash_generation(&self) -> u64 {
        self.codehash_generation
    }
}

/// Classifies a decoded launch against the qualified profiles.
///
/// `candidate` is the hash of the launch transaction that carried
/// `launch`.
pub fn classify_launch(
    deployment: &ValidatedDeployment,
    candidate: B256,
    launch: &LaunchParamsV6<'_>,
) -> Result<ClassifiedLaunch, UnsupportedLaunch> {
    if !deployment.economics_witnessed() {
        return Err(UnsupportedLaunch::EconomicsNotWitnessed);
    }
    if launch.token_version != TOKEN_VERSION_TAXED_V3 {
        return Err(UnsupportedLaunch::TokenVersion);
    }
    if launch.quote_token == Address::from_slice(&DOGE_TOKEN)
        && launch.extension_id == B256::from_slice(&DOGE_EXTENSION_ID)
    {
        return Err(UnsupportedLaunch::DogeRuntimeDisabled);
    }
    if launch.dex_thresh != NATIVE_DEX_THRESH {
        return Err(UnsupportedLaunch::DexThresh);
    }
    if launch.migrator_type != NATIVE_MIGRATOR_TYPE {
        return Err(UnsupportedLaunch::MigratorType);
    }
    if launch.quote_token != Address::ZERO {
        return Err(UnsupportedLaunch::QuoteToken);
    }
    if !launch.quote_amt.is_zero() {
        return Err(UnsupportedLaunch::CreatorPurchase);
    }
    if !launch.permit.is_empty() {
        return Err(UnsupportedLaunch::Permit);
    }
    if launch.extension_id != B256::ZERO || !launch.extension_data.is_empty() {
        return Err(UnsupportedLaunch::Extension);
    }
    if launch.dex_id != NATIVE_DEX_ID {
        return Err(UnsupportedLaunch::DexId);
    }
    if launch.lp_fee_profile != NATIVE_LP_FEE_PROFILE {
        return Err(UnsupportedLaunch::LpFeeProfile);
    }
    if launch.buy_tax != NATIVE_TAX_BPS {
        return Err(UnsupportedLaunch::BuyTax);
    }
    if launch.sell_tax != NATIVE_TAX_BPS {
        return Err(UnsupportedLaunch::SellTax);
    }
    if launch.tax_duration != NATIVE_TAX_DURATION {
        return Err(UnsupportedLaunch::TaxDuration);
    }
    if launch.anti_farmer_duration != NATIVE_ANTI_FARMER_DURATION {
        return Err(UnsupportedLaunch::AntiFarmerDuration);
    }
    if launch.mkt_bps != NATIVE_MKT_BPS
        || launch.deflation_bps != 0
        || launch.dividend_bps != 0
        || launch.lp_bps != 0
    {
        return Err(UnsupportedLaunch::Distribution);
    }
    if launch.minimum_share_balance != U256::ZERO {
        return Err(UnsupportedLaunch::MinimumShareBalance);
    }
    if launch.dividend_token != Address::ZERO {
        return Err(UnsupportedLaunch::DividendToken);
    }
    if launch.commission_receiver != Address::ZERO {
        return Err(UnsupportedLaunch::CommissionReceiver);
    }
    if launch.beneficiary != Address::from_slice(&WITNESSED_BENEFICIARY) {
        return Err(UnsupportedLaunch::Beneficiary);
    }
    if launch.name != WITNESSED_NAME
        || launch.symbol != WITNESSED_SYMBOL
        || launch.uri != WITNESSED_URI
    {
        return Err(UnsupportedLaunch::Metadata);
    }

    Ok(ClassifiedLaunch {
        profile: LaunchProfile::NativeCurve20NoCreatorPurchase,
        candidate,
        token: deployment.deployment().taxed_v3_token(launch.salt),
        buy_tax_bps: launch.buy_tax,
        codehash_generation: deployment.codehash_generation(),
    })
}

#[cfg(test)]
pub(crate) mod testkit {
    //! The witnessed native launch for tests of dependent modules.

    use super::*;
    use crate::transactions::legacy::testdata::unhex;

    /// Salt of the G0.6a EVM crosscheck launch.
    pub const WITNESSED_SALT: &str =
        "09c4b20c7956530d4bf91b3569ed173abfce47d16d412d94dde5e6f4034c6eeb";
    /// Token returned for that salt by the mainnet `eth_call` of the launch.
    pub const WITNESSED_TOKEN: &str = "e51c4deb1038afcd95ef12d4a649cdbcf6fa7777";

    /// Test transaction hash standing for the witnessed launch.
    pub fn candidate_a() -> B256 {
        B256::from_slice(&[0xa1u8; 32])
    }

    /// Test transaction hash of another launch.
    pub fn candidate_b() -> B256 {
        B256::from_slice(&[0xb2u8; 32])
    }

    /// Launch parameters of the G0.6a EVM crosscheck
    /// (`g06a_economics_evm_crosscheck.cjs`).
    pub fn witnessed_native_launch() -> LaunchParamsV6<'static> {
        let mut launcher = [0u8; 20];
        launcher[18] = 0xa1;
        launcher[19] = 0x1c;
        LaunchParamsV6 {
            name: b"DarkBot G06A Economics",
            symbol: b"DBG06E",
            uri: b"ipfs://darkbot-g06a-economics",
            dex_thresh: 1,
            salt: B256::from_slice(&unhex(WITNESSED_SALT)),
            migrator_type: 1,
            quote_token: Address::ZERO,
            quote_amt: U256::ZERO,
            beneficiary: Address::from_slice(&launcher),
            permit: &[],
            extension_id: B256::ZERO,
            extension_data: &[],
            dex_id: 0,
            lp_fee_profile: 0,
            buy_tax: 100,
            sell_tax: 100,
            tax_duration: 31_536_000,
            anti_farmer_duration: 86_400,
            mkt_bps: 10_000,
            deflation_bps: 0,
            dividend_bps: 0,
            lp_bps: 0,
            minimum_share_balance: U256::ZERO,
            dividend_token: Address::ZERO,
            commission_receiver: Address::ZERO,
            token_version: 6,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;
    use crate::transactions::legacy::testdata::unhex;
    use crate::venues::flap::abi::{decode_new_token_v6, encode_new_token_v6};
    use crate::venues::flap::deployment::testkit::validated_mainnet;

    const DOGE_LAUNCH: &str =
        include_str!("../../../fixtures/flap/launch_v6_doge_block_124415357.hex");
    const NATIVE_CREATOR_BUY: &str =
        include_str!("../../../fixtures/flap/launch_v6_native_creator_buy_pending.hex");
    const NATIVE_SPLIT_TAX: &str =
        include_str!("../../../fixtures/flap/launch_v6_native_split_tax_pending.hex");

    #[test]
    fn witnessed_native_launch_is_classified_through_its_calldata() {
        let deployment = validated_mainnet(7);
        // Encode, decode and classify, as the runtime sees a launch.
        let mut calldata = Vec::new();
        encode_new_token_v6(&witnessed_native_launch(), &mut calldata);
        let launch = decode_new_token_v6(&calldata, calldata.len()).unwrap();
        assert_eq!(launch, witnessed_native_launch());

        let classified = classify_launch(&deployment, candidate_a(), &launch).unwrap();
        assert_eq!(
            classified.profile(),
            LaunchProfile::NativeCurve20NoCreatorPurchase
        );
        assert_eq!(classified.candidate(), candidate_a());
        assert_eq!(
            classified.token().as_slice(),
            unhex(WITNESSED_TOKEN).as_slice()
        );
        assert_eq!(classified.buy_tax_bps(), 100);
        assert_eq!(classified.codehash_generation(), 7);
    }

    #[test]
    fn salt_selects_the_token_and_nothing_else_is_free() {
        let deployment = validated_mainnet(1);
        let witnessed =
            classify_launch(&deployment, candidate_a(), &witnessed_native_launch()).unwrap();

        // Another salt is another token of the same profile.
        let mut launch = witnessed_native_launch();
        launch.salt = B256::from_slice(&[0x22u8; 32]);
        let other = classify_launch(&deployment, candidate_a(), &launch).unwrap();
        assert_ne!(other.token(), witnessed.token());
        assert_eq!(other.profile(), witnessed.profile());
    }

    /// Classifies the witnessed launch after one field was changed.
    fn changed(
        mutate: impl FnOnce(&mut LaunchParamsV6<'static>),
    ) -> Result<ClassifiedLaunch, UnsupportedLaunch> {
        let mut launch = witnessed_native_launch();
        mutate(&mut launch);
        classify_launch(&validated_mainnet(1), candidate_a(), &launch)
    }

    #[test]
    fn every_bound_field_fails_closed() {
        use UnsupportedLaunch::*;
        let other = Address::from_slice(&[0x01u8; 20]);

        assert_eq!(changed(|l| l.token_version = 7), Err(TokenVersion));
        assert_eq!(changed(|l| l.dex_thresh = 2), Err(DexThresh));
        assert_eq!(changed(|l| l.migrator_type = 0), Err(MigratorType));
        assert_eq!(changed(|l| l.quote_token = other), Err(QuoteToken));
        assert_eq!(
            changed(|l| l.quote_amt = U256::from(1u64)),
            Err(CreatorPurchase)
        );
        assert_eq!(changed(|l| l.permit = &[0x01]), Err(Permit));
        assert_eq!(
            changed(|l| l.extension_id = B256::from_slice(&[1u8; 32])),
            Err(Extension)
        );
        assert_eq!(changed(|l| l.extension_data = &[0x01]), Err(Extension));
        assert_eq!(changed(|l| l.dex_id = 1), Err(DexId));
        assert_eq!(changed(|l| l.lp_fee_profile = 1), Err(LpFeeProfile));
        assert_eq!(changed(|l| l.buy_tax = 0), Err(BuyTax));
        assert_eq!(changed(|l| l.sell_tax = 200), Err(SellTax));
        assert_eq!(
            changed(|l| l.tax_duration = 3_153_600_000),
            Err(TaxDuration)
        );
        assert_eq!(
            changed(|l| l.anti_farmer_duration = 3_600),
            Err(AntiFarmerDuration)
        );
        assert_eq!(changed(|l| l.mkt_bps = 5_000), Err(Distribution));
        assert_eq!(changed(|l| l.deflation_bps = 1), Err(Distribution));
        assert_eq!(changed(|l| l.dividend_bps = 1), Err(Distribution));
        assert_eq!(changed(|l| l.lp_bps = 1), Err(Distribution));
        assert_eq!(
            changed(|l| l.minimum_share_balance = U256::from(1u64)),
            Err(MinimumShareBalance)
        );
        assert_eq!(changed(|l| l.dividend_token = other), Err(DividendToken));
        assert_eq!(
            changed(|l| l.commission_receiver = other),
            Err(CommissionReceiver)
        );

        // Without any change the launch is admissible.
        assert!(changed(|_| ()).is_ok());
    }

    #[test]
    fn unwitnessed_beneficiary_and_metadata_fail_closed() {
        use UnsupportedLaunch::*;
        let other = Address::from_slice(&[0x11u8; 20]);

        assert_eq!(changed(|l| l.beneficiary = other), Err(Beneficiary));
        assert_eq!(changed(|l| l.name = b"another name"), Err(Metadata));
        assert_eq!(changed(|l| l.symbol = b"OTHER"), Err(Metadata));
        assert_eq!(changed(|l| l.uri = b"ipfs://other"), Err(Metadata));
        // The G0.7 testnet launch identity is not a qualified mainnet one.
        assert_eq!(
            changed(|l| l.name = b"DarkBot G07 Birth Test"),
            Err(Metadata)
        );
    }

    #[test]
    fn captured_foreign_launches_decode_but_are_not_admissible() {
        let deployment = validated_mainnet(1);

        let calldata = unhex(NATIVE_CREATOR_BUY.trim());
        let launch = decode_new_token_v6(&calldata, calldata.len()).unwrap();
        assert_eq!(
            classify_launch(&deployment, candidate_b(), &launch),
            Err(UnsupportedLaunch::CreatorPurchase)
        );

        let calldata = unhex(NATIVE_SPLIT_TAX.trim());
        let launch = decode_new_token_v6(&calldata, calldata.len()).unwrap();
        assert_eq!(
            classify_launch(&deployment, candidate_b(), &launch),
            Err(UnsupportedLaunch::BuyTax)
        );
    }

    #[test]
    fn doge_launch_is_recognised_and_disabled() {
        let deployment = validated_mainnet(1);
        let calldata = unhex(DOGE_LAUNCH.trim());
        let launch = decode_new_token_v6(&calldata, calldata.len()).unwrap();
        assert_eq!(
            classify_launch(&deployment, candidate_b(), &launch),
            Err(UnsupportedLaunch::DogeRuntimeDisabled)
        );
    }
}
