{self}: {
  lib,
  pkgs,
  config,
  ...
}:
with lib; let
  system = pkgs.system;
  robotica-slint = self.packages.${system}.robotica-slint;

  cfg = config.services.robotica-slint;
  sound_path = cfg.config.audio.sound_path;
  init = pkgs.writeShellScript "init" ''
    for i in {1..10}; do
      mpc repeat on && exit
      sleep 1
    done
    mpc repeat on
  '';
  pre_speech = pkgs.writeShellScript "pre-speech" ''
    mkdir -p "$HOME/cache"
    hash="$(echo -n "$*" | md5sum | awk '{print $1}')"
    filename="$HOME/cache/$hash.mp3"
    if ! test -f "$filename"; then
      ${pkgs.python310Packages.gtts}/bin/gtts-cli --output "$filename" "$*"
    fi
  '';
  speech = pkgs.writeShellScript "speech" ''
    mkdir -p "$HOME/cache"
    hash="$(echo -n "$*" | md5sum | awk '{print $1}')"
    filename="$HOME/cache/$hash.mp3"
    if ! test -f "$filename"; then
      ${pkgs.espeak}/bin/espeak -ven+f5 -k5 -s 130 -w /tmp/out.wav "$*"
      ${pkgs.alsa-utils}/bin/aplay ${sound_path}/start.wav
      ${pkgs.alsa-utils}/bin/aplay -q /tmp/out.wav
      ${pkgs.alsa-utils}/bin/aplay ${sound_path}/middle.wav
      ${pkgs.alsa-utils}/bin/aplay -q /tmp/out.wav
      ${pkgs.alsa-utils}/bin/aplay ${sound_path}/stop.wav
    else
      ${pkgs.alsa-utils}/bin/aplay ${sound_path}/start.wav
      ${pkgs.mpg123}/bin/mpg123 $filename
      ${pkgs.alsa-utils}/bin/aplay ${sound_path}/middle.wav
      ${pkgs.mpg123}/bin/mpg123 $filename
      ${pkgs.alsa-utils}/bin/aplay ${sound_path}/stop.wav
    fi
  '';
  audio_programs_type = types.submodule {
    options = {
      init = mkOption {
        type = types.listOf types.str;
        default = ["${init}"];
      };
      set_volume = mkOption {
        type = types.listOf types.str;
        default = ["${pkgs.alsa-utils}/bin/amixer" "set" "Speaker"];
      };
      mpc = mkOption {
        type = types.listOf types.str;
        default = ["${pkgs.mpc-cli}/bin/mpc"];
      };
      pre_say = mkOption {
        type = types.listOf types.str;
        default = ["${pre_speech}"];
      };
      say = mkOption {
        type = types.listOf types.str;
        default = ["${speech}"];
      };
      play_sound = mkOption {
        type = types.listOf types.str;
        default = ["${pkgs.alsa-utils}/bin/aplay"];
      };
    };
  };
  audio_targets_type = types.attrsOf types.str;

  audio_type = types.submodule {
    options = {
      programs = mkOption {
        type = audio_programs_type;
        default = {};
      };
      messages_enabled_subtopic = mkOption {type = types.str;};
      topic_substr = mkOption {type = types.str;};
      targets = mkOption {type = audio_targets_type;};
      sound_path = mkOption {type = types.path;};
    };
  };

  controller_str_type = types.enum ["light"];

  controller_light_type = types.submodule {
    options = {
      type = mkOption {type = types.enum ["light"];};
      topic_substr = mkOption {type = types.str;};
      action = mkOption {type = types.enum ["toggle"];};
      scene = mkOption {type = types.str;};
    };
  };

  controller_music_type = types.submodule {
    options = {
      type = mkOption {type = types.enum ["music"];};
      topic_substr = mkOption {type = types.str;};
      action = mkOption {type = types.enum ["toggle"];};
      play_list = mkOption {type = types.str;};
    };
  };

  controller_switch_type = types.submodule {
    options = {
      type = mkOption {type = types.enum ["switch"];};
      topic_substr = mkOption {type = types.str;};
      action = mkOption {type = types.enum ["toggle"];};
    };
  };

  controller_zwave_type = types.submodule {
    options = {
      type = mkOption {type = types.enum ["zwave"];};
      topic_substr = mkOption {type = types.str;};
      action = mkOption {type = types.enum ["toggle"];};
    };
  };

  controller_tasmota_type = types.submodule {
    options = {
      type = mkOption {type = types.enum ["tasmota"];};
      topic_substr = mkOption {type = types.str;};
      action = mkOption {type = types.enum ["toggle"];};
      power_postfix = mkOption {type = types.str;};
    };
  };

  hdmi_type = types.submodule {
    options = {
      type = mkOption {type = types.enum ["hdmi"];};
      topic_substr = mkOption {type = types.str;};
      input = {type = types.int;};
      output = {type = types.int;};
    };
  };

  controller_type = mkOptionType {
    name = "controller-type";
    check = x:
      if !isAttrs x
      then throw "Controller type should be attrs"
      else if x.type == "light"
      then controller_light_type.check x
      else if x.type == "music"
      then controller_music_type.check x
      else if x.type == "switch"
      then controller_switch_type.check x
      else if x.type == "zwave"
      then controller_zwave_type.check x
      else if x.type == "tasmota"
      then controller_tasmota_type.check x
      else if x.type == "hdmi"
      then hdmi_type.check x
      else throw "Unknown type ${x.type}";
  };
  # types.oneOf [ controller_light_type controller_music_type ];

  button_type = types.submodule {
    options = {
      id = mkOption {type = types.str;};
      title = mkOption {type = types.str;};
      controller = mkOption {type = controller_type;};
      icon = mkOption {type = types.str;};
    };
  };

  button_row_type = types.submodule {
    options = {
      id = mkOption {type = types.str;};
      title = mkOption {type = types.str;};
      buttons = mkOption {type = types.listOf button_type;};
    };
  };

  screen_on = pkgs.writeShellScript "screen-on" ''
    echo 0 > /sys/class/backlight/rpi_backlight/bl_power
  '';

  screen_off = pkgs.writeShellScript "screen-off" ''
    echo 4 > /sys/class/backlight/rpi_backlight/bl_power
  '';

  ui_programs_type = types.submodule {
    options = {
      turn_screen_on = mkOption {
        type = types.listOf types.str;
        default = ["${screen_on}"];
      };
      turn_screen_off = mkOption {
        type = types.listOf types.str;
        default = ["${screen_off}"];
      };
    };
  };

  ui_type = types.submodule {
    options = {
      instance = mkOption {type = types.str;};
      programs = mkOption {
        type = ui_programs_type;
        default = {};
      };
      number_per_row = mkOption {type = types.int;};
      backlight_on_time = mkOption {
        type = types.int;
        default = 0;
      };
      buttons = mkOption {type = types.listOf button_row_type;};
    };
  };

  persistent_state_type = types.submodule {
    options = {state_path = mkOption {type = types.path;};};
  };

  config_type = types.submodule {
    options = {
      audio = mkOption {type = audio_type;};
      ui = mkOption {type = ui_type;};
      persistent_state = mkOption {
        type = persistent_state_type;
        default = {};
      };
    };
  };
in {
  imports = [./sway.nix];

  options.services.robotica-slint = {
    enable = mkEnableOption "robotica-slint service";
    config = mkOption {type = config_type;};
    secrets_path = mkOption {type = types.path;};
    user = mkOption {type = types.str;};
  };

  config = mkIf cfg.enable (let
    wrapper = pkgs.writeShellScript "robotica-slint" ''
      trap "echo Exiting wrapper...; exit $?" INT TERM EXIT KILL
      export SLINT_CONFIG_FILE=${config_file}
      export SLINT_SECRETS_FILE=${cfg.secrets_path}
      mkdir -p "${cfg.config.persistent_state.state_path}"
      exec "${robotica-slint}/bin/robotica-slint" > /tmp/slint.log 2>&1
    '';
    config_file = pkgs.writeTextFile {
      name = "robotica-slint.yaml";
      text = lib.generators.toYAML {} cfg.config;
    };
  in {
    services.sway = {
      enable = true;
      program = "${wrapper}";
      user = cfg.user;
    };
  });
}
