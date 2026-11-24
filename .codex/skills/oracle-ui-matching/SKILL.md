---
name: oracle-ui-matching
description: Match a deterministic egui snapshot to a Harbour design oracle (a PNG rendered from the design HTML) with the scripts/design-oracles pipeline, Kineprism reports and accessibility snapshots. Use for pixel-conscious UI implementation work, not ordinary UI changes.
---

# Oracle UI Matching

Use this skill when a screen or component must match a Harbour board closely. The oracles are not
generated images: `scripts/design-oracles/` renders the design's HTML boards
(`docs/design-oracles/harbour/src/<ID>.html` plus `harbour.css`) with Chromium into
`docs/design-oracles/harbour/<ID>.png`, using the same static fonts the app embeds. The pipeline,
its commands and the font handling are described in `scripts/design-oracles/README.md`.

## The pipeline

1. **Board and test.** Every board has a `[[board]]` entry in
   `docs/design-oracles/harbour/manifest.toml`. Its `snapshot` names the PNG written by the
   board's own egui_kittest test (`crates/components/tests/c*.rs` for component boards,
   `crates/styrhous/src/ui/tests/snapshots/` for the A screens). A board without a test has
   `snapshot = ""` and is skipped. Write the test with the project's `harness.ui_harness(...)` so
   the pixel and accessibility snapshots stay together, and keep it deterministic: screen tests
   inject state and `WorkerResult`s through `MockWorker`, using the shared design dataset
   (`ui/tests/design_fixtures.rs`) and a fixed `WallClock`.
2. **Measure, don't eyeball.** Take geometry from the HTML/CSS (computed values via the
   Playwright probe in `scripts/design-oracles`), then express it with `design::{space, radius,
   size, elevation, stroke}` and `design::type_scale` roles. If the design needs a measurement
   that is missing, add it to `design::size` (or the owning component) rather than a literal in
   a screen.
3. **Compare.** `scripts/design-oracles/compare.sh [ID ...]` runs Kineprism per board (and per
   `[[board.regions]]` crop) and writes `target/design-oracles/scoreboard.md` plus, per board,
   `report.json`, `diff.png` and a `non-aa/` pass. A pending `<snapshot>.new.png` is compared in
   place of the accepted fixture, so you can iterate before accepting anything.
4. **Read the report.** Raw MAE (`metrics.raw.mae`) is the acceptance metric; lower is better and
   it is only meaningful for equal canvases. The `non-aa/` pass ignores antialiased edges: its
   "Changed (non-AA)" regions are the worklist, because they exclude the glyph and curve noise that
   Chromium and egui rasterise differently. Moved, resized, added and removed regions mean real
   layout differences; fix those first.
5. **Know the floor.** The `CAL` calibration board (`src/CAL.html`, painted by
   `crates/components/tests/calibration.rs`) is the irreducible Chromium/egui difference
   (raw MAE about 0.00047). Component boards reach 0.00026 to 0.00056. Do not chase a board
   below that bar, and do not accept a board that regresses against its earlier score.
6. **Regions.** When a board mixes finished and unfinished parts, or one part is noisy, add
   `[[board.regions]]` entries (name, x, y, width, height in board pixels) to the manifest so
   each part has its own score and an unfinished one does not hide progress in another.

Run `compare.sh` for all boards before and after a change that touches shared components or
design constants, and report any score that moved.

## Reducing mismatch efficiently

Work from shared geometry to detail:

1. **Topology:** the same controls, grouping and rows exist. A missing or detached group
   dominates the error.
2. **Vertical anchors:** header dividers, section titles, card bounds, row heights, footers.
3. **Horizontal anchors:** insets, handles, titles, input starts, action buttons. Reuse the
   anchor values across rows.
4. **Control treatment:** padding, fixed height, border, radius, fill, and the vertical centring
   of text and icons. Prefer components in `crates/components` and theme roles over per-screen
   styling; extend a component rather than painting a one-off.
5. **Text:** the type role's line box and baseline (`role.label`, `role.label_on_baseline`).

A single shared offset belongs in its layout primitive; do not compensate with per-label
spacing. If a change worsens raw MAE and the diff shows no compensating gain, revert it.

## Verification and accessibility

- Read every changed accessibility snapshot. Check that items in a row share a centre and
  height, icons and text are centred in their boxes, margins are consistent, and no same-layer
  overlap is introduced. Never accept a snapshot just to make a test pass; accept with
  `UPDATE_SNAPSHOTS=1` only after reviewing the `.new` files.
- Drive interaction with real pointer or keyboard input, never `accesskit_click`.
- A button the design shows whose action does not exist is `Button::not_implemented()`.
- Run the focused test after each iteration, then the whole `components` and `styrhous` suites
  once the snapshot is final. Leave no `*.new.*`, `*.diff.png` or `*.old.png` files behind.

## Supplementary tooling

`compare_ui_images` (Kineprism MCP tool) is the primary comparison for ad-hoc crops; see
[iteration commands](references/iteration.md). `scripts/compare-oracle-snapshot.sh` and
`scripts/review-oracle-diff.sh` add ImageMagick and Codex CLI visual review; their numbers never
replace the Kineprism score as the acceptance criterion.

## Guardrails

- Oracles are generated from the design HTML. Never edit an oracle PNG to hide a mismatch;
  change the design source and re-render (`npm run extract`, `npm run render`) only when the
  design itself changed.
- Do not make global palette or component changes to improve one crop unless the component is
  genuinely shared and every board score is rechecked.
- Preserve unrelated work in a dirty worktree and keep temporary crops and diffs under `target/`.
- Stop when the board is at the calibration bar or further changes stop improving a meaningful
  metric or visible defect. Report raw MAE, region counts, residuals, report artifacts and tests.
