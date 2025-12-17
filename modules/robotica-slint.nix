{ self }:
{
  lib,
  pkgs,
  config,
  ...
}:
let
  inherit (lib)
    types
    mkOption
    mkEnableOption
    mkIf
    ;

  system = pkgs.stdenv.hostPlatform.system;

  cfg = config.services.robotica-slint;
  robotica-slint = cfg.package;
  sound_path = cfg.config.audio.sound_path;
  piperVoice = {
    onnx = pkgs.fetchurl {
      url = "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_GB/semaine/medium/en_GB-semaine-medium.onnx";
      hash = "sha256-1tq287kttD6j94x/INyOrbR6HxXYocnUUc88zSAaL2Y=";
      # hash = lib.fakeHash;
    };
    json = pkgs.fetchurl {
      url = "https://huggingface.co/rhasspy/piper-voices/raw/main/en/en_GB/semaine/medium/en_GB-semaine-medium.onnx.json";
      hash = "sha256-ZCXcuHhoQEO3fXcrFzrgBthqWDsRAwPt2ki4Q47O5e4=";
      # hash = lib.fakeHash;
    };
  };
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
    tmp1="$HOME/cache/$hash.1.wav"
    tmp2="$HOME/cache/$hash.2.wav"
    filename="$HOME/cache/$hash.wav"
    if ! test -f "$filename"; then
      echo "$*" | ${pkgs.piper-tts}/bin/piper --model ${cfg.voice.onnx_file} --config ${cfg.voice.json_file} --output_file "$tmp1"
      ${pkgs.sox}/bin/sox -G "$tmp1" -r 44100 -c 2 "$tmp2"
      rm "$tmp1"
      mv "$tmp2" "$filename"
    fi
  '';
  speech = pkgs.writeShellScript "speech" ''
    mkdir -p "$HOME/cache"
    hash="$(echo -n "$*" | md5sum | awk '{print $1}')"
    filename="$HOME/cache/$hash.wav"
    if ! test -f "$filename"; then
      ${pkgs.espeak}/bin/espeak-ng -ven+f5 -k5 -s 130 -w /tmp/out.wav "$*"
      ${pkgs.alsa-utils}/bin/aplay ${sound_path}/start.wav
      ${pkgs.alsa-utils}/bin/aplay -q /tmp/out.wav
      ${pkgs.alsa-utils}/bin/aplay ${sound_path}/middle.wav
      ${pkgs.alsa-utils}/bin/aplay -q /tmp/out.wav
      ${pkgs.alsa-utils}/bin/aplay ${sound_path}/stop.wav
    else
      ${pkgs.alsa-utils}/bin/aplay ${sound_path}/start.wav
      ${pkgs.alsa-utils}/bin/aplay -q "$filename"
      ${pkgs.alsa-utils}/bin/aplay ${sound_path}/middle.wav
      ${pkgs.alsa-utils}/bin/aplay -q "$filename"
      ${pkgs.alsa-utils}/bin/aplay ${sound_path}/stop.wav
    fi
  '';
  audio_programs_type = types.submodule {
    options = {
      init = mkOption {
        type = types.listOf types.str;
        default = [ "${init}" ];
      };
      set_volume = mkOption {
        type = types.listOf types.str;
        default = [
          "${pkgs.alsa-utils}/bin/amixer"
          "set"
          "Speaker"
        ];
      };
      mpc = mkOption {
        type = types.listOf types.str;
        default = [ "${pkgs.mpc-cli}/bin/mpc" ];
      };
      pre_say = mkOption {
        type = types.listOf types.str;
        default = [ "${pre_speech}" ];
      };
      say = mkOption {
        type = types.listOf types.str;
        default = [ "${speech}" ];
      };
      play_sound = mkOption {
        type = types.listOf types.str;
        default = [ "${pkgs.alsa-utils}/bin/aplay" ];
      };
    };
  };
  audio_targets_type = types.attrsOf types.str;

  audio_type = types.submodule {
    options = {
      programs = mkOption {
        type = audio_programs_type;
        default = { };
      };
      messages_enabled_subtopic = mkOption { type = types.str; };
      topic_substr = mkOption { type = types.str; };
      targets = mkOption { type = audio_targets_type; };
      sound_path = mkOption { type = types.path; };
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
        default = [ "${screen_on}" ];
      };
      turn_screen_off = mkOption {
        type = types.listOf types.str;
        default = [ "${screen_off}" ];
      };
    };
  };

  ui_type = types.submodule {
    options = {
      programs = mkOption {
        type = ui_programs_type;
        default = { };
      };
      ui_config_name = mkOption { type = types.str; };
      number_per_row = mkOption { type = types.int; };
      backlight_on_time = mkOption {
        type = types.int;
        default = 0;
      };
    };
  };

  persistent_state_type = types.submodule {
    options = {
      state_path = mkOption { type = types.path; };
    };
  };

  voice_type = types.submodule {
    options = {
      onnx_file = mkOption { type = types.path; };
      json_file = mkOption { type = types.path; };
    };
  };

  config_type = types.submodule {
    options = {
      audio = mkOption { type = audio_type; };
      ui = mkOption { type = ui_type; };
      persistent_state = mkOption {
        type = persistent_state_type;
        default = { };
      };
    };
  };
in
{
  options.services.robotica-slint = {
    enable = mkEnableOption "robotica-slint service";
    package = lib.mkPackageOption self.packages.${system} "robotica-slint" { };
    config = mkOption { type = config_type; };
    secrets_path = mkOption { type = types.path; };
    user = mkOption { type = types.str; };
    voice = mkOption {
      type = voice_type;
      default = {
        onnx_file = piperVoice.onnx;
        json_file = piperVoice.json;
      };
    };
  };

  config = mkIf cfg.enable (
    let
      wrapper = pkgs.writeShellScript "robotica-slint" ''
        trap "echo Exiting wrapper...; exit $?" INT TERM EXIT KILL
        export SLINT_CONFIG_FILE=${config_file}
        export SLINT_SECRETS_FILE=${cfg.secrets_path}
        mkdir -p "${cfg.config.persistent_state.state_path}"
        exec "${robotica-slint}/bin/robotica-slint" > /tmp/slint.log 2>&1
      '';
      config_file = pkgs.writeTextFile {
        name = "robotica-slint.yaml";
        text = lib.generators.toYAML { } cfg.config;
      };
    in
    {
      services.cage = {
        enable = true;
        program = "${wrapper}";
        user = cfg.user;
      };
    }
  );
}
