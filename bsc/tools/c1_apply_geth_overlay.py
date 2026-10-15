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
MARKER = "DARKBOT_C1_FASTLANE_C1_4"
MARKER_FILE = ".darkbot-c1-fastlane-c14"
LEGACY_MARKER_FILE = ".darkbot-c1-fastlane-v1"


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

    if (root / LEGACY_MARKER_FILE).exists():
        raise SystemExit(
            "the checkout carries the C1.1-C1.3 overlay; reset it to the pin first:\n"
            f"  git -C {root} checkout -- eth core\n"
            f"  rm -rf {root}/darkbot {root}/{LEGACY_MARKER_FILE}"
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
    verify_hooks(root)
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
\tp.knownTxs.Add(hash)
\treturn p2p.Send(p.rw, TransactionsMsg, []rlp.RawValue{rlp.RawValue(raw)})
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
        '"math/big"\n',
        '"math/big"\n\t"runtime"\n',
        "handler runtime import",
    )
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
\tif err := fastlane.InitFromEnv(); err != nil {
\t\treturn nil, fmt.Errorf("darkbot fastlane init: %w", err)
\t}
\tif err := fastlane.InitIngressV2FromEnv(); err != nil {
\t\tfastlane.Close()
\t\treturn nil, fmt.Errorf("darkbot fastlane v2 ingress init: %w", err)
\t}
\tif fastlane.Enabled() || fastlane.IngressV2Enabled() {
\t\tfastLaneOwned = true
\t\tdefer func() {
\t\t\tif fastLaneOwned {
\t\t\t\tfastlane.Close()
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
\tif fastlane.Enabled() {
\t\th.wg.Add(1)
\t\tgo h.fastLaneOutboundLoop()
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
\tfastlane.Close()
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
    outbound_loop = '''// fastLaneOutboundLoop consumes exact signed transaction bytes from the
// Rust->geth SPSC ring and hands the same inner transaction envelope to
// selected ETH peers. A successful p2p.Send return is recorded separately; it
// is not remote receipt, inclusion or execution.
func (h *handler) fastLaneOutboundLoop() {
\tdefer h.wg.Done()
\tspins := uint32(0)
\tfor {
\t\tselect {
\t\tcase <-h.stopCh:
\t\t\treturn
\t\tdefault:
\t\t}

\t\trecord, ok, err := fastlane.TryOutbound()
\t\tif err != nil {
\t\t\tlog.Error("DarkBot Fast Lane outbound stopped", "err", err)
\t\t\treturn
\t\t}
\t\tif !ok {
\t\t\tspins++
\t\t\tif spins&1023 == 0 {
\t\t\t\truntime.Gosched()
\t\t\t}
\t\t\tcontinue
\t\t}
\t\tspins = 0
\t\tif record.Kind != fastlane.OutboundSignedTx {
\t\t\tlog.Error("DarkBot Fast Lane unexpected outbound record", "kind", record.Kind)
\t\t\treturn
\t\t}
\t\tvar tx types.Transaction
\t\tif err := tx.UnmarshalBinary(record.Payload); err != nil {
\t\t\tlog.Error("DarkBot Fast Lane invalid signed transaction", "err", err)
\t\t\treturn
\t\t}
\t\tif tx.Hash() != record.Hash {
\t\t\tlog.Error("DarkBot Fast Lane outbound hash mismatch", "have", tx.Hash(), "want", record.Hash)
\t\t\treturn
\t\t}

\t\twanted := fastlane.OutboundFanout()
\t\tsent := 0
\t\tfor _, peer := range h.peers.allNonEVNPeers() {
\t\t\tif sent >= wanted {
\t\t\t\tbreak
\t\t\t}
\t\t\tif peer.KnownTransaction(record.Hash) {
\t\t\t\tcontinue
\t\t\t}
\t\t\tif err := peer.SendRawTransaction(record.Hash, record.Payload); err != nil {
\t\t\t\tcontinue
\t\t\t}
\t\t\tpeerID := peer.NodeID()
\t\t\tfastlane.PublishOutboundHandoff(record.Hash, peerID[:])
\t\t\tsent++
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
        '''\tfastlane.PublishReorg(originalOldHead, originalNewHead, commonBlock)

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
\t\tfastlane.PublishCanonical(block.Header(), len(block.Transactions()))
\t\tbc.chainFeed.Send(ChainEvent{
''',
        "canonical write hook",
    )
    replace_once(
        blockchain,
        '''\tbc.chainFeed.Send(ChainEvent{
\t\tHeader:       head.Header(),
''',
        '''\tfastlane.PublishCanonical(head.Header(), len(head.Transactions()))
\tbc.chainFeed.Send(ChainEvent{
\t\tHeader:       head.Header(),
''',
        "set canonical hook",
    )



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
    wait = handler.find("h.wg.Wait()\n\tfastlane.Close()")
    require(wait >= 0 and handler.find("fastlane.ShutdownIngressV2()", wait) > wait, "shutdown after wg.Wait")

    require(re.search(r"darkbotFastLane\s+\*fastlane\.PeerIngressV2", peer) is not None, "peer field")
    require(peer.count("func (p *Peer) DarkBotFastLane()") == 1, "peer accessor")
    require(peer.count('"github.com/ethereum/go-ethereum/darkbot/fastlane"') == 1, "peer import")
    print("C1_4_HOOK_ORDER_VERIFIED")


if __name__ == "__main__":
    raise SystemExit(main())
