//! Provider framing: exactly what the plan carries, nothing added, and no
//! secret in what can be inspected.

mod common;

use common::*;
use darkbot_core::inclusion::capability::AdapterTable;
use darkbot_core::inclusion::plan::{MergePolicy, ProviderValidity, SubmissionPlan};
use darkbot_runner::inclusion::frame::{Endpoint, Frame, FrameError};
use darkbot_runner::inclusion::{blockrazor, club48, local_geth};

/// The request expected for a bundle as `redacted_request` shows it: the
/// BUY slot zeros, an authorization value as `*`.
fn expected_request(world: &World, endpoint: &Endpoint<'_>, options: &str) -> String {
    let body = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":\"{}\",\"method\":\"eth_sendBundle\",\
         \"params\":[{{\"txs\":[\"0x{}\",\"0x{}\"],\"maxBlockNumber\":{MAX_BLOCK}{options}}}]}}",
        request_id(world),
        hex(&world.launch_raw),
        "0".repeat(world.raw_buy.len() * 2),
    );
    let authorization = endpoint
        .authorization
        .map(|token| format!("Authorization: {}\r\n", "*".repeat(token.len())))
        .unwrap_or_default();
    format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\n{authorization}\r\n{body}",
        endpoint.path,
        endpoint.host,
        body.len(),
    )
}

fn shown(frame: &Frame) -> String {
    String::from_utf8(frame.redacted_request()).unwrap()
}

#[test]
fn club48_request_carries_exactly_the_birth_entry_envelope() {
    let world = World::prepared();
    let table = frozen_table();
    let plan = both_builders(&world, &table);
    let frame = club48_frame(&world, &plan, &table);

    // `noMerge` and `positionFirst` exactly as the plan derived them for
    // 48Club, and the plan's `maxBlockNumber`. No `revertingTxHashes`, no
    // timestamps, no authorization.
    let expected = expected_request(
        &world,
        &CLUB48_ENDPOINT,
        ",\"noMerge\":true,\"positionFirst\":true",
    );
    assert_eq!(shown(&frame), expected);
    assert_eq!(frame.len(), expected.len());
    assert!(!frame.is_filled());
    assert_eq!(frame.plan(), &plan);
    assert_eq!(frame.adapter(), adapter(CLUB48));
    // The request id is the BUY's hash.
    assert_eq!(frame.request_id(), request_id(&world).as_bytes());
}

#[test]
fn blockrazor_gets_no_merge_only_when_planned_and_never_position_first() {
    let world = World::prepared();
    let table = frozen_table();

    // Both builders under the birth-entry profile: the plan requests
    // `noMerge`; `positionFirst` is not derived for BlockRazor.
    let plan = both_builders(&world, &table);
    let frame = blockrazor_frame(&world, &plan, &table);
    let expected = expected_request(&world, &BLOCKRAZOR_ENDPOINT, ",\"noMerge\":true");
    assert_eq!(shown(&frame), expected);

    // BlockRazor alone under the provider default: neither option.
    let plan = plan_for(
        &world,
        &table,
        &[BLOCKRAZOR],
        MergePolicy::ProviderDefault,
        None,
    );
    let frame = blockrazor_frame(&world, &plan, &table);
    let expected = expected_request(&world, &BLOCKRAZOR_ENDPOINT, "");
    assert_eq!(shown(&frame), expected);
    assert!(!expected.contains("noMerge"));
    assert!(!expected.contains("positionFirst"));
}

#[test]
fn the_authorization_never_shows_in_the_inspectable_request() {
    let world = World::prepared();
    let table = frozen_table();
    let plan = both_builders(&world, &table);
    let frame = blockrazor_frame(&world, &plan, &table);
    let token = BLOCKRAZOR_ENDPOINT.authorization.unwrap();
    let shown = shown(&frame);
    assert!(!shown.contains(token));
    assert!(shown.contains(&format!("Authorization: {}\r\n", "*".repeat(token.len()))));
    // Redacting does not change the length the header announces.
    assert_eq!(shown.len(), frame.len());
}

#[test]
fn timestamps_are_written_only_when_the_plan_sets_them() {
    let world = World::prepared();
    let table = frozen_table();
    let validity = ProviderValidity {
        max_block_number: MAX_BLOCK,
        min_timestamp: Some(1_700_000_000),
        max_timestamp: Some(1_900_000_000),
    };
    let plan = plan_for(
        &world,
        &table,
        &[BLOCKRAZOR],
        MergePolicy::ProviderDefault,
        Some(validity),
    );
    let frame = blockrazor_frame(&world, &plan, &table);
    let expected = expected_request(
        &world,
        &BLOCKRAZOR_ENDPOINT,
        ",\"minTimestamp\":1700000000,\"maxTimestamp\":1900000000",
    );
    assert_eq!(shown(&frame), expected);
}

