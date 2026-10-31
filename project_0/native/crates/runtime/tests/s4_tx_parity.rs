//! S4-01/S4-03 (PHASENPLAN S4: TX Build, Serialization, Signatures, relevante Fees): the parity
//! corpus of the six real Mainnet receivership liquidations committed in
//! `docs/evidence/AUDIT_20261005` (raw `getTransaction` answers, each pinned by its SHA-256)
//! against the production paths — the wire decode (`chain::decode_transaction`), the rebuilt
//! signed bytes (`tx::chain_message_bytes`, `tx::chain_wire`), the plan gate (`planner::steps_of`,
//! `validate_receivership`, `receivership_allowlist`), the receivership builders and their layout
//! (`receivership::builders`), the compile (`tx::build`, `tx::v1_message_bytes`, the E2 check
//! `tx::message_matches_plan`) and the fee (`pipeline::planned_fee_lamports`).
//!
//! Every deviation is one of these explained rules; no other passes:
//! - E-OPT: a winner writes an absent `withdraw_all`/`repay_all` as `Some(false)`, the core as
//!   `None`; the program reads both as `unwrap_or(false)` — the same instruction.
//! - E-HEAP: the v1 winners request the default 32 KiB heap frame explicitly, the core leaves the
//!   default — the same heap at no cost.
//! - E-PRIV: a winner asks for the marginfi group writable, which every one of its instructions
//!   only reads (the official account metas, read-only in both pinned program versions), or for
//!   the invoked marginfi program writable, which the runtime demotes (no upgradeable loader in
//!   the message: checked); the core asks for exactly the program's privileges — less lock
//!   contention, the same instructions.
//! - E-FEE: a winner names its own wallet as EndLiquidation's optional `fee_payer`; without one
//!   the liquidation receiver pays the flat fee — the same wallet, so the same payer.
//! - E-ORDER: one winner was compiled by a client with another account order; the E2 check finds
//!   exactly the core's instructions and keys (with the winner's privileges) in its message.
use darkbot_project0::planner::{self, RouteKind, Step};
use darkbot_project0::protocol;
use darkbot_project0::receivership::{builders, ReceivershipParams};
use darkbot_solana::chain::{ChainTx, TxWireVersion};
use darkbot_solana::tx::{self, BuildVersion, Resources, TxLimits, TxPlan};
use solana_message::AddressLookupTableAccount;
use solana_sdk::hash::Hash;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::message::VersionedMessage;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use std::path::Path;
use std::str::FromStr;

/// How the core's compile of a winner relates to the bytes it signed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Compile {
    /// The core compiles exactly the signed bytes: the winner's signature verifies over them.
    Identical,
    /// E-HEAP: exactly the signed bytes without the winner's explicit default heap request.
    HeapRequest,
    /// E-ORDER: another account order, the same instructions and keys.
    OtherOrder,
}

/// E-PRIV: the key a winner asks writable beyond the program's privileges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Raised {
    Group,
    Program,
}

/// S4-01: one corpus winner and the explained deviations its parity shows — exactly these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Winner {
    signature: &'static str,
    /// SHA-256 of its committed `getTransaction` answer.
    sha256: &'static str,
    compile: Compile,
    /// E-OPT: its instructions whose arguments the core encodes with `None`.
    opt: usize,
    raised: Option<Raised>,
    /// E-FEE: it names the receiver as EndLiquidation's `fee_payer`.
    fee_payer: bool,
}

