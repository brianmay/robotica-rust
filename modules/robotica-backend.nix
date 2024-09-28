{self}: {
  lib,
  pkgs,
  config,
  ...
}:
with lib; let
  cfg = config.services.robotica-backend;

  system = pkgs.system;
  robotica-backend = self.packages.${system}.brian-backend;
  robotica-frontend = self.packages.${system}.robotica-frontend;

  robotica_config = pkgs.writeTextFile {
    name = "robotica-backend-config";
    text = lib.generators.toYAML {} cfg.config;
  };

  wrapper = pkgs.writeShellScriptBin "robotica-backend" ''
    export RUST_LOG=info
    export ROBOTICA_DEBUG="${boolToString (cfg.debug)}"
    export CONFIG_FILE="${robotica_config}"
    export SECRETS_FILE="${cfg.secrets}"

    mkdir -p "${cfg.data_dir}"
    mkdir -p "${cfg.data_dir}/state"
    exec "${robotica-backend}/bin/brian-backend"
  '';

  executor_type = types.submodule {
    options = {
      instance = mkOption {type = types.str;};
      classifications_file = mkOption {type = types.path;};
      schedule_file = mkOption {type = types.path;};
      sequences_file = mkOption {type = types.path;};
    };
  };

  http_type = types.submodule {
    options = {
      instance = mkOption {type = types.str;};
      root_url = mkOption {type = types.str;};
      static_path = mkOption {
        type = types.path;
        default = "${robotica-frontend}";
      };
      http_listener = mkOption {
        type = types.str;
        default = "[::]:4000";
      };
    };
  };

  persistent_state_type = types.submodule {
    options = {
      state_path = mkOption {
        type = types.path;
        default = "${cfg.data_dir}/state";
      };
    };
  };

  logging_remote_type = types.submodule {
    options = {
      endpoint = mkOption {type = types.str;};
      organization = mkOption {type = types.str;};
      stream_name = mkOption {type = types.str;};
    };
  };

  logging_type = types.submodule {
    options = {
      deployment_environment = mkOption {type = types.str;};
      remote = mkOption {type = logging_remote_type;};
    };
  };

  teslamate_type =
    types.submodule {options = {url = mkOption {type = types.str;};};};

  tesla_audience_type = types.submodule {
    options = {
      errors = mkOption {type = types.str;};
      locations = mkOption {type = types.str;};
      doors = mkOption {type = types.str;};
      charging = mkOption {type = types.str;};
      private = mkOption {type = types.str;};
    };
  };

  car_type = types.submodule {
    options = {
      id = mkOption {type = types.str;};
      make = mkOption {type = types.enum ["tesla"];};
      name = mkOption {type = types.str;};
      teslamate_id = mkOption {type = types.number;};
      tesla_id = mkOption {type = types.number;};
      teslamate = mkOption {type = teslamate_type;};
      audience = mkOption {type = tesla_audience_type;};
      amber_display = mkOption {
        type = lib.types.nullOr display_type;
        default = null;
      };
    };
  };

  lifx_type = types.submodule {
    options = {broadcast = mkOption {type = types.str;};};
  };

  color_type = types.submodule {
    options = {
      hue = mkOption {type = types.float;};
      saturation = mkOption {type = types.float;};
      brightness = mkOption {type = types.float;};
      kelvin = mkOption {type = types.number;};
    };
  };

  power_color_type = types.submodule {
    options = {
      power = mkOption {type = types.enum ["on" "off"];};
      single = mkOption {type = color_type;};
    };
  };

  light_device_type = types.submodule {
    options = {
      type = mkOption {type = types.enum ["lifx" "debug"];};
      lifx_id = mkOption {type = types.number;};
    };
  };

  light_scene_type = types.submodule {
    options = {
      type = mkOption {type = types.enum ["fixed_color"];};
      value = mkOption {type = power_color_type;};
    };
  };

  light_type = types.submodule {
    options = {
      id = mkOption {type = types.str;};
      device = mkOption {type = light_device_type;};
      topic_substr = mkOption {type = types.str;};
      scenes = mkOption {
        type = types.attrsOf light_scene_type;
        default = {};
      };
      flash_color = mkOption {type = power_color_type;};
    };
  };

  split_type = types.submodule {
    options = {
      id = mkOption {type = types.str;};
      name = mkOption {type = types.str;};
      scenes = mkOption {
        type = types.attrsOf light_scene_type;
        default = {};
      };
      flash_color = mkOption {type = power_color_type;};
      begin = mkOption {type = types.number;};
      number = mkOption {type = types.number;};
    };
  };

  strip_type = types.submodule {
    options = {
      id = mkOption {type = types.str;};
      device = mkOption {type = light_device_type;};
      topic_substr = mkOption {type = types.str;};
      number_of_lights = mkOption {type = types.number;};
      splits = mkOption {type = types.listOf split_type;};
    };
  };

  metric_type = types.submodule {
    options = {
      mqtt_topic = mkOption {type = types.str;};
      influx_topic = mkOption {type = types.str;};
      metric_type = mkOption {
        type = types.enum [
          "shelly_switch_status"
          "shelly_notify"
          "fish_tank"
          "zwave_switch"
          "anavi_temperature"
          "anavi_humidity"
          "zwave_f64"
          "zwave_u8"
        ];
      };
    };
  };

  display_type = types.submodule {
    options = {
      url = mkOption {type = types.str;};
      mac = mkOption {type = types.str;};
    };
  };

  hot_water_type = types.submodule {
    options = {
      amber_display = mkOption {
        type = lib.types.nullOr display_type;
        default = null;
      };
    };
  };

  config_type = types.submodule {
    options = {
      executor = mkOption {
        type = executor_type;
        default = {};
      };
      http = mkOption {
        type = http_type;
        default = {};
      };
      persistent_state = mkOption {
        type = persistent_state_type;
        default = {};
      };
      cars = mkOption {type = types.listOf car_type;};
      logging = mkOption {type = logging_type;};
      lifx = mkOption {type = lifx_type;};
      lights = mkOption {type = types.listOf light_type;};
      strips = mkOption {type = types.listOf strip_type;};
      metrics = mkOption {type = types.listOf metric_type;};
      hot_water = mkOption {
        type = lib.types.nullOr hot_water_type;
        default = null;
      };
    };
  };
in {
  options.services.robotica-backend = {
    enable = mkEnableOption "robotica-backend service";
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

    systemd.services.robotica-backend = {
      wantedBy = ["multi-user.target"];
      after = ["network.target"];
      serviceConfig = {
        User = "robotica";
        ExecStart = "${wrapper}/bin/robotica-backend";
      };
    };
  };
}
