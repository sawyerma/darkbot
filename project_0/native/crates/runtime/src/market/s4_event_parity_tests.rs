//! S4-02 (PHASENPLAN S4: Decode against the official crates): every marginfi event the core
//! decodes — receivership, classic, deleverage, bankruptcy, withdraw and repay flows — against the
//! pinned official event types (`marginfi::events`, mrgn-0.1.12-rc2) and their Anchor encoding:
//! the discriminators, the field order and widths and the exact lengths, on official encodings of
//! distinct values and on the six real receiver events of the S4-01 corpus.
//!
//! Explained deviation E-FINITE: the official type decodes an infinite amount (borsh refuses only
//! NaN), the core and the pinned verifier refuse every non-finite one (the program converts its
//! I80F48 amounts and never logs one).
use super::*;
use anchor_lang::{AnchorDeserialize, Discriminator, Event};
use marginfi::events as official;
use marginfi_type_crate::types::WrappedI80F48;

fn key(n: u8) -> Pubkey {
    Pubkey::new_from_array([n; 32])
}

fn account_header(signer: Option<Pubkey>) -> official::AccountEventHeader {
    official::AccountEventHeader {
        signer,
        marginfi_account: key(2),
        marginfi_account_authority: key(3),
        marginfi_group: key(4),
    }
}

fn balances(base: f64) -> official::LiquidationBalances {
    official::LiquidationBalances {
        liquidatee_asset_balance: base + 0.5,
        liquidatee_liability_balance: base + 1.25,
        liquidator_asset_balance: base + 2.0,
        liquidator_liability_balance: base + 3.5,
        liquidator_liability_bank_asset_balance: base + 4.75,
    }
}

fn program() -> Pubkey {
    Pubkey::from_str(darkbot_project0::protocol::PROGRAM_ID_MAINNET).unwrap()
}

/// `data` logged by the marginfi program, as the runtime writes it.
fn logged(data: &[u8]) -> Vec<String> {
    let p = program();
    vec![
        format!("Program {p} invoke [1]"),
        format!(
            "Program data: {}",
            base64::engine::general_purpose::STANDARD.encode(data)
        ),
        format!("Program {p} success"),
    ]
}

fn receiver_event() -> official::LiquidationReceiverEvent {
    official::LiquidationReceiverEvent {
        marginfi_account: key(5),
        liquidation_receiver: key(6),
        liquidatee_assets_seized: 1_234.5,
        liquidatee_liability_repaid: 1_000.25,
        lamps_fee_paid: 0x0102_0304,
    }
}

fn bankruptcy_event() -> official::LendingPoolBankHandleBankruptcyEvent {
    official::LendingPoolBankHandleBankruptcyEvent {
        header: account_header(Some(key(1))),
        bank: key(9),
        mint: key(10),
        bad_debt: 77.5,
        covered_amount: 33.25,
        socialized_amount: 44.25,
        premium_written_off: 5.125,
    }
}

