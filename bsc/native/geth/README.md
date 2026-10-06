# C1 pinned bsc-geth baseline

C1 uses the official BNB Chain BSC client repository as the chain-client base.

Pinned production baseline:

```text
repository  https://github.com/bnb-chain/bsc.git
release     v1.7.8
commit      cdb7548b5baacfdae92f9f63437d6456411665f3
tree        d8d88097521d43cef66047fdd93dd6825a2a7843
go.mod      go 1.25.0
```

The pin is source identity only. It does not claim that upstream v1.7.8 already
contains DarkBot's Fast-Lane hooks, mmap/SPSC transport, producer timestamps,
outbound handoff instrumentation or C1 latency evidence.

The later C1 patched source must remain attributable to this exact parent
commit. Any upstream change or rebase requires a new pin and requalification.

Verify a checkout with:

```bash
bsc/native/tools/c1_verify_geth_pin.sh /path/to/bsc-geth
```

A valid untouched checkout ends with `C1_GETH_PIN_PASS`. The verifier rejects
a different commit/tree/tag/module/go version and rejects tracked local
modifications. Once the C1 hook patch exists, its build provenance will be
recorded separately rather than weakening this pristine-source verifier.

No funded action and no LIVE authorization follows from this pin.
