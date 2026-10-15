#!/usr/bin/env python3
"""Applies the DarkBot C1 Fast-Lane overlay to the pinned bsc-geth checkout.

C1.4: peer ingress (Transactions and PooledTransactions) and TXPOOL_ACCEPTED
publish over Wire V2; every ETH peer connection binds one exclusive peer lane.
V1 remains only for canonical/reorg evidence and local-geth outbound, which
later C1 packages migrate; V1 is not C1.4-authoritative ingress.
"""
from __future__ import annotations

import re
import shutil
import subprocess
import sys
from pathlib import Path

PIN = "cdb7548b5baacfdae92f9f63437d6456411665f3"
MARKER = "DARKBOT_C1_FASTLANE_C1_7"
MARKER_FILE = ".darkbot-c1-fastlane-c17"
# Earlier overlays: C1.1-C1.3 and C1.4. Their hooks differ (V1 chain
# evidence and V1 outbound), so a tree carrying them is reset first.
LEGACY_MARKER_FILE = ".darkbot-c1-fastlane-v1"
C14_MARKER_FILE = ".darkbot-c1-fastlane-c14"


def run(cwd: Path, *args: str) -> str:
    return subprocess.check_output(args, cwd=cwd, text=True).strip()


def replace_once(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text()
    if new in text:
        return
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one source match, found {count}")
    path.write_text(text.replace(old, new, 1))


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} /path/to/bsc-geth", file=sys.stderr)
        return 2

    root = Path(sys.argv[1]).resolve()
    if not (root / ".git").is_dir():
        raise SystemExit(f"not a git checkout: {root}")
    head = run(root, "git", "rev-parse", "HEAD")
    if head != PIN:
        raise SystemExit(f"C1 overlay refuses HEAD {head}; expected {PIN}")

    for stale in (LEGACY_MARKER_FILE, C14_MARKER_FILE):
        if (root / stale).exists():
            raise SystemExit(
                "the checkout carries an earlier C1 overlay; reset it to the pin first:\n"
                f"  git -C {root} checkout -- eth core\n"
                f"  rm -rf {root}/darkbot {root}/{stale}"
            )
    marker = root / MARKER_FILE
    already = marker.is_file() and marker.read_text().strip() == MARKER
    dirty = run(root, "git", "status", "--porcelain", "--untracked-files=no")
    if dirty and not already:
        raise SystemExit("tracked geth tree is dirty before C1 overlay")

    here = Path(__file__).resolve()
    darkbot_root = here.parents[2]
    overlay = darkbot_root / "bsc/geth/overlay/darkbot/fastlane"
    destination = root / "darkbot/fastlane"
    destination.mkdir(parents=True, exist_ok=True)
    for source in overlay.glob("*.go"):
        shutil.copy2(source, destination / source.name)

    if not already:
        patch_sources(root)
    patch_c19b_testnet_accept_txs(root)
    verify_hooks(root)
    verify_chain_ownership(root)
    marker.write_text(MARKER + "\n")
    print("C1_GETH_OVERLAY_APPLIED")
    print(f"marker={MARKER}")
    print(f"base={PIN}")
    return 0


