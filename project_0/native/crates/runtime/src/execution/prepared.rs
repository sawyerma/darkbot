//! P2-06: prepared candidate cache. Per candidate account it keeps the part of its receivership
//! plans that depends only on account bytes — health observation list, active banks, integration
//! refresh skeleton, record-init decision — and once per group the protocol LUT address plan,
//! each with the exact bytes of every account its derivation read (`Recording`). Bounded
//! (oldest first out) and versioned (`SCHEMA`, `Context`). By construction it holds no health,
//! oracle value, quote or route, amount, blockhash, fee/tip or signature: those stay per call.
//! P2-07: a hit needs the same schema and context (generation, program, group, liquidator), a
//! slot not before preparation and every dependency byte-identical in the evaluated view —
//! including the pinned ProgramData — otherwise the template is prepared afresh.
use darkbot_project0::health::AccountSource;
use solana_message::AddressLookupTableAccount;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

pub const SCHEMA: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context {
    /// The owner's ingress generation: a new session invalidates every older template.
    pub generation: u64,
    pub program: Pubkey,
    pub group: Pubkey,
    pub liquidator: Pubkey,
}

/// The account-dependent template of a candidate's receivership plans. Its errors are reported
/// where the uncached builder reported them (per pair); only a complete template is cached.
#[derive(Clone, Debug, PartialEq)]
pub struct Skeleton {
    pub observation: Vec<Pubkey>,
    /// Active banks before repay, then the integration refreshes (first error wins, as before).
    pub banks_and_refreshes: Result<(Vec<Pubkey>, Vec<Instruction>), String>,
    pub init_record: Result<bool, String>,
}

/// What every dependency was at preparation: absent, or exactly this owner, lamports and data.
type Snapshot = Vec<(Pubkey, Option<(Pubkey, u64, Box<[u8]>)>)>;

struct Entry<T: ?Sized> {
    schema: u32,
    context: Context,
    slot: u64,
    deps: Snapshot,
    value: Arc<T>,
}

impl<T: ?Sized> Entry<T> {
    fn holds<S: AccountSource + ?Sized>(&self, state: &S, context: &Context, slot: u64) -> bool {
        self.schema == SCHEMA
            && self.context == *context
            && slot >= self.slot
            && self
                .deps
                .iter()
                .all(|(k, was)| match (state.account(k), was) {
                    (None, None) => true,
                    (Some((o, l, d)), Some((wo, wl, wd))) => o == *wo && l == *wl && d == &wd[..],
                    _ => false,
                })
    }
}

/// An account source that records every key read through it.
pub struct Recording<'a, S: ?Sized> {
    inner: &'a S,
    reads: RefCell<Vec<Pubkey>>,
}

impl<'a, S: AccountSource + ?Sized> Recording<'a, S> {
    pub fn new(inner: &'a S) -> Self {
        Self {
            inner,
            reads: RefCell::new(Vec::new()),
        }
    }

    fn snapshot(self, pinned: &[Pubkey]) -> Snapshot {
        let mut keys = self.reads.into_inner();
        keys.extend_from_slice(pinned);
        keys.sort_unstable();
        keys.dedup();
        keys.into_iter()
            .map(|k| {
                let now = self.inner.account(&k).map(|(o, l, d)| (o, l, d.into()));
                (k, now)
            })
            .collect()
    }
}

