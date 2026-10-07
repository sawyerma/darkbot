# P6-MT Tranche 1 r3 — Local Qualification

Date: 2026-10-07

## Provenance

- integration base: `9e2a7a4a6893b078e984f33c67c9c27f1d53624a`
- qualified code commit: `91435080ca7eb01d6ae201402b31d2f368210c26`
- commit subject: `[Project 0] integrate P6-MT tranche 1 r3`
- external candidate: `P6MT_v4.6_tranche1_r3.zip`
- candidate ZIP SHA-256: `a95b8b7405a74bbcbee8e9c7cc94c4506439ec3f04774815cc23ccfd74a0cb1d`
- integration scope: `project_0/native/crates/darkbot-runner/`

GitHub comparison of base → qualified commit contains exactly one commit and only these nine production files:

```text
builder.rs
lib.rs
main.rs
pipeline.rs
record.rs
remote_workers.rs
route.rs
winner.rs
winner_tracker.rs (new)
```

No BSC tree and no `darkbot-common`, `darkbot-solana`, or `darkbot-project0` source was changed by the qualified commit.

## Qualified scope

- F1 WinnerTracker revision/stale-result hardening.
- F8 typed winner parser and regression coverage.
- P1-C executable-floor economics with missing-floor → `Evaluation::Unknown`.
- Explicit `route_min_out_unknown` evidence marker.
- Three candidate-specific Clippy fixes discovered during qualification.

## Final local gates

```text
workspace build / crate_health             PASS
workspace build / crate_health + txv1      PASS
workspace tests / crate_health             PASS
workspace tests / crate_health + txv1      PASS
F8 ignored diagnostic probe                PASS
F8_SDK_DECODE_PROBE                        ok=true err=None
```

## Differential Clippy

Tool: `clippy 0.1.93`

The exact pre-tranche base and qualified candidate were checked with identical commands for both feature sets.

```text
crate_health:
  baseline normalized lints = 8
  candidate normalized lints = 8
  candidate-only lints = 0

crate_health + txv1:
  baseline normalized lints = 8
  candidate normalized lints = 8
  candidate-only lints = 0
```

The remaining eight lints are baseline debt. This is not a full-workspace strict-Clippy PASS.

## Decision

**P6-MT Tranche 1 r3: LOCAL CODE QUALIFICATION PASS.**

## Non-claims

This does not prove Mainnet Shadow qualification, P1-A/P1-B closure, Drift support, controlled/production LIVE readiness, landing probability, capture rate, or positive realized PnL. Full P6-MT remains incomplete and LIVE remains blocked.
