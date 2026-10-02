{
  description = "Styrhous desktop application and development environment";

  inputs.nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";

  outputs =
    { self, nixpkgs, ... }:
    let
      linuxSystems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllLinuxSystems = nixpkgs.lib.genAttrs linuxSystems;
      pkgsBySystem = forAllLinuxSystems (system: import nixpkgs {
        inherit system;
        config.allowUnfree = true;
      });
      system = "x86_64-linux";
      pkgs = pkgsBySystem.${system};
      playwrightBrowsers = pkgs.playwright-driver.browsers;
      runtimeLibrariesFor = pkgs: with pkgs; [
        libGL
        libx11
        libxcb
        libxcursor
        libxi
        libxkbcommon
        libxrandr
        openssl
        vulkan-loader
        wayland
      ];
      runtimeLibraries = runtimeLibrariesFor pkgs;
    in
    {
      packages = forAllLinuxSystems (
        system:
        let
          systemPkgs = pkgsBySystem.${system};
          styrhous = systemPkgs.callPackage ./nix/package.nix {
            runtimeLibraries = runtimeLibrariesFor systemPkgs;
          };
        in
        {
          inherit styrhous;
          default = styrhous;
        }
      );

      apps = forAllLinuxSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/styrhous";
          meta.description = "Launch Styrhous";
        };
      });

      devShells.${system}.default = pkgs.mkShell {
        packages =
          (with pkgs; [
            cargo-nextest
            codex
            dotnet-sdk_10
            gh
            imagemagick
            jetbrains.rust-rover
            kind
            kubectl
            nodejs_22
            pulumi
          ])
          ++ runtimeLibraries;

        env = {
          LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath runtimeLibraries}:/run/opengl-driver/lib";
          PLAYWRIGHT_BROWSERS_PATH = "${playwrightBrowsers}";
          PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1";
        };
      };
    };
}
