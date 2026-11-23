#!/usr/bin/env bash
# Regenerates the static Harbour font files the egui app embeds from the exact Google Fonts sources
# kept in docs/design-oracles/harbour/fonts/ (google/fonts commit: see README.md).
#
# egui cannot apply variable-font axes, so the variable sources are instanced with fontTools:
# - Figtree at every weight the boards request from Google Fonts.
#   Each weight also gets a "Figtree Tabular" copy whose digits are the `tnum` forms
#   (tabular-figures.py), for the boards' `font-variant-numeric: tabular-nums` text.
# - Bricolage Grotesque at each (opsz, wght) pair the boards render. Chrome applies
#   `font-optical-sizing: auto`, i.e. opsz = font-size in CSS px, so every display size needs its
#   own instance. Re-run `npm run audit-fonts` after design changes and update BRICOLAGE_BOLD_OPSZ.
# - IBM Plex Mono / IBM Plex Sans Condensed are static upstream and copied unchanged (Plex Mono
#   includes SemiBold for the boards' weight-600 text, which Google Fonts would only synthesize).
# - DejaVu Sans (Book, from the pinned nixpkgs dejavu_fonts) is subset to the characters the
#   boards use that a primary Harbour font lacks (fallback-codepoints.py). It is the single fallback
#   face, written to both the app and the oracle fonts, with the codepoints in unicode-range.txt.
#   Re-run after `npm run extract` so the subset follows the boards' text. An empty subset is an
#   error: the app embeds the fallback face unconditionally.
#
# Usage: scripts/design-oracles/instance-fonts.sh
# fontTools comes from the flake's pinned nixpkgs, so the output is byte-for-byte reproducible.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
src="$repo_root/docs/design-oracles/harbour/fonts"
boards="$repo_root/docs/design-oracles/harbour/src"
out="$repo_root/crates/components/assets/fonts"

# Sizes (CSS px) at which the boards render Bricolage Grotesque; all of them use weight 700.
BRICOLAGE_BOLD_OPSZ=(13 15 16 18 20 26 28)
# Figtree weights requested from Google Fonts: name -> wght.
FIGTREE_WEIGHTS=(Regular:400 Medium:500 SemiBold:600 Bold:700)

# The nixpkgs input flake.lock pins, found through the root node's inputs (not by position in the
# file), without copying the repository into the store.
nixpkgs="$(HARBOUR_FLAKE_LOCK="$repo_root/flake.lock" nix eval --impure --raw --expr '
  let
    lock = builtins.fromJSON (builtins.readFile (builtins.getEnv "HARBOUR_FLAKE_LOCK"));
    locked = lock.nodes.${lock.nodes.${lock.root}.inputs.nixpkgs}.locked;
  in
  if locked.type == "github" then "github:${locked.owner}/${locked.repo}/${locked.rev}"
  else throw "flake.lock pins nixpkgs as a ${locked.type} input; instance-fonts.sh expects github"
')"

# Always re-run inside the pinned fontTools, whatever the host has, so the output is reproducible.
if [[ -z "${HARBOUR_FONTTOOLS_SHELL:-}" ]]; then
  HARBOUR_FONTTOOLS_SHELL=1 exec nix shell --impure --expr \
    "(builtins.getFlake \"${nixpkgs}\").legacyPackages.\${builtins.currentSystem}.python3.withPackages (p: [ p.fonttools ])" \
    --command bash "${BASH_SOURCE[0]}" "$@"
fi
python3 -c 'import fontTools' && command -v pyftsubset >/dev/null

# fontTools stamps head.modified with the current time unless SOURCE_DATE_EPOCH is set.
export SOURCE_DATE_EPOCH=0

# Bricolage has no STAT axis values for arbitrary opsz, so only Figtree gets --update-name-table.
instance() {
  local source="$1" output="$2"
  shift 2
  fonttools varLib.instancer --quiet "$source" "$@" --output "$output"
}

# The fallback face is only replaced once its new subset is known (below), so a failure never
# leaves the app without the file fonts.rs embeds.
primary_families=(figtree bricolagegrotesque ibmplexmono ibmplexsanscondensed)
for family in "${primary_families[@]}" dejavu; do
  mkdir -p "$out/$family"
done
for family in "${primary_families[@]}"; do
  rm -f "$out/$family"/*.ttf
done

for entry in "${FIGTREE_WEIGHTS[@]}"; do
  instance "$src/figtree/Figtree[wght].ttf" "$out/figtree/Figtree-${entry%%:*}.ttf" \
    "wght=${entry##*:}" --update-name-table
  python3 -I "$script_dir/tabular-figures.py" "$out/figtree/Figtree-${entry%%:*}.ttf" \
    "$out/figtree/FigtreeTabular-${entry%%:*}.ttf"
done

for opsz in "${BRICOLAGE_BOLD_OPSZ[@]}"; do
  instance "$src/bricolagegrotesque/BricolageGrotesque[opsz,wdth,wght].ttf" \
    "$out/bricolagegrotesque/BricolageGrotesque-Bold-opsz${opsz}.ttf" \
    "opsz=${opsz}" wdth=100 wght=700
done

cp "$src"/ibmplexmono/IBMPlexMono-{Regular,Medium,SemiBold}.ttf "$out/ibmplexmono/"
cp "$src"/ibmplexsanscondensed/IBMPlexSansCondensed-{Medium,SemiBold}.ttf "$out/ibmplexsanscondensed/"
for family in "${primary_families[@]}"; do
  cp "$src/$family/OFL.txt" "$out/$family/OFL.txt"
done

dejavu_sans="$(nix build --no-link --print-out-paths "$nixpkgs#dejavu_fonts")/share/fonts/truetype/DejaVuSans.ttf"
unicodes="$(python3 -I "$script_dir/fallback-codepoints.py" "$boards" "$dejavu_sans" \
  "$out"/figtree/*.ttf "$out"/bricolagegrotesque/*.ttf "$out"/ibmplexmono/*.ttf \
  "$out"/ibmplexsanscondensed/*.ttf)"
fallback="DejaVuSans-HarbourFallback.ttf"
if [[ -z "$unicodes" ]]; then
  echo "error: the boards use no character the primary fonts lack, so there is no fallback subset." >&2
  echo "The app embeds $fallback unconditionally (crates/components/src/design/fonts.rs);" >&2
  echo "drop the fallback face there before regenerating the fonts without it." >&2
  exit 1
fi
pyftsubset "$dejavu_sans" --unicodes="$unicodes" --no-hinting --output-file="$out/dejavu/$fallback"
cp "$out/dejavu/$fallback" "$src/dejavu/$fallback"
printf '%s\n' "$unicodes" >"$src/dejavu/unicode-range.txt"
cp "$src/dejavu/LICENSE" "$out/dejavu/LICENSE"
echo "fallback unicode-range: $unicodes"

(cd "$out" && ls -l "${primary_families[@]}" dejavu)
