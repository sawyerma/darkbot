//! The network cost model against the real receipts of
//! `fixtures/bsc/receipts_20261026.txt` (bsc-dataseed, read-only).

use super::*;

const RECEIPTS: &str = include_str!("../../../fixtures/bsc/receipts_20261026.txt");

struct Row {
    class: PathClass,
    tx: [u8; 32],
    status: ExecutionStatus,
    gas_limit: u64,
    cost: IncludedCost,
}

fn rows() -> Vec<Row> {
    RECEIPTS
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let venue = match f[0] {
                "FLAP" => Venue::Flap,
                "CLASSIC" => Venue::FourClassic,
                "OPENFOUR" => Venue::OpenFour,
                other => panic!("venue {other}"),
            };
            let action = match f[1] {
                "LAUNCH" => Action::Launch,
                "BUY" => Action::Buy,
                "SELL" => Action::Sell,
                "APPROVE" => Action::Approve,
                "REVERTED" => Action::Reverted,
                other => panic!("class {other}"),
            };
            let status = match f[5] {
                "1" => ExecutionStatus::Succeeded,
                "0" => ExecutionStatus::Reverted,
                other => panic!("status {other}"),
            };
            Row {
                class: PathClass { venue, action },
                tx: crate::transactions::legacy::testdata::unhex(f[4])
                    .try_into()
                    .unwrap(),
                status,
                gas_limit: f[6].parse().unwrap(),
                cost: IncludedCost {
                    gas_used: f[7].parse().unwrap(),
                    effective_gas_price_wei: U256::from(f[8].parse::<u128>().unwrap()),
                    status,
                },
            }
        })
        .collect()
}

/// Every receipt: the fee is exactly gas used × effective gas price, a
/// revert is charged like a success, gas used never exceeds the limit, and
/// a revert is exactly the receipts with status 0.
#[test]
fn realized_cost_is_gas_used_times_effective_price_for_success_and_revert() {
    let rows = rows();
    assert_eq!(rows.len(), 35);
    for r in &rows {
        assert!(r.cost.gas_used <= r.gas_limit);
        assert_eq!(
            r.cost.fee_wei().unwrap(),
            r.cost.effective_gas_price_wei * U256::from(r.cost.gas_used)
        );
        assert_eq!(
            r.status == ExecutionStatus::Reverted,
            r.class.action == Action::Reverted
        );
    }
    // A Flap BUY: 284 708 gas at 0.1 gwei.
    let buy = rows
        .iter()
        .find(|r| {
            r.class
                == (PathClass {
                    venue: Venue::Flap,
                    action: Action::Buy,
                })
        })
        .unwrap();
    assert_eq!(
        buy.cost.fee_wei().unwrap(),
        U256::from(28_470_800_000_000u64)
    );
    // A reverted Classic BUY is charged too.
    let revert = rows
        .iter()
        .find(|r| r.class.venue == Venue::FourClassic && r.class.action == Action::Reverted)
        .unwrap();
    assert!(revert.cost.fee_wei().unwrap() > U256::ZERO);
    let huge = IncludedCost {
        gas_used: u64::MAX,
        effective_gas_price_wei: U256::MAX,
        status: ExecutionStatus::Succeeded,
    };
    assert_eq!(huge.fee_wei(), Err(CostError::Overflow));
}

/// One nonce: a replaced and a dropped attempt cost nothing, the included
/// one (success or revert) once; a second inclusion or the same attempt
/// twice is refused, never summed.
#[test]
fn a_nonce_lifecycle_charges_its_one_inclusion_once() {
    let rows = rows();
    let included = rows[2].cost;
    let reverted = rows
        .iter()
        .find(|r| r.status == ExecutionStatus::Reverted)
        .unwrap();
    let a = |b: u8, outcome| Attempt {
        tx_hash: [b; 32],
        outcome,
    };
    let c = lifecycle_cost(&[
        a(1, AttemptOutcome::NotIncluded),
        a(2, AttemptOutcome::NotIncluded),
        a(3, AttemptOutcome::Included(included)),
    ])
    .unwrap();
    assert_eq!(
        (c.attempts, c.not_included, c.fee_wei, c.included),
        (
            3,
            2,
            included.fee_wei().unwrap(),
            Some(([3; 32], ExecutionStatus::Succeeded))
        )
    );
    // The real reverted receipt, by its own hash.
    let c = lifecycle_cost(&[Attempt {
        tx_hash: reverted.tx,
        outcome: AttemptOutcome::Included(reverted.cost),
    }])
    .unwrap();
    assert_eq!(c.fee_wei, reverted.cost.fee_wei().unwrap());
    assert_eq!(c.included, Some((reverted.tx, ExecutionStatus::Reverted)));
    // Nothing landed: no chain gas.
    let c = lifecycle_cost(&[a(5, AttemptOutcome::NotIncluded)]).unwrap();
    assert_eq!((c.fee_wei, c.included), (U256::ZERO, None));
    assert_eq!(
        lifecycle_cost(&[
            a(6, AttemptOutcome::Included(included)),
            a(7, AttemptOutcome::Included(included))
        ]),
        Err(CostError::DoubleInclusion([6; 32], [7; 32]))
    );
    assert_eq!(
        lifecycle_cost(&[
            a(8, AttemptOutcome::NotIncluded),
            a(8, AttemptOutcome::NotIncluded)
        ]),
        Err(CostError::DuplicateAttempt([8; 32]))
    );
    assert_eq!(lifecycle_cost(&[]), Err(CostError::NoAttempt));
}

/// The counterfactual fee of a class is its largest observed gas used at
/// the replay block's price; a class without evidence fails closed.
#[test]
fn counterfactual_cost_uses_the_class_evidence_or_fails_closed() {
    let mut e = GasEvidence::default();
    for r in rows()
        .iter()
        .filter(|r| r.status == ExecutionStatus::Succeeded || r.class.action == Action::Reverted)
    {
        e.add(r.class, r.cost.gas_used).unwrap();
    }
    let open_launch = PathClass {
        venue: Venue::OpenFour,
        action: Action::Launch,
    };
    let c = e.class(open_launch).unwrap();
    assert_eq!(
        (c.receipts, c.gas_used_min, c.gas_used_max),
        (9, 3_496_036, 4_673_544)
    );
    let price = U256::from(50_000_000u64);
    assert_eq!(
        e.counterfactual_fee_wei(open_launch, price).unwrap(),
        price * U256::from(4_673_544u64)
    );
    // No OpenFour revert was observed: no cost is guessed for it.
    let open_revert = PathClass {
        venue: Venue::OpenFour,
        action: Action::Reverted,
    };
    assert_eq!(
        e.counterfactual_fee_wei(open_revert, price),
        Err(CostError::NoEvidence(open_revert))
    );
    // Every venue has an approval receipt (the sell path's prerequisite).
    for venue in [Venue::Flap, Venue::FourClassic, Venue::OpenFour] {
        assert!(
            e.class(PathClass {
                venue,
                action: Action::Approve
            })
            .is_some()
        );
    }
}
