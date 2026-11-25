{
  lib,
  copyDesktopItems,
  kubectl,
  librsvg,
  makeDesktopItem,
  makeWrapper,
  runtimeLibraries,
  rustPlatform,
  hostedLicenseOrigin ? null,
}:

rustPlatform.buildRustPackage {
  pname = "styrhous";
  version = (builtins.fromTOML (builtins.readFile ../crates/styrhous/Cargo.toml)).package.version;

  src = ../.;
  cargoLock.lockFile = ../Cargo.lock;
  cargoBuildFlags = [
    "-p"
    "styrhous"
    "--bin"
    "styrhous"
  ];
  buildNoDefaultFeatures = true;

  # The workspace suite includes UI snapshots and Kind-backed integration tests.
  doCheck = false;

  nativeBuildInputs = [
    copyDesktopItems
    librsvg
    makeWrapper
  ];
  buildInputs = runtimeLibraries;

  env = lib.optionalAttrs (hostedLicenseOrigin != null) {
    STYRHOUS_HOSTED_LICENSE_ORIGIN = hostedLicenseOrigin;
  };

  # GitHub's flake archive can contain the Git LFS pointer instead of this PNG.
  postPatch = ''
    rsvg-convert -w 512 -h 512 assets/icons/kubernetes-dev-ui.svg \
      -o assets/icons/kubernetes-dev-ui.png

    # The fonts are compiled in with include_bytes!, and egui panics at startup on a pointer.
    if lfs_pointers=$(grep -rl --include='*.ttf' '^version https://git-lfs' crates/components/assets/fonts); then
      echo "Git LFS pointers instead of fonts:" >&2
      echo "$lfs_pointers" >&2
      exit 1
    fi
  '';

  desktopItems = [
    (makeDesktopItem {
      name = "styrhous";
      desktopName = "Styrhous";
      comment = "Styrhous — the Kubernetes wheelhouse";
      exec = "styrhous";
      icon = "styrhous";
      categories = [
        "Development"
        "Utility"
      ];
      terminal = false;
    })
  ];

  postInstall = ''
    install -Dm644 assets/icons/kubernetes-dev-ui.png \
      "$out/share/icons/hicolor/512x512/apps/styrhous.png"
    install -Dm644 LICENSE.md "$out/share/licenses/styrhous/LICENSE.md"

    # Same manifest as the release packages. The target/ rows come from
    # scripts/generate-third-party-legal.sh, which needs network access.
    while IFS=$'\t' read -r repository_path packaged_filename _ ||
      [[ -n "$repository_path" ]]; do
      if [[ -z "$repository_path" || "$repository_path" == \#* ||
        "$repository_path" == target/* ]]; then
        continue
      fi
      install -Dm644 "$repository_path" "$out/share/doc/styrhous/$packaged_filename"
    done <legal/resources.tsv
  '';

  postFixup = ''
    wrapProgram "$out/bin/styrhous" \
      --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath runtimeLibraries}:/run/opengl-driver/lib" \
      --prefix PATH : "${lib.makeBinPath [ kubectl ]}"
  '';

  meta = {
    description = "Desktop UI for exploring Kubernetes clusters";
    homepage = "https://github.com/zlepper/styrhous";
    license = lib.licenses.unfree;
    mainProgram = "styrhous";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
  };
}
