#!/usr/bin/env bash

set -euo pipefail

if [[ "$#" -ne 1 ]] || [[ ! -d "$1" ]]; then
    echo "usage: $0 <extracted-package-root>" >&2
    exit 2
fi

readonly package_root="$1"
script_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly script_directory
repository_root="$(cd "$script_directory/.." && pwd)"
readonly repository_root

while IFS=$'\t' read -r repository_path packaged_filename _release_asset; do
    if [[ -z "$repository_path" ]] || [[ "$repository_path" == \#* ]]; then
        continue
    fi

    match_count=0
    packaged_path=""
    while IFS= read -r -d '' candidate; do
        packaged_path="$candidate"
        match_count=$((match_count + 1))
    done < <(find "$package_root" -type f -name "$packaged_filename" -print0)

    if [[ "$match_count" -ne 1 ]]; then
        echo "expected exactly one $packaged_filename in $package_root, found $match_count" >&2
        exit 1
    fi

    if ! cmp --silent "$repository_root/$repository_path" "$packaged_path"; then
        echo "packaged $packaged_filename does not match $repository_path" >&2
        exit 1
    fi
done <"$repository_root/legal-resources.tsv"

echo "packaged legal resources are present"
