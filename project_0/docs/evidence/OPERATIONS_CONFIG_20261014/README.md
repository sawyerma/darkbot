# Operational configuration — local qualification (2026-10-14)

Implementation extends the existing Rust provider Budget and WSS ingress; the Python
CLI compiles profiles through the same Rust contract using a private temporary file.
No second observer, verifier, economics or submission engine is introduced.
Usage, parameter scope, precedence and non-claims: ../../OPERATIONAL_CONFIG.md.

Regression coverage includes profile/environment/file/CLI precedence, invalid types
and financial keys, endpoint redaction, private handoff/cleanup, duration and exact
snapshot boundaries, bootstrap versus capture timing, bounded socket/admission/call/
payload capacity, 429/adaptive recovery, explicit endpoint fallback and no retry of
sendTransaction. Local loopback fixtures are used; no Mainnet calls were made.
Python tests cover the existing capture launcher, verification modes, reports,
economics invariants and the new operational bridge. No user-owned 5h replay ran.

The all-targets Clippy differential uses the existing FE00 baseline and keys
(file, lint code, message), ignoring shifted line numbers. Changed-region rustfmt
checks preserve unrelated legacy formatting. Logs, commands, exact test counts,
code hashes and gate statuses are in this directory.

Local configuration qualification is not provider capacity or first-profit proof.
FE-01/02 current program/source/state parity, complete selected transaction
simulation, executable exit and positive conservative net edge remain OPEN.
FE-04 is PARTIAL_LOCAL: pilot CLI/approval lifecycle, production Intent/Current
assembly, qualified exit and complete cumulative realized-PnL accounting remain.
FE-03/05 actual Shadow/provider qualification and FE-06 financial execution are
separate permission gates. LIVE and submission remain blocked; no paid fallback
or provider purchase was made. Race, Jito and M4 obligations are unchanged.

Final gates: Rust **370 txv1 / 368 health PASS**, 0 failures, 1 existing ignored per matrix; Python **65 PASS**; both release builds PASS; changed-region formatting PASS; all-targets Clippy **38/38**, zero added diagnostics. See qualification.json and individual logs.

Copied terminal logs normalize trailing blank lines only; result lines are unchanged.
