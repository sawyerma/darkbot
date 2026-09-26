# DarkBot — Phase B Optional Cleanup Contract (DRAFT)

Status: **DRAFT / OPTIONAL / NOT APPROVED / NOT ACTIVE**  
Roadmap position: between Phase A and later work only if explicitly approved  
Execution rule: **DO NOT EXECUTE automatically**  
Authority: this file describes possible cleanup only; `docs/BSC_DUAL_VENUE_ROADMAP.md` decides whether Phase B is ever activated.

---

## 1. Purpose

Phase B exists only as a possible non-semantic cleanup pass.

It is not required for BSC functionality and may remain skipped forever.

The purpose, if it is later approved, is limited to removing proven temporary adapter/fallback wiring, tightening type boundaries and reducing coupling without changing any behavior.

This document is intentionally **not frozen as a final implementation contract**.

It must be independently reviewed and may be edited before any Phase-B implementation is authorized.

No coding agent may infer permission to execute Phase B merely because this file exists.

---

## 2. Activation gate

Phase B may start only if all of the following are true:

1. the user explicitly approves Phase B;
2. an independent review identifies concrete cleanup targets;
3. each target is listed by exact file/symbol and reason;
4. the review proves the cleanup is useful and not speculative;
5. existing Solana/Pump and BSC behavior can be preserved exactly;
6. the complete pre-edit suite is green;
7. required parity/performance baselines are available for the touched path.

If no concrete cleanup need exists:

**SKIP PHASE B.**

---

## 3. Allowed scope if later approved

Potentially allowed:

- remove temporary compatibility/fallback wiring that is proven unused;
- tighten existing adapter/protocol typing;
- remove duplicated imports or trivial coupling;
- simplify existing control flow without semantic change;
- improve names/comments where they are objectively stale or misleading;
- delete dead code only when reachability/usage is proven absent.

Every cleanup must reduce complexity or ambiguity.

---

## 4. Explicitly forbidden

Phase B must not:

- add features;
- add BSC networking;
- add Flap/Four decoding;
- add economics;
- change Strategy A/B/C;
- change replay semantics;
- change PAPER semantics;
- change fee semantics;
- change config hashes;
- change historical DB meaning;
- add migrations;
- add a new registry/runtime architecture;
- introduce a new event bus;
- introduce multiprocessing/microservices;
- add an ORM;
- add speculative abstraction for future chains;
- add hardcoded provider URLs, credentials or protocol constants;
- introduce a virtual environment/vendor tree into the repository;
- modify `docs/phase_a/**`.

---

## 5. Simplicity rule

Any proposed Phase-B change must make the system objectively simpler.

A cleanup is rejected if it:

- increases the number of layers;
- adds indirection without removing more complexity;
- creates a second way to do the same thing;
- introduces generic machinery for hypothetical future use;
- moves stable working code merely for aesthetics.

Prefer no change over speculative cleanup.

---

## 6. Required evidence before approval

Before Phase B can be converted from DRAFT to APPROVED, produce a short review table:

| Target | Current problem | Proposed simplification | Semantic risk | Tests proving parity | Decision |
| --- | --- | --- | --- | --- | --- |

Every row must have a concrete repository target.

No generic "refactor architecture" item is acceptable.

---

## 7. Test gate if Phase B is ever executed

At minimum:

- complete pre-edit suite;
- complete post-edit suite;
- compileall;
- legacy config-hash equality;
- touched-path parity test;
- touched-path performance comparison;
- no changes under `docs/phase_a/**`.

Any unexplained semantic or reproducible performance difference fails Phase B.

---

## 8. Stop rule

If a proposed cleanup requires semantic decisions, a schema change, new product behavior or new BSC functionality:

**STOP. It is not Phase B.**

---

## 9. Current decision

Current status remains:

**SKIPPED / NOT APPROVED.**

This file may be reviewed and changed later before any implementation.
