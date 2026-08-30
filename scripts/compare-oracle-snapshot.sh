#!/usr/bin/env bash
# Compare a deterministic UI snapshot with an approved visual oracle.
#
# Usage:
#   ./scripts/compare-oracle-snapshot.sh <oracle.png> <snapshot.png> [output-directory]

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

if [[ "$#" -lt 2 ]] || [[ "$#" -gt 3 ]]; then
    echo "usage: $0 <oracle.png> <snapshot.png> [output-directory]" >&2
    exit 2
fi

oracle=$1
snapshot=$2
output_dir=${3:-"$repo_root/target/visual-diffs"}

if ! command -v magick >/dev/null; then
    echo "ImageMagick is required. Run this command inside: nix-shell -p imagemagick" >&2
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

changed_pixels() {
    local threshold=$1
    magick "$oracle" "$snapshot" -compose difference -composite -colorspace gray \
        -threshold "$threshold" -format '%[fx:round(mean*w*h)]' info:
}

echo "oracle:   $oracle"
echo "snapshot: $snapshot"
echo "size:     $dimensions"
echo "AE:       $(metric AE)"
echo "MAE:      $(metric MAE)"
echo "RMSE:     $(metric RMSE)"
echo "pixels above 2%: $(changed_pixels 2%)"
echo "pixels above 5%: $(changed_pixels 5%)"
echo "difference: $difference"
echo "amplified difference: $amplified_difference"
