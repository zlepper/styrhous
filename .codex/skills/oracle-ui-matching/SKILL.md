---
name: oracle-ui-matching
description: Match a deterministic UI snapshot to an approved raster design oracle using image generation, Kineprism comparison reports and artifacts, and accessibility snapshots. Use for pixel-conscious UI implementation work, not ordinary UI changes.
---

# Oracle UI Matching

Use this skill when a UI must closely match an approved raster oracle. It is
particularly suited to a design oracle derived from an existing deterministic
snapshot with the `imagegen` skill, where full-screen comparison would include
irrelevant application layers.

## Establish the oracle

1. Start from the closest existing deterministic snapshot. Inspect it before
   editing with `view_image` so its canvas size, background, blade placement,
   typography, and visible surrounding layers are preserved.
2. If a new design is needed, use the `imagegen` skill to edit that snapshot.
   Give exact visible strings and list invariants. Change one design concern at
   a time. Do not generate a replacement oracle merely because the code is
   currently mismatched.
3. Get approval for generated variants before treating them as an oracle. Copy
   approved artifacts into the project using a non-destructive name, unless the
   user explicitly authorizes replacement.

## Measurement loop

Keep the UI test deterministic, with a snapshot for every relevant state—for
example, the normal blade and its expanded editor. Regenerate a snapshot only
after reaching a consistent implementation state, then inspect both its pixel
and accessibility output before accepting it.

For each state:

1. Select equivalent UI content. Compare the complete screenshots when their
   surrounding layers are relevant. For one common rectangle, use the
   `region: { x, y, width, height }` parameter of `compare_ui_images`.
2. When equivalent content begins at different origins, crop each source
   independently to the same final dimensions, reset page geometry, and pass
   those crops to `compare_ui_images`. See [iteration commands](references/iteration.md).
3. Call `compare_ui_images` with absolute `expected_path` and `actual_path`
   PNG paths and an ignored `target/visual-diffs/<state>` `output_dir`. Set
   `force: true` on repeated iterations for the same state.
4. Inspect `report.json` and all generated artifacts: annotated `expected.png`,
   `actual.png`, and `diff.png`. Use reported changed, moved, added, removed,
   and resized regions to locate the largest repeated mismatch.
5. Correct that mismatch, regenerate the targeted snapshot, and compare again.
   Keep a before/after metric record and visually inspect the report artifacts.

Use `region` only for a rectangle that represents the same UI content in both
sources; otherwise crop the sources independently first. See [iteration
commands](references/iteration.md) for the input-selection examples.

## How to reduce mismatch efficiently

Work from shared geometry to detail. A useful order is:

1. **Topology:** ensure the same controls, grouping, editor placement, and
   fallback rows exist. A missing or detached group dominates pixel error.
2. **Vertical anchors:** align the header divider, section titles, card bounds,
   row heights, dividers, preview, and footer. Change shared spacing or row
   geometry, not each child independently.
3. **Horizontal anchors:** align the card inset, drag handle, main title,
   chip/input start, and action buttons. Reuse those anchor values across rows.
4. **Control treatment:** tune padding, fixed control height, border, radius,
   fill, and the vertical centering of text/icons. Prefer established design
   components and semantic tokens over per-screen colors.
5. **State-specific layout:** an expanded row can require a separate rendering
   path when a fixed-height reorder table cannot contain it. Keep controls
   interactive and preserve drag behavior in the non-expanded state.

Use Kineprism's report artifacts and findings to distinguish a uniform offset
from a local styling mismatch. A single shared offset should be fixed at its
layout primitive. Do not compensate for it with one-off per-label spacing.

Treat `metrics.raw.mae` as the primary fidelity target for coordinate-aligned
inputs: lower is better. Zero is exact only when both coverage fields are 1.0
and the canvases match; otherwise unmatched image area may be omitted from the
score. Record it with the state and bounds, and compare the same metric over
equivalent inputs only. For independent crops, record both source rectangles
and their equal final dimensions. When Kineprism finds a global translation,
record `metrics.global_aligned.mae` as diagnostic context; it explains
displacement but must not replace the raw MAE target. Likewise, use any
structural-alignment metric to understand residual styling differences, not to
hide a layout regression. Different rasterizers, antialiasing, or a generated
oracle can prevent literal zero, so use MAE alongside artifact and
accessibility review rather than optimizing it in isolation.

## Verification and accessibility

- Use the project's snapshot harness so pixel and accessibility snapshots are
  checked together. Never accept a snapshot just to make a test pass.
- Read every changed accessibility snapshot. Check that grouped controls share
  expected centers/heights, text is vertically centered in inputs, buttons have
  adequate hit targets, and no meaningful same-layer overlaps are introduced.
- Preserve behavior while matching pixels: labels for custom controls should be
  clickable, reordering must not leave focus on the wrong row, and an expanded
  state must not silently disable unrelated actions without an intentional
  design decision.
- Run the focused snapshot test after each substantive visual iteration, then
  run the relevant broader suite once the final snapshot is accepted.

## Guardrails

- Keep generated oracles and deterministic snapshots distinct. Do not edit an
  approved oracle to conceal an implementation mismatch.
- Do not make global palette or component changes solely to improve one crop
  unless the component is genuinely shared and the broader snapshot suite is
  reviewed.
- Preserve unrelated work in a dirty worktree. Put temporary crops and diffs
  under an ignored build/output directory when possible.
- Stop when the requested visual fidelity is reached or further changes no
  longer improve a meaningful metric or visual defect. Report the final scope,
  raw MAE, relevant aligned metric (if any), report artifacts, and tests run.
