C1.10 technical freeze, IMPLEMENTED_TESTED/FROZEN, bindings_sha256 75d8d7b080f003b7ce92883c8454dc3919e525144230df0cdc232f801a06fd82
darkbot_commit 818525b172383a0b96202ae09ec9d683192a9027, final_c1_source_sha256 e8e63ad11a9e70f4fcafd65b8e8dcda72dfbb16ca370d28c03247bb844989090 (539 files)
Supersedes c1-10-freeze-fce61350 (Paket 3 review: measured != frozen binaries, statements). Every bound binary is a reproducible build of
this source and bit-identical to the binary that produced the bound C1.9/C1.9b evidence (hashes in SHA256SUMS and build_commands.txt).
Gates C1.5-C1.9b accepted technically under order V2 Paket 3 (Claude Opus 5.5); no economic approval; non-claims: T0_TO_T_BUY_LT_1MS NOT_PROVEN,
MAINNET_INGRESS_COVERAGE NOT_PROVEN, PROVIDER_ORDERING UNQUALIFIED, LIVE BLOCKED.
Verify: clean detached clone at darkbot_commit; rebuild per build_commands.txt (or the local copies in bsc/.local/c1_freeze/); pinned bsc-geth
checkout at bsc/.local/c1_freeze/bsc-geth (HEAD cdb7548b); then
  bsc/.local/c1_freeze/bin/c1_freeze --repo . --inputs bsc/.local/c1_freeze/inputs.dbrec --verify bsc/.local/c1_freeze/c1-10-freeze-record.dbrec
