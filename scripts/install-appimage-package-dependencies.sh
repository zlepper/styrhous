#!/usr/bin/env bash
set -euo pipefail

# The Debian package declares libxkbcommon-x11-0 as a runtime dependency. The
# AppImage bundles that dynamically loaded library, while the Mesa packages
# provide a software renderer for headless package smoke tests.
sudo apt-get update
sudo apt-get install --yes libegl1 libgl1-mesa-dri libxkbcommon-x11-0

script_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly script_directory
repository_root="$(cd "$script_directory/.." && pwd)"
readonly repository_root
readonly notice_directory="$repository_root/target/legal/appimage-native-licenses"
readonly library_directory="$repository_root/target/appimage-native-libraries"

rm -rf -- "$notice_directory"
mkdir -p "$notice_directory"
rm -rf -- "$library_directory"
mkdir -p "$library_directory"

library_path="$(
    dpkg-query --listfiles libxkbcommon-x11-0 \
        | awk '/\/libxkbcommon-x11\.so\.0$/ { print; exit }'
)"
readonly library_path
if [[ -z "$library_path" ]] || [[ ! -f "$library_path" ]]; then
    echo "libxkbcommon-x11-0 did not install libxkbcommon-x11.so.0" >&2
    exit 1
fi
cp -L -- "$library_path" "$library_directory/libxkbcommon-x11.so.0"

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
