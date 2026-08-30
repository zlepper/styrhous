#!/usr/bin/env bash

set -euo pipefail

if [[ "$#" -ne 1 ]] || [[ ! -d "$1" ]]; then
    echo "usage: $0 <extracted-appimage-root>" >&2
    exit 2
fi

readonly appimage_root="$1"
script_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly script_directory
repository_root="$(cd "$script_directory/.." && pwd)"
readonly repository_root
readonly allowlist="$repository_root/appimage-native-libraries.tsv"
readonly generated_notices="$repository_root/target/legal/appimage-native-licenses"

library_count=0
while IFS= read -r -d '' library_path; do
    library_name="${library_path##*/}"
    library_count=$((library_count + 1))
    matched_package=""

    while IFS=$'\t' read -r library_pattern package_name; do
        if [[ -z "$library_pattern" ]] || [[ "$library_pattern" == \#* ]]; then
            continue
        fi

        # The manifest entries are reviewed shell globs, not literal strings.
        # shellcheck disable=SC2254
        case "$library_name" in
            $library_pattern)
                matched_package="$package_name"
                break
                ;;
        esac
    done <"$allowlist"

    if [[ -z "$matched_package" ]]; then
        echo "unreviewed native library in AppImage: $library_name" >&2
        exit 1
    fi

    packaged_notice_count="$(find "$appimage_root" -type f -path "*/APPIMAGE_NATIVE_LICENSES/$matched_package.txt" -print | wc -l)"
    if [[ "$packaged_notice_count" -ne 1 ]]; then
        echo "expected one packaged copyright notice for $library_name ($matched_package)" >&2
        exit 1
    fi

    packaged_notice="$(find "$appimage_root" -type f -path "*/APPIMAGE_NATIVE_LICENSES/$matched_package.txt" -print -quit)"
    if ! cmp --silent "$generated_notices/$matched_package.txt" "$packaged_notice"; then
        echo "packaged copyright notice for $matched_package is stale" >&2
        exit 1
    fi
done < <(find "$appimage_root" \( -type f -o -type l \) -name '*.so*' -print0)

if [[ "$library_count" -eq 0 ]]; then
    echo "expected the AppImage to bundle at least one native shared library" >&2
    exit 1
fi

echo "AppImage native libraries and copyright notices are reviewed"
