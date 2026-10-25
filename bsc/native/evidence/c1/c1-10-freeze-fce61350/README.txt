C1.10 technical freeze, IMPLEMENTED_TESTED/FROZEN, bindings_sha256 aa02f5913782dc5a9a0f61c0a3d3dd7f93a1b1e2cca9df072630716a2bc9ca2c
darkbot_commit fce613504eed40c8b79bb23685685b0b321cf5e7, final_c1_source_sha256 2617ebde3e45990b7a37d000d5501f9019d467d46cb6c8fe05aa7a979ca8c1a7 (502 files)
Gates C1.5-C1.9b accepted technically under order V2 Paket 3 (Claude Opus 5.5); no economic approval; non-claims: T0_TO_T_BUY_LT_1MS NOT_PROVEN, MAINNET_INGRESS_COVERAGE NOT_PROVEN, PROVIDER_ORDERING UNQUALIFIED, LIVE BLOCKED.
Verify: clean detached clone at darkbot_commit; bound binaries (SHA256SUMS) in bsc/.local/c1_freeze/; pinned bsc-geth checkout at bsc/.local/c1_freeze/bsc-geth (HEAD cdb7548b); then
  bsc/.local/c1_freeze/bin/c1_freeze --repo . --inputs bsc/.local/c1_freeze/inputs.dbrec --verify bsc/.local/c1_freeze/c1-10-freeze-record.dbrec