#[test]
fn the_launch_is_sent_as_given_and_before_the_buy() {
    let world = World::prepared();
    let table = frozen_table();
    let plan = both_builders(&world, &table);
    let frame = club48_frame(&world, &plan, &table);
    let (launch, buy_slot) = bundle_txs(&frame.redacted_request());
    assert_eq!(launch, world.launch_raw);
    assert_eq!(buy_slot, vec![0u8; world.raw_buy.len()]);

    // Other launch bytes than the plan refers to: refused, not re-signed.
    let mut other = world.launch_raw.clone();
    other[5] ^= 0x01;
    let refused = club48::frame(
        &plan,
        &table,
        adapter(CLUB48),
        &CLUB48_ENDPOINT,
        &other,
        FRAME_CAPACITY,
    );
    assert_eq!(refused.err(), Some(FrameError::LaunchBytesMismatch));
}

#[test]
fn local_geth_frame_is_only_the_slot_of_the_raw_buy() {
    let world = World::prepared();
    let table = frozen_table();
    let plan = plan(&world, &table, single_inputs(&world));
    let frame = local_frame(&plan, &table);
    assert_eq!(frame.len(), world.raw_buy.len());
    assert_eq!(frame.redacted_request(), vec![0u8; world.raw_buy.len()]);
    assert!(!frame.is_filled());

    // A bundle plan has no local frame.
    let bundle = both_builders(&world, &table);
    assert_eq!(
        local_geth::frame(&bundle, &table, adapter(LOCAL), FRAME_CAPACITY).err(),
        Some(FrameError::AdapterNotInPlan)
    );
}

#[test]
fn framing_refuses_what_the_plan_does_not_carry() {
    let world = World::prepared();
    let table = frozen_table();
    let plan = both_builders(&world, &table);
    let raw = &world.launch_raw;
    let club = |endpoint: &Endpoint<'_>, capacity: usize| {
        club48::frame(&plan, &table, adapter(CLUB48), endpoint, raw, capacity).err()
    };
    let razor = |endpoint: &Endpoint<'_>| {
        blockrazor::frame(
            &plan,
            &table,
            adapter(BLOCKRAZOR),
            endpoint,
            raw,
            FRAME_CAPACITY,
        )
        .err()
    };

    // The adapter index is bound to another kind.
    let other_kind = club48::frame(
        &plan,
        &table,
        adapter(BLOCKRAZOR),
        &CLUB48_ENDPOINT,
        raw,
        FRAME_CAPACITY,
    );
    assert_eq!(other_kind.err(), Some(FrameError::WrongAdapterKind));
    // The adapter is not part of the plan.
    let lone = plan_for(
        &world,
        &table,
        &[BLOCKRAZOR],
        MergePolicy::ProviderDefault,
        None,
    );
    let not_planned = club48::frame(
        &lone,
        &table,
        adapter(CLUB48),
        &CLUB48_ENDPOINT,
        raw,
        FRAME_CAPACITY,
    );
    assert_eq!(not_planned.err(), Some(FrameError::AdapterNotInPlan));
    // The table is not the one the plan was formed against.
    let record = *table.record(adapter(CLUB48)).unwrap();
    let newer = AdapterTable::for_tests(
        CHAIN,
        CAPABILITY_GENERATION + 1,
        &[(adapter(CLUB48), record)],
    )
    .unwrap();
    let stale = club48::frame(
        &plan,
        &newer,
        adapter(CLUB48),
        &CLUB48_ENDPOINT,
        raw,
        FRAME_CAPACITY,
    );
    assert_eq!(stale.err(), Some(FrameError::StaleCapabilities));

    // Authorization: none for 48Club, required for BlockRazor.
    assert_eq!(
        club(&BLOCKRAZOR_ENDPOINT, FRAME_CAPACITY),
        Some(FrameError::UnexpectedAuthorization)
    );
    let unauthorized = Endpoint {
        authorization: None,
        ..BLOCKRAZOR_ENDPOINT
    };
    assert_eq!(razor(&unauthorized), Some(FrameError::MissingAuthorization));

    // Nothing that could break out of a header.
    for endpoint in [
        Endpoint {
            host: "club48.test\r\nX-Injected: 1",
            ..CLUB48_ENDPOINT
        },
        Endpoint {
            host: "",
            ..CLUB48_ENDPOINT
        },
        Endpoint {
            path: "no-slash",
            ..CLUB48_ENDPOINT
        },
        Endpoint {
            path: "/a b",
            ..CLUB48_ENDPOINT
        },
    ] {
        assert_eq!(
            club(&endpoint, FRAME_CAPACITY),
            Some(FrameError::InvalidEndpoint)
        );
    }
    let injected = Endpoint {
        authorization: Some("token\r\nX-Injected: 1"),
        ..BLOCKRAZOR_ENDPOINT
    };
    assert_eq!(razor(&injected), Some(FrameError::InvalidEndpoint));

    // The request must fit the declared capacity; it never grows.
    let exact = club48_frame(&world, &plan, &table).len();
    assert_eq!(
        club(&CLUB48_ENDPOINT, exact - 1),
        Some(FrameError::Oversize)
    );
    assert_eq!(club(&CLUB48_ENDPOINT, exact), None);
}

/// A bundle plan through `adapters` with `merge` and, if given, another
/// provider validity.
fn plan_for(
    world: &World,
    table: &AdapterTable,
    adapters: &[u8],
    merge: MergePolicy,
    validity: Option<ProviderValidity>,
) -> SubmissionPlan {
    let mut inputs = bundle_inputs(world, adapters, merge);
    if validity.is_some() {
        inputs.provider_validity = validity;
    }
    plan(world, table, inputs)
}
