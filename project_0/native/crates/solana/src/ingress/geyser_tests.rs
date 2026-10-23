use super::proto::subscribe_update::UpdateOneof as U;
use super::proto::*;
use super::*;
use crate::ingress::SlotStatus;
use darkbot_common::channel::bounded;
use std::cell::RefCell;
use std::rc::Rc;

fn update(u: U) -> SubscribeUpdate {
    SubscribeUpdate {
        filters: vec![],
        update_oneof: Some(u),
    }
}

fn account(key: Pubkey, slot: u64, write_version: u64, bank: Option<u64>) -> SubscribeUpdate {
    update(U::Account(SubscribeUpdateAccount {
        account: Some(SubscribeUpdateAccountInfo {
            pubkey: key.to_bytes().to_vec(),
            lamports: 5,
            owner: Pubkey::new_from_array([9; 32]).to_bytes().to_vec(),
            data: vec![1, 2, 3],
            write_version,
            ..Default::default()
        }),
        slot,
        is_startup: false,
        bank_id: bank,
    }))
}

fn slot(
    slot: u64,
    parent: Option<u64>,
    status: proto::SlotStatus,
    bank: Option<u64>,
) -> SubscribeUpdate {
    update(U::Slot(SubscribeUpdateSlot {
        slot,
        parent,
        status: status as i32,
        dead_error: None,
        bank_id: bank,
    }))
}

fn tx(
    slot: u64,
    bank: u64,
    message: MessageBody,
    meta: Option<TransactionStatusMeta>,
) -> SubscribeUpdate {
    update(U::Transaction(SubscribeUpdateTransaction {
        transaction: Some(SubscribeUpdateTransactionInfo {
            signature: vec![7; 64],
            is_vote: false,
            transaction: Some(Transaction {
                signatures: vec![vec![7; 64]],
                message: Some(message),
            }),
            meta,
            index: 3,
        }),
        slot,
        bank_id: bank,
    }))
}

fn message(versioned: bool, lookups: usize, config: bool) -> MessageBody {
    MessageBody {
        header: Some(MessageHeader {
            num_required_signatures: 1,
            num_readonly_signed_accounts: 0,
            num_readonly_unsigned_accounts: 1,
        }),
        account_keys: vec![vec![1; 32], vec![2; 32]],
        recent_blockhash: vec![3; 32],
        instructions: vec![CompiledInstruction {
            program_id_index: 1,
            accounts: vec![0, 2],
            data: vec![9],
        }],
        versioned,
        address_table_lookups: (0..lookups)
            .map(|_| MessageAddressTableLookup {
                account_key: vec![4; 32],
                writable_indexes: vec![0],
                readonly_indexes: vec![1],
            })
            .collect(),
        config: config.then_some(TransactionConfigProto {
            priority_fee: Some(10),
            compute_unit_limit: Some(200_000),
            loaded_accounts_data_size_limit: None,
            heap_size: None,
        }),
    }
}

fn meta() -> TransactionStatusMeta {
    TransactionStatusMeta {
        err: None,
        fee: 5_000,
        pre_balances: vec![10, 0, 0, 0],
        post_balances: vec![5, 0, 0, 0],
        inner_instructions_none: true,
        log_messages: vec!["Program log: x".into()],
        pre_token_balances: vec![TokenBalanceProto {
            account_index: 2,
            mint: Pubkey::new_from_array([5; 32]).to_string(),
            ui_token_amount: Some(UiTokenAmount {
                decimals: 6,
                amount: "18446744073709551615".into(),
            }),
            owner: String::new(),
            program_id: String::new(),
        }],
        loaded_writable_addresses: vec![vec![6; 32]],
        loaded_readonly_addresses: vec![vec![8; 32]],
        compute_units_consumed: Some(1_234),
        ..Default::default()
    }
}

