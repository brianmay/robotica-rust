{
  description = "Control IO devices";

  inputs = {
    rust-overlay.url = "github:oxalica/rust-overlay";

    cargo2nix = {
      url = "github:cargo2nix/cargo2nix/unstable";
      inputs.rust-overlay.follows = "rust-overlay";
    };
    flake-utils.follows = "cargo2nix/flake-utils";
    nixpkgs.follows = "cargo2nix/nixpkgs";
  };

  outputs = inputs:
    with inputs;
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ cargo2nix.overlays.default ];
        };

        packageOverrides = pkgs:
          pkgs.rustBuilder.overrides.all ++ [
            (pkgs.rustBuilder.rustLib.makeOverride {
              name = "yeslogic-fontconfig-sys";
              overrideAttrs = drv: {
                propagatedNativeBuildInputs =
                  drv.propagatedNativeBuildInputs or [ ] ++ [ pkgs.fontconfig ];
              };
            })
            (pkgs.rustBuilder.rustLib.makeOverride {
              name = "skia-bindings";
              overrideAttrs = drv: {
                propagatedNativeBuildInputs =
                  drv.propagatedNativeBuildInputs or [ ];
              };
            })
          ];

        # create the workspace & dependencies package set
        rustPkgs = pkgs.rustBuilder.makePackageSet {
          rustVersion = "1.68.1";
          packageFun = import ./Cargo.nix;
          extraRustComponents = [ "clippy" "rustfmt" ];
          packageOverrides = packageOverrides;
        };

        # The workspace defines a development shell with all of the dependencies
        # and environment settings necessary for a regular `cargo build`
        workspaceShell = rustPkgs.workspaceShell {
          # This adds cargo2nix to the project shell via the cargo2nix flake
          packages = [ cargo2nix.packages."${system}".cargo2nix ];
        };

        robotica-slint = (rustPkgs.workspace.robotica-slint { }).bin;

      in rec {
        packages = {
          robotica-slint = robotica-slint;
          default = robotica-slint;
        };
        devShell = workspaceShell;
      });
}
