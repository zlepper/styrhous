# Kineprism comparison workflow

Use `compare_ui_images` for every oracle comparison. The expected and actual
input paths must be absolute PNG paths under an MCP workspace root; `output_dir`
must be an absolute directory path under the ignored `target/visual-diffs/` tree.

## Complete screenshot or common sub-region

For full screenshots, omit `region`. When the same rectangle represents the
same UI content in both images, pass that rectangle directly to the tool:

```json
{
  "expected_path": "/home/rasmus/projects/kubernetes-dev-ui/target/oracles/example.png",
  "actual_path": "/home/rasmus/projects/kubernetes-dev-ui/crates/styrhous/tests/snapshots/example.png",
  "output_dir": "/home/rasmus/projects/kubernetes-dev-ui/target/visual-diffs/example",
  "region": { "x": 784, "y": 8, "width": 744, "height": 1010 },
  "force": true
}
```

`region` is applied at the same coordinates in both inputs. Do not use it when
the corresponding foreground content has different source origins.

## Equivalent crops with different source origins

The example below compares a foreground blade that begins at different
x-coordinates in the oracle and deterministic snapshot. Crop each source to an
equal-sized image first, then call `compare_ui_images` on the crops.

```bash
mkdir -p target/oracles target/oracle-crops target/visual-diffs

magick target/oracles/example.png \
  -crop 744x1010+784+7 +repage \
  target/oracle-crops/example-oracle.png

magick crates/styrhous/tests/snapshots/example.png \
  -crop 744x1010+744+7 +repage \
  target/oracle-crops/example-snapshot.png
```

```json
{
  "expected_path": "/home/rasmus/projects/kubernetes-dev-ui/target/oracle-crops/example-oracle.png",
  "actual_path": "/home/rasmus/projects/kubernetes-dev-ui/target/oracle-crops/example-snapshot.png",
  "output_dir": "/home/rasmus/projects/kubernetes-dev-ui/target/visual-diffs/example",
  "force": true
}
```

The coordinates above are illustrative. Determine them from the actual image,
the UI's accessibility bounds, or an inspected screenshot. The crops must have
identical final dimensions; their source origins need not match.

For every comparison, inspect the output directory's `report.json`, annotated
`expected.png`, `actual.png`, and `diff.png`. Kineprism reports raw, globally
aligned, and structural-alignment metrics when applicable, in addition to its
classified difference regions.

Track `metrics.raw.mae` as the primary score for equivalent inputs. It is a
normalized value where lower is better. Treat zero as exact only when
`expected_coverage` and `actual_coverage` are both 1.0 and the canvases match.
If global alignment is reported, record `metrics.global_aligned.mae` as context,
but do not compare it against a raw MAE from another iteration or use it to
accept a position regression.

Track the state, `metrics.raw.mae`, aligned MAE when present, and the design
change that caused each material improvement. For a shared region, record its
bounds; for independent crops, record the expected source rectangle, actual
source rectangle, and equal final crop dimensions. If a change worsens the raw
MAE and the report artifacts reveal no compensating visual improvement, revert
that change rather than accumulating pixel-tuned offsets.