/// The first field key of an encoded message: (field number, wire type).
fn first_key(bytes: &[u8]) -> (u32, u8) {
    let (mut v, mut shift) = (0u64, 0);
    for b in bytes {
        v |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    ((v >> 3) as u32, (v & 7) as u8)
}

/// S1-02 proto fidelity: every field this adapter reads or writes has the number and wire type
/// of the pinned `geyser.proto` / `solana-storage.proto` (commit `2a548669`); a drifted tag fails.
#[test]
fn field_numbers_match_the_pinned_proto() {
    let (varint, len) = (0u8, 2u8);
    let k = |m: &dyn Fn() -> Vec<u8>| first_key(&m());
    let info = |f: &dyn Fn(&mut SubscribeUpdateAccountInfo)| {
        let mut m = SubscribeUpdateAccountInfo::default();
        f(&mut m);
        m.encode_to_vec()
    };
    assert_eq!(first_key(&info(&|m| m.pubkey = vec![1])), (1, len));
    assert_eq!(first_key(&info(&|m| m.lamports = 1)), (2, varint));
    assert_eq!(first_key(&info(&|m| m.owner = vec![1])), (3, len));
    assert_eq!(first_key(&info(&|m| m.data = vec![1])), (6, len));
    assert_eq!(first_key(&info(&|m| m.write_version = 1)), (7, varint));
    let acc = |f: &dyn Fn(&mut SubscribeUpdateAccount)| {
        let mut m = SubscribeUpdateAccount::default();
        f(&mut m);
        m.encode_to_vec()
    };
    assert_eq!(
        first_key(&acc(&|m| m.account = Some(Default::default()))),
        (1, len)
    );
    assert_eq!(first_key(&acc(&|m| m.slot = 1)), (2, varint));
    assert_eq!(first_key(&acc(&|m| m.is_startup = true)), (3, varint));
    assert_eq!(first_key(&acc(&|m| m.bank_id = Some(1))), (4, varint));
    let sl = |f: &dyn Fn(&mut SubscribeUpdateSlot)| {
        let mut m = SubscribeUpdateSlot::default();
        f(&mut m);
        m.encode_to_vec()
    };
    assert_eq!(first_key(&sl(&|m| m.slot = 1)), (1, varint));
    assert_eq!(first_key(&sl(&|m| m.parent = Some(1))), (2, varint));
    assert_eq!(first_key(&sl(&|m| m.status = 2)), (3, varint));
    assert_eq!(
        first_key(&sl(&|m| m.dead_error = Some("x".into()))),
        (4, len)
    );
    assert_eq!(first_key(&sl(&|m| m.bank_id = Some(1))), (5, varint));
    let ti = |f: &dyn Fn(&mut SubscribeUpdateTransactionInfo)| {
        let mut m = SubscribeUpdateTransactionInfo::default();
        f(&mut m);
        m.encode_to_vec()
    };
    assert_eq!(first_key(&ti(&|m| m.signature = vec![1])), (1, len));
    assert_eq!(
        first_key(&ti(&|m| m.transaction = Some(Default::default()))),
        (3, len)
    );
    assert_eq!(
        first_key(&ti(&|m| m.meta = Some(Default::default()))),
        (4, len)
    );
    assert_eq!(first_key(&ti(&|m| m.index = 1)), (5, varint));
    let t = |f: &dyn Fn(&mut SubscribeUpdateTransaction)| {
        let mut m = SubscribeUpdateTransaction::default();
        f(&mut m);
        m.encode_to_vec()
    };
    assert_eq!(first_key(&t(&|m| m.slot = 1)), (2, varint));
    assert_eq!(first_key(&t(&|m| m.bank_id = 1)), (3, varint));
    let mb = |f: &dyn Fn(&mut MessageBody)| {
        let mut m = MessageBody::default();
        f(&mut m);
        m.encode_to_vec()
    };
    assert_eq!(
        first_key(&mb(&|m| m.header = Some(Default::default()))),
        (1, len)
    );
    assert_eq!(
        first_key(&mb(&|m| m.account_keys = vec![vec![1]])),
        (2, len)
    );
    assert_eq!(first_key(&mb(&|m| m.recent_blockhash = vec![1])), (3, len));
    assert_eq!(
        first_key(&mb(&|m| m.instructions = vec![Default::default()])),
        (4, len)
    );
    assert_eq!(first_key(&mb(&|m| m.versioned = true)), (5, varint));
    assert_eq!(
        first_key(&mb(&|m| m.address_table_lookups = vec![Default::default()])),
        (6, len)
    );
    assert_eq!(
        first_key(&mb(&|m| m.config = Some(Default::default()))),
        (7, len)
    );
    let me = |f: &dyn Fn(&mut TransactionStatusMeta)| {
        let mut m = TransactionStatusMeta::default();
        f(&mut m);
        m.encode_to_vec()
    };
    assert_eq!(
        first_key(&me(&|m| m.err = Some(Default::default()))),
        (1, len)
    );
    assert_eq!(first_key(&me(&|m| m.fee = 1)), (2, varint));
    assert_eq!(first_key(&me(&|m| m.pre_balances = vec![1])), (3, len));
    assert_eq!(
        first_key(&me(&|m| m.inner_instructions_none = true)),
        (10, varint)
    );
    assert_eq!(
        first_key(&me(&|m| m.log_messages_none = true)),
        (11, varint)
    );
    assert_eq!(
        first_key(&me(&|m| m.loaded_writable_addresses = vec![vec![1]])),
        (12, len)
    );
    assert_eq!(
        first_key(&me(&|m| m.loaded_readonly_addresses = vec![vec![1]])),
        (13, len)
    );
    assert_eq!(
        first_key(&me(&|m| m.compute_units_consumed = Some(1))),
        (16, varint)
    );
    assert_eq!(first_key(&me(&|m| m.cost_units = Some(1))), (17, varint));
    for (u, tag) in [
        (U::Account(Default::default()), 2),
        (U::Slot(Default::default()), 3),
        (U::Transaction(Default::default()), 4),
        (U::Ping(Default::default()), 6),
        (U::Pong(Default::default()), 9),
        (U::EntryUpdateParent(Default::default()), 13),
    ] {
        assert_eq!(k(&|| update(u.clone()).encode_to_vec()), (tag, len));
    }
    let rq = |f: &dyn Fn(&mut SubscribeRequest)| {
        let mut m = SubscribeRequest::default();
        f(&mut m);
        m.encode_to_vec()
    };
    assert_eq!(
        first_key(&rq(&|m| {
            m.accounts.insert("a".into(), Default::default());
        })),
        (1, len)
    );
    assert_eq!(
        first_key(&rq(&|m| {
            m.slots.insert("s".into(), Default::default());
        })),
        (2, len)
    );
    assert_eq!(
        first_key(&rq(&|m| {
            m.transactions.insert("t".into(), Default::default());
        })),
        (3, len)
    );
    assert_eq!(first_key(&rq(&|m| m.commitment = Some(1))), (6, varint));
    assert_eq!(
        first_key(&rq(&|m| m.ping = Some(Default::default()))),
        (9, len)
    );
    assert_eq!(first_key(&rq(&|m| m.from_slot = Some(1))), (11, varint));
    assert_eq!(
        first_key(
            &SubscribeRequestFilterAccounts {
                owner: vec!["o".into()],
                ..Default::default()
            }
            .encode_to_vec()
        ),
        (3, len)
    );
    assert_eq!(
        first_key(
            &Memcmp {
                offset: 1,
                bytes: None
            }
            .encode_to_vec()
        ),
        (1, varint)
    );
}

/// Accounts keep `write_version`; an exact duplicate is dropped; another bank for the same slot
/// is an ambiguous fork (Gap, the update is not delivered); broken keys are protocol errors.
#[test]
fn accounts_keep_write_version_drop_duplicates_and_gap_on_fork_ambiguity() {
    let mut m = GeyserMapper::new();
    let k = Pubkey::new_unique();
    let a = m.map(account(k, 10, 7, Some(1)), 1).unwrap();
    let IngressEvent::Account(u) = &a.events[0] else {
        panic!()
    };
    assert_eq!(
        (u.slot, u.write_version, u.lamports, u.source),
        (10, Some(7), 5, SourceKind::Geyser)
    );
    assert_eq!(u.data, [1, 2, 3]);
    assert!(m
        .map(account(k, 10, 7, Some(1)), 2)
        .unwrap()
        .events
        .is_empty());
    assert_eq!(m.stats.duplicates, 1);
    assert_eq!(
        m.map(account(k, 10, 8, Some(1)), 3).unwrap().events.len(),
        1,
        "newer version"
    );
    let fork = m.map(account(k, 10, 9, Some(2)), 4).unwrap();
    assert!(matches!(
        fork.events[..],
        [IngressEvent::Gap {
            from_slot: Some(9),
            to_slot: Some(10),
            ..
        }]
    ));
    let mut broken = account(k, 11, 1, None);
    if let Some(U::Account(a)) = &mut broken.update_oneof {
        a.account.as_mut().unwrap().pubkey.pop();
    }
    assert_eq!(m.map(broken, 5).unwrap_err(), "GEYSER_ACCOUNT_KEY_LENGTH");
    assert_eq!(
        m.map(SubscribeUpdate::default(), 6).unwrap_err(),
        "GEYSER_UPDATE_KIND_UNKNOWN"
    );
}

/// Slots: processed/confirmed/finalized map to `SlotUpdate`, finalized also to `Root`; a dead
/// slot, a second bank for a slot, a cleared bank and a never-seen parent are gaps; an unknown
/// status is a protocol error; first-shred/completed are no canonical events.
#[test]
fn slots_roots_dead_slots_banks_and_parent_gaps() {
    use proto::SlotStatus as P;
    let mut m = GeyserMapper::new();
    let ev = m
        .map(slot(10, Some(9), P::SlotProcessed, None), 1)
        .unwrap()
        .events;
    assert!(matches!(
        ev[..],
        [IngressEvent::Slot(SlotUpdate {
            slot: 10,
            parent: Some(9),
            status: SlotStatus::Processed,
            ..
        })]
    ));
    let ev = m
        .map(slot(10, Some(9), P::SlotFinalized, None), 2)
        .unwrap()
        .events;
    assert!(matches!(
        ev[..],
        [IngressEvent::Slot(_), IngressEvent::Root { slot: 10, .. }]
    ));
    assert_eq!(m.root, Some(10));
    assert!(m
        .map(slot(11, Some(10), P::SlotFirstShredReceived, None), 3)
        .unwrap()
        .events
        .is_empty());
    assert!(m
        .map(slot(11, Some(10), P::SlotCreatedBank, Some(5)), 4)
        .unwrap()
        .events
        .is_empty());
    let ev = m
        .map(slot(11, Some(10), P::SlotCreatedBank, Some(6)), 5)
        .unwrap()
        .events;
    assert!(
        matches!(
            ev[..],
            [IngressEvent::Gap {
                to_slot: Some(11),
                ..
            }]
        ),
        "second bank"
    );
    let ev = m
        .map(slot(12, Some(11), P::SlotDead, None), 6)
        .unwrap()
        .events;
    assert!(
        matches!(
            ev[..],
            [IngressEvent::Gap {
                to_slot: Some(12),
                ..
            }]
        ),
        "dead slot"
    );
    m.map(slot(13, Some(12), P::SlotCreatedBank, Some(9)), 7)
        .unwrap();
    let clear = update(U::EntryUpdateParent(SubscribeUpdateEntryUpdateParent {
        slot: 13,
        cleared_bank_id: 9,
        parent_slot: 11,
    }));
    assert!(matches!(
        m.map(clear, 8).unwrap().events[..],
        [IngressEvent::Gap {
            to_slot: Some(13),
            ..
        }]
    ));
    let ev = m
        .map(slot(20, Some(18), P::SlotProcessed, None), 9)
        .unwrap()
        .events;
    assert!(
        matches!(
            ev[..],
            [
                IngressEvent::Gap {
                    from_slot: Some(10),
                    to_slot: Some(20),
                    ..
                },
                IngressEvent::Slot(_)
            ]
        ),
        "{ev:?}"
    );
    let mut unknown = slot(21, None, P::SlotProcessed, None);
    if let Some(U::Slot(s)) = &mut unknown.update_oneof {
        s.status = 99;
    }
    assert_eq!(
        m.map(unknown, 10).unwrap_err(),
        "GEYSER_SLOT_STATUS_UNKNOWN"
    );
}

/// Transactions map onto the shared `ChainTx`: legacy, v0 with ALT and loaded addresses (the
/// account keys resolve), v1 with its config; statuses from the bincode error; `*_none` flags
/// keep absence; a broken field makes only the payload an error (envelope kept).
#[test]
fn transactions_map_legacy_v0_alt_v1_and_status() {
    let mut m = GeyserMapper::new();
    let observed =
        |u: SubscribeUpdate, m: &mut GeyserMapper| match m.map(u, 1).unwrap().events.pop() {
            Some(IngressEvent::Transaction(t)) => *t,
            other => panic!("{other:?}"),
        };
    let legacy = observed(tx(5, 0, message(false, 0, false), Some(meta())), &mut m);
    let chain = legacy.tx.unwrap();
    assert_eq!(
        (chain.version, legacy.tx_index_in_slot),
        (Some(TxWireVersion::Legacy), Some(3))
    );
    assert_eq!(legacy.signature, bs58::encode([7u8; 64]).into_string());
    assert_eq!(chain.message.address_table_lookups, None);
    let meta_out = chain.meta.unwrap();
    assert_eq!(
        (meta_out.status.clone(), meta_out.fee),
        (TxStatus::Success, Some(5_000))
    );
    assert_eq!(meta_out.inner_instructions, None, "inner_instructions_none");
    assert_eq!(meta_out.pre_token_balances.unwrap()[0].amount, u64::MAX);
    let v0 = observed(tx(5, 0, message(true, 1, false), Some(meta())), &mut m)
        .tx
        .unwrap();
    assert_eq!(v0.version, Some(TxWireVersion::Number(0)));
    let keys = v0.account_keys().unwrap();
    assert_eq!(keys.len(), 4, "static + loaded writable + loaded readonly");
    assert_eq!(keys[2], Pubkey::new_from_array([6; 32]));
    let v1 = observed(tx(5, 0, message(true, 0, true), None), &mut m)
        .tx
        .unwrap();
    assert_eq!(v1.version, Some(TxWireVersion::Number(1)));
    assert_eq!(
        v1.message.transaction_config.unwrap().compute_unit_limit,
        Some(200_000)
    );
    let mut failed = meta();
    failed.err = Some(proto::TransactionError {
        err: bincode::serialize(&solana_sdk::transaction::TransactionError::AccountInUse).unwrap(),
    });
    let status = |meta: TransactionStatusMeta, m: &mut GeyserMapper| {
        observed(tx(5, 0, message(false, 0, false), Some(meta)), m)
            .tx
            .unwrap()
            .meta
            .unwrap()
            .status
    };
    assert_eq!(
        status(failed, &mut m),
        TxStatus::Failed(solana_sdk::transaction::TransactionError::AccountInUse)
    );
    let mut garbled = meta();
    garbled.err = Some(proto::TransactionError { err: vec![0xff; 3] });
    assert_eq!(
        status(garbled, &mut m),
        TxStatus::Unknown("TX_ERROR_UNDECODABLE")
    );
    let mut broken = message(true, 1, false);
    broken.account_keys[0].pop();
    let envelope = observed(tx(5, 0, broken, None), &mut m);
    assert_eq!(envelope.tx.unwrap_err(), ChainError("GEYSER_KEY_LENGTH"));
    assert_eq!(envelope.slot, 5);
    let inconsistent = observed(tx(5, 0, message(false, 1, false), None), &mut m);
    assert_eq!(
        inconsistent.tx.unwrap_err(),
        ChainError("GEYSER_LEGACY_WITH_V0_V1_FIELDS")
    );
}

/// Framing: a message split over chunks is decoded once complete; a compressed or oversized
/// frame is a protocol error.
#[test]
fn frames_are_bounded_and_uncompressed() {
    let bytes = frame(&account(Pubkey::new_unique(), 1, 1, None));
    let mut f = FrameDecoder::new(1 << 20);
    f.push(&bytes[..3]);
    assert!(f.next_message::<SubscribeUpdate>().unwrap().is_none());
    f.push(&bytes[3..]);
    assert!(f.next_message::<SubscribeUpdate>().unwrap().is_some());
    assert!(f.next_message::<SubscribeUpdate>().unwrap().is_none());
    let mut compressed = bytes.clone();
    compressed[0] = 1;
    let mut f = FrameDecoder::new(1 << 20);
    f.push(&compressed);
    assert_eq!(
        f.next_message::<SubscribeUpdate>().unwrap_err(),
        "GEYSER_FRAME_COMPRESSED"
    );
    let mut f = FrameDecoder::new(4);
    f.push(&bytes);
    assert_eq!(
        f.next_message::<SubscribeUpdate>().unwrap_err(),
        "GEYSER_FRAME_TOO_LARGE"
    );
}

/// The request carries owners, accounts, filters, slots, transactions (no votes), commitment
/// and `from_slot` only when given.
#[test]
fn subscribe_request_encodes_the_subscription() {
    let (program, account_key) = (Pubkey::new_unique(), Pubkey::new_unique());
    let s = Subscription {
        programs: vec![program],
        program_filters: vec![(program, vec![super::super::ProgramFilter::DataSize(165)])],
        accounts: vec![account_key],
        slots: true,
        roots: true,
        transactions_mentioning: vec![program],
        ..Default::default()
    };
    let r = SubscribeRequest::decode(
        subscribe_request(&s, proto::CommitmentLevel::Processed, None).as_slice(),
    )
    .unwrap();
    assert_eq!(r.accounts["accounts"].owner, [program.to_string()]);
    assert_eq!(r.accounts["accounts"].account, [account_key.to_string()]);
    assert_eq!(
        r.accounts["program_0"].filters[0].filter,
        Some(proto::accounts_filter::Filter::Datasize(165))
    );
    assert!(r.slots.contains_key("slots"));
    assert_eq!(r.transactions["transactions"].vote, Some(false));
    assert_eq!((r.commitment, r.from_slot), (Some(0), None));
    let resumed = SubscribeRequest::decode(
        subscribe_request(&s, proto::CommitmentLevel::Confirmed, Some(77)).as_slice(),
    )
    .unwrap();
    assert_eq!((resumed.commitment, resumed.from_slot), (Some(1), Some(77)));
}

/// K-INGRESS-PRE gate: `SubscribeDeshred` ALT resolution needs geyser 14.2.0 or later.
#[test]
fn deshred_needs_the_pinned_alt_fix() {
    for (v, ok) in [
        ("14.2.0", true),
        ("v15.0.1", true),
        ("14.1.9", false),
        ("13.2.0", false),
        ("x", false),
    ] {
        assert_eq!(deshred_supported(v), ok, "{v}");
    }
}

/// Mock transport: scripted streams (chunks) per connect, recorded requests and pongs.
struct Mock {
    streams: RefCell<VecDeque<Result<Vec<Vec<u8>>, StreamError>>>,
    requests: Rc<RefCell<Vec<Vec<u8>>>>,
    pongs: Rc<RefCell<Vec<i32>>>,
}
struct MockStream {
    chunks: VecDeque<Vec<u8>>,
    pongs: Rc<RefCell<Vec<i32>>>,
}
impl GeyserStream for MockStream {
    fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, StreamError> {
        Ok(self.chunks.pop_front())
    }
    fn pong(&mut self, id: i32) -> Result<(), StreamError> {
        self.pongs.borrow_mut().push(id);
        Ok(())
    }
}
impl GeyserTransport for Mock {
    type Stream = MockStream;
    fn connect(&mut self, request: &[u8]) -> Result<MockStream, StreamError> {
        self.requests.borrow_mut().push(request.to_vec());
        match self.streams.borrow_mut().pop_front() {
            Some(Ok(chunks)) => Ok(MockStream {
                chunks: chunks.into(),
                pongs: self.pongs.clone(),
            }),
            Some(Err(e)) => Err(e),
            None => Err(StreamError::Retryable("down".into())),
        }
    }
}

type Scripted = (
    BinaryGeyserSource<Mock>,
    Rc<RefCell<Vec<Vec<u8>>>>,
    Rc<RefCell<Vec<i32>>>,
);

fn source(streams: Vec<Result<Vec<Vec<u8>>, StreamError>>, replay: bool) -> Scripted {
    let (requests, pongs) = (Rc::new(RefCell::new(vec![])), Rc::new(RefCell::new(vec![])));
    let mut config = GeyserConfig::publicnode();
    config.max_reconnects = 2;
    config.replay_from_slot = replay;
    (
        BinaryGeyserSource {
            config,
            subscription: Subscription {
                slots: true,
                ..Default::default()
            },
            transport: Mock {
                streams: RefCell::new(streams.into()),
                requests: requests.clone(),
                pongs: pongs.clone(),
            },
            clock: Clock::new(),
        },
        requests,
        pongs,
    )
}

fn drain(rx: &darkbot_common::channel::Consumer<IngressEvent>) -> Vec<IngressEvent> {
    std::iter::from_fn(|| rx.try_pop()).collect()
}

/// S1-08 offline source: frames split across chunks are delivered in order; a server ping gets a
/// pong; a stream end reconnects with `Reconnected` and, without proven replay, a `Gap` from the
/// last slot; exhausted reconnects end `Fatal` with the halt raised. PublicNode is configured,
/// never connected.
#[test]
fn source_reconnects_with_gap_answers_pings_and_fails_closed() {
    use proto::SlotStatus as P;
    let k = Pubkey::new_unique();
    let mut first = Vec::new();
    first.extend(frame(&account(k, 30, 1, Some(4))));
    first.extend(frame(&update(U::Ping(Default::default()))));
    first.extend(frame(&slot(30, Some(29), P::SlotFinalized, None)));
    let (a, b) = first.split_at(7);
    let second = frame(&account(k, 31, 2, Some(5)));
    let (src, requests, pongs) = source(
        vec![Ok(vec![a.to_vec(), b.to_vec()]), Ok(vec![second])],
        false,
    );
    assert_eq!(src.config.endpoint, "https://solana.publicnode.com");
    let (tx, rx) = bounded("geyser", 64);
    let halt = IngressHalt::new();
    let err = src
        .run(tx, Shutdown::new(), halt.clone(), SlotWatch::new())
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "ingress protocol: GEYSER_RECONNECTS_EXHAUSTED"
    );
    let events = drain(&rx);
    let kinds: Vec<_> = events
        .iter()
        .map(|e| match e {
            IngressEvent::Account(_) => "account",
            IngressEvent::Slot(_) => "slot",
            IngressEvent::Root { .. } => "root",
            IngressEvent::Reconnected { .. } => "reconnected",
            IngressEvent::Gap { .. } => "gap",
            IngressEvent::Fatal { .. } => "fatal",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "account",
            "slot",
            "root",
            "reconnected",
            "gap",
            "account",
            "fatal"
        ]
    );
    assert!(matches!(
        events[4],
        IngressEvent::Gap {
            from_slot: Some(30),
            to_slot: None,
            ..
        }
    ));
    assert_eq!(*pongs.borrow(), [1]);
    assert_eq!(
        requests.borrow().len(),
        3,
        "two streams, one failed connect"
    );
    assert!(halt.is_raised());
    // With proven replay the reconnect resumes from the last slot instead of a gap.
    let (src, requests, _) = source(
        vec![Ok(vec![frame(&account(k, 40, 1, None))]), Ok(vec![])],
        true,
    );
    let (tx, rx) = bounded("geyser", 64);
    let _ = src.run(tx, Shutdown::new(), IngressHalt::new(), SlotWatch::new());
    assert!(!drain(&rx)
        .iter()
        .any(|e| matches!(e, IngressEvent::Gap { .. })));
    let resumed = SubscribeRequest::decode(requests.borrow()[1].as_slice()).unwrap();
    assert_eq!(resumed.from_slot, Some(40));
}