/// S4-01: the corpus.
const CORPUS: [Winner; 6] = [
    Winner {
        signature: "2cebUJBWZYNW9ZNRweyV4NygKshP6dcnR4M8z3rTUHjNA6yEfkWWLRpXdqRtBPpTF68BGph964ebYtuEcxdvHJEf",
        sha256: "9822cde8c32c5544fe90571c6a208e3b770a0effe0e92e1388a59b4fecad4329",
        compile: Compile::HeapRequest,
        opt: 2,
        raised: Some(Raised::Group),
        fee_payer: false,
    },
    Winner {
        signature: "38ZXvW6P5sGZmdzj4QbUPSdQnCnJEBHoXTh4vvvfitFPNDHRsgPjELCnFxxCfW2wygE8J6k2Yv8RJJXPUVchBm7F",
        sha256: "e657660c193834f907ae90782c9353439a292bf3c21d0e0ba2290559824d43b6",
        compile: Compile::HeapRequest,
        opt: 2,
        raised: Some(Raised::Group),
        fee_payer: false,
    },
    Winner {
        signature: "3ab7mnuTErWjRtS8iz4Nm619VNvm3FSw47VVvdjYnSLNzB2btCoreDsQqmzfb4Qt5oBA29saPQ98Rgy2nyAUExRB",
        sha256: "3a0d2e639d1efea1fef30ff6ff9dbc4f0476056f04e8559e5df4d466d51c1643",
        compile: Compile::Identical,
        opt: 2,
        raised: Some(Raised::Program),
        fee_payer: false,
    },
    Winner {
        signature: "59Qt6h1iPnEgfaDaEi5Y5jVvqHJCs4gzwvHqWW37wVJMHyThYjM6zsS8AmE9bvoxqdUj8AMnNbBuKHQtXysdnwNa",
        sha256: "b165bd3f55a931a9475614952fad12a5dc940ad18d85ac8822eedbf80a8b95e0",
        compile: Compile::OtherOrder,
        opt: 0,
        raised: None,
        fee_payer: true,
    },
    Winner {
        signature: "C6JnpzGrv53y9dEExtpDYkzTbexLEu7oikPRNx8sCqf85Jvobc7UBpsaDSgGHzEgeB88HkVFqMkCkvs1ZqVWRwZ",
        sha256: "5dbaa1dd5bccdeb89637e8fb3fdb62c8c029c0ec1b1d9ed9df353664d52a8251",
        compile: Compile::Identical,
        opt: 2,
        raised: Some(Raised::Program),
        fee_payer: false,
    },
    Winner {
        signature: "gLVXyfVZmizZqpF9752x3uhgJixW5Jqrp3Agjhhr6cNUnU5aQDBAxQ1CF9RWSpy2oB2K9EAroFJaGPTkz5vkUzB",
        sha256: "3fcb335041090423158fb05d883b4c55e7e0897c9bbe9fdba541f8b7a6964dbb",
        compile: Compile::HeapRequest,
        opt: 2,
        raised: Some(Raised::Group),
        fee_payer: false,
    },
];

/// The default heap frame of the runtime (MIN_HEAP_FRAME_BYTES): what an explicit E-HEAP
/// request asks for.
const DEFAULT_HEAP_FRAME_BYTES: u32 = 32 * 1024;

fn corpus() -> Vec<(ChainTx, Winner)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/evidence/AUDIT_20261005");
    CORPUS
        .iter()
        .map(|&winner| {
            let (signature, sha256) = (winner.signature, winner.sha256);
            let path = dir.join(format!("{signature}.json"));
            let digest = darkbot_runner::analytics::journal::file_sha256(&path).unwrap();
            assert_eq!(
                darkbot_runner::analytics::journal::hex(&digest),
                sha256,
                "{signature}: the corpus file changed"
            );
            let raw = std::fs::read(&path).unwrap();
            let tx =
                darkbot_solana::chain::decode_transaction(&serde_json::from_slice(&raw).unwrap())
                    .unwrap();
            assert_eq!(tx.signature(), Some(signature));
            assert!(tx.status().is_success(), "{signature}");
            (tx, winner)
        })
        .collect()
}

fn version(tx: &ChainTx) -> TxWireVersion {
    tx.version.expect("the corpus names every version")
}

