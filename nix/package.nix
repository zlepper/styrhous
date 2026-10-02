{
  lib,
  copyDesktopItems,
  kubectl,
  librsvg,
  makeDesktopItem,
  makeWrapper,
  runtimeLibraries,
  rustPackages_1_97,
  hostedLicenseOrigin ? null,
}:

rustPackages_1_97.rustPlatform.buildRustPackage {
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
    install -Dm644 legal/THIRD_PARTY_NOTICES.md \
      "$out/share/doc/styrhous/THIRD_PARTY_NOTICES.md"
    install -Dm644 crates/components/assets/fonts/inter/LICENSE.txt \
      "$out/share/doc/styrhous/Inter-OFL-1.1.txt"
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
