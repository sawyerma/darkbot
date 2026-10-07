# DarkBot

DarkBot is organized by product/domain at repository root.

```text
darkbot/
├── bsc/         BSC Birth Entry / Gate0
├── project_0/   Solana Project 0 liquidator
├── solana/      Solana / Pump.fun and shared Solana Python infrastructure
├── frontend/    one shared React control-plane/dashboard
└── README.md
```

## Ownership

**BSC:** everything specific to BSC lives below `bsc/`: source boundary, Gate0, tests, docs, config, tools and evidence.

**Project 0:** everything specific to the Project 0 liquidator lives below `project_0/`. The Rust production workspace is `project_0/native/`; the authoritative phase plan is `project_0/docs/PHASENPLAN.md`.

**Solana:** the existing Pump.fun/reference system and reusable Python Solana infrastructure live below `solana/`.

**Frontend:** there is one frontend for all three domains, not one frontend per bot. Domain separation is inside `frontend/src/features/{bsc,project0,solana}`. Shared API, realtime, layout and UI code exist exactly once.

## Frontend rule

The frontend is a control plane, not another trading engine.

```text
backend/core
  -> REST snapshot
  -> sequenced WebSocket events
  -> React frontend
```

Protocol decoding, canonical state, health, economics, transaction construction, signing, submission and reconciliation remain in the backend/native code.

The frontend technical base comes from the existing `ws_ai_front` React/Vite/Tailwind setup, but its old application structure is not carried forward.

## Project entry points

- `bsc/README.md`
- `project_0/README.md`
- `project_0/docs/PHASENPLAN.md`
- `solana/README.md`
- `frontend/README.md`

## Standard workflow for external code deliveries and project exports

This workflow is mandatory for **all DarkBot projects** (`bsc/`, `project_0/`, `solana/`, `frontend/`) whenever code is exported for external implementation/review and later integrated back into `main`.

The goal is always the same:

```text
protect local evidence/data
→ synchronize the complete repository
→ export an exact committed project snapshot
→ record the source commit
→ receive an external candidate
→ review before integration
→ integrate locally
→ run the project's real gates
→ commit only accepted changes
→ re-check remote state
→ push
→ fetch and verify local HEAD == origin/main
```

### 1. Protect local work before synchronization

Start from the repository root and inspect the complete working tree:

```bash
REPO="/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot"

cd "$REPO"

git status --short
git fetch origin
git rev-parse HEAD
git rev-parse origin/main
```

Untracked captures, shadow data, measurements, generated evidence or other valuable local files must be moved or copied **outside the repository** before cleanup or synchronization.

Do **not** use `git clean` as the normal synchronization method.

Do **not** use `git reset --hard` on the root DarkBot repository as a normal integration step.

### 2. Synchronize the complete repository before exporting one project

Project work must start from a known full-repository `main`, not from an independently synchronized subdirectory:

```bash
cd "$REPO"

git pull --ff-only

echo "LOCAL:"
git rev-parse HEAD

echo "REMOTE:"
git rev-parse origin/main

git status --short
```

Proceed only when the intended source commit is known and there are no unintended local changes.

### 3. Export from the exact committed Git object, never from the working tree

For an external implementation/review package, export the required project directly with `git archive <commit>`.

Example for Project 0:

```bash
DEST="/Users/sawyer_ma/Desktop/Delta_Mac/DarkBot_Project0_Fable"
COMMIT="$(git -C "$REPO" rev-parse HEAD)"

rm -rf "$DEST"
mkdir -p "$DEST"

git -C "$REPO" archive "$COMMIT" project_0 | tar -x -C "$DEST"

cat > "$DEST/SOURCE_COMMIT.txt" <<EOF
repository=https://github.com/sawyerma/darkbot.git
commit=$COMMIT
scope=project_0
EOF
```

For another project, replace only the exported scope and destination. Include additional cross-project/root documents only when they are required by that task.

If a ZIP is handed to an external implementer, record its SHA-256 as well as the exact source commit.

### 4. External candidates are never integrated blindly

Before applying a returned ZIP/patch:

- confirm the declared source/base commit;
- confirm that only the authorized project/scope changed;
- inspect the changed-file list and diff;
- verify that unrelated projects are untouched;
- inspect for secrets, local data, generated bulk data and accidental binaries;
- verify supplied checksums when present;
- run `git apply --check` before applying a patch.

Example:

```bash
cd "$REPO"
git status --short
git fetch origin
git rev-parse HEAD
git rev-parse origin/main

git apply --check "/path/to/candidate.patch"
```

A candidate ZIP or successful patch application is **not** a PASS result.

### 5. Integrate locally and run the authoritative project gates

After integration:

```bash
git status --short
git diff --check
git diff --stat
```

Then inspect the project-specific diff and run the tests, builds, static checks, simulations, measurements and evidence gates required by that project's authoritative plan.

The rule is:

```text
implemented/applied ≠ tested
tested ≠ qualified unless the required evidence exists
qualified ≠ LIVE unless the project plan explicitly unlocks LIVE
```

Never convert `NOT_RUN`, `UNKNOWN`, `NOT_PROVEN` or a partial result into PASS.

### 6. Commit only the accepted project state

Stage only the intended scope and inspect it before committing:

```bash
git status --short
git add <authorized-project-paths>

git diff --cached --check
git diff --cached --stat
git status
```

Commit with a project/gate-specific message only after the required local acceptance gates have passed or the commit is explicitly labeled as a non-PASS checkpoint/evidence commit.

### 7. Re-check the remote immediately before push

Before pushing:

```bash
git fetch origin

echo "LOCAL HEAD:"
git rev-parse HEAD

echo "REMOTE MAIN:"
git rev-parse origin/main

git status --short
```

If `origin/main` advanced unexpectedly, stop and reconcile/review first. Do not blindly overwrite or force-push `main`.

### 8. Push and independently verify the result

Push:

```bash
git push origin main
```

Then verify again rather than relying only on the push message:

```bash
git fetch origin

echo "LOCAL:"
git rev-parse HEAD

echo "REMOTE:"
git rev-parse origin/main

git status --short
git log -3 --oneline --decorate
```

The normal successful end state is:

```text
git rev-parse HEAD == git rev-parse origin/main
```

with both `HEAD -> main` and `origin/main` on the intended accepted commit.

### 9. Scope-isolation rule

When one project is being changed, unrelated project trees must remain untouched unless the task explicitly requires a shared/root change. In particular, a BSC delivery must not silently modify Project 0, and a Project 0 delivery must not silently modify BSC.

Any intentional root/shared change must be called out separately in the review and commit.