/// The instructions of `tx` with the privileges its message gives each key (signer: the first
/// `num_required_signatures` keys; writable by the header, loaded writable addresses).
fn decompiled(tx: &ChainTx) -> Vec<Instruction> {
    let keys = tx.account_keys().unwrap();
    let (required, readonly_signed, readonly_unsigned) = tx.message.header.unwrap();
    let (required, readonly_signed, readonly_unsigned) = (
        usize::from(required),
        usize::from(readonly_signed),
        usize::from(readonly_unsigned),
    );
    let statics = tx.message.account_keys.len();
    let loaded_writable = tx
        .meta
        .as_ref()
        .and_then(|m| m.loaded_addresses.as_ref())
        .map_or(0, |l| l.writable.len());
    let writable = |i: usize| {
        if i < required {
            i < required - readonly_signed
        } else if i < statics {
            i < statics - readonly_unsigned
        } else {
            i < statics + loaded_writable
        }
    };
    tx.message
        .instructions
        .iter()
        .map(|ix| Instruction {
            program_id: keys[usize::from(ix.program_id_index)],
            accounts: ix
                .accounts
                .iter()
                .map(|&a| AccountMeta {
                    pubkey: keys[usize::from(a)],
                    is_signer: usize::from(a) < required,
                    is_writable: writable(usize::from(a)),
                })
                .collect(),
            data: ix.data.clone(),
        })
        .collect()
}

/// The lookup tables a v0 winner loads from, as far as it shows them: each loaded address at its
/// index, every other index a key no instruction names.
fn lookup_tables(tx: &ChainTx) -> Vec<AddressLookupTableAccount> {
    let loaded = tx.meta.as_ref().and_then(|m| m.loaded_addresses.as_ref());
    let (mut writable, mut readonly) = (
        loaded.iter().flat_map(|l| &l.writable),
        loaded.iter().flat_map(|l| &l.readonly),
    );
    tx.message
        .address_table_lookups
        .iter()
        .flatten()
        .map(|l| {
            let len = l
                .writable_indexes
                .iter()
                .chain(&l.readonly_indexes)
                .map(|&i| usize::from(i) + 1)
                .max()
                .unwrap_or(0);
            let mut addresses: Vec<Pubkey> = (0..len).map(|_| Pubkey::new_unique()).collect();
            for &i in &l.writable_indexes {
                addresses[usize::from(i)] = *writable.next().unwrap();
            }
            for &i in &l.readonly_indexes {
                addresses[usize::from(i)] = *readonly.next().unwrap();
            }
            AddressLookupTableAccount {
                key: l.account_key,
                addresses,
            }
        })
        .collect()
}

fn is_compute_budget(ix: &Instruction) -> bool {
    ix.program_id == Pubkey::from_str(protocol::COMPUTE_BUDGET_PROGRAM_ID).unwrap()
}

fn u32_at(data: &[u8]) -> u32 {
    u32::from_le_bytes(data[1..5].try_into().unwrap())
}

/// The core's resources for the winner's own: the v0 compute budget instructions (limit 2,
/// price 3, loaded accounts data size 4), the v1 configuration (its total priority fee as a
/// price per CU, exactly).
fn resources(tx: &ChainTx, ixs: &[Instruction]) -> Resources {
    if version(tx) == TxWireVersion::Number(1) {
        let c = tx.message.transaction_config.unwrap();
        let limit = c.compute_unit_limit.unwrap();
        let fee = u128::from(c.priority_fee.unwrap_or(0)) * 1_000_000;
        assert_eq!(
            fee % u128::from(limit),
            0,
            "a price per CU the core can name"
        );
        return Resources {
            compute_unit_limit: limit,
            priority_micro_lamports_per_cu: u64::try_from(fee / u128::from(limit)).unwrap(),
            loaded_accounts_data_size: c.loaded_accounts_data_size_limit,
        };
    }
    let mut r = Resources {
        compute_unit_limit: 0,
        priority_micro_lamports_per_cu: 0,
        loaded_accounts_data_size: None,
    };
    for ix in ixs.iter().filter(|ix| is_compute_budget(ix)) {
        match ix.data[0] {
            2 => r.compute_unit_limit = u32_at(&ix.data),
            3 => {
                r.priority_micro_lamports_per_cu =
                    u64::from_le_bytes(ix.data[1..9].try_into().unwrap())
            }
            4 => r.loaded_accounts_data_size = Some(u32_at(&ix.data)),
            other => panic!("compute budget instruction {other}"),
        }
    }
    r
}

