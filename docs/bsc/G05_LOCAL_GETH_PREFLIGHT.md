# G0.5 Local bsc-geth / P2P preflight

Purpose: establish whether a local BSC mainnet geth-compatible node is already present and usable for the next G0.5 inclusion-path measurement.

This preflight is read-only. It does not submit any transaction.

Run from the repository root:

~~~bash
command -v geth || true
command -v bsc || true
command -v bsc-geth || true

find "$HOME" -maxdepth 4 -type s -name geth.ipc 2>/dev/null | head -20
~~~

If a local IPC socket is found, query only read-only node state:

~~~bash
GETH_BIN="$(command -v geth || command -v bsc || command -v bsc-geth)"
IPC_PATH="/replace/with/discovered/geth.ipc"

"$GETH_BIN" attach "$IPC_PATH" --exec 'web3.clientVersion'
"$GETH_BIN" attach "$IPC_PATH" --exec 'eth.chainId'
"$GETH_BIN" attach "$IPC_PATH" --exec 'net.peerCount'
"$GETH_BIN" attach "$IPC_PATH" --exec 'eth.syncing'
~~~

Required before building the local-geth G0.5 transport benchmark:

- chainId = 56;
- peerCount > 0;
- syncing = false;
- IPC reachable locally.

If no local BSC geth exists, do not fabricate a local-geth PASS. Record the path as unavailable on the current machine and decide whether to provision a pinned BSC geth or continue G0.5 with the best measured external candidate.

Exact G0.5 timing contract remains:

~~~text
T_pre -> local decode/build/sign -> Rust-to-IPC queue/commit -> geth/network handoff -> T_send
~~~

An IPC write or local geth acceptance alone is not the final adapter-specific T_send boundary.
