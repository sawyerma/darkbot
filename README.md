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

**Verbindliche gemeinsame Project_0/BSC-Backend↔Frontend-Regeln:** [frontend/README.md](frontend/README.md). Beide Projekte benutzen dieselbe F0-VAR-Inventurmethodik und **denselben Gateway-/Protobuf-Datenfluss**, aber getrennte native Fach-Owner und projektgebundene öffentliche Projektionen.

```text
Native Project_0 / BSC Source
  -> projektspezifische öffentliche Projektion + Protobuf
  -> nativer Reporter / Engine-Feed -> gemeinsames Gateway / Topic-WS
  -> projektspezifischer Adapter / Store
  -> fünf vorhandene Frontend-Reiter pro Projekt
```

Commands gehen nur über autorisierte Gateway-/Native-Controller; der Browser ist weder Trading-Engine noch BUY-Hot-Path. Feldzuordnung, Source-Census, Statusklassen, Safety und Übergabe je Backend-Paket sind in [frontend/README.md](frontend/README.md) festgelegt.

## Project entry points

- `bsc/README.md`
- `project_0/README.md`
- `project_0/docs/PHASENPLAN.md`
- `solana/README.md`
- `frontend/README.md`

## Fable ZIP fast path — default procedure

When the user asks for a Fable handoff ZIP, **do not turn packaging into a new qualification project**.

The default goal is simple:

```text
current committed source
+ the task-specific work order
+ already-existing build/test evidence for that same source, when available
+ source identity + checksums
→ ZIP
```

### Mandatory assistant behavior

1. **Use `origin/main` as the source commit. Never copy the working tree.**
2. **Do not touch or clean unrelated untracked/local work.**
3. **Do not rerun compilation/tests merely because a ZIP is being created** when the exact project source at `origin/main` already has a successful current baseline recorded.
4. If the relevant project source changed since the last successful baseline, run only the **minimum authoritative build/test commands needed for that changed scope**.
5. `cargo fmt --check`, compiler warnings, dead-code warnings, cosmetic formatting differences, or other already-known non-fatal baseline warnings are **documentation only** and must **not block packaging** unless the project plan explicitly defines them as a release gate.
6. A stale `Cargo.lock` is fixed once in an isolated worktree; commit only the intended lockfile change, then continue packaging. Do not create repeated packaging/preflight loops.
7. Revisions in unrelated project trees (for example `project_0/**` while packaging BSC) must not force a new BSC review. Compare only the exported/task-relevant scope.
8. The final ZIP must contain:
   - exact committed source for the requested scope;
   - `SOURCE_COMMIT.txt`;
   - the task-specific Fable work order;
   - `BASELINE_BUILD_STATUS.md` when baseline evidence exists;
   - `SHA256SUMS.txt`;
   - no `.git`, Cargo `target/`, caches, captures, secrets, or unrelated projects.
9. Final ZIP size limit remains **80 MiB** unless explicitly overridden.
10. Once the ZIP passes the structural checks, **stop**. Do not invent another review cycle before handing it to the user.

### 10-second packaging path

For a project whose current committed source is already qualified enough for handoff:

```bash
REPO="/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot"
OUT="/Users/sawyer_ma/Desktop/Delta_Mac"
SCOPE="bsc"
BASE="$(git -C "$REPO" rev-parse origin/main)"
NAME="DarkBot_${SCOPE}_FABLE_${BASE:0:7}"
WORK="$OUT/$NAME"
ZIP="$OUT/$NAME.zip"

rm -rf "$WORK" "$ZIP"
mkdir -p "$WORK/repo"

git -C "$REPO" archive "$BASE" "$SCOPE" README.md | tar -x -C "$WORK/repo"

cat > "$WORK/SOURCE_COMMIT.txt" <<EOF
repository=https://github.com/sawyerma/darkbot.git
commit=$BASE
scope=$SCOPE
EOF

# Copy the prepared task-specific work order into:
#   $WORK/FABLE_WORK_ORDER.md
# Reuse existing current baseline evidence when it is already valid for BASE.

cd "$WORK"
find . -type f ! -name SHA256SUMS.txt -print0 | sort -z | xargs -0 shasum -a 256 > SHA256SUMS.txt

cd "$OUT"
zip -qry -9 "$ZIP" "$NAME"

test "$(stat -f%z "$ZIP")" -le 83886080
shasum -a 256 "$ZIP"

echo "FABLE_ZIP=$ZIP"
```