def patch_sources(root: Path) -> None:

    handler_eth = root / "eth/handler_eth.go"
    replace_once(
        handler_eth,
        '"github.com/ethereum/go-ethereum/core/types"\n',
        '"github.com/ethereum/go-ethereum/core/types"\n\t"github.com/ethereum/go-ethereum/darkbot/fastlane"\n',
        "handler_eth import",
    )
    replace_once(
        handler_eth,
        '''\t\tif err := handleTransactions(peer, txs, true); err != nil {
\t\t\treturn fmt.Errorf("Transactions: %v", err)
\t\t}
\t\treturn h.txFetcher.Enqueue(peer.ID(), txs, false)
''',
        '''\t\t// DarkBot C1.4: T_packet_decoded, then geth's basic validation
\t\t// (unchanged result), then T_ingress, prefilter and publish into the
\t\t// peer's own Wire-V2 lane. A Fast-Lane refusal never reaches geth.
\t\tif err := fastlane.IngressV2(peer.DarkBotFastLane(), fastlane.IngressSourceEthBroadcast, txs, func() error {
\t\t\treturn handleTransactions(peer, txs, true)
\t\t}); err != nil {
\t\t\treturn fmt.Errorf("Transactions: %v", err)
\t\t}
\t\treturn h.txFetcher.Enqueue(peer.ID(), txs, false)
''',
        "direct transaction ingress hook",
    )
    replace_once(
        handler_eth,
        '''\t\tif err := handleTransactions(peer, txs, false); err != nil {
\t\t\treturn fmt.Errorf("PooledTransactions: %v", err)
\t\t}
\t\treturn h.txFetcher.Enqueue(peer.ID(), txs, true)
''',
        '''\t\t// DarkBot C1.4: same boundary as Transactions, source eth-pooled.
\t\tif err := fastlane.IngressV2(peer.DarkBotFastLane(), fastlane.IngressSourceEthPooled, txs, func() error {
\t\t\treturn handleTransactions(peer, txs, false)
\t\t}); err != nil {
\t\t\treturn fmt.Errorf("PooledTransactions: %v", err)
\t\t}
\t\treturn h.txFetcher.Enqueue(peer.ID(), txs, true)
''',
        "pooled transaction ingress hook",
    )

    peer = root / "eth/protocols/eth/peer.go"
    replace_once(peer, '"math/big"', '"fmt"\n\t"math/big"', "peer fmt import")
    replace_once(
        peer,
        '''func (p *Peer) SendTransactions(txs types.Transactions) error {
\t// Mark all the transactions as known, but ensure we don't overflow our limits
\tfor _, tx := range txs {
\t\tp.knownTxs.Add(tx.Hash())
\t}
\treturn p2p.Send(p.rw, TransactionsMsg, txs)
}
''',
        '''func (p *Peer) SendTransactions(txs types.Transactions) error {
\t// Mark all the transactions as known, but ensure we don't overflow our limits
\tfor _, tx := range txs {
\t\tp.knownTxs.Add(tx.Hash())
\t}
\treturn p2p.Send(p.rw, TransactionsMsg, txs)
}

// SendRawTransaction sends one already-signed transaction without decoding or
// re-encoding its inner transaction envelope. The outer ETH Transactions list
// framing is generated by p2p.Send; raw is emitted as an RLP RawValue.
func (p *Peer) SendRawTransaction(hash common.Hash, raw []byte) error {
\tif len(raw) == 0 {
\t\treturn fmt.Errorf("empty signed transaction")
\t}
\t// EIP-2718 typed envelopes are RLP strings inside an ETH list;
\t// legacy envelopes are already RLP lists. Never decode/re-sign here.
\tenvelope := rlp.RawValue(raw)
\tif raw[0] < 0x80 {
\t\tencoded, err := rlp.EncodeToBytes(raw)
\t\tif err != nil {
			return err
		}
\t\tenvelope = encoded
\t}
\tp.knownTxs.Add(hash)
\treturn p2p.Send(p.rw, TransactionsMsg, []rlp.RawValue{envelope})
}

// DarkBotFastLane returns the connection's C1.4 Fast-Lane ingress producer,
// nil when the Fast Lane is disabled. Only the peer's handler goroutine uses
// it.
func (p *Peer) DarkBotFastLane() *fastlane.PeerIngressV2 {
\treturn p.darkbotFastLane
}

// SetDarkBotFastLane stores the producer once, after registration and before
// the message loop starts.
func (p *Peer) SetDarkBotFastLane(ingress *fastlane.PeerIngressV2) {
\tp.darkbotFastLane = ingress
}
''',
        "raw outbound peer method and C1.4 accessors",
    )
    replace_once(
        peer,
        "type Peer struct {\n",
        "type Peer struct {\n"
        "\t// DarkBot C1.4 Fast-Lane ingress producer of this connection, owned by\n"
        "\t// the peer's handler goroutine.\n"
        "\tdarkbotFastLane *fastlane.PeerIngressV2\n\n",
        "peer fastlane field",
    )
    replace_once(
        peer,
        '"github.com/ethereum/go-ethereum/core/types"\n',
        '"github.com/ethereum/go-ethereum/core/types"\n\t"github.com/ethereum/go-ethereum/darkbot/fastlane"\n',
        "peer fastlane import",
    )

    handler = root / "eth/handler.go"
    replace_once(
        handler,
        '"github.com/ethereum/go-ethereum/core/types"\n',
        '"github.com/ethereum/go-ethereum/core/types"\n\t"github.com/ethereum/go-ethereum/darkbot/fastlane"\n',
        "handler fastlane import",
    )
    replace_once(
        handler,
        '''func newHandler(config *handlerConfig) (*handler, error) {
\t// Create the protocol manager with the base fields
''',
        '''func newHandler(config *handlerConfig) (*handler, error) {
\tfastLaneOwned := false
\tif err := fastlane.InitIngressV2FromEnv(); err != nil {
\t\treturn nil, fmt.Errorf("darkbot fastlane v2 init: %w", err)
\t}
\tif fastlane.IngressV2Enabled() {
\t\tfastLaneOwned = true
\t\tdefer func() {
\t\t\tif fastLaneOwned {
\t\t\t\tfastlane.ShutdownIngressV2()
\t\t\t}
\t\t}()
\t}

\t// Create the protocol manager with the base fields
''',
        "handler startup init",
    )
    replace_once(
        handler,
        '''\th.txFetcher = fetcher.NewTxFetcher(h.txpool.Has, addTxs, fetchTx, h.removePeer)
\th.chainSync = newChainSyncer(h)
\treturn h, nil
}
''',
        '''\th.txFetcher = fetcher.NewTxFetcher(h.txpool.Has, addTxs, fetchTx, h.removePeer)
\th.chainSync = newChainSyncer(h)
\tfastLaneOwned = false
\treturn h, nil
}
''',
        "handler startup ownership commit",
    )
    replace_once(
        handler,
        '''func (h *handler) Start(maxPeers int, maxPeersPerIP int) {
\th.maxPeers = maxPeers
\th.maxPeersPerIP = maxPeersPerIP
''',
        '''func (h *handler) Start(maxPeers int, maxPeersPerIP int) {
\th.maxPeers = maxPeers
\th.maxPeersPerIP = maxPeersPerIP
\tif err := fastlane.CheckIngressV2PeerCapacity(maxPeers); err != nil {
\t\tlog.Crit("DarkBot Fast Lane V2 peer capacity", "err", err)
\t}
\t// DarkBot C1.7: the outbound coordinator and its peer workers are
\t// started once here; the maintenance goroutine keeps their peer
\t// selection current, outside the BUY dispatch path.
\tif fastlane.OutboundV2Enabled() {
\t\tfastlane.StartOutboundV2()
\t\th.wg.Add(1)
\t\tgo h.fastLaneOutboundMaintenance()
\t}
''',
        "handler outbound loop start",
    )
    replace_once(
        handler,
        '''\th.peers.close()
\th.wg.Wait()

\tlog.Info("Ethereum protocol stopped")
}
''',
        '''\th.peers.close()
\th.wg.Wait()
\tfastlane.StopOutboundV2()
\tfastlane.ShutdownIngressV2()

\tlog.Info("Ethereum protocol stopped")
}
''',
        "handler fastlane close",
    )
    replace_once(
        handler,
        '''func (h *handler) txBroadcastLoop() {
\tdefer h.wg.Done()
\tfor {
\t\tselect {
\t\tcase event := <-h.txsCh:
\t\t\th.BroadcastTransactions(event.Txs)
''',
        '''func (h *handler) txBroadcastLoop() {
\tdefer h.wg.Done()
\tfor {
\t\tselect {
\t\tcase event := <-h.txsCh:
\t\t\tfastlane.PublishTxpoolAcceptedV2(event.Txs)
\t\t\th.BroadcastTransactions(event.Txs)
''',
        "txpool accepted hook (C1.4 Txpool lane, sole producer)",
    )
    replace_once(
        handler,
        "\tdefer h.unregisterPeer(peer.ID())\n",
        "\tdefer h.unregisterPeer(peer.ID())\n"
        "\n"
        "\t// DarkBot C1.4: one Fast-Lane peer lane for this connection. The\n"
        "\t// deferred Close runs after handler(peer) returned, i.e. after the\n"
        "\t// peer's last publish, and before unregisterPeer.\n"
        "\tdarkbotIngress := fastlane.BindPeerIngressV2(peer.NodeID())\n"
        "\tpeer.SetDarkBotFastLane(darkbotIngress)\n"
        "\tdefer darkbotIngress.Close()\n"
        "\tif darkbotIngress != nil && !darkbotIngress.Covered() {\n"
        "\t\tpeer.Log().Warn(\"DarkBot Fast Lane coverage gap\", \"reason\", darkbotIngress.GapReason())\n"
        "\t}\n",
        "peer lane binding at registration",
    )
    insert_anchor = '''// txReannounceLoop announces local pending transactions to connected peers again.
func (h *handler) txReannounceLoop() {
'''
    outbound_loop = '''// fastLaneOutboundMaintenance keeps the peer selection of the DarkBot C1.7
// outbound workers current. Selecting and copying the peer set happens here,
// on a timer; the BUY dispatch path only reads the selection the workers
// already hold. The workers themselves live in the fastlane package and are
// never created per transaction.
func (h *handler) fastLaneOutboundMaintenance() {
\tdefer h.wg.Done()
\tticker := time.NewTicker(250 * time.Millisecond)
\tdefer ticker.Stop()
\tfor {
\t\tpeers := h.peers.allNonEVNPeers()
\t\ttargets := make([]fastlane.OutboundTargetV2, 0, len(peers))
\t\tfor _, peer := range peers {
\t\t\ttargets = append(targets, fastlane.OutboundTargetV2{
\t\t\t\tNodeID: peer.NodeID(),
\t\t\t\tSend:   peer.SendRawTransaction,
\t\t\t\tKnown:  peer.KnownTransaction,
\t\t\t})
\t\t}
\t\tif err := fastlane.AssignOutboundTargetsV2(targets); err != nil {
\t\t\tlog.Error("DarkBot Fast Lane outbound peer selection", "err", err)
\t\t}
\t\tselect {
\t\tcase <-h.stopCh:
\t\t\treturn
\t\tcase <-ticker.C:
\t\t}
\t}
}

'''
    replace_once(handler, insert_anchor, outbound_loop + insert_anchor, "outbound loop implementation")

    blockchain = root / "core/blockchain.go"
    replace_once(
        blockchain,
        '"github.com/ethereum/go-ethereum/core/vm"\n',
        '"github.com/ethereum/go-ethereum/core/vm"\n\t"github.com/ethereum/go-ethereum/darkbot/fastlane"\n',
        "blockchain fastlane import",
    )
    replace_once(
        blockchain,
        '''func (bc *BlockChain) reorg(oldHead *types.Header, newHead *types.Header) error {
\tvar (
''',
        '''func (bc *BlockChain) reorg(oldHead *types.Header, newHead *types.Header) error {
\toriginalOldHead, originalNewHead := oldHead, newHead
\tvar (
''',
        "reorg original heads",
    )
    replace_once(
        blockchain,
        '''\t// Acquire the tx-lookup lock before mutation. This step is essential
''',
        '''\tfastlane.PublishReorgV2(originalOldHead, originalNewHead, commonBlock)

\t// Acquire the tx-lookup lock before mutation. This step is essential
''',
        "reorg provenance publish",
    )
    replace_once(
        blockchain,
        '''\tif status == CanonStatTy {
\t\tbc.chainFeed.Send(ChainEvent{
''',
        '''\tif status == CanonStatTy {
\t\tfastlane.PublishCanonicalV2(block.Header(), len(block.Transactions()))
\t\tbc.chainFeed.Send(ChainEvent{
''',
        "canonical write hook",
    )
    replace_once(
        blockchain,
        '''\tbc.chainFeed.Send(ChainEvent{
\t\tHeader:       head.Header(),
''',
        '''\tfastlane.PublishCanonicalV2(head.Header(), len(head.Transactions()))
\tbc.chainFeed.Send(ChainEvent{
\t\tHeader:       head.Header(),
''',
        "set canonical hook",
    )