/// The core's discriminators are the official ones, each payload decodes into exactly the
/// official fields, and both pinned bankruptcy layouts decode exactly (0.1.11 ends after the
/// socialized amount, 0.1.12 appends the premium written off).
#[test]
fn s4_02_the_core_decodes_every_official_event_layout() {
    // Receivership (decoder of the observed logs).
    assert_eq!(
        official::LiquidationReceiverEvent::DISCRIMINATOR,
        &super::super::LIQUIDATION_RECEIVER_EVENT_DISCRIMINATOR[..]
    );
    let r = receiver_event();
    let data = r.data();
    assert_eq!(data.len(), super::super::LIQUIDATION_RECEIVER_EVENT_LEN);
    let d = super::super::decode_receiver_events(&logged(&data), &program());
    assert_eq!((d.decoder_errors, d.events.len()), (0, 1));
    let e = &d.events[0];
    assert_eq!(
        (
            e.target,
            e.receiver,
            e.seized_equity_usd,
            e.repaid_equity_usd,
            e.protocol_flat_fee_lamports
        ),
        (
            r.marginfi_account,
            r.liquidation_receiver,
            r.liquidatee_assets_seized,
            r.liquidatee_liability_repaid,
            r.lamps_fee_paid
        )
    );

    // Classic liquidation.
    assert_eq!(
        official::LendingAccountLiquidateEvent::DISCRIMINATOR,
        &CLASSIC_EVENT[..]
    );
    let c = official::LendingAccountLiquidateEvent {
        header: account_header(Some(key(1))),
        liquidatee_marginfi_account: key(7),
        liquidatee_marginfi_account_authority: key(8),
        asset_bank: key(9),
        asset_mint: key(10),
        liability_bank: key(11),
        liability_mint: key(12),
        liquidatee_pre_health: -3.5,
        liquidatee_post_health: 0.75,
        pre_balances: balances(10.0),
        post_balances: balances(20.0),
    };
    let e = decode_payload(&c.data()).unwrap().unwrap();
    assert_eq!(e.kind, LiquidationType::Classic);
    assert_eq!(
        (
            e.actor.as_str(),
            e.liquidator_account.as_deref(),
            e.target.as_str()
        ),
        (
            key(1).to_string().as_str(),
            Some(key(2).to_string().as_str()),
            key(7).to_string().as_str()
        )
    );
    assert_eq!(
        (
            e.asset_bank,
            e.asset_mint,
            e.liability_bank,
            e.liability_mint
        ),
        (
            Some(key(9).to_string()),
            Some(key(10).to_string()),
            Some(key(11).to_string()),
            Some(key(12).to_string())
        )
    );
    let five = |b: &official::LiquidationBalances| {
        [
            b.liquidatee_asset_balance,
            b.liquidatee_liability_balance,
            b.liquidator_asset_balance,
            b.liquidator_liability_balance,
            b.liquidator_liability_bank_asset_balance,
        ]
    };
    let Some(NativeEvidence::Classic {
        pre_balances,
        post_balances,
        pre_health,
        post_health,
        ..
    }) = e.native
    else {
        panic!("classic evidence");
    };
    assert_eq!(
        (pre_balances, post_balances, pre_health, post_health),
        (
            five(&c.pre_balances),
            five(&c.post_balances),
            c.liquidatee_pre_health,
            c.liquidatee_post_health
        )
    );
    let unsigned = official::LendingAccountLiquidateEvent {
        header: account_header(None),
        ..c
    };
    assert_eq!(
        decode_payload(&unsigned.data()).err(),
        Some("CLASSIC_SIGNER_MISSING")
    );

    // Forced deleverage.
    assert_eq!(
        official::DeleverageEvent::DISCRIMINATOR,
        &DELEVERAGE_EVENT[..]
    );
    let v = official::DeleverageEvent {
        marginfi_account: key(5),
        risk_admin: key(6),
        deleveragee_assets_seized: 9.5,
        deleveragee_liability_repaid: 8.25,
    };
    let e = decode_payload(&v.data()).unwrap().unwrap();
    assert_eq!(e.kind, LiquidationType::ForcedDeleverage);
    assert_eq!(
        (e.target, e.actor, e.seized, e.repaid),
        (
            key(5).to_string(),
            key(6).to_string(),
            Some(9.5),
            Some(8.25)
        )
    );

    // Bankruptcy, both pinned layouts.
    assert_eq!(
        official::LendingPoolBankHandleBankruptcyEvent::DISCRIMINATOR,
        &BANKRUPTCY_EVENT[..]
    );
    let b = bankruptcy_event();
    let data = b.data();
    let e = decode_payload(&data).unwrap().unwrap();
    assert_eq!(e.kind, LiquidationType::BankruptcySettlement);
    assert_eq!(
        (e.actor, e.target, e.liability_bank, e.liability_mint),
        (
            key(1).to_string(),
            key(2).to_string(),
            Some(key(9).to_string()),
            Some(key(10).to_string())
        )
    );
    let Some(NativeEvidence::BankruptcyPremium {
        bad_debt_native_f64,
        covered_native_f64,
        socialized_native_f64,
        premium_written_off_native_f64,
        ..
    }) = e.native
    else {
        panic!("0.1.12 bankruptcy evidence");
    };
    assert_eq!(
        (
            bad_debt_native_f64,
            covered_native_f64,
            socialized_native_f64,
            premium_written_off_native_f64
        ),
        (
            b.bad_debt,
            b.covered_amount,
            b.socialized_amount,
            b.premium_written_off
        )
    );
    // 0.1.11: the same event without its last field (the premium was appended).
    let legacy = &data[..data.len() - 8];
    let e = decode_payload(legacy).unwrap().unwrap();
    let Some(NativeEvidence::Bankruptcy {
        bad_debt_native_f64,
        covered_native_f64,
        socialized_native_f64,
        ..
    }) = e.native
    else {
        panic!("0.1.11 bankruptcy evidence");
    };
    assert_eq!(
        (
            bad_debt_native_f64,
            covered_native_f64,
            socialized_native_f64
        ),
        (b.bad_debt, b.covered_amount, b.socialized_amount)
    );

    // Withdraw and repay flows.
    let share = WrappedI80F48 { value: [7; 16] };
    let w = official::LendingAccountWithdrawEvent {
        header: account_header(Some(key(1))),
        bank: key(9),
        mint: key(10),
        amount: 0x0102_0304_0506_0708,
        close_balance: true,
        share_amount: share,
    };
    let r = official::LendingAccountRepayEvent {
        header: account_header(None),
        bank: key(11),
        mint: key(12),
        amount: 42,
        close_balance: false,
        share_amount: share,
    };
    for (data, kind, bank, mint, amount) in [
        (w.data(), "WITHDRAW", key(9), key(10), w.amount),
        (r.data(), "REPAY", key(11), key(12), r.amount),
    ] {
        assert_eq!(
            native_flow(&data).unwrap(),
            Some(NativeFlow {
                kind: kind.into(),
                target: key(2).to_string(),
                bank: bank.to_string(),
                mint: mint.to_string(),
                amount: amount.to_string(),
            })
        );
    }
    assert_eq!(
        official::LendingAccountWithdrawEvent::DISCRIMINATOR,
        &w.data()[..8]
    );
    assert_eq!(
        official::LendingAccountRepayEvent::DISCRIMINATOR,
        &r.data()[..8]
    );
}