If the task needs additional files outside the selected scope, add only those explicitly required by the work order. **Do not expand the package to unrelated projects.**

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

### 3a. Preferred Fable handoff: compiled baseline bundle

For substantial Fable implementation work, especially native Go/Rust work, prefer a **slim compiled baseline bundle** over a source-only ZIP.

**Hard package limit:** the final Fable ZIP must not exceed **80 MiB (83,886,080 bytes)** unless a task explicitly documents and approves an exception before packaging.

Reason: Fable may not have the required Go/Rust toolchains or network access. A source-only archive is still the authoritative source snapshot, but it does not tell Fable which exact baseline was locally proven to compile and pass the current gates. The compiled baseline bundle adds that evidence without weakening the rule that Fable's later modifications must be recompiled and requalified locally.

Keep the compiled bundle small by default:

- export only the project/task scope actually needed by Fable, plus required root/normative documents;
- do not include unrelated projects merely because they share the monorepo;
- never package Cargo `target/`, Go build caches or other build caches;
- do not package a complete second copy of a large pinned external repository when selected pinned source context plus exact pin identity is sufficient;
- do not package large host-specific binaries merely to prove they were built: record their SHA-256 and retain the build/test logs instead;
- include full binaries only when Fable actually needs to inspect or execute them and the 80 MiB cap still holds;
- retain exact build/test logs, toolchain identity, source hashes and package checksums.

The rule is:

```text
exact committed DarkBot base
+ exact pinned external dependency source
+ disposable real Git checkout when an integration tool requires .git
+ locally applied accepted overlay
+ locally executed authoritative baseline build/tests
+ build/test logs
+ toolchain identity
+ useful baseline binaries
+ package manifest/checksums
→ remove all .git metadata
→ ZIP
→ record ZIP SHA-256
```

Never build the Fable package by copying the current working tree. Local uncommitted work, unrelated project changes, captures and untracked evidence must not leak into the package.

#### Mandatory package identity

Every compiled Fable baseline must record:

- exact DarkBot base commit;
- exact pin/commit of any external repository used by the build;
- exact source/archive name;
- local OS/architecture;
- Go/Rust/toolchain versions used;
- exact build/test commands actually executed;
- exact results, including failures if any;
- SHA-256 of the final ZIP;
- SHA-256 manifest for the files inside the package where practical.

The final package itself should normally contain **no `.git` directory**.

If a repository integration script requires Git metadata to verify a pin, use a disposable real checkout for the build step, then remove its `.git` directory before packaging. Do not bypass the script's pin verification merely to make an archive tree work.

#### Recommended package layout

```text
<bundle>/
  repo/                       # exact DarkBot git archive of BASE
  upstream_<dependency>/      # pristine exact pinned dependency source
  build/
    <dependency>-overlayed/   # accepted overlay applied for baseline build
    bin/                      # useful locally built baseline binaries
    logs/                     # baseline build/test logs
    cargo-metadata.json       # when relevant
  BASELINE_BUILD_STATUS.md
  SHA256SUMS.txt
```

Compiled binaries are supporting baseline evidence, not a substitute for source or logs. They may be host-specific and may not run in Fable's environment. Under the default 80 MiB limit, prefer recording the binary SHA-256 in `BASELINE_BUILD_STATUS.md` and omitting the binary itself.

#### BSC/Fable example

For BSC native work, the pattern is:

```bash
REPO="/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot"
OUT="/Users/sawyer_ma/Desktop/Delta_Mac"

BASE="$(git -C "$REPO" rev-parse origin/main)"
GETH_PIN="<authoritative pinned bsc-geth commit>"
SOURCE_GETH="$REPO/bsc/vendor/bsc-geth"

NAME="DarkBot_<scope>_FULL_COMPILED_BASE_${BASE:0:7}"
WORK="$OUT/$NAME"
ZIP="$OUT/$NAME.zip"

git -C "$REPO" fetch origin

test "$(git -C "$REPO" rev-parse origin/main)" = "$BASE"

rm -rf "$WORK"
rm -f "$ZIP"

mkdir -p \
  "$WORK/repo" \
  "$WORK/upstream_bsc_geth" \
  "$WORK/build/logs" \
  "$WORK/build/bin"

git -C "$REPO" archive "$BASE" | tar -x -C "$WORK/repo"

test "$(git -C "$SOURCE_GETH" rev-parse "$GETH_PIN")" = "$GETH_PIN"

git -C "$SOURCE_GETH" archive "$GETH_PIN" | \
  tar -x -C "$WORK/upstream_bsc_geth"

git clone --shared --no-checkout \
  "$SOURCE_GETH" \
  "$WORK/build/bsc-geth-overlayed"

git -C "$WORK/build/bsc-geth-overlayed" \
  checkout --detach "$GETH_PIN"

test "$(
  git -C "$WORK/build/bsc-geth-overlayed" rev-parse HEAD
)" = "$GETH_PIN"

python3 \
  "$WORK/repo/bsc/native/tools/c1_apply_geth_overlay.py" \
  "$WORK/build/bsc-geth-overlayed" \
  | tee "$WORK/build/logs/geth-overlay.log"
```

Then run the **actual authoritative baseline gates for the current phase**. Do not mechanically reuse an old test list when the phase has changed.

For the C1 native baseline used for the C1.5-C1.10 Fable handoff, this included the relevant pattern:

```bash
export PATH="$HOME/sdk/go1.25.0/bin:$PATH"

sw_vers
uname -a
uname -m
go version
rustc --version
cargo --version

cd "$WORK/repo/bsc/native"

cargo metadata --format-version 1 --locked --offline \
  > "$WORK/build/cargo-metadata.json"

cargo fmt --all -- --check

cargo test \
  -p darkbot-fastlane \
  --locked \
  --offline

cargo test \
  -p darkbot-fastlane \
  --release \
  --locked \
  --offline

cargo clippy \
  -p darkbot-fastlane \
  --all-targets \
  --locked \
  --offline \
  -- \
  -D warnings

cd "$WORK/build/bsc-geth-overlayed"

go test -race ./darkbot/fastlane
go test ./eth/protocols/eth ./eth
go test ./core
go build -o "$WORK/build/bin/geth_macos_arm64" ./cmd/geth
```

Capture the complete output of these commands under `build/logs/`. Add any phase-specific Rust binaries or tools that were actually built and useful for the handoff.

Before producing the final ZIP:

```bash
rm -rf "$WORK/build/bsc-geth-overlayed/.git"

test ! -e "$WORK/build/bsc-geth-overlayed/.git"

cd "$WORK"

find . \
  -type f \
  ! -name SHA256SUMS.txt \
  -print0 \
  | sort -z \
  | xargs -0 shasum -a 256 \
  > SHA256SUMS.txt

cd "$OUT"

zip -qry -9 "$ZIP" "$NAME"

shasum -a 256 "$ZIP"
ls -lh "$ZIP"

BYTES="$(stat -f%z "$ZIP")"
MAX_BYTES=83886080

test "$BYTES" -le "$MAX_BYTES" || {
  echo "FABLE_PACKAGE_TOO_LARGE bytes=$BYTES max=$MAX_BYTES" >&2
  exit 1
}

echo "FABLE_PACKAGE_SIZE_PASS bytes=$BYTES max=$MAX_BYTES"
```

The package must include a short `BASELINE_BUILD_STATUS.md` stating that the bundled build/test results prove only the supplied **baseline**, not Fable's subsequent changes.

If Fable cannot compile its returned candidate, its report must still say the exact truth, for example:

```text
IMPLEMENTED / NOT COMPILED
```

The locally compiled baseline does **not** authorize Fable to claim its modified candidate compiles. After Fable returns a ZIP/patch, integrate and qualify it locally under the normal gate-by-gate workflow below.

For future Fable work, this compiled-baseline procedure is the default whenever the task materially depends on native compilation, pinned external repositories or exact build/test compatibility.

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
