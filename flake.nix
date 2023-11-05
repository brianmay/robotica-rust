{
  description = "IOT automation for people who think like programmers";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-23.05";
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    poetry2nix = {
      url = "github:nix-community/poetry2nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    node2nix = {
      url = "github:svanderburg/node2nix";
      # inputs.nixpkgs.follows = "nixpkgs";
      flake = false;
    };
    flockenzeit.url = "github:balsoft/flockenzeit";
    # See https://github.com/cachix/devenv/issues/756
    devenv.url = "github:cachix/devenv/v0.6.3";
  };

  outputs = inputs@{ self, nixpkgs, flake-utils, rust-overlay, crane, poetry2nix
    , nixpkgs-unstable, node2nix, flockenzeit, devenv }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        # pkgs_arm = nixpkgs.legacyPackages."aarch64-linux";

        # pkgs = import nixpkgs {
        #   inherit system;
        #   crossSystem = "aarch64-linux";
        #   overlays = [
        #     (import rust-overlay)
        #     (self: super: {
        #       inherit (pkgs_arm)
        #         openssl protobuf fontconfig freetype xorg mesa wayland
        #         libxkbcommon;
        #     })
        #   ];
        # };

        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        pkgs_unstable = nixpkgs-unstable.legacyPackages.${system};
        nodejs = pkgs.nodejs_20;

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

        nodePackages =
          import robotica-frontend/default.nix { inherit pkgs system nodejs; };

        build_env = {
          BUILD_DATE =
            with flockenzeit.lib.splitSecondsSinceEpoch { } self.lastModified;
            "${F}T${T}${Z}";
          VCS_REF = "${self.rev or "dirty"}";
        };

        robotica-frontend = let
          common = {
            src = ./.;
            pname = "robotica-frontend";
            version = "0.0.0";
            cargoExtraArgs = "-p robotica-frontend";
            nativeBuildInputs = with pkgs; [ pkgconfig ];
            buildInputs = with pkgs;
              [ # openssl python3
                protobuf
              ];
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
          } // common // build_env);

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
            webpack
          '';

          installPhase = ''
            mkdir $out
            cp -rv dist/* $out/
          '';
        };

        brian-backend = let
          common = {
            src = ./.;
            pname = "brian-backend";
            version = "0.0.0";
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
          } // common // build_env);

          wrapper = pkgs.writeShellScriptBin "brian-backend" ''
            export PATH="${poetry_env}/bin:$PATH"
            exec ${pkg}/bin/brian-backend "$@"
          '';
        in {
          clippy = clippy;
          coverage = coverage;
          pkg = wrapper;
        };

        freeswitch = let
          common = {
            src = ./.;
            pname = "robotica-freeswitch";
            version = "0.0.0";
            cargoExtraArgs = "-p freeswitch";
            nativeBuildInputs = with pkgs; [ pkgconfig ];
            buildInputs = with pkgs; [ openssl python3 protobuf ];
            # installCargoArtifactsMode = "use-zstd";
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
          } // common // build_env);

          wrapper = pkgs.writeShellScriptBin "freeswitch" ''
            export PATH="${poetry_env}/bin:$PATH"
            exec ${pkg}/bin/freeswitch "$@"
          '';
        in {
          clippy = clippy;
          coverage = coverage;
          pkg = wrapper;
        };

        robotica-slint = let
          common = {
            src = ./.;
            pname = "robotica-slint";
            version = "0.0.0";
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
          } // common // build_env);

          wrapper = pkgs.writeShellScriptBin "robotica-slint" ''
            export LD_LIBRARY_PATH="${pkgs.libGL}/lib:${pkgs.dbus.lib}/lib:$LD_LIBRARY_PATH"
            exec ${pkg}/bin/robotica-slint "$@"
          '';
        in {
          clippy = clippy;
          coverage = coverage;
          pkg = wrapper;
        };

        pkg_node2nix = (import "${node2nix}/default.nix" {
          inherit system pkgs nodejs;
        }).package;

        pkg_node2nix_wrapper = pkgs.writeShellScriptBin "update_node" ''
          cd robotica-frontend
          exec ${pkg_node2nix}/bin/node2nix --development -l
        '';

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
          freeswitch-clippy = freeswitch.clippy;
          freeswitch-coverage = freeswitch.coverage;
          freeswitch = freeswitch.pkg;
        };

        devShells.default = devenv.lib.mkShell {
          inherit inputs pkgs;
          modules = [{
            packages = with pkgs; [
              poetry2nix.packages.${system}.poetry
              poetry_env
              rust-analyzer
              pkgconfig
              openssl
              protobuf
              fontconfig
              freetype
              nodejs
              wasm-pack
              pkgs_unstable.wasm-bindgen-cli
              slint-lsp
              rustPlatform
              # https://github.com/NixOS/nixpkgs/issues/156890
              binaryen
              pkg_node2nix
              pkg_node2nix_wrapper
              nodePackages.nodeDependencies
              gcc
            ];
            enterShell = ''
              # kludge for https://github.com/cachix/devenv/issues/862
              export PKG_CONFIG_PATH_FOR_TARGET="$PKG_CONFIG_PATH"
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
              export ROBOTICA_DEBUG=true
              export CONFIG_FILE="$PWD/robotica-backend.yaml"
              export SLINT_CONFIG_FILE="$PWD/robotica-slint.yaml"
              export STATIC_PATH="${robotica-frontend-bindgen}"
            '';
            processes.mqtt = { exec = "${pkgs.mosquitto}/bin/mosquitto"; };
            processes.influxdb = { exec = "${pkgs.influxdb}/bin/influxd"; };
          }];
        };
        packages = {
          robotica-frontend = robotica-frontend-bindgen;
          brian-backend = brian-backend.pkg;
          robotica-slint = robotica-slint.pkg;
          freeswitch = freeswitch.pkg;
        };
      });
}