/// The mint of a token account the winner moved (its pre/post token balances).
fn mint_of(tx: &ChainTx, account: &Pubkey) -> Pubkey {
    let keys = tx.account_keys().unwrap();
    let meta = tx.meta.as_ref().unwrap();
    meta.pre_token_balances
        .iter()
        .chain(&meta.post_token_balances)
        .flatten()
        .find(|b| keys[usize::from(b.account_index)] == *account)
        .map(|b| b.mint)
        .expect("a token balance of the account")
}

/// The anchor `Option<bool>` of a withdraw/repay argument list (discriminator, u64 amount,
/// option): one byte for `None`, two for `Some`, its value the official decode's `all`.
fn flag(data: &[u8], all: bool) -> Option<bool> {
    match data.len() {
        17 => None,
        18 => Some(all),
        n => panic!("argument list of {n} bytes"),
    }
}

/// The privileges (signer, writable) the instructions ask for, per key — what a message grants.
fn privileges(ixs: &[Instruction]) -> std::collections::BTreeMap<Pubkey, (bool, bool)> {
    let mut out = std::collections::BTreeMap::new();
    for ix in ixs {
        out.entry(ix.program_id).or_insert((false, false));
        for m in &ix.accounts {
            let p = out.entry(m.pubkey).or_insert((false, false));
            p.0 |= m.is_signer;
            p.1 |= m.is_writable;
        }
    }
    out
}

fn keys(metas: &[AccountMeta]) -> Vec<Pubkey> {
    metas.iter().map(|m| m.pubkey).collect()
}

/// S4-03 serialization + signatures: the production rebuild of each decoded winner is exactly
/// the bytes its signers signed (every signature verifies over it, never over a changed byte)
/// and its wire is the signatures and that message as the official serializers write them,
/// within the version's size limit.
#[test]
fn s4_03_every_winner_signature_verifies_over_the_rebuilt_message() {
    let limits = TxLimits::default();
    for (tx, _) in corpus() {
        let name = tx.signatures[0].clone();
        let message = tx::chain_message_bytes(&tx.message, version(&tx)).expect(&name);
        assert_eq!(
            tx.signatures.len(),
            usize::from(tx.message.header.unwrap().0)
        );
        for (signature, key) in tx.signatures.iter().zip(&tx.message.account_keys) {
            let signature = Signature::from_str(signature).unwrap();
            assert!(signature.verify(key.as_ref(), &message), "{name}");
            let mut changed = message.clone();
            *changed.last_mut().unwrap() ^= 1;
            assert!(!signature.verify(key.as_ref(), &changed), "{name}");
        }
        let wire = tx::chain_wire(&tx).expect(&name);
        let signatures = 64 * tx.signatures.len();
        if version(&tx) == TxWireVersion::Number(1) {
            assert_eq!(&wire[..message.len()], &message[..], "{name}");
            assert_eq!(wire.len(), message.len() + signatures);
            assert!(wire.len() <= limits.max_bytes_v1, "{name}");
        } else {
            assert_eq!(&wire[wire.len() - message.len()..], &message[..], "{name}");
            assert_eq!(wire.len(), 1 + signatures + message.len());
            assert!(wire.len() <= limits.max_bytes_v0, "{name}");
        }
    }
}

