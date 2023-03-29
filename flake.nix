{
  description = "A very basic flake";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.flake-utils.url = "github:numtide/flake-utils";
  inputs.rust-overlay.url = "github:oxalica/rust-overlay";

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachSystem flake-utils.lib.allSystems (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rust_pkgs = pkgs.rust-bin.stable.latest;
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rust_pkgs.minimal;
          rustc = rust_pkgs.minimal;
        };

        pkg = rustPlatform.buildRustPackage {
          pname = "robotica-slint";
          version = "0.0.1";
          src = ./.;
          cargoBuildFlags = "-p robotica-slint";

          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "getrandom-0.2.8" =
                "sha256-B6xHsgEgJMYG2xj2pNRnjD/b71fSTRVC9Jv/fH3b2Ok=";
              "rumqttc-0.18.0" =
                "sha256-bZA887J8+G8JReMYT2elBtq7iRSM/mNy7/9wlPCNxrI=";
            };
          };

          nativeBuildInputs = with pkgs; [
            pkgconfig
            openssl
            protobuf
            fontconfig
            freetype
            xorg.libxcb
          ];

          PKG_CONFIG_PATH =
            "${pkgs.openssl.dev}/lib/pkgconfig:${pkgs.fontconfig.dev}/lib/pkgconfig:${pkgs.freetype.dev}/lib/pkgconfig";
        };
      in {
        devShell = pkgs.mkShell {
          buildInputs = with pkgs; [
            rust_pkgs.default
            rust-analyzer
            pkgconfig
            openssl
            protobuf

            # slint
            fontconfig
            xorg.libxcb
          ];
        };
        packages = { robotica-slint = pkg; };
      });
}
