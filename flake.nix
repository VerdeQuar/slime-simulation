{
  inputs = {
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.05";
  };

  outputs = {
    self,
    flake-utils,
    naersk,
    nixpkgs,
    fenix,
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = (import nixpkgs) {
          inherit system;
          overlays = [fenix.overlays.default];
        };

        naersk' = pkgs.callPackage naersk {};
        buildInputs = with pkgs; [
          wayland
          libxkbcommon
          vulkan-loader
          alsa-lib
          libudev-zero
          # vulkan-tools
          # vulkan-headers
          # vulkan-validation-layers
          # xorg.libX11
          # xorg.libXrandr
          # xorg.libXcursor
          # xorg.libXi
        ];
      in rec {
        defaultPackage = naersk'.buildPackage {
          src = ./.;
        };

        devShell = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            alejandra
            rust-analyzer

            pkg-config
            # alsa-lib
            # libudev-zero

            # wayland
            # vulkan-tools
            # vulkan-headers
            # vulkan-loader
            # vulkan-validation-layers
            (pkgs.fenix.stable.withComponents [
              "cargo"
              "clippy"
              "rust-std"
              "rust-src"
              "rustc"
              "rustfmt"
            ])
          ];
          buildInputs = buildInputs;
          shellHook = ''
            export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${nixpkgs.lib.makeLibraryPath buildInputs}"
          '';
        };
      }
    );
}
