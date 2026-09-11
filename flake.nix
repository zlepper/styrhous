{
  description = "Styrhous development environment";

  inputs.nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        config.allowUnfree = true;
      };
      playwrightBrowsers = pkgs.playwright-driver.browsers;
      runtimeLibraries = with pkgs; [
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
    in
    {
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
