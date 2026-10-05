# DarkBot production/economic audit bundle

This directory contains the reproducible evidence collected for the full audit at Git `799048f8fa95df4c29f2aa6b35bcc3615405fe19` on 2026-10-05.

The zip also contains a clean `git archive` of that exact DarkBot commit and source snapshots of the public comparison repositories. Secrets and local `.env` files are excluded. The public RPC replay used `https://api.mainnet-beta.solana.com`; no signing or submission RPC was called.

## Contents

- `COMMANDS_AND_RESULTS.md`: commands issued, purpose, exit/result, and pointers to raw output.
- `AUDIT_FINDINGS.md`: findings and limitations established by the audit run.
- `main_repo_799048f.tar`: tracked DarkBot source at the audited commit.
- `references/`: source snapshots of the pinned comparison repositories.
- `evidence/`: copied audit scripts, raw RPC transaction JSON, historical replay results, cache counterexamples, and test logs.
- `checksums.sha256`: SHA-256 for every bundle file.

The audit evidence is additive and was not committed to `main` by this packaging step. The source tree itself was not changed by the audit bundle creation.

One audit probe intentionally failed to deserialize Solana's JSON response because the local SDK type expected a different JSON shape. Its exact panic is retained in `evidence/winner_economics_repro.txt` and recorded as unproven; it is not presented as a PASS.