/// Fail closed: an update kind the pinned proto does not know, a compressed frame and a full sink
/// each stop the source with the halt raised (never a silent drop).
#[test]
fn unknown_kind_compressed_frame_and_overflow_halt() {
    let unknown = {
        // field 14 is not in the pinned `SubscribeUpdate` oneof
        let body = [0x72u8, 0x00];
        let mut f = vec![0u8];
        f.extend_from_slice(&(body.len() as u32).to_be_bytes());
        f.extend_from_slice(&body);
        f
    };
    let mut compressed = frame(&account(Pubkey::new_unique(), 1, 1, None));
    compressed[0] = 1;
    for (bytes, code) in [
        (unknown, "GEYSER_UPDATE_KIND_UNKNOWN"),
        (compressed, "GEYSER_FRAME_COMPRESSED"),
    ] {
        let (src, _, _) = source(vec![Ok(vec![bytes])], false);
        let (tx, rx) = bounded("geyser", 8);
        let halt = IngressHalt::new();
        let err = src
            .run(tx, Shutdown::new(), halt.clone(), SlotWatch::new())
            .unwrap_err();
        assert_eq!(err.to_string(), format!("ingress protocol: {code}"));
        assert!(halt.is_raised());
        assert!(matches!(
            drain(&rx).last(),
            Some(IngressEvent::Fatal { .. })
        ));
    }
    let k = Pubkey::new_unique();
    let many: Vec<u8> = (0..3)
        .flat_map(|v| frame(&account(k, 9, v, None)))
        .collect();
    let (src, _, _) = source(vec![Ok(vec![many])], false);
    let (tx, _rx) = bounded("geyser", 1);
    let halt = IngressHalt::new();
    let err = src
        .run(tx, Shutdown::new(), halt.clone(), SlotWatch::new())
        .unwrap_err();
    assert_eq!(err.to_string(), "ingress protocol: GEYSER_SINK_OVERFLOW");
    assert!(halt.is_raised());
}

