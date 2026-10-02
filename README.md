# Styrhous

Styrhous is a desktop UI for exploring Kubernetes clusters.

## Install on NixOS

Add this repository to your NixOS flake inputs and install its package:

```nix
{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.styrhous.url = "github:zlepper/styrhous";

  outputs = { nixpkgs, styrhous, ... }: {
    nixosConfigurations.my-host = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ({ pkgs, ... }: {
          environment.systemPackages = [
            styrhous.packages.${pkgs.system}.default
          ];
        })
      ];
    };
  };
}
```

The package also supports `aarch64-linux`. Use `nix run github:zlepper/styrhous`
to launch it without adding it to your system configuration. The package includes
`kubectl` for terminal actions and uses your installed terminal emulator.

The hosted licensing URL is not stored in this repository. To build it into the
app, override the package in `environment.systemPackages`:

```nix
(styrhous.packages.${pkgs.system}.default.override {
  hostedLicenseOrigin = "https://your-license-server.example";
})
```

Without that override, you can select a custom licensing server in the app.
See [LICENSE.md](LICENSE.md) for the source-available evaluation terms.
