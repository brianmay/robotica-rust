{
  description = "IOT automation for people who think like programmers";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.flake-utils.url = "github:numtide/flake-utils";
  inputs.rust-overlay.url = "github:oxalica/rust-overlay";
  inputs.crane = {
    url = "github:ipetkov/crane";
    inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, crane }:
    flake-utils.lib.eachSystem flake-utils.lib.allSystems (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rust_pkgs = pkgs.rust-bin.stable.latest;
        rustPlatform = pkgs.rust-bin.stable.latest.default.override {
          # targets = [ "wasm32-unknown-unknown" ];
        };

        craneLib = (crane.mkLib pkgs).overrideScope' (final: prev: {
          rustc = rustPlatform;
          cargo = rustPlatform;
          rustfmt = rustPlatform;
        });
        src = ./.;

        # Build *just* the cargo dependencies, so we can reuse
        # all of that work (e.g. via cachix) when running in CI
        cargoArtifacts = craneLib.buildDepsOnly { inherit src; };

        pkg = craneLib.buildPackage {
          inherit cargoArtifacts src;
          cargoExtraArgs = "-p robotica-slint";

          # Add extra inputs here or any other derivation settings
          doCheck = true;
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
            slint-lsp
            rustPlatform
            # (rust_pkgs.default.override {
            #   extensions = [ "rust-src" ];
            #   targets = [ "wasm32-unknown-unknown" ];
            # })
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
