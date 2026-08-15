#!/usr/bin/env bash
# Compare the injected resource-table fixture with the approved visual oracle.
#
# Usage:
#   nix-shell -p imagemagick --run ./scripts/compare-oracle-snapshot.sh
#   ./scripts/compare-oracle-snapshot.sh [oracle.png] [snapshot.png] [output-directory]

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
oracle=${1:-"$repo_root/crates/app/integration_resource_table_oracle.png"}
snapshot=${2:-"$repo_root/crates/app/tests/snapshots/oracle_resource_table_injected.png"}
output_dir=${3:-"$repo_root/target/visual-diffs"}

if ! command -v magick >/dev/null; then
    echo "ImageMagick is required. Run: nix-shell -p imagemagick --run ./scripts/compare-oracle-snapshot.sh" >&2
    exit 127
fi

for image in "$oracle" "$snapshot"; do
    if [[ ! -f "$image" ]]; then
        echo "Missing image: $image" >&2
        exit 2
    fi
done

mkdir -p "$output_dir"

dimensions=$(magick identify -format '%wx%h' "$oracle")
snapshot_dimensions=$(magick identify -format '%wx%h' "$snapshot")
if [[ "$dimensions" != "$snapshot_dimensions" ]]; then
    echo "Image dimensions differ: oracle=$dimensions snapshot=$snapshot_dimensions" >&2
    exit 3
fi

metric() {
    magick compare -metric "$1" "$oracle" "$snapshot" null: 2>&1 || true
}

oracle_name=$(basename "$oracle" .png)
difference="$output_dir/${oracle_name}-difference.png"
amplified_difference="$output_dir/${oracle_name}-difference-autolevel.png"
magick "$oracle" "$snapshot" -compose difference -composite -colorspace gray "$difference"
magick "$difference" -auto-level "$amplified_difference"

temporary_dir=$(mktemp -d)
trap 'rm -rf "$temporary_dir"' EXIT

region_mae() {
    local name=$1 x=$2 y=$3 width=$4 height=$5
    local oracle_region="$temporary_dir/${name}-oracle.png"
    local snapshot_region="$temporary_dir/${name}-snapshot.png"
    magick "$oracle" -crop "${width}x${height}+${x}+${y}" +repage "$oracle_region"
    magick "$snapshot" -crop "${width}x${height}+${x}+${y}" +repage "$snapshot_region"
    printf '  %-13s %s\n' "$name:" "$(magick compare -metric MAE "$oracle_region" "$snapshot_region" null: 2>&1 || true)"
}

changed_pixels() {
    local threshold=$1
    magick "$oracle" "$snapshot" -compose difference -composite -colorspace gray \
        -threshold "$threshold" -format '%[fx:round(mean*w*h)]' info:
}

sample() {
    local label=$1 x=$2 y=$3
    local oracle_color snapshot_color
    oracle_color=$(magick "$oracle" -format "%[pixel:p{$x,$y}]" info:)
    snapshot_color=$(magick "$snapshot" -format "%[pixel:p{$x,$y}]" info:)
    printf '  %-13s oracle=%-20s snapshot=%s\n' "$label:" "$oracle_color" "$snapshot_color"
}

echo "oracle:   $oracle"
echo "snapshot: $snapshot"
echo "size:     $dimensions"
echo "AE:       $(metric AE)"
echo "MAE:      $(metric MAE)"
echo "RMSE:     $(metric RMSE)"
echo "pixels above 2%: $(changed_pixels 2%)"
echo "pixels above 5%: $(changed_pixels 5%)"
echo "regions (MAE):"
if [[ "$oracle" == *"inspector_details"* ]]; then
    region_mae properties 24 96 694 800
    region_mae detail-tables 744 96 744 800
    echo "surface samples:"
    sample canvas 20 20
    sample properties 100 200
    sample detail-tables 900 200
else
    region_mae rail 0 0 68 1024
    region_mae navigation 68 0 292 1024
    region_mae toolbar 360 0 1176 102
    region_mae table-header 360 102 1176 72
    region_mae table-body 360 174 1176 670
    echo "surface samples:"
    sample rail 34 300
    sample navigation-left 300 500
    sample navigation-right 350 500
    sample navigation-bottom 340 900
    sample toolbar 700 20
    sample table-header 500 130
    sample table-body 800 900
fi
echo "difference: $difference"
echo "amplified difference: $amplified_difference"