/// Whatever the official layout refuses the core refuses: a payload with one byte more or less, a
/// NaN amount; E-FINITE: an infinite amount, which the official type decodes, the core refuses.
#[test]
fn s4_02_the_core_refuses_what_the_official_layout_refuses() {
    let receiver = receiver_event().data();
    let bankruptcy = bankruptcy_event().data();
    for data in [&receiver, &bankruptcy] {
        let mut longer = data.clone();
        longer.push(0);
        let shorter = &data[..data.len() - 1];
        for wrong in [&longer[..], shorter] {
            if data == &receiver {
                assert!(official::LiquidationReceiverEvent::try_from_slice(&wrong[8..]).is_err());
                let d = super::super::decode_receiver_events(&logged(wrong), &program());
                assert_eq!((d.events.len(), d.decoder_errors), (0, 1));
            } else {
                assert!(
                    official::LendingPoolBankHandleBankruptcyEvent::try_from_slice(&wrong[8..])
                        .is_err()
                );
                // One byte short of 0.1.12 is no layout; one byte long is none either.
                assert!(decode_payload(wrong).is_err(), "{}", wrong.len());
            }
        }
    }
    // The seized amount follows the discriminator and the two keys.
    let mut nan = receiver.clone();
    nan[72..80].copy_from_slice(&f64::NAN.to_le_bytes());
    assert!(official::LiquidationReceiverEvent::try_from_slice(&nan[8..]).is_err());
    let d = super::super::decode_receiver_events(&logged(&nan), &program());
    assert_eq!((d.events.len(), d.decoder_errors), (0, 1));
    let infinite = official::LiquidationReceiverEvent {
        liquidatee_assets_seized: f64::INFINITY,
        ..receiver_event()
    };
    let decoded =
        official::LiquidationReceiverEvent::try_from_slice(&infinite.data()[8..]).unwrap();
    assert!(decoded.liquidatee_assets_seized.is_infinite());
    let d = super::super::decode_receiver_events(&logged(&infinite.data()), &program());
    assert_eq!((d.events.len(), d.decoder_errors), (0, 1));
    let infinite = official::LendingPoolBankHandleBankruptcyEvent {
        premium_written_off: f64::INFINITY,
        ..bankruptcy_event()
    };
    assert_eq!(
        decode_payload(&infinite.data()).err(),
        Some("EVENT_NONFINITE")
    );
}