def patch_c19b_testnet_accept_txs(root: Path) -> None:
    handler = root / "eth/handler.go"
    text = handler.read_text()

    old_block = (
        '\tif os.Getenv("DARKBOT_FASTLANE_V2_SCOPE") == "c19b-testnet" {\n'
        '\t\th.acceptTxs.Store(true)\n'
        '\t}\n'
    )

    new_block = (
        '\tallowC19bUnsynced, err := fastlane.C19bUnsyncedAcceptTxsAllowed(h.networkID)\n'
        '\tif err != nil {\n'
        '\t\treturn nil, fmt.Errorf("darkbot c1.9b qualification gate: %w", err)\n'
        '\t}\n'
        '\tif allowC19bUnsynced {\n'
        '\t\th.acceptTxs.Store(true)\n'
        '\t}\n'
    )

    if old_block in text:
        text = text.replace(old_block, new_block, 1)
    elif new_block not in text:
        anchor = '\th.chainSync = newChainSyncer(h)\n'
        count = text.count(anchor)
        if count != 1:
            raise SystemExit(
                f"C1_9B_ACCEPT_TXS_ANCHOR_FAIL expected=1 found={count}"
            )
        text = text.replace(
            anchor,
            anchor + "\n" + new_block + "\n",
            1,
        )

    if '\t"os"\n' in text and "os." not in text:
        text = text.replace('\t"os"\n', "", 1)

    handler.write_text(text)

    check = handler.read_text()

    if "C19bUnsyncedAcceptTxsAllowed(h.networkID)" not in check:
        raise SystemExit("C1_9B_HARDENED_GATE_VERIFY_FAIL")

    if old_block in check:
        raise SystemExit("C1_9B_OLD_SCOPE_ONLY_GATE_STILL_PRESENT")

    print("C1_9B_HARDENED_ACCEPT_TXS_PATCHED")

