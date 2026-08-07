{
  description = "Example flake with a devShell";

inputs.nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";

outputs = { self, nixpkgs}:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; config.allowUnfree = true; };
      lib = pkgs.lib;
    in {
      devShells.x86_64-linux.default = pkgs.mkShell rec {
        buildInputs = with pkgs; [
          libxcb
          libGL
          libxkbcommon
          openssl
          wayland
          kind
          kubectl
          cargo-nextest
          jetbrains.rust-rover
          imagemagick

             xorg.libXcursor
                      xorg.libXrandr
                      xorg.libXi
                      xorg.libX11
        ];
        shellHook = ''
            export LD_LIBRARY_PATH="${lib.makeLibraryPath buildInputs}:/run/opengl-driver/lib"
        '';
      };
    };
}
