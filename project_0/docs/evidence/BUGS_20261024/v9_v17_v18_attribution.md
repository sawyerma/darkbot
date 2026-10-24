# BUGS V9 / V17 / V18 — own persisted causes and explicit texts — 2026-10-24 (LOCAL_PASS, offline)

Offline, no Mainnet, no LIVE, no submission. Code commit `4d08b10d`. Gate `project_0/scripts/gate.sh`
on it: fmt, clippy ×4,
test_all 811/0/10, test_default 809/0/10, runner 372/0/3 ×2, release, Python OK — all exit 0.

| ID | Change | Regression tests |
|---|---|---|
| V17 (Q5) | `LossReason::Plan` (appended): `PlanErrorKind::Other` ⇒ PLAN, `HealthUnavailable` ⇒ HEALTH (both ROUTING before); Routing stays ROUTING; FeeState stays UNKNOWN (P5) | `q5_evaluate_records_the_typed_plan_error` (real `Pipeline::evaluate`), `q5_plan_errors_are_classified_by_kind_not_text` |
| V9 | `LossReason::Safety` (appended): a plan the economics would act on that the LIVE Safety preview refuses ⇒ `DeniedBySafety` + SAFETY (before: ECONOMIC_SKIP/THRESHOLD_TOO_HIGH); the admission-boundary denials (preview, day loss, reservation) carry SAFETY (before: no loss) | `v9_safety_denials_are_their_own_cause` (real LIVE `Pipeline::evaluate`, preview `TotalCapital` and boundary `DailyLoss`; nothing reserved, journaled or sent) |
| V18 (Q3) | explicit texts instead of derived Debug: `submit::transaction_error_text` (exhaustive over the pinned `TransactionError`/`InstructionError`, 92 shapes checked once equal to the pin's Debug), `Deny::record_text` + `HaltReason::record_name`, `planner::PlanError::record_text`, `Evaluation::record_text` (all byte-identical to the former Debug); `EXIT_BUILD` uses the error's Display | `q3_simulation_and_landing_errors_have_an_explicit_text` (real JSON-RPC wire), `c4_reconcile_…` (landing `err`), `q3_reason_texts_of_own_types_are_explicit_and_unchanged` |

Schema and attribution model:
- `ECON_SHADOW_EVALUATION` schema 2 → 3 (identical layout, `LossReason` appended only). Schema 2 stays
  readable; a schema-2 frame or a JSONL line carrying PLAN/SAFETY is forged and refused. Golden
  `fixtures/shadow_evaluations_schema2.p0j` (sha256 `8cf15a28…57b3d`) was written by the writer before
  this change; test `v17_earlier_writer_evaluations_keep_their_attribution`.
- Reports: model `EXHAUSTIVE_V2` (buckets PLAN, SAFETY; golden `econ_reports.json` +2 zero buckets ×4,
  model label only); a report keeps the model it was written under. An offline replay
  (`p0-econ-replay`) over evaluations of the earlier writer labels its summaries
  `EXHAUSTIVE_V1_RECORDS` (their ROUTING/ECONOMIC_SKIP still include the causes split since).
- `OWNER_OPPORTUNITY`, `ECON_CANONICAL`, `ECON_OPPORTUNITY` keep their schema: their old bytes decode
  unchanged; an older binary refuses a frame with a new cause (`PAYLOAD_DECODE`, fail-closed).
- Deliberate text change: the simulation failure code is again `RPC_SIMULATION_FAILED <TransactionError>`
  (the 3.1.12 pin had silently written `UiTransactionError(<…>)`); free text, no reader parses it.

Mutations fail-before (11): Other→Routing, HealthUnavailable→Routing, preview not counted, preview
decision kept, boundary without loss, schema not bumped, forged frame accepted, forged JSONL
accepted, replay label missing, simulation wrapper back, Deny text changed.

Not changed: serde identifier names in evidence JSON (separate topic in V18), operator log lines.
