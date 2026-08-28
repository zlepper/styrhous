# Crop and comparison commands

Use explicit, verified bounds for each image. The example below compares a
foreground blade that begins at different x-coordinates in the oracle and the
deterministic snapshot, while producing equal-sized crops.

```bash
mkdir -p target/oracle-crops target/visual-diffs

magick docs/design/oracles/example.png \
  -crop 744x1010+784+7 +repage \
  target/oracle-crops/example-oracle.png

magick crates/app/tests/snapshots/example.png \
  -crop 744x1010+744+7 +repage \
  target/oracle-crops/example-snapshot.png

./scripts/compare-oracle-snapshot.sh \
  target/oracle-crops/example-oracle.png \
  target/oracle-crops/example-snapshot.png \
  target/visual-diffs/example
```

The coordinates above are illustrative. Determine them from the actual image,
the UI's accessibility bounds, or an inspected screenshot. The crops must have
identical final dimensions; their source origins need not match.

Create a side-by-side review image when the visual-review helper is unavailable:

```bash
magick target/oracle-crops/example-oracle.png \
  target/oracle-crops/example-snapshot.png \
  +append target/oracle-crops/example-comparison.png
```

The comparison script emits raw ImageMagick metrics and normalized values in
parentheses. Use the normalized MAE for progress tracking:

```text
MAE: 1957.64 (0.0298717)  ->  2.99%
```

Track the state, crop bounds, normalized MAE, and the design change that caused
each material improvement. If a change worsens the score and the diff reveals
no compensating visual improvement, revert that change rather than accumulating
pixel-tuned offsets.