def require(condition: bool, label: str) -> None:
    if not condition:
        raise SystemExit(f"C1_4_HOOK_VERIFY_FAIL {label}")


def verify_hooks(root: Path) -> None:
    """Structural check of the patched sources, tolerant of gofmt."""
    handler_eth = (root / "eth/handler_eth.go").read_text()
    handler = (root / "eth/handler.go").read_text()
    peer = (root / "eth/protocols/eth/peer.go").read_text()

    require(handler_eth.count("fastlane.IngressV2(") == 2, "two ingress boundaries")
    require("fastlane.PublishTransactionAt(" not in handler_eth, "no V1 peer ingress")
    require("fastlane.MonotonicNS()" not in handler_eth, "no V1 hook clock")
    for match in re.finditer(r"fastlane\.IngressV2\(", handler_eth):
        tail = handler_eth[match.start():]
        enqueue = tail.find("h.txFetcher.Enqueue(")
        require(enqueue > 0, "boundary precedes Enqueue")
        require(re.search(r"handleTransactions\(peer, txs, (true|false)\)", tail[:enqueue]) is not None,
                "geth validation runs inside the boundary")
    for source in ("IngressSourceEthBroadcast", "IngressSourceEthPooled"):
        require(handler_eth.count("fastlane." + source) == 1, source)

    require(handler.count("fastlane.BindPeerIngressV2(peer.NodeID())") == 1, "one bind per connection")
    unregister = handler.find("defer h.unregisterPeer(peer.ID())")
    close_ = handler.find("defer darkbotIngress.Close()")
    run_handler = handler.find("return handler(peer)")
    require(0 <= unregister < close_ < run_handler, "close deferred after unregister, before the message loop")
    require(handler.count("fastlane.PublishTxpoolAcceptedV2(event.Txs)") == 1, "txpool lane sole producer")
    require("fastlane.PublishTransaction(fastlane.TxpoolAccepted" not in handler, "no V1 txpool evidence")
    require(handler.count("fastlane.InitIngressV2FromEnv()") == 1, "startup init")
    require(handler.count("fastlane.CheckIngressV2PeerCapacity(maxPeers)") == 1, "peer capacity check")
    wait = handler.find("h.wg.Wait()\n\tfastlane.StopOutboundV2()")
    require(wait >= 0 and handler.find("fastlane.ShutdownIngressV2()", wait) > wait, "shutdown after wg.Wait")

    require(re.search(r"darkbotFastLane\s+\*fastlane\.PeerIngressV2", peer) is not None, "peer field")
    require(peer.count("func (p *Peer) DarkBotFastLane()") == 1, "peer accessor")
    require(peer.count('"github.com/ethereum/go-ethereum/darkbot/fastlane"') == 1, "peer import")
    print("C1_4_HOOK_ORDER_VERIFIED")

    # C1.7: local-geth outbound runs on the V2 lanes with persistent workers.
    for v1 in ("fastlane.InitFromEnv()", "fastlane.Enabled()", "fastlane.Close()", "fastlane.TryOutbound()",
               "fastlane.OutboundFanout()", "fastlane.PublishOutboundHandoff(", "fastLaneOutboundLoop"):
        require(v1 not in handler, f"no V1 outbound path: {v1}")
    require(handler.count("fastlane.StartOutboundV2()") == 1, "outbound started once")
    require(handler.count("go h.fastLaneOutboundMaintenance()") == 1, "one maintenance goroutine")
    require(handler.count("fastlane.AssignOutboundTargetsV2(targets)") == 1, "peer selection in maintenance")
    maintenance = function_body(handler, "handler", "fastLaneOutboundMaintenance")
    require(maintenance is not None, "maintenance function")
    require("allNonEVNPeers()" in maintenance and "ticker.C" in maintenance, "selection on a timer")
    require("peer.SendRawTransaction" in maintenance and "peer.KnownTransaction" in maintenance, "raw send target")
    for forbidden in ("go func", "go h.", "go fastlane."):
        require(forbidden not in maintenance, "maintenance starts no goroutine")
    # Outside the maintenance goroutine the peer set is not walked for the
    # Fast Lane: no fastlane call shares a function with allNonEVNPeers.
    for receiver, name, body in go_functions(handler):
        if name != "fastLaneOutboundMaintenance" and "allNonEVNPeers()" in body:
            require("fastlane." not in body, f"no per-record peer selection in {name}")
    print("C1_7_OUTBOUND_HOOKS_VERIFIED")


