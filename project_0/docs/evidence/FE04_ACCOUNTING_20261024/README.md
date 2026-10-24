# FE-04 attempt archive, cumulative accounting, read-only treasury — 2026-10-24 (LOCAL_PASS, offline)

Commits `286f9cc8` (archive + accounting) and `366271b5` (treasury view + approval history).
Gates exit 0: 822/0/10, 820/0/10, runner 377/0/3 ×2 and 823/0/10, 821/0/10, runner 378/0/3 ×2;
release; Python OK (incl. `tests/integration/test_fe04_cli.py` on the release `p0ctl`).
Mutations fail-before: 6 (archive not replayed, approval reusable, open position archived,
unsealed file read as history, open inventory ignored, zero counted as profit) + 2 (holds
spendable, cross-asset profit without price).

| Requirement (PHASENPLAN §0.6/§13 FE-04/FE-07, P8, WALLETS_TREASURY §3) | Code | Test |
|---|---|---|
| One-shot per approval, no auto-rearm; next attempt only with a new approval | `one_shot/archive.rs` `single_use`, `Journal::archive` | `fe04_archive_keeps_the_attempt_history_and_approvals_single_use` |
| Day loss limit survives finished attempts (B3) | `setup.rs` `Fe04Start::start` archive replay | `fe04_archived_attempt_stays_booked_after_restart` |
| Cumulative realized net after all own attempts; GO/STOP verdict | `execution/accounting.rs` `Ledger::{totals, verdict}` | archive test, `fe07_verdict_is_loss_or_no_trade_from_settled_attempts` |
| Withdrawable ≠ balance ≠ shadow; UNKNOWN never 0 | `Ledger::{realized_deltas, withdrawable}` | `withdrawable_is_the_settled_profit_in_the_asset_after_holds_and_reserve` |
| Terminal text, offline | `p0ctl fe04 accounting|archive`, `approval show --journal-dir` | `test_fe04_cli` |

NOT_RUN: real attempts, exits and chain finality (FE-06/07), production admission caller (FE-06),
any treasury transfer (no code; separate approval), Mainnet, LIVE.
