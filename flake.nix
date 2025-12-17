{
  description = "IOT automation for people who think like programmers";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
    pyproject-nix = {
      url = "github:pyproject-nix/pyproject.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    uv2nix = {
      url = "github:pyproject-nix/uv2nix";
      inputs.pyproject-nix.follows = "pyproject-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    pyproject-build-systems = {
      url = "github:pyproject-nix/build-system-pkgs";
      inputs.pyproject-nix.follows = "pyproject-nix";
      inputs.uv2nix.follows = "uv2nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flockenzeit.url = "github:balsoft/flockenzeit";
    devenv.url = "github:cachix/devenv";
  };

  outputs =
    inputs@{
      self,
      nixpkgs,
      nixpkgs-unstable,
      flake-utils,
      rust-overlay,
      crane,
      pyproject-nix,
      uv2nix,
      pyproject-build-systems,
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
          pkgs-unstable = nixpkgs-unstable.legacyPackages.${system};
          python = pkgs.python312;
          nodejs = pkgs.nodejs_20;

          wasm-bindgen-cli = pkgs.buildWasmBindgenCli rec {
            src = pkgs.fetchCrate {
              pname = "wasm-bindgen-cli";
              version = "0.2.106";
              hash = "sha256-M6WuGl7EruNopHZbqBpucu4RWz44/MSdv6f0zkYw+44=";
            };
            # src = pkgs.fetchFromGitHub {
            #   owner = "rustwasm";
            #   repo = "wasm-bindgen";
            #   rev = "0.2.106";
            #   sha256 = "sha256-ZNqbec3fQeoIKVEdN0uXptDQlEAQNc1MAWqoTcPBUWk=";
            # };

            cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
              inherit src;
              inherit (src) pname version;
              hash = "sha256-ElDatyOwdKwHg3bNH/1pcxKI7LXkhsotlDPQjiLHBwA=";
            };
          };

          python_venv =
            let
              inherit (nixpkgs) lib;
              workspace = uv2nix.lib.workspace.loadWorkspace { workspaceRoot = ./robotica-tokio/python; };

              # Create package overlay from workspace.
              overlay = workspace.mkPyprojectOverlay {
                sourcePreference = "sdist";
              };

              # Extend generated overlay with build fixups
              #
              # Uv2nix can only work with what it has, and uv.lock is missing essential metadata to perform some builds.
              # This is an additional overlay implementing build fixups.
              # See:
              # - https://pyproject-nix.github.io/uv2nix/FAQ.html
              pyprojectOverrides =
                final: prev:
                # Implement build fixups here.
                # Note that uv2nix is _not_ using Nixpkgs buildPythonPackage.
                # It's using https://pyproject-nix.github.io/pyproject.nix/build.html
                let
                  inherit (final) resolveBuildSystem;
                  inherit (builtins) mapAttrs;

                  # Build system dependencies specified in the shape expected by resolveBuildSystem
                  # The empty lists below are lists of optional dependencies.
                  #
                  # A package `foo` with specification written as:
                  # `setuptools-scm[toml]` in pyproject.toml would be written as
                  # `foo.setuptools-scm = [ "toml" ]` in Nix
                  buildSystemOverrides = {
                    x-wr-timezone.setuptools = [ ];
                    recurring-ical-events.setuptools = [ ];
                    recurring-ical-events.hatchling = [ ];
                    recurring-ical-events.hatch-vcs = [ ];
                    icalendar.setuptools = [ ];
                    icalendar.hatchling = [ ];
                    icalendar.hatch-vcs = [ ];
                    packaging.flit-core = [ ];
                    pathspec.flit-core = [ ];
                    pluggy.setuptools = [ ];
                    click.flit-core = [ ];
                    python-dateutil.setuptools = [ ];
                    pytz.setuptools = [ ];
                    six.setuptools = [ ];
                    tzdata.setuptools = [ ];
                  };

                in
                mapAttrs (
                  name: spec:
                  prev.${name}.overrideAttrs (old: {
                    nativeBuildInputs = old.nativeBuildInputs ++ resolveBuildSystem spec;
                  })
                ) buildSystemOverrides;

              pythonSet =
                (pkgs.callPackage pyproject-nix.build.packages {
                  inherit python;
                }).overrideScope
                  (
                    lib.composeManyExtensions [
                      pyproject-build-systems.overlays.default
                      overlay
                      pyprojectOverrides
                    ]
                  );

              venv = pythonSet.mkVirtualEnv "robotica-python" workspace.deps.default;

            in
            venv;

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
            npmDepsHash = "sha256-Y+kqYPmjIBNj1uIXnB8XJlm9ib9U2y4MeOCJcCVNMJE=";
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
              ${wasm-bindgen-cli}/bin/wasm-bindgen \
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

          robotica-backend =
            let
              common = {
                src = ./.;
                pname = "robotica-backend";
                version = "0.0.0";
                cargoExtraArgs = "-p robotica-backend";
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

              wrapper = pkgs.writeShellScriptBin "robotica-backend" ''
                export PYTHONPATH="${python_venv}/lib/python3.12/site-packages"
                exec ${pkg}/bin/robotica-backend "$@"
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
                export PATH="${python_venv}/bin:$PATH"
                exec ${pkg}/bin/robotica-freeswitch "$@"
              '';
            in
            {
              clippy = clippy;
              coverage = coverage;
              pkg = wrapper;
            };

          robotica-slint =
            crossSystem: localSystem:
            let
              pkgs = nixpkgs.legacyPackages.${crossSystem};
              crossPkgs = import nixpkgs {
                inherit crossSystem localSystem;
                overlays = [ (import rust-overlay) ];
              };

              # rustPlatform = crossPkgs.rust-bin.stable.latest.default.override {
              #   targets = [ "wasm32-unknown-unknown" ];
              #   extensions = [ "rust-src" ];
              # };

              # craneLib = (crane.mkLib crossPkgs).overrideToolchain rustPlatform;
              craneLib = (crane.mkLib crossPkgs).overrideToolchain (p: p.rust-bin.stable.latest.default);

              common = {
                src = ./.;
                pname = "robotica-slint";
                version = "0.0.0";
                cargoExtraArgs = "-p robotica-slint";
                nativeBuildInputs = with crossPkgs; [ pkg-config ];
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
              libPath = libPath;
              clippy = clippy;
              coverage = coverage;
              pkg = pkg;
            };

          libPath = pkgs.lib.makeLibraryPath [
            pkgs.libGL
            pkgs.libxkbcommon
            pkgs.dbus.lib
            pkgs.wayland
            pkgs.fontconfig
          ];
          devShell = devenv.lib.mkShell {
            inherit inputs pkgs;
            modules = [
              {
                packages = [
                  pkgs.uv
                  python_venv
                  pkgs-unstable.rust-analyzer
                  pkgs.pkg-config
                  pkgs.openssl
                  pkgs.protobuf
                  pkgs.fontconfig
                  pkgs.freetype
                  nodejs
                  pkgs.wasm-pack
                  wasm-bindgen-cli
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
                  echo "Python path: ${python_venv}"
                  export LD_LIBRARY_PATH="${libPath}"
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

          test_robotica_backend = pkgs.testers.nixosTest {
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
                    presence_trackers = [ ];
                    occupancy_sensors = [ ];
                    night_mode = [ ];
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
                  extensions = ps: [ ps.postgis ];
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
            robotica-slint-clippy = (robotica-slint system system).clippy;
            # robotica-slint-coverage = (robotica-slint system system).coverage;
            robotica-slint = (robotica-slint system system).pkg;
            robotica-frontend-clippy = robotica-frontend.clippy;
            robotica-frontend = robotica-frontend.pkg;
            robotica-backend-clippy = robotica-backend.clippy;
            # robotica-backend-coverage = robotica-backend.coverage;
            robotica-backend = robotica-backend.pkg;
            robotica-freeswitch-clippy = robotica-freeswitch.clippy;
            # freeswitch-coverage = robotica-freeswitch.coverage;
            robotica-freeswitch = robotica-freeswitch.pkg;
            inherit robotica-frontend-bindgen;
            # TODO check if this works in CI
            # inherit test_robotica_backend;
          };

          devShells.default = devShell;
          packages = {
            robotica-frontend = robotica-frontend-bindgen;
            robotica-backend = robotica-backend.pkg;
            robotica-slint = (robotica-slint system system).pkg;
            robotica-slint-aarch64 = (robotica-slint "aarch64-linux" system).pkg;
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
