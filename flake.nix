{
  description = "IOT automation for people who think like programmers";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
    poetry2nix = {
      url = "github:nix-community/poetry2nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flockenzeit.url = "github:balsoft/flockenzeit";
    devenv.url = "github:cachix/devenv";
  };

  outputs =
    inputs@{
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
      crane,
      poetry2nix,
      flockenzeit,
      devenv,
    }:
    flake-utils.lib.eachSystem
      [
        "x86_64-linux"
        "aarch64-linux"
      ]
      (
        system:
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
          nodejs = pkgs.nodejs_20;

          p2n = import poetry2nix { inherit pkgs; };
          poetry_env = p2n.mkPoetryEnv {
            python = pkgs.python3;
            projectDir = ./robotica-tokio/python;
            overrides = p2n.defaultPoetryOverrides.extend (
              self: super: {
                x-wr-timezone = super.x-wr-timezone.overridePythonAttrs (old: {
                  buildInputs = (old.buildInputs or [ ]) ++ [ super.setuptools ];
                });
                recurring-ical-events = super.recurring-ical-events.overridePythonAttrs (old: {
                  buildInputs = (old.buildInputs or [ ]) ++ [ super.setuptools ];
                });
              }
            );
          };
          rustPlatform = pkgs.rust-bin.stable.latest.default.override {
            targets = [ "wasm32-unknown-unknown" ];
            extensions = [ "rust-src" ];
          };

          craneLib = (crane.mkLib pkgs).overrideToolchain rustPlatform;

          nodePackages = pkgs.buildNpmPackage {
            name = "robotica-frontend";
            src = ./robotica-frontend;
            # npmDepsHash = "sha256-1bhWY/wOCYq0J5AYm9Mp9M7DfGCMpp7wBtHNWqV0+5c=";
            # npmDepsHash = pkgs.lib.fakeHash;
            npmDepsHash = builtins.readFile ./npm-deps-hash;
            dontNpmBuild = true;
            inherit nodejs;

            installPhase = ''
              mkdir $out
              cp -r node_modules $out
              ln -s $out/node_modules/.bin $out/bin
            '';
          };

          build_env = {
            BUILD_DATE = with flockenzeit.lib.splitSecondsSinceEpoch { } self.lastModified; "${F}T${T}${Z}";
            VCS_REF = "${self.rev or "dirty"}";
          };

          robotica-frontend =
            let
              common = {
                src = ./.;
                pname = "robotica-frontend";
                version = "0.0.0";
                cargoExtraArgs = "-p robotica-frontend";
                nativeBuildInputs = with pkgs; [ pkg-config ];
                buildInputs = with pkgs; [
                  # openssl python3
                  protobuf
                ];
                CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
                doCheck = false;
              };

              # Build *just* the cargo dependencies, so we can reuse
              # all of that work (e.g. via cachix) when running in CI
              cargoArtifacts = craneLib.buildDepsOnly common;

              # Run clippy (and deny all warnings) on the crate source.
              clippy = craneLib.cargoClippy (
                {
                  inherit cargoArtifacts;
                  cargoClippyExtraArgs = "-- --deny warnings";
                }
                // common
              );

              # Build the actual crate itself.
              pkg = craneLib.buildPackage (
                {
                  inherit cargoArtifacts;
                  doCheck = false;
                }
                // common
                // build_env
              );
            in
            {
              clippy = clippy;
              pkg = pkg;
            };

          robotica-frontend-bindgen = pkgs.stdenv.mkDerivation {
            name = "robotica-frontend-bindgen";
            src = ./robotica-frontend;

            buildPhase = ''
              ${pkgs.wasm-bindgen-cli}/bin/wasm-bindgen \
                --target bundler \
                --out-dir pkg \
                --omit-default-module-path \
                ${robotica-frontend.pkg}/lib/robotica_frontend.wasm

              ln -s ${nodePackages}/node_modules ./node_modules
              export PATH="${nodejs}/bin:${nodePackages}/bin:$PATH"
              webpack
            '';

            installPhase = ''
              mkdir $out
              cp -rv dist/* $out/
            '';
          };

          brian-backend =
            let
              common = {
                src = ./.;
                pname = "brian-backend";
                version = "0.0.0";
                cargoExtraArgs = "-p brian-backend";
                nativeBuildInputs = with pkgs; [ pkg-config ];
                buildInputs = with pkgs; [
                  openssl
                  python3
                  protobuf
                ];
                # See https://github.com/ipetkov/crane/issues/414#issuecomment-1860852084
                # for possible work around if this is required in the future.
                # installCargoArtifactsMode = "use-zstd";
              };

              # Build *just* the cargo dependencies, so we can reuse
              # all of that work (e.g. via cachix) when running in CI
              cargoArtifacts = craneLib.buildDepsOnly common;

              # Run clippy (and deny all warnings) on the crate source.
              clippy = craneLib.cargoClippy (
                {
                  inherit cargoArtifacts;
                  cargoClippyExtraArgs = "-- --deny warnings";
                }
                // common
              );

              # Next, we want to run the tests and collect code-coverage, _but only if
              # the clippy checks pass_ so we do not waste any extra cycles.
              coverage = craneLib.cargoTarpaulin ({ cargoArtifacts = clippy; } // common);

              # Build the actual crate itself.
              pkg = craneLib.buildPackage (
                {
                  inherit cargoArtifacts;
                  doCheck = true;
                  # CARGO_LOG = "cargo::core::compiler::fingerprint=info";
                }
                // common
                // build_env
              );

              wrapper = pkgs.writeShellScriptBin "brian-backend" ''
                export PATH="${poetry_env}/bin:$PATH"
                exec ${pkg}/bin/brian-backend "$@"
              '';
            in
            {
              clippy = clippy;
              coverage = coverage;
              pkg = wrapper;
            };

          robotica-freeswitch =
            let
              common = {
                src = ./.;
                pname = "robotica-freeswitch";
                version = "0.0.0";
                cargoExtraArgs = "-p robotica-freeswitch";
                nativeBuildInputs = with pkgs; [ pkg-config ];
                buildInputs = with pkgs; [
                  openssl
                  python3
                  protobuf
                ];
              };

              # Build *just* the cargo dependencies, so we can reuse
              # all of that work (e.g. via cachix) when running in CI
              cargoArtifacts = craneLib.buildDepsOnly common;

              # Run clippy (and deny all warnings) on the crate source.
              clippy = craneLib.cargoClippy (
                {
                  inherit cargoArtifacts;
                  cargoClippyExtraArgs = "-- --deny warnings";
                }
                // common
              );

              # Next, we want to run the tests and collect code-coverage, _but only if
              # the clippy checks pass_ so we do not waste any extra cycles.
              coverage = craneLib.cargoTarpaulin ({ cargoArtifacts = clippy; } // common);

              # Build the actual crate itself.
              pkg = craneLib.buildPackage (
                {
                  inherit cargoArtifacts;
                  doCheck = true;
                }
                // common
                // build_env
              );

              wrapper = pkgs.writeShellScriptBin "robotica-freeswitch" ''
                export PATH="${poetry_env}/bin:$PATH"
                exec ${pkg}/bin/robotica-freeswitch "$@"
              '';
            in
            {
              clippy = clippy;
              coverage = coverage;
              pkg = wrapper;
            };

          robotica-slint =
            let
              common = {
                src = ./.;
                pname = "robotica-slint";
                version = "0.0.0";
                cargoExtraArgs = "-p robotica-slint";
                nativeBuildInputs = with pkgs; [ pkg-config ];
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
              clippy = craneLib.cargoClippy (
                {
                  inherit cargoArtifacts;
                  cargoClippyExtraArgs = "-- --deny warnings";
                }
                // common
              );

              # Next, we want to run the tests and collect code-coverage, _but only if
              # the clippy checks pass_ so we do not waste any extra cycles.
              coverage = craneLib.cargoTarpaulin ({ cargoArtifacts = clippy; } // common);

              libPath = pkgs.lib.makeLibraryPath [
                pkgs.libGL
                pkgs.libxkbcommon
                pkgs.dbus.lib
                pkgs.wayland
                pkgs.fontconfig
              ];

              # Build the actual crate itself.
              pkg = craneLib.buildPackage (
                {
                  inherit cargoArtifacts;
                  doCheck = true;

                  postFixup = pkgs.lib.optional pkgs.stdenv.isLinux ''
                    rpath=$(patchelf --print-rpath $out/bin/robotica-slint)
                    patchelf --set-rpath "$rpath:${libPath}" $out/bin/robotica-slint
                  '';
                }
                // common
                // build_env
              );
            in
            {
              clippy = clippy;
              coverage = coverage;
              pkg = pkg;
            };

          devShell = devenv.lib.mkShell {
            inherit inputs pkgs;
            modules = [
              {
                packages = [
                  pkgs.poetry
                  poetry_env
                  pkgs.rust-analyzer
                  pkgs.pkg-config
                  pkgs.openssl
                  pkgs.protobuf
                  pkgs.fontconfig
                  pkgs.freetype
                  nodejs
                  pkgs.wasm-pack
                  pkgs.wasm-bindgen-cli
                  pkgs.slint-lsp
                  rustPlatform
                  pkgs.cargo-expand
                  # https://github.com/NixOS/nixpkgs/issues/156890
                  pkgs.binaryen
                  nodePackages
                  pkgs.prefetch-npm-deps
                  pkgs.gcc
                  pkgs.sqlx-cli
                  pkgs.influxdb2
                  pkgs.mosquitto
                ];
                enterShell = ''
                  export ROBOTICA_DEBUG=true
                  export PYTHONPATH="${poetry_env}/lib/python3.11/site-packages"
                  export CONFIG_FILE="$PWD/robotica-backend.yaml"
                  export SLINT_CONFIG_FILE="$PWD/robotica-slint.yaml"
                  export STATIC_PATH="robotica-frontend/dist"
                  export DATABASE_URL="postgresql://robotica:your_secure_password_here@localhost/robotica"
                '';
                processes.mqtt = {
                  exec = "${pkgs.mosquitto}/bin/mosquitto -c mosquitto.conf";
                };
                processes.influxdb = {
                  exec = "${pkgs.influxdb2}/bin/influxd";
                };
                services.postgres = {
                  enable = true;
                  package = pkgs.postgresql_15.withPackages (ps: [ ps.postgis ]);
                  listen_addresses = "127.0.0.1";
                  initialDatabases = [ { name = "robotica"; } ];
                  initialScript = ''
                    \c robotica;
                    CREATE USER robotica with encrypted password 'your_secure_password_here';
                    GRANT ALL PRIVILEGES ON DATABASE robotica TO robotica;
                    -- GRANT ALL ON SCHEMA public TO robotica;
                    ALTER USER robotica WITH SUPERUSER;
                  '';
                };
              }
            ];
          };

          test_robotica_backend = pkgs.nixosTest {
            name = "robotica-backend";
            nodes.machine =
              { ... }:
              {
                imports = [
                  self.nixosModules.robotica-backend
                ];
                services.robotica-backend = {
                  enable = true;
                  config = {
                    logging = {
                      deployment_environment = "prod";
                    };
                    cars = [ ];
                    lights = [ ];
                    strips = [ ];
                    metrics = [ ];
                  };

                  secrets = pkgs.writeText "secrets.yaml" ''
                    database_url: "postgresql://robotica:your_secure_password_here@localhost/robotica"
                    mqtt:
                      host: "localhost"
                      port: 1883
                      credentials:
                        type: "UsernamePassword"
                        username: "robotica"
                        password: "pretend_secret"
                    influxdb:
                      url: "http://localhost:8086"
                      token: "token"
                      database: "sensors"
                  '';
                  debug = false;
                };
                system.stateVersion = "24.05";

                services.postgresql = {
                  enable = true;
                  package = pkgs.postgresql_15;
                  extraPlugins = ps: [ ps.postgis ];
                  initialScript = pkgs.writeText "init.psql" ''
                    CREATE DATABASE robotica;
                    CREATE USER robotica with encrypted password 'your_secure_password_here';
                    ALTER DATABASE robotica OWNER TO robotica;
                    ALTER USER robotica WITH SUPERUSER;
                  '';
                };

                services.mosquitto = {
                  enable = true;
                  # logType = [ "all" ];
                  listeners = [
                    {
                      port = 1883;
                      users.robotica = {
                        password = "pretend_secret";
                        acl = [ "readwrite #" ];
                      };
                    }
                  ];
                };

                services.influxdb2 = {
                  enable = true;
                  provision = {
                    users.passwordFile = pkgs.writeText "influx.txt" "influx_password";
                    organizations.robotica = {
                      buckets.backet = { };
                      auths.robotica = {
                        tokenFile = pkgs.writeText "token.txt" "token";
                        allAccess = true;
                      };
                    };
                  };
                };
              };

            testScript = ''
              machine.wait_for_unit("robotica-backend.service")
              # machine.wait_for_open_port(4000)
              # machine.succeed("${pkgs.curl}/bin/curl --fail -v http://localhost:4000/_health")
            '';
          };
        in
        {
          # Disable coverage checks as broken since Rust 1.77:
          # Mar 27 05:16:41.964 ERROR cargo_tarpaulin::test_loader: Error parsing debug information from binary: An I/O error occurred while reading.
          # Mar 27 05:16:41.964  WARN cargo_tarpaulin::test_loader: Stripping symbol information can prevent tarpaulin from working. If you want to do this pass `--engine=llvm`
          # Mar 27 05:16:41.965 ERROR cargo_tarpaulin: Error while parsing binary or DWARF info.
          # Error: "Error while parsing binary or DWARF info."
          checks = {
            robotica-slint-clippy = robotica-slint.clippy;
            # robotica-slint-coverage = robotica-slint.coverage;
            robotica-slint = robotica-slint.pkg;
            robotica-frontend-clippy = robotica-frontend.clippy;
            robotica-frontend = robotica-frontend.pkg;
            brian-backend-clippy = brian-backend.clippy;
            # brian-backend-coverage = brian-backend.coverage;
            brian-backend = brian-backend.pkg;
            robotica-freeswitch-clippy = robotica-freeswitch.clippy;
            # freeswitch-coverage = robotica-freeswitch.coverage;
            robotica-freeswitch = robotica-freeswitch.pkg;
          };

          devShells.default = devShell;
          packages = {
            robotica-frontend = robotica-frontend-bindgen;
            brian-backend = brian-backend.pkg;
            robotica-slint = robotica-slint.pkg;
            robotica-freeswitch = robotica-freeswitch.pkg;
            devenv-up = devShell.config.procfileScript;
          };
        }
      )
    // {
      nixosModules = {
        robotica-backend = import ./modules/robotica-backend.nix { inherit self; };
        robotica-slint = import ./modules/robotica-slint.nix { inherit self; };
        robotica-freeswitch = import ./modules/robotica-freeswitch.nix { inherit self; };
      };
    };
}