/// The six real receiver events of the S4-01 corpus decode as the official event.
#[test]
fn s4_02_real_receiver_events_decode_as_the_official_event() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../docs/evidence/AUDIT_20261005");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "json")
                && p.file_stem().is_some_and(|s| s.len() >= 80)
        })
        .collect();
    files.sort();
    assert_eq!(files.len(), 6);
    for path in files {
        let raw: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let tx = darkbot_solana::chain::decode_transaction(&raw).unwrap();
        let logs = tx.meta.unwrap().log_messages.unwrap();
        let official_events: Vec<official::LiquidationReceiverEvent> = logs
            .iter()
            .filter_map(|l| l.strip_prefix("Program data: "))
            .map(|b| {
                base64::engine::general_purpose::STANDARD
                    .decode(b.trim())
                    .unwrap()
            })
            .filter(|b| b.starts_with(official::LiquidationReceiverEvent::DISCRIMINATOR))
            .map(|b| official::LiquidationReceiverEvent::try_from_slice(&b[8..]).unwrap())
            .collect();
        let d = super::super::decode_receiver_events(&logs, &program());
        assert_eq!(d.decoder_errors, 0, "{path:?}");
        assert_eq!(official_events.len(), 1, "{path:?}");
        assert_eq!(d.events.len(), official_events.len(), "{path:?}");
        for (ours, theirs) in d.events.iter().zip(&official_events) {
            assert_eq!(
                (
                    ours.target,
                    ours.receiver,
                    ours.seized_equity_usd,
                    ours.repaid_equity_usd,
                    ours.protocol_flat_fee_lamports
                ),
                (
                    theirs.marginfi_account,
                    theirs.liquidation_receiver,
                    theirs.liquidatee_assets_seized,
                    theirs.liquidatee_liability_repaid,
                    theirs.lamps_fee_paid
                ),
                "{path:?}"
            );
        }
    }
}

/// Review L1: the variant appended for 0.1.12 keeps every recorded variant's encoding — the
/// three-variant layout as the journals wrote it decodes unchanged, and re-encodes identically.
#[test]
fn s4_02_recorded_native_evidence_decodes_unchanged() {
    use crate::analytics::payload::{decode_wire, encode_wire};
    #[derive(serde::Serialize)]
    enum Recorded {
        Classic {
            provenance: String,
            pre_balances: [f64; 5],
            post_balances: [f64; 5],
            pre_health: f64,
            post_health: f64,
            liquidator_liability_cost_native_f64: f64,
            target_debt_reduction_native_f64: f64,
            insurance_component_native_f64: f64,
            asset_decimals: Option<u8>,
            liability_decimals: Option<u8>,
            historical_asset_price: Option<f64>,
            historical_liability_price: Option<f64>,
        },
        Bankruptcy {
            provenance: String,
            bad_debt_native_f64: f64,
            covered_native_f64: f64,
            socialized_native_f64: f64,
            liability_decimals: Option<u8>,
        },
        Flows {
            scope: String,
            flows: Vec<NativeFlow>,
            units: String,
        },
    }
    let flow = || NativeFlow {
        kind: "WITHDRAW".into(),
        target: key(2).to_string(),
        bank: key(9).to_string(),
        mint: key(10).to_string(),
        amount: "42".into(),
    };
    let cases = [
        (
            Recorded::Classic {
                provenance: "p".into(),
                pre_balances: [1.0, 2.0, 3.0, 4.0, 5.0],
                post_balances: [6.0, 7.0, 8.0, 9.0, 10.0],
                pre_health: -1.5,
                post_health: 0.5,
                liquidator_liability_cost_native_f64: 3.0,
                target_debt_reduction_native_f64: 2.0,
                insurance_component_native_f64: 1.0,
                asset_decimals: Some(6),
                liability_decimals: None,
                historical_asset_price: Some(1.25),
                historical_liability_price: None,
            },
            NativeEvidence::Classic {
                provenance: "p".into(),
                pre_balances: [1.0, 2.0, 3.0, 4.0, 5.0],
                post_balances: [6.0, 7.0, 8.0, 9.0, 10.0],
                pre_health: -1.5,
                post_health: 0.5,
                liquidator_liability_cost_native_f64: 3.0,
                target_debt_reduction_native_f64: 2.0,
                insurance_component_native_f64: 1.0,
                asset_decimals: Some(6),
                liability_decimals: None,
                historical_asset_price: Some(1.25),
                historical_liability_price: None,
            },
        ),
        (
            Recorded::Bankruptcy {
                provenance: "b".into(),
                bad_debt_native_f64: 100.0,
                covered_native_f64: 40.0,
                socialized_native_f64: 60.0,
                liability_decimals: Some(9),
            },
            NativeEvidence::Bankruptcy {
                provenance: "b".into(),
                bad_debt_native_f64: 100.0,
                covered_native_f64: 40.0,
                socialized_native_f64: 60.0,
                liability_decimals: Some(9),
            },
        ),
        (
            Recorded::Flows {
                scope: "s".into(),
                flows: vec![flow()],
                units: "native".into(),
            },
            NativeEvidence::Flows {
                scope: "s".into(),
                flows: vec![flow()],
                units: "native".into(),
            },
        ),
    ];
    for (recorded, now) in cases {
        let bytes = encode_wire(&recorded).unwrap();
        assert_eq!(decode_wire::<NativeEvidence>(&bytes).unwrap(), now);
        assert_eq!(encode_wire(&now).unwrap(), bytes);
    }
}
