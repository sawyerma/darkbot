# DarkBot BSC — C1.8 Chain-lane single-producer ownership

**Scope:** pinned bsc-geth v1.7.8, `cdb7548b5baacfdae92f9f63437d6456411665f3`, with the C1 overlay applied.
**Status:** analysis of the pinned source, checked mechanically by the overlay tool. It is not a gate result; C1.8 is accepted locally.

## Claim

The Wire-V2 Chain lane (lane 2, class `ChainEvidence`) has exactly one producer at a time: every patched publisher runs while `BlockChain.chainmu` is held.

## The lock

`chainmu` is a `*syncx.ClosableMutex` (`internal/syncx/mutex.go`). Its `TryLock` is a blocking channel receive (`_, ok := <-cm.ch`). Despite the name it waits; it returns `false` only after `Close`, and a caller that gets `false` returns `errChainStopped` without writing. Holding it is mutual exclusion.

## The publishers

| Hook | In function | How `chainmu` is held |
|---|---|---|
| `fastlane.PublishCanonicalV2` | `BlockChain.writeBlockAndSetHead`, when the block became canonical | by every caller, see below |
| `fastlane.PublishReorgV2` | `BlockChain.reorg`, before the tx-lookup mutation | by every caller, see below |
| `fastlane.PublishCanonicalV2` | `BlockChain.SetCanonical` | `SetCanonical` locks `chainmu` itself before the hook |
| `fastlane.PublishCanonicalV2` | `BlockChain.writeKnownBlock`, after its head write (E7-2) | by every caller, see below |
| `fastlane.PublishHeadResetV2` (GAP, reason 1) | `BlockChain.setHeadBeyondRoot`, after the rewind and `loadLastState` (every `SetHead`/`SetHeadWithTimestamp` and startup repair) (E7-2) | locks `chainmu` itself before the hook |
| `fastlane.PublishHeadResetV2` (GAP, reason 2) | `BlockChain.SnapSyncCommitHead`, after the head store (E7-2) | locks `chainmu` itself before the hook |
| `fastlane.PublishHeadResetV2` (GAP, reason 3) | `BlockChain.ResetWithGenesisBlock`, after the genesis markers (E7-2) | locks `chainmu` itself before the hook |

**Head-write inventory (E7-2, mechanical):** the overlay tool requires that the in-memory head (`bc.currentBlock.Store`) is written only by `writeHeadBlock`, `setHeadBeyondRoot`, `SnapSyncCommitHead`, `ResetWithGenesisBlock` and `loadLastState`; that `writeHeadBlock` is called only by `writeBlockAndSetHead`, `SetCanonical`, `reorg`, `writeKnownBlock` and `ResetWithGenesisBlock`; and that `loadLastState` is called by no other `BlockChain` method than `setHeadBeyondRoot` (its other caller, `NewBlockChain`, runs before the Fast Lane exists). A new head writer in a future pin fails the check. Every head change therefore leaves a CANONICAL_HEAD, REORG or GAP record; the Rust side checks that the records form a contiguous head history and fails closed otherwise.

Call paths into the functions that do not lock themselves, all inside `core/blockchain.go`:

```text
writeBlockAndSetHead <- WriteBlockAndSetHead            locks chainmu
writeBlockAndSetHead <- ProcessBlock <- insertChain <- InsertChain                 locks chainmu
                                                    <- InsertBlockWithoutSetHead   locks chainmu
                                                    <- recoverAncestors <- SetCanonical  locks chainmu
                                                    <- insertSideChain <- insertChain    (recursion, same lock)
reorg <- writeBlockAndSetHead        (above)
reorg <- writeKnownBlock <- insertChain   (above)
reorg <- SetCanonical                locks chainmu
```

No path reaches a hook without `chainmu`.

## The one condition that is not the lock

`ProcessBlock` is exported and does not lock. It is also called from outside `core`: `eth/api_debug.go`, `DebugAPI.ExecutionWitness`. That call passes `ExecuteConfig{WriteState: false, ...}`. `ProcessBlock` reaches `writeBlockAndSetHead` only under `config.WriteState && config.WriteHead`, so the debug call never reaches the hook.

The claim therefore rests on two things: the lock, and the configuration of one exported call. A future caller of `ProcessBlock` with `WriteState` and `WriteHead` set and without `chainmu` would break it without touching any hook. Two safeguards cover that:

1. **Mechanical check.** `c1_apply_geth_overlay.py` runs `verify_chain_ownership` after patching and prints `C1_8_CHAIN_OWNERSHIP_VERIFIED`. It fails if `TryLock` no longer blocks, if a hook is missing, moved or left on V1, if any in-package path into a hook function lacks the lock, or if a call of an exported non-locking method on such a path, outside `BlockChain`, does not pass `WriteState: false`.
2. **Run-time detector.** `chainPublisherV2` marks its publish section with an atomic flag. A publish that finds the flag set writes nothing and is counted in `ownership_violations`; the loss is reported in-band by a `GAP` record before the next record. HOT0 treats a `GAP` on the Chain lane as lost evidence and refuses admission. The detector never blocks the chain-write path.

## What this does not claim

- It is a statement about this pinned source with this overlay. A different pin or a changed overlay needs the check rerun.
- C1 publishes evidence at reviewed hooks. It changes no chain mutation, ancestor determination, tx lookup, head state, canonical or reorg ordering.
- Head changes outside canonical/reorg evidence (`SetHead` rewinds, snap-sync head commits, genesis resets) are reported only as reasoned GAP records: evidence that the history is broken, not a description of the new chain. C2 must reconcile independently before it trusts a chain view again.
- The Rust side does not turn these records into chain state. C2 is the only reconciler.

## Tests

- `chain_v2_test.go`: record field semantics; a second publisher detected and reported by `GAP`; a full lane losing nothing silently; publishers under one lock (single producer, zero violations); publishers without a lock (lane intact, every call accounted for).
- `stress_test.go`: canonical and reorg evidence under simultaneous peer ingress, txpool evidence and outbound.
- `hot0.rs`: a `GAP` on the Chain lane refuses admission.