/// Production path: Geyser-mapped events feed the one canonical `StateCore` (no second state
/// core). Out-of-order updates lose (older write version, older slot); a fork switch orphans the
/// writes of the abandoned slot at the next root (halt); a gap halts.
#[test]
fn geyser_events_feed_the_one_canonical_core() {
    use crate::state::{ApplyOutcome, StateCore};
    use proto::SlotStatus as P;
    let mut m = GeyserMapper::new();
    let mut core = StateCore::new(16);
    core.install_snapshot(5, vec![]);
    let mut apply = |u: SubscribeUpdate, core: &mut StateCore| -> Vec<Option<ApplyOutcome>> {
        m.map(u, 1)
            .unwrap()
            .events
            .into_iter()
            .map(|e| core.on_event(e))
            .collect()
    };
    let (k, orphan) = (Pubkey::new_unique(), Pubkey::new_unique());
    assert_eq!(
        apply(account(k, 10, 2, Some(1)), &mut core),
        [Some(ApplyOutcome::Applied)]
    );
    assert_eq!(
        apply(account(k, 10, 1, Some(1)), &mut core),
        [Some(ApplyOutcome::StaleWriteVersion)]
    );
    assert_eq!(
        apply(account(k, 9, 5, None), &mut core),
        [Some(ApplyOutcome::StaleSlot)]
    );
    assert_eq!(core.get(&k).unwrap().write_version, Some(2));
    assert_eq!(core.get(&k).unwrap().source, SourceKind::Geyser);
    apply(slot(11, Some(10), P::SlotProcessed, None), &mut core);
    apply(slot(12, Some(10), P::SlotProcessed, None), &mut core);
    assert_eq!(
        apply(account(orphan, 11, 1, None), &mut core),
        [Some(ApplyOutcome::Applied)]
    );
    apply(slot(12, Some(10), P::SlotFinalized, None), &mut core);
    assert!(
        core.get(&orphan).is_none(),
        "writes of the abandoned fork are dropped"
    );
    assert_eq!(core.counters().orphaned_writes, 1);
    assert!(!core.run_state().is_tradable());
    let ev = m
        .map(slot(30, Some(20), P::SlotProcessed, None), 2)
        .unwrap()
        .events;
    assert!(matches!(ev[0], IngressEvent::Gap { .. }));
    for e in ev {
        core.on_event(e);
    }
    assert!(core.gap_open());
}