CHAIN_HOOKS = {
    "writeBlockAndSetHead": "fastlane.PublishCanonicalV2(",
    "reorg": "fastlane.PublishReorgV2(",
    "SetCanonical": "fastlane.PublishCanonicalV2(",
}
CHAINMU_LOCK = "bc.chainmu.TryLock()"


def go_functions(source: str):
    """Top-level functions of a Go file as (receiver type, name, text)."""
    heads = [(m.start(), m.group(1), m.group(2))
             for m in re.finditer(r"^func (?:\(\w+ \*?(\w+)\) )?(\w+)\(", source, re.M)]
    out = []
    for i, (start, receiver, name) in enumerate(heads):
        end = heads[i + 1][0] if i + 1 < len(heads) else len(source)
        out.append((receiver, name, source[start:end]))
    return out


def function_body(source: str, receiver: str, name: str):
    for found_receiver, found_name, body in go_functions(source):
        if found_receiver == receiver and found_name == name:
            return body
    return None


def verify_chain_ownership(root: Path) -> None:
    """C1.8: every Chain-lane publisher runs with BlockChain.chainmu held.

    Checked on the patched pinned source, mechanically:
      1. chainmu is syncx.ClosableMutex and its TryLock blocks (a channel
         receive), so holding it is mutual exclusion;
      2. the Chain-lane hooks are exactly the three reviewed call sites;
      3. every in-package call path into a hook function passes through a
         BlockChain method that locks chainmu before the call;
      4. an exported method on such a path that does not lock itself is called
         from outside BlockChain only with ExecuteConfig.WriteState false,
         which cannot reach the head write.
    """
    mutex = (root / "internal/syncx/mutex.go").read_text()
    try_lock = function_body(mutex, "ClosableMutex", "TryLock")
    require(try_lock is not None and "<-cm.ch" in try_lock, "ClosableMutex.TryLock blocks on its channel")

    methods = {}
    core_sources = {}
    for path in sorted((root / "core").glob("*.go")):
        if path.name.endswith("_test.go"):
            continue
        text = path.read_text()
        core_sources[path.name] = text
        for receiver, name, body in go_functions(text):
            if receiver == "BlockChain":
                methods[name] = body
    blockchain = core_sources["blockchain.go"]
    require(re.search(r"chainmu\s+\*syncx\.ClosableMutex", blockchain) is not None, "chainmu is a ClosableMutex")

    all_core = "".join(core_sources.values())
    require(all_core.count("fastlane.PublishCanonicalV2(") == 2, "two canonical hooks")
    require(all_core.count("fastlane.PublishReorgV2(") == 1, "one reorg hook")
    require("fastlane.PublishCanonical(" not in all_core and "fastlane.PublishReorg(" not in all_core,
            "no V1 chain hook")
    for name, hook in CHAIN_HOOKS.items():
        require(name in methods and hook in methods[name], f"hook inside BlockChain.{name}")
    set_canonical = methods["SetCanonical"]
    require(0 <= set_canonical.find(CHAINMU_LOCK) < set_canonical.find(CHAIN_HOOKS["SetCanonical"]),
            "SetCanonical locks chainmu before its hook")

    unlocked_exported = set()

    def callers(target):
        found = []
        for name, body in methods.items():
            if name == target:
                continue
            for match in re.finditer(r"\bbc\." + re.escape(target) + r"\(", body):
                found.append((name, CHAINMU_LOCK in body[:match.start()]))
        return found

    def trace(target, seen):
        """True when every call path into `target` holds chainmu."""
        found = callers(target)
        if not found:
            return False
        for name, locked in found:
            if locked or name in seen:
                continue
            if name[0].isupper():
                unlocked_exported.add(name)
            if not trace(name, seen | {name}):
                return False
        return True

    for name in ("writeBlockAndSetHead", "reorg"):
        require(trace(name, {name}), f"every path into BlockChain.{name} holds chainmu")

    # An exported method that does not lock can be called from anywhere.
    for name in sorted(unlocked_exported):
        for path in sorted(root.rglob("*.go")):
            if path.name.endswith("_test.go") or "/build/" in str(path):
                continue
            text = core_sources.get(path.name) if path.parent == root / "core" else None
            if text is None:
                text = path.read_text(errors="replace")
            for match in re.finditer(r"\." + name + r"\(", text):
                if path.parent == root / "core" and inside_blockchain_method(text, match.start()):
                    continue
                window = text[max(0, match.start() - 800):match.start()]
                config = window[window.rfind("ExecuteConfig{"):] if "ExecuteConfig{" in window else ""
                require(re.search(r"WriteState:\s*false", config) is not None
                        and re.search(r"WriteHead:\s*true", config) is None,
                        f"{path.relative_to(root)} calls {name} without WriteState false")
    print("C1_8_CHAIN_OWNERSHIP_VERIFIED")


def inside_blockchain_method(source: str, offset: int) -> bool:
    position = 0
    for receiver, _name, body in go_functions(source):
        start = source.find(body, position)
        if start <= offset < start + len(body):
            return receiver == "BlockChain"
        position = start
    return False


if __name__ == "__main__":
    raise SystemExit(main())