impl<S: AccountSource + ?Sized> AccountSource for Recording<'_, S> {
    fn account(&self, key: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
        self.reads.borrow_mut().push(*key);
        self.inner.account(key)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

pub struct PreparedCache {
    capacity: usize,
    /// Read by no derivation but a dependency of all (the ProgramData: an upgrade invalidates).
    pinned: Vec<Pubkey>,
    skeletons: HashMap<Pubkey, Entry<Skeleton>>,
    order: VecDeque<Pubkey>,
    luts: Option<Entry<[AddressLookupTableAccount]>>,
    pub stats: Stats,
}

impl PreparedCache {
    pub fn new(capacity: usize, pinned: Vec<Pubkey>) -> Self {
        Self {
            capacity: capacity.max(1),
            pinned,
            skeletons: HashMap::new(),
            order: VecDeque::new(),
            luts: None,
            stats: Stats::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.skeletons.len()
    }
    pub fn is_empty(&self) -> bool {
        self.skeletons.is_empty()
    }

    /// `account`'s template: the cached one if it still holds, else `prepare` reads it afresh
    /// through a recording view (cached only when complete).
    pub fn skeleton<S: AccountSource + ?Sized, E>(
        &mut self,
        state: &S,
        account: &Pubkey,
        context: Context,
        slot: u64,
        prepare: impl FnOnce(&Recording<'_, S>) -> Result<Skeleton, E>,
    ) -> Result<Arc<Skeleton>, E> {
        if let Some(e) = self.skeletons.get(account) {
            if e.holds(state, &context, slot) {
                self.stats.hits += 1;
                return Ok(e.value.clone());
            }
        }
        self.stats.misses += 1;
        let rec = Recording::new(state);
        let value = Arc::new(prepare(&rec)?);
        if value.banks_and_refreshes.is_err() || value.init_record.is_err() {
            if self.skeletons.remove(account).is_some() {
                self.order.retain(|k| k != account);
            }
            return Ok(value);
        }
        let entry = Entry {
            schema: SCHEMA,
            context,
            slot,
            deps: rec.snapshot(&self.pinned),
            value: value.clone(),
        };
        if self.skeletons.insert(*account, entry).is_none() {
            self.order.push_back(*account);
            while self.skeletons.len() > self.capacity {
                let Some(old) = self.order.pop_front() else {
                    break;
                };
                if self.skeletons.remove(&old).is_some() {
                    self.stats.evictions += 1;
                }
            }
        }
        Ok(value)
    }

    /// The group's protocol LUT plan at `slot`. It is cached only while it cannot change with a
    /// later slot (the same plan at `slot` and at the end of time: no table still extending).
    pub fn lookup_tables<S: AccountSource + ?Sized>(
        &mut self,
        state: &S,
        context: Context,
        slot: u64,
        plan: impl Fn(&Recording<'_, S>, u64) -> Vec<AddressLookupTableAccount>,
    ) -> Arc<[AddressLookupTableAccount]> {
        if let Some(e) = &self.luts {
            if e.holds(state, &context, slot) {
                self.stats.hits += 1;
                return e.value.clone();
            }
        }
        self.stats.misses += 1;
        let rec = Recording::new(state);
        let now = plan(&rec, slot);
        let settled = now == plan(&rec, u64::MAX);
        let now: Arc<[AddressLookupTableAccount]> = now.into();
        self.luts = settled.then(|| Entry {
            schema: SCHEMA,
            context,
            slot,
            deps: rec.snapshot(&self.pinned),
            value: now.clone(),
        });
        now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Map(HashMap<Pubkey, (Pubkey, u64, Vec<u8>)>);
    impl AccountSource for Map {
        fn account(&self, k: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
            self.0.get(k).map(|(o, l, d)| (*o, *l, d.as_slice()))
        }
    }

    fn skeleton(reads: &[Pubkey], state: &impl AccountSource) -> Skeleton {
        for k in reads {
            let _ = state.account(k);
        }
        Skeleton {
            observation: reads.to_vec(),
            banks_and_refreshes: Ok((vec![], vec![])),
            init_record: Ok(false),
        }
    }

    /// P2-06/P2-07: a template is reused only while its context, slot order and every byte it
    /// was derived from (and the pinned ProgramData) are unchanged; the cache stays bounded and
    /// never keeps an incomplete template.
    #[test]
    fn templates_hold_only_while_every_dependency_is_unchanged() {
        let (a, b, bank, program_data) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let owner = Pubkey::new_unique();
        let mut state = Map(HashMap::from([
            (a, (owner, 1, vec![1])),
            (bank, (owner, 1, vec![2])),
            (program_data, (owner, 1, vec![3])),
        ]));
        let ctx = Context {
            generation: 1,
            program: owner,
            group: Pubkey::new_unique(),
            liquidator: Pubkey::new_unique(),
        };
        let mut cache = PreparedCache::new(1, vec![program_data]);
        let get = |cache: &mut PreparedCache, state: &Map, ctx: Context, slot| {
            cache
                .skeleton(state, &a, ctx, slot, |rec| {
                    Ok::<_, ()>(skeleton(&[a, bank], rec))
                })
                .unwrap();
            cache.stats
        };
        let s = get(&mut cache, &state, ctx, 10);
        assert_eq!((s.hits, s.misses), (0, 1));
        assert_eq!(get(&mut cache, &state, ctx, 11).hits, 1);
        // An earlier slot, another generation, a changed dependency, lamports, a program upgrade.
        assert_eq!(get(&mut cache, &state, ctx, 9).misses, 2);
        let next = Context {
            generation: 2,
            ..ctx
        };
        assert_eq!(get(&mut cache, &state, next, 11).misses, 3);
        assert_eq!(get(&mut cache, &state, ctx, 11).misses, 4);
        state.0.get_mut(&bank).unwrap().2 = vec![9];
        assert_eq!(get(&mut cache, &state, ctx, 11).misses, 5);
        assert_eq!(get(&mut cache, &state, ctx, 11).hits, 2);
        state.0.get_mut(&a).unwrap().1 = 2;
        assert_eq!(get(&mut cache, &state, ctx, 11).misses, 6);
        state.0.get_mut(&program_data).unwrap().2 = vec![4];
        assert_eq!(get(&mut cache, &state, ctx, 11).misses, 7);
        // A read account that appears later invalidates as well.
        let absent = Pubkey::new_unique();
        cache
            .skeleton(&state, &b, ctx, 11, |rec| {
                Ok::<_, ()>(skeleton(&[b, absent], rec))
            })
            .unwrap();
        assert_eq!((cache.len(), cache.stats.evictions), (1, 1), "bounded");
        state.0.insert(absent, (owner, 1, vec![]));
        let before = cache.stats.misses;
        cache
            .skeleton(&state, &b, ctx, 11, |rec| {
                Ok::<_, ()>(skeleton(&[b, absent], rec))
            })
            .unwrap();
        assert_eq!(cache.stats.misses, before + 1);
        // An incomplete template is returned but never cached.
        let failing = |rec: &Recording<'_, Map>| {
            let mut s = skeleton(&[a], rec);
            s.init_record = Err("x".into());
            Ok::<_, ()>(s)
        };
        assert!(cache
            .skeleton(&state, &a, ctx, 11, failing)
            .unwrap()
            .init_record
            .is_err());
        assert!(cache
            .skeleton(&state, &a, ctx, 11, failing)
            .unwrap()
            .init_record
            .is_err());
        assert_eq!(cache.len(), 1);
        assert!(cache.skeletons.contains_key(&b));
    }

    /// P2-06: a LUT plan that still depends on the slot is never cached.
    #[test]
    fn slot_dependent_lut_plans_are_not_cached() {
        let state = Map(HashMap::new());
        let ctx = Context {
            generation: 0,
            program: Pubkey::new_unique(),
            group: Pubkey::new_unique(),
            liquidator: Pubkey::new_unique(),
        };
        let table = |n: usize| AddressLookupTableAccount {
            key: Pubkey::default(),
            addresses: vec![Pubkey::default(); n],
        };
        let mut cache = PreparedCache::new(4, vec![]);
        // Extending until slot 20: never cached before it.
        let extending =
            |_: &Recording<'_, Map>, slot: u64| vec![table(if slot > 20 { 2 } else { 1 })];
        for slot in [10, 10, 20] {
            assert_eq!(
                *cache.lookup_tables(&state, ctx, slot, extending),
                [table(1)]
            );
        }
        assert_eq!((cache.stats.hits, cache.stats.misses), (0, 3));
        assert_eq!(*cache.lookup_tables(&state, ctx, 21, extending), [table(2)]);
        assert_eq!(*cache.lookup_tables(&state, ctx, 22, extending), [table(2)]);
        assert_eq!((cache.stats.hits, cache.stats.misses), (1, 4));
    }
}
