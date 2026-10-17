# Frontend F1 Graphite design freeze – Implementation status

**Frozen:** The existing `features/project0/styles/graphite.css` and `features/project0/pages/WalletsPanel.tsx` are byte-identical to the reviewed branch; their SHA-256 values are in `design/reference/FREEZE_SHA256`. **Do not change those sources for F1.**

**Design system:** `shared/ui/graphite/tokens.css` defines the values used by `graphite.css` and presentation-only `GraphiteShell`, `Tabs`, `Rows`, and `Notice`. These shared components are not wired to Project_0 or to any trading backend. No status badge grants LIVE authorization. `npm run lint:css` rejects hardcoded colors in future feature/shared CSS; existing P0 CSS is explicitly frozen instead of retroactively restyled.

**Verification:** `npm run check:freeze` compares repo bytes. `npm run visual:update` generates browser snapshots in a **controlled, pinned environment** and must be separately reviewed before committing. `npm run visual` compares against **committed** snapshots with `maxDiffPixels: 0`. Until approved snapshots exist, screenshot tests should FAIL as NOT_BASELINED and F1 must not be marked VERIFIED. Use `npm run build` before `npm run visual` (visual Playwright preview reads built `dist/`). The supplied V7 screenshots are useful visual references but are not substitutes for Playwright baselines.

**Known reference mismatch:** Original approved `project0_graphite_v7_fleet.html` SHA-256: `887aac385b04ca315615844399f42ff611b10de204a5cb9e3bc7ffb2c87eef36`; current Git copy: `a1e3b27b5a3ed0a3821c75c0f1a7e9ae51f03025c16003c192a986b6e8498499`. The Git copy is frozen as actually present **only** to prevent further silent drift, not certified as the approved master. Resolve and commit the byte-exact original before final F1 visual acceptance.

**Boundary:** No changes to P0 Engine, BSC Engine, wallet authorizations or existing P0 components. Cross-feature TypeScript boundary check from F3 remains active. F1 is currently **IMPLEMENTED PARTIALLY / VISUAL NOT_RUN**, not a PASS.
