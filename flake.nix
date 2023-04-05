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

          nativeBuildInputs = with pkgs; [ pkgconfig ];

          buildInputs = with pkgs; [
            openssl
            protobuf
            fontconfig
            freetype
            xorg.libxcb
            xorg.libX11
            xorg.libXcursor
            xorg.libXrandr
            xorg.libXi
            mesa
            # dbus
            # libGL
            wayland
            libxkbcommon
          ];
        };
        wrapper = pkgs.writeShellScriptBin "robotica-slint" ''
          export LD_LIBRARY_PATH="${pkgs.libGL}/lib:${pkgs.dbus.lib}/lib:$LD_LIBRARY_PATH"
          exec ${pkg}/bin/robotica-slint "$@"
        '';

      in {
        devShell = pkgs.mkShell {
          buildInputs = with pkgs; [
            rust-analyzer
            pkgconfig
            openssl
            protobuf
            nodejs
            wasm-pack
            (rust_pkgs.default.override {
              extensions = [ "rust-src" ];
              targets = [ "wasm32-unknown-unknown" ];
            })
            # fontconfig
            # freetype
            # xorg.libxcb
            # xorg.libX11
            # xorg.libXcursor
            # xorg.libXrandr
            # xorg.libXi
            # mesa
            # dbus
            # libGL
            # wayland
            # libxkbcommon
          ];
          shellHook = ''
            export LD_LIBRARY_PATH="${pkgs.fontconfig}/lib:$LD_LIBRARY_PATH"
            export LD_LIBRARY_PATH="${pkgs.freetype}/lib:$LD_LIBRARY_PATH"
            export LD_LIBRARY_PATH="${pkgs.xorg.libxcb}/lib:$LD_LIBRARY_PATH"
            export LD_LIBRARY_PATH="${pkgs.xorg.libX11}/lib:$LD_LIBRARY_PATH"
            export LD_LIBRARY_PATH="${pkgs.xorg.libXcursor}/lib:$LD_LIBRARY_PATH"
            export LD_LIBRARY_PATH="${pkgs.xorg.libXrandr}/lib:$LD_LIBRARY_PATH"
            export LD_LIBRARY_PATH="${pkgs.xorg.libXi}/lib:$LD_LIBRARY_PATH"
            export LD_LIBRARY_PATH="${pkgs.mesa}/lib:$LD_LIBRARY_PATH"
            export LD_LIBRARY_PATH="${pkgs.dbus}/lib:$LD_LIBRARY_PATH"
            export LD_LIBRARY_PATH="${pkgs.libGL}/lib:$LD_LIBRARY_PATH"
            export LD_LIBRARY_PATH="${pkgs.wayland}/lib:$LD_LIBRARY_PATH"
            export LD_LIBRARY_PATH="${pkgs.libxkbcommon}/lib:$LD_LIBRARY_PATH"
          '';
        };
        packages = { robotica-slint = wrapper; };

      });
}
