{self}: {
  lib,
  pkgs,
  config,
  ...
}: let
  inherit (lib) types boolToString mkOption mkEnableOption mkIf;

  cfg = config.services.robotica-freeswitch;

  system = pkgs.stdenv.system;
  robotica-freeswitch = self.packages.${system}.robotica-freeswitch;

  wrapper = pkgs.writeShellScriptBin "robotica-freeswitch" ''
    export RUST_LOG=info
    export ROBOTICA_DEBUG="${boolToString (cfg.debug)}"
    export CONFIG_FILE="/etc/robotica-freeswitch.yaml"
    export SECRETS_FILE="${cfg.secrets}"
    exec "${robotica-freeswitch}/bin/robotica-freeswitch"
  '';

  freeswitch_type = types.submodule {
    options = {
      listen_address = mkOption {
        type = types.str;
        default = "[::]:8084";
      };
      topic = mkOption {type = types.str;};
      audience = mkOption {type = types.str;};
    };
  };

  config_type = types.submodule {
    options = {freeswitch = mkOption {type = freeswitch_type;};};
  };
in {
  options.services.robotica-freeswitch = {
    enable = mkEnableOption "robotica-freeswitch service";
    config = mkOption {type = config_type;};
    debug = mkOption {type = types.bool;};
    secrets = mkOption {type = types.path;};
    data_dir = mkOption {
      type = types.str;
      default = "/var/lib/robotica";
    };
  };

  config = mkIf cfg.enable {
    users.users.robotica = {
      isSystemUser = true;
      description = "Robotica user";
      group = "robotica";
      createHome = true;
      home = "${cfg.data_dir}";
    };

    users.groups.robotica = {};

    environment.etc."robotica-freeswitch.yaml" = {
      text = lib.generators.toYAML {} cfg.config;
    };

    systemd.services.robotica-freeswitch = {
      wantedBy = ["multi-user.target"];
      serviceConfig = {
        User = "robotica";
        ExecStart = "${wrapper}/bin/robotica-freeswitch";
      };
    };
  };
}
