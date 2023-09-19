{
  description = "IOT automation for people who think like programmers";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-23.05";
  inputs.flake-utils.url = "github:numtide/flake-utils";
  inputs.rust-overlay.url = "github:oxalica/rust-overlay";
  inputs.crane = {
    url = "github:ipetkov/crane";
    inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, crane }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rustPlatform = pkgs.rust-bin.stable.latest.default.override {
          targets = [ "wasm32-unknown-unknown" ];
          extensions = [ "rust-src" ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain rustPlatform;

        common = {
          src = ./.;
          cargoExtraArgs = "-p robotica-slint";
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

        # Build *just* the cargo dependencies, so we can reuse
        # all of that work (e.g. via cachix) when running in CI
        cargoArtifacts = craneLib.buildDepsOnly common;

        # Run clippy (and deny all warnings) on the crate source.
        clippy = craneLib.cargoClippy ({
          inherit cargoArtifacts;
          cargoClippyExtraArgs = "-- --deny warnings";
        } // common);

        # Next, we want to run the tests and collect code-coverage, _but only if
        # the clippy checks pass_ so we do not waste any extra cycles.
        coverage =
          craneLib.cargoTarpaulin ({ cargoArtifacts = clippy; } // common);

        # Build the actual crate itself, _but only if the previous tests pass_.
        pkg = craneLib.buildPackage ({
          cargoArtifacts = clippy;
          doCheck = true;
        } // common);

        wrapper = pkgs.writeShellScriptBin "robotica-slint" ''
          export LD_LIBRARY_PATH="${pkgs.libGL}/lib:${pkgs.dbus.lib}/lib:$LD_LIBRARY_PATH"
          exec ${pkg}/bin/robotica-slint "$@"
        '';

      in {
        checks = { inherit clippy coverage pkg; };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rust-analyzer
            pkgconfig
            openssl
            protobuf
            fontconfig
            nodejs
            wasm-pack
            wasm-bindgen-cli
            slint-lsp
            rustPlatform
            # https://github.com/NixOS/nixpkgs/issues/156890
            binaryen
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
