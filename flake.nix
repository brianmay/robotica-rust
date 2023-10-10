{
  description = "IOT automation for people who think like programmers";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-23.05";
  inputs.nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.flake-utils.url = "github:numtide/flake-utils";
  inputs.rust-overlay.url = "github:oxalica/rust-overlay";
  inputs.crane = {
    url = "github:ipetkov/crane";
    inputs.nixpkgs.follows = "nixpkgs";
  };
  inputs.poetry2nix = {
    url = "github:nix-community/poetry2nix";
    inputs.nixpkgs.follows = "nixpkgs";
  };
  inputs.node2nix = {
    url = "github:svanderburg/node2nix";
    # inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, crane, poetry2nix
    , nixpkgs-unstable, node2nix }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        pkgs_unstable = nixpkgs-unstable.legacyPackages.${system};

        poetry = poetry2nix.legacyPackages.${system};
        poetry_env = poetry.mkPoetryEnv {
          python = pkgs.python3;
          projectDir = ./robotica-backend/python;
          overrides = poetry.defaultPoetryOverrides.extend (self: super: {
            x-wr-timezone = super.x-wr-timezone.overridePythonAttrs (old: {
              buildInputs = (old.buildInputs or [ ]) ++ [ super.setuptools ];
            });
            recurring-ical-events =
              super.recurring-ical-events.overridePythonAttrs (old: {
                buildInputs = (old.buildInputs or [ ]) ++ [ super.setuptools ];
              });
          });

        };
        rustPlatform = pkgs.rust-bin.stable.latest.default.override {
          targets = [ "wasm32-unknown-unknown" ];
          extensions = [ "rust-src" ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain rustPlatform;

        nodePackages = import robotica-frontend/default.nix {
          inherit pkgs system;
          nodejs = pkgs.nodejs_20;
        };

        robotica-frontend = let
          common = {
            src = ./.;
            cargoExtraArgs = "-p robotica-frontend";
            nativeBuildInputs = with pkgs; [ pkgconfig ];
            buildInputs = with pkgs;
              [ # openssl python3
                protobuf
              ];
            # installCargoArtifactsMode = "use-zstd";
            CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
            doCheck = false;
          };

          # Build *just* the cargo dependencies, so we can reuse
          # all of that work (e.g. via cachix) when running in CI
          cargoArtifacts = craneLib.buildDepsOnly common;

          # Run clippy (and deny all warnings) on the crate source.
          clippy = craneLib.cargoClippy ({
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "-- --deny warnings";
          } // common);

          # Build the actual crate itself, _but only if the previous tests pass_.
          pkg = craneLib.buildPackage ({
            inherit cargoArtifacts;
            doCheck = false;
          } // common);

        in {
          clippy = clippy;
          pkg = pkg;
        };

        robotica-frontend-bindgen = pkgs.stdenv.mkDerivation {
          name = "robotica-frontend-bindgen";
          src = ./robotica-frontend;

          buildPhase = ''
            ${pkgs_unstable.wasm-bindgen-cli}/bin/wasm-bindgen \
              --target bundler \
              --out-dir pkg \
              --omit-default-module-path \
              ${robotica-frontend.pkg}/lib/robotica_frontend.wasm

            ln -s ${nodePackages.nodeDependencies}/lib/node_modules ./node_modules
            export PATH="${nodePackages.nodeDependencies}/bin:$PATH"
            ls -l pkg
            webpack
          '';

          installPhase = ''
            mkdir $out
            # cp -rv pkg $out/
            cp -rv dist/* $out/
          '';
        };

        brian-backend = let
          common = {
            src = ./.;
            cargoExtraArgs = "-p brian-backend";
            nativeBuildInputs = with pkgs; [ pkgconfig ];
            buildInputs = with pkgs; [ openssl python3 protobuf ];
            installCargoArtifactsMode = "use-zstd";
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
            inherit cargoArtifacts;
            doCheck = true;
            # CARGO_LOG = "cargo::core::compiler::fingerprint=info";
          } // common);

          wrapper = pkgs.writeShellScriptBin "brian-backend" ''
            exec ${pkg}/bin/brian-backend "$@"
          '';
        in {
          clippy = clippy;
          coverage = coverage;
          pkg = wrapper;
        };

        brian-backend-bindgen = pkgs.writeShellScriptBin "brian-backend" ''
          export STATIC_PATH="${robotica-frontend-bindgen}"
          exec ${brian-backend.pkg}/bin/brian-backend "$@"
        '';

        robotica-slint = let
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
            inherit cargoArtifacts;
            doCheck = true;
          } // common);

          wrapper = pkgs.writeShellScriptBin "robotica-slint" ''
            export LD_LIBRARY_PATH="${pkgs.libGL}/lib:${pkgs.dbus.lib}/lib:$LD_LIBRARY_PATH"
            exec ${pkg}/bin/robotica-slint "$@"
          '';
        in {
          clippy = clippy;
          coverage = coverage;
          pkg = wrapper;
        };

      in {
        checks = {
          robotica-slint-clippy = robotica-slint.clippy;
          robotica-slint-coverage = robotica-slint.coverage;
          robotica-slint = robotica-slint.pkg;
          robotica-frontend-clippy = robotica-frontend.clippy;
          robotica-frontend = robotica-frontend.pkg;
          brian-backend-clippy = brian-backend.clippy;
          brian-backend-coverage = brian-backend.coverage;
          brian-backend = brian-backend.pkg;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            poetry2nix.packages.${system}.poetry
            poetry_env
            rust-analyzer
            pkgconfig
            openssl
            protobuf
            fontconfig
            nodejs_20
            wasm-pack
            pkgs_unstable.wasm-bindgen-cli
            slint-lsp
            rustPlatform
            # https://github.com/NixOS/nixpkgs/issues/156890
            binaryen
            node2nix.packages.${system}.node2nix
            nodePackages.nodeDependencies
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
        packages = {
          robotica-frontend = robotica-frontend-bindgen;
          brian-backend = brian-backend-bindgen;
          robotica-slint = robotica-slint.pkg;
        };

      });
}