/// S4-03 TX build: the plan gate takes every winner's receivership (its steps from its bytes);
/// the core's builders rebuild each of its marginfi instructions from the accounts the winner
/// named — the derived PDAs (liquidation record, vault authority, fee state), the account order,
/// the start/end layout (`liquidation_layout`, `bank_only_layout`) and the argument bytes; the
/// core's compile of that plan is the winner's signed message (`Compile`); the network fee is the
/// core's planned fee of its resources.
#[test]
fn s4_03_the_core_rebuilds_and_compiles_every_winner() {
    let program = Pubkey::from_str(protocol::PROGRAM_ID_MAINNET).unwrap();
    let table = builders::instruction_table(&program).unwrap();
    let allowlist = protocol::receivership_allowlist();
    let global = protocol::receivership_allowed_programs();
    let token_2022 = Pubkey::from_str(protocol::TOKEN_2022_PROGRAM_ID).unwrap();
    let upgradeable_loader = Pubkey::from_str(protocol::BPF_LOADER_UPGRADEABLE_ID).unwrap();
    let limits = TxLimits::default();
    let mut observed = Vec::new();
    for (tx, winner) in corpus() {
        let name = tx.signatures[0].clone();
        let all = decompiled(&tx);
        let payer = tx.message.account_keys[0];
        let real: Vec<Instruction> = all
            .iter()
            .filter(|ix| !is_compute_budget(ix))
            .cloned()
            .collect();
        assert!(all.iter().all(|ix| global.contains(&ix.program_id)));

        // The plan gate takes the winner's receivership.
        let steps = planner::steps_of(&real, &table).unwrap();
        let swaps: Vec<usize> = (0..steps.len())
            .filter(|&i| matches!(steps[i], Step::Swap(_)))
            .collect();
        let route = if swaps.is_empty() {
            RouteKind::NoSwap
        } else {
            RouteKind::FreePublicAllowed
        };
        planner::validate_receivership(&steps, &allowlist, route, false).expect(&name);
        let at = |s: Step| steps.iter().position(|x| *x == s).unwrap();
        let (start, withdraw, repay, end) = (
            &real[at(Step::StartLiquidation)],
            &real[at(Step::Withdraw)],
            &real[at(Step::Repay)],
            &real[at(Step::EndLiquidation)],
        );

        // The accounts the winner named.
        let account = start.accounts[0].pubkey;
        let group = start.accounts[2].pubkey;
        let receiver = start.accounts[3].pubkey;
        let observation = keys(&start.accounts[5..]);
        let authority = withdraw.accounts[2].pubkey;
        assert_eq!(authority, receiver, "{name}: the receiver withdraws");
        assert_eq!(repay.accounts[2].pubkey, receiver, "{name}: and repays");
        let amounts = builders::built_amounts(&real, &program).unwrap();
        let (w_bank, w_dest, w_vault, w_tp) = (
            withdraw.accounts[3].pubkey,
            withdraw.accounts[4].pubkey,
            withdraw.accounts[6].pubkey,
            withdraw.accounts[7].pubkey,
        );
        let (r_bank, r_source, r_vault, r_tp) = (
            repay.accounts[3].pubkey,
            repay.accounts[4].pubkey,
            repay.accounts[5].pubkey,
            repay.accounts[6].pubkey,
        );
        let (w_mint, r_mint) = (mint_of(&tx, &w_dest), mint_of(&tx, &r_source));
        let w_obs = 8 + usize::from(w_tp == token_2022);
        assert_eq!(keys(&withdraw.accounts[w_obs..]), observation, "{name}");
        let r_obs = 7 + usize::from(r_tp == token_2022);
        let repay_all = amounts.repay.1;
        assert_eq!(
            keys(&repay.accounts[r_obs..]),
            if repay_all {
                observation.clone()
            } else {
                vec![]
            },
            "{name}"
        );
        let global_fee_wallet = end.accounts[5].pubkey;
        let real_end_banks = keys(&end.accounts[8..]);

        // The core's layout of the active banks: End's bank-only order, the closed bank kept
        // only in Start's observation.
        let mut active = real_end_banks.clone();
        if repay_all {
            active.push(r_bank);
        }
        let (start_observation, end_banks) = builders::liquidation_layout(
            &observation,
            builders::bank_only_layout(active),
            &r_bank,
            repay_all,
        );
        assert_eq!(end_banks, real_end_banks, "{name}: the bank-only order");

        // The core's instructions, with the winner's exact argument encoding.
        let w_flag = flag(&withdraw.data, amounts.withdraw.1);
        let r_flag = flag(&repay.data, repay_all);
        let build = |w_flag: Option<bool>, r_flag: Option<bool>| {
            builders::build_receivership(ReceivershipParams {
                program: &program,
                group: &group,
                marginfi_account: &account,
                liquidator: &receiver,
                global_fee_wallet: &global_fee_wallet,
                start_observation: start_observation.clone(),
                end_banks: end_banks.clone(),
                init_record: false,
                pre_refreshes: vec![],
                withdraws: vec![builders::withdraw(
                    &program,
                    &group,
                    &account,
                    &receiver,
                    &w_bank,
                    &w_mint,
                    &w_dest,
                    &w_vault,
                    &w_tp,
                    amounts.withdraw.0,
                    w_flag,
                    &observation,
                )],
                swaps: swaps.iter().map(|&i| real[i].clone()).collect(),
                repays: vec![builders::repay(
                    &program,
                    &group,
                    &account,
                    &receiver,
                    &r_bank,
                    &r_mint,
                    &r_source,
                    &r_vault,
                    &r_tp,
                    amounts.repay.0,
                    r_flag,
                    &observation,
                )],
            })
        };
        let mut ours = build(w_flag, r_flag);
        assert_eq!(ours.len(), real.len(), "{name}");
        // E-FEE: the core's End leaves `fee_payer` out (the program's id in its place, anchor's
        // absent optional account); a winner may name the receiver, who pays either way.
        let end_at = at(Step::EndLiquidation);
        let fee_payer = &mut ours[end_at].accounts[7];
        assert_eq!(
            (fee_payer.pubkey, fee_payer.is_signer, fee_payer.is_writable),
            (program, false, false),
            "{name}"
        );
        let named_fee_payer = real[end_at].accounts[7].pubkey == receiver;
        if named_fee_payer {
            *fee_payer = AccountMeta::new(receiver, true);
        }
        for (i, (o, r)) in ours.iter().zip(&real).enumerate() {
            assert_eq!(o.program_id, r.program_id, "{name} #{i}");
            assert_eq!(keys(&o.accounts), keys(&r.accounts), "{name} #{i}");
            assert_eq!(o.data, r.data, "{name} #{i}");
        }

        // E-OPT: the core's own encoding (a plain withdraw never withdraws all; a repay names
        // `repay_all` only when it closes) is the same instruction.
        assert!(!amounts.withdraw.1, "{name}: E2 never withdraws all");
        let core = build(None, repay_all.then_some(true));
        let mut opt = 0;
        assert_eq!(builders::built_amounts(&core, &program), Ok(amounts));
        for (o, r) in core.iter().zip(&real) {
            if o.data != r.data {
                assert_eq!(r.data.len(), o.data.len() + 1, "{name}: only E-OPT");
                assert_eq!((o.data[16], &r.data[16..]), (0, &[1u8, 0][..]), "{name}");
                assert_eq!(o.data[..16], r.data[..16], "{name}");
                opt += 1;
            }
        }

        // E-PRIV: the core asks for the program's privileges; a winner may ask the group or the
        // invoked program writable.
        let (core_p, real_p) = (privileges(&ours), privileges(&real));
        assert_eq!(
            core_p.keys().collect::<Vec<_>>(),
            real_p.keys().collect::<Vec<_>>(),
            "{name}"
        );
        let raised: Vec<Pubkey> = core_p
            .iter()
            .filter(|(k, p)| real_p[*k] != **p)
            .map(|(k, p)| {
                assert!([group, program].contains(k), "{name}: only E-PRIV: {k}");
                // The runtime demotes an invoked program's write lock only without the
                // upgradeable loader among the message's keys.
                assert!(
                    *k != program || !tx.account_keys().unwrap().contains(&upgradeable_loader),
                    "{name}"
                );
                assert_eq!((*p, real_p[k]), ((false, false), (false, true)), "{name}");
                *k
            })
            .collect();
        let raised_as = match raised.as_slice() {
            [] => None,
            [k] if *k == group => Some(Raised::Group),
            [k] if *k == program => Some(Raised::Program),
            more => panic!("{name}: {more:?}"),
        };
        for m in ours.iter_mut().flat_map(|ix| ix.accounts.iter_mut()) {
            m.is_writable |= raised.contains(&m.pubkey);
        }

        // The core's compile of exactly this plan (with the winner's privileges).
        let plan = TxPlan {
            version: match version(&tx) {
                TxWireVersion::Number(1) => BuildVersion::V1,
                TxWireVersion::Number(0) => BuildVersion::V0,
                v => panic!("{name}: {v:?}"),
            },
            payer,
            instructions: ours,
            lookup_tables: lookup_tables(&tx).into(),
            resources: resources(&tx, &all),
        };
        let blockhash = Hash::from_str(tx.message.recent_blockhash.as_deref().unwrap()).unwrap();
        let signed = tx::chain_message_bytes(&tx.message, version(&tx)).unwrap();
        let signature = Signature::from_str(&name).unwrap();
        if plan.version == BuildVersion::V1 {
            let ours = tx::v1_message_bytes(&plan, blockhash, &limits).unwrap();
            assert_eq!(winner.compile, Compile::HeapRequest, "{name}");
            let config = tx.message.transaction_config.unwrap();
            assert_eq!(config.heap_size, Some(DEFAULT_HEAP_FRAME_BYTES), "{name}");
            let mut without = tx.message.clone();
            without.transaction_config = Some(darkbot_solana::chain::TransactionConfig {
                heap_size: None,
                ..config
            });
            assert_eq!(
                Some(ours),
                tx::chain_message_bytes(&without, version(&tx)),
                "{name}"
            );
        } else {
            let built = tx::build(&plan, blockhash, &limits).unwrap();
            let ours = built.tx.message.serialize();
            let real_message: VersionedMessage = bincode::deserialize(&signed).unwrap();
            assert_eq!(
                tx::message_matches_plan(&real_message, &plan),
                Ok(()),
                "{name}"
            );
            assert_eq!(
                built.locked_accounts,
                tx.account_keys().unwrap().len(),
                "{name}"
            );
            assert_eq!(tx::requested_priority_lamports(&built.tx), Some(0));
            match winner.compile {
                Compile::Identical => {
                    assert_eq!(ours, signed, "{name}");
                    assert!(signature.verify(payer.as_ref(), &ours), "{name}");
                }
                Compile::OtherOrder => assert_ne!(ours, signed, "{name}"),
                Compile::HeapRequest => panic!("{name}: v0 has no heap request here"),
            }
        }

        // The network fee is the core's planned fee of the winner's resources.
        assert_eq!(
            tx.meta.as_ref().unwrap().fee,
            darkbot_runner::execution::pipeline::planned_fee_lamports(
                tx.signatures.len() as u64,
                &plan.resources
            ),
            "{name}"
        );
        observed.push(Winner {
            opt,
            raised: raised_as,
            fee_payer: named_fee_payer,
            ..winner
        });
    }
    assert_eq!(
        observed, CORPUS,
        "exactly the explained deviations of each winner"
    );
}
