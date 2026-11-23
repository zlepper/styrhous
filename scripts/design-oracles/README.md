# Harbour design oracles

Turns the Harbour design artboards into oracle PNGs and compares egui snapshots against them
with Kineprism. Run everything inside the flake devShell (Node 22 and the Playwright Chromium
build pinned to `@playwright/test` 1.64.1):

```bash
cd scripts/design-oracles
nix develop ../.. -c npm ci                          # once
nix develop ../.. -c npm run extract -- <design-dir>  # dir containing Main.dc.html, components/, app/
nix develop ../.. -c npm run render                  # all boards, or: npm run render -- A02 C02
nix develop ../.. -c npm run audit-fonts             # font sizes/weights in use, fallback glyphs
nix develop ../.. -c npm run extract-icons           # the boards' icon set -> crates/components/assets/icons
./compare.sh [ID ...]                                # Kineprism -> target/design-oracles/
nix develop ../.. -c npm test                        # unit tests of the scripts (node --test)
```

- `extract` writes plain HTML to `docs/design-oracles/harbour/src/<ID>.html` (the `<x-dc>` wrapper,
  artifact runtime and Google Fonts link removed) plus `fonts.css` and `harbour.css` (the stylesheet
  every board shares). Hand-written calibration boards (`"calibration": true` in their
  `harbour-board` meta, currently `CAL.html`) link `harbour.css` and survive a re-extract. The new
  `src/` is built in a staging directory and swapped in only once every board converted, so a
  failed extract leaves the previous one in place.
- `extract-icons` writes the 61 `.ic-*` mask icons of `harbour.css` (Heroicons outline, stroke
  1.5) as white-stroked SVGs into `crates/components/assets/icons/`, so egui tints exactly the
  paths Chromium masks.
- `render` screenshots each board at its artboard size (deviceScaleFactor 1, font hinting off)
  into `docs/design-oracles/harbour/<ID>.png`. It fails on any non-file network request, on a
  font face that does not load, or on a Bricolage size without a static instance. Output is
  byte-for-byte reproducible (a full render takes about 5 s).
- `compare.sh` reads `docs/design-oracles/harbour/manifest.toml`, compares every board whose
  `snapshot` is set (or the given IDs, plus each `[[board.regions]]` crop), and writes
  `target/design-oracles/scoreboard.md`. A pending egui_kittest candidate
  (`<snapshot>.new.png`) is compared instead of the fixture when it exists. Kineprism defaults to
  `../better-image-diff/target/x86_64-unknown-linux-gnu/release/kineprism`, falls back to
  `npx @zlepper/kineprism@0.0.5`, and can be overridden with `KINEPRISM=/path/to/kineprism`. A
  comparison that fails (Kineprism error, missing report or image) becomes an `error` row and the
  others still run.

## Calibration board

`src/CAL.html` is a hand-written board (type roles, radii, strokes, shadows, icons) that
`crates/components/tests/calibration.rs` paints with the design APIs. `./compare.sh CAL` measures
the irreducible Chromium/egui difference, which is the pixel-match bar for every other board.
Change the HTML and the test together.

## Fonts

The boards use Figtree, Bricolage Grotesque, IBM Plex Mono and IBM Plex Sans Condensed (SIL OFL
1.1). `docs/design-oracles/harbour/fonts/` holds the exact Google Fonts sources from
[google/fonts](https://github.com/google/fonts) commit `06bfe0d62a98375cb09c942fa18e68bd32cff057`.
`instance-fonts.sh` turns them into the static files the app embeds
(`crates/components/assets/fonts/`): egui cannot apply variable axes, and Chrome optically sizes
Bricolage Grotesque (opsz = font size in px), so each Bricolage size gets its own instance. The
oracles load those same static files (`lib.mjs` reads the faces from that directory), so Chrome and
egui rasterize identical outlines. IBM Plex Mono also ships its real SemiBold for the boards'
weight-600 text (Google Fonts would only let Chrome synthesize it); IBM Plex Sans Condensed weight
400 renders with Medium, as on Google Fonts.

Glyphs the Harbour fonts lack come from a single fallback face: DejaVu Sans (Book, nixpkgs
`dejavu_fonts` 2.37 at the flake's pinned revision), subset by `instance-fonts.sh` to exactly the
characters the boards use that a primary font lacks (currently ↵ ⌘ ⌫; computed by
`fallback-codepoints.py`). It is written to `crates/components/assets/fonts/dejavu/` and
`docs/design-oracles/harbour/fonts/dejavu/` (with `LICENSE` and `unicode-range.txt`). `render` puts
it directly after the primary family in every font stack, limited to that unicode-range, and
`audit-fonts` fails if any glyph still comes from a host font. `instance-fonts.sh` fails rather than
write an empty subset, because the app embeds the face unconditionally. Re-run it after
`npm run extract`. After a design change, run `npm run audit-fonts`, add any new Bricolage size to
`instance-fonts.sh`, and re-run it.

### Sizes the egui text styles need

Besides the Bricolage instances, the boards use these non-integer sizes, which egui must support
as exact point sizes:

- Figtree 13.5 (weights 500, 600)
- IBM Plex Mono 10.5, 11.5 and 12.5 (10.5: 400/500/600; 11.5: 400/500/600; 12.5: 400)

Run `npm run audit-fonts` for the full family/size/weight list per board.
