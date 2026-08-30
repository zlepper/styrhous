#!/usr/bin/env bash
set -euo pipefail

# Keep this in sync with package.metadata.packager.appimage.libs in Cargo.toml.
# The Debian package declares the corresponding runtime dependency separately.
sudo apt-get update
sudo apt-get install --yes libxkbcommon-x11-0

script_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly script_directory
repository_root="$(cd "$script_directory/.." && pwd)"
readonly repository_root
readonly notice_directory="$repository_root/target/legal/appimage-native-licenses"

rm -rf -- "$notice_directory"
mkdir -p "$notice_directory"

while IFS=$'\t' read -r library_pattern package_name; do
    if [[ -z "$library_pattern" ]] || [[ "$library_pattern" == \#* ]]; then
        continue
    fi

    if ! dpkg-query --show "$package_name" >/dev/null 2>&1; then
        echo "expected AppImage library package $package_name to be installed" >&2
        exit 1
    fi

    copyright_file="/usr/share/doc/$package_name/copyright"
    if [[ ! -f "$copyright_file" ]]; then
        echo "missing copyright file for AppImage library package $package_name" >&2
        exit 1
    fi

    cp -L -- "$copyright_file" "$notice_directory/$package_name.txt"
done <"$repository_root/legal/appimage-native-libraries.tsv"
