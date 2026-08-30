#!/usr/bin/env bash

set -euo pipefail

if [[ "$#" -ne 1 ]] || [[ ! -d "$1" ]]; then
    echo "usage: $0 <release-assets-directory>" >&2
    exit 2
fi

script_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly script_directory
repository_root="$(cd "$script_directory/.." && pwd)"
readonly repository_root
readonly destination_directory="$1"

while IFS=$'\t' read -r repository_path packaged_filename release_asset; do
    if [[ -z "$repository_path" ]] || [[ "$repository_path" == \#* ]] ||
        [[ "$release_asset" != "yes" ]]; then
        continue
    fi

    cp -- "$repository_root/$repository_path" "$destination_directory/$packaged_filename"
    printf '%s\0' "$destination_directory/$packaged_filename"
done <"$repository_root/legal/resources.tsv"
