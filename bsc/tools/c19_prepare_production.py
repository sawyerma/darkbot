#!/usr/bin/env python3
"""Measurement-only bridge into the real ETH peer package in a disposable clone.

Fastlane cannot import ETH (ETH already imports fastlane). Export the existing
integrated producer harness only in the temporary clone, then call it from an
ETH test with real p2p.Server selected peers. No production import cycle, mock
MsgWriter, duplicate send implementation or per-BUY goroutine is introduced.
"""
import sys
from pathlib import Path


def function(source, name):
    start = source.index("func " + name + "(")
    brace = source.index("{", start)
    depth = 1
    end = brace + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


def prepare(root):
    lane = root / "darkbot/fastlane"
    test = lane / "c19_latency_test.go"
    s = test.read_text()
    s = s.replace("func TestC19NativeLatencyProducer(t *testing.T) {",
                  "func C19RunProducer(t *testing.T, factory func(*testing.T, byte, int) OutboundTargetV2) {\n c19MakeTarget = factory")
    (lane / "c19_measurement_bridge.go").write_text(s)
    test.unlink()
    ingress = (lane / "ingress_test.go").read_text()
    chain = (lane / "chain_v2_test.go").read_text()
    helpers = '''package fastlane
import (
 "testing"
 "math/big"
 "github.com/ethereum/go-ethereum/common"
 "github.com/ethereum/go-ethereum/core/types"
 "github.com/ethereum/go-ethereum/crypto"
)
const c14TestKeyHex = "4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318"
var c14Portal = common.HexToAddress("0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0")
'''
    for name in ("c14Accept", "c14Node", "c14BuyCalldata", "c14SmallLaunch", "c14Tx", "c14Start", "c14Bind"):
        helpers += function(ingress, name) + "\n"
    helpers += function(chain, "c15Header") + "\n"
    (lane / "c19_measurement_helpers.go").write_text(helpers)
    template = Path(__file__).with_name("c19_production_peer.go.txt")
    (root / "eth/protocols/eth/c19_production_test.go").write_text(template.read_text())


if __name__ == "__main__":
    prepare(Path(sys.argv[1]))
