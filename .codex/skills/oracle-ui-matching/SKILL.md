---
name: oracle-ui-matching
description: Match a deterministic UI snapshot to an approved raster design oracle using image generation, ImageMagick crops and metrics, visual diffs, and accessibility snapshots. Use for pixel-conscious UI implementation work, not ordinary UI changes.
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

1. Identify the foreground comparison region in *both* images. Use the actual
   blade/content bounds for each image; generated and deterministic images may
   have different absolute origins even when their canvases are identical.
2. Crop each source independently with ImageMagick, preserving the same final
   size and resetting page geometry. See [iteration commands](references/iteration.md).
3. Run the repository's oracle comparison script against the two crops. Use its
   difference and auto-level difference images to locate remaining mismatches.
4. Correct the largest repeated mismatch, regenerate the targeted snapshot,
   crop again, and compare again. Keep a before/after metric record.
5. Use the repository's visual-review helper when it works in the environment;
   otherwise inspect an oracle/snapshot side-by-side montage and the amplified
   diff manually.

Do not compare a crop from one image against the same absolute rectangle in the
other unless the corresponding blade bounds have been verified. Misaligned crop
origins produce misleadingly high error scores and hide the actual UI issue.

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

Use the amplified difference image to distinguish a uniform offset (duplicated
edges/text throughout a region) from a local styling mismatch. A single shared
offset should be fixed at its layout primitive. Do not compensate for it with
one-off per-label spacing.

ImageMagick's MAE commonly includes a 16-bit raw value and a normalized value
in parentheses. Compare normalized values (multiply by 100 for a percentage)
and direction of change between equivalent crops. Different rasterizers,
antialiasing, or a generated oracle can prevent literal zero; use the score
alongside visual review rather than optimizing raw MAE in isolation.

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
  longer improve a meaningful metric or visual defect. Report the final crop,
  normalized metric, and tests run.
