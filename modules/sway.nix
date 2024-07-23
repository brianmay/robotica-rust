{ config, pkgs, lib, ... }:

with lib;

let cfg = config.services.sway;
in {
  options.services.sway.enable =
    mkEnableOption (lib.mdDoc "sway kiosk service");

  options.services.sway.user = mkOption {
    type = types.str;
    default = "demo";
    description = lib.mdDoc ''
      User to log-in as.
    '';
  };

  options.services.sway.extraArguments = mkOption {
    type = types.listOf types.str;
    default = [ ];
    defaultText = literalExpression "[]";
    description =
      lib.mdDoc "Additional command line arguments to pass to sway.";
    example = [ "-d" ];
  };

  options.services.sway.program = mkOption {
    type = types.path;
    default = "${pkgs.xterm}/bin/xterm";
    defaultText = literalExpression ''"''${pkgs.xterm}/bin/xterm"'';
    description = lib.mdDoc ''
      Program to run in sway.
    '';
  };

  config = mkIf cfg.enable {
    environment.etc."sway.conf" = {
      text = ''
        exec "${cfg.program}"
        default_border none
      '';
    };

    # The service is partially based off of the one provided in the
    # sway wiki at
    # https://github.com/Hjdskes/sway/wiki/Starting-sway-on-boot-with-systemd.
    systemd.services."sway-tty1" = {
      enable = true;
      after = [
        "systemd-user-sessions.service"
        "plymouth-start.service"
        "plymouth-quit.service"
        "systemd-logind.service"
        "getty@tty1.service"
      ];
      before = [ "graphical.target" ];
      wants =
        [ "dbus.socket" "systemd-logind.service" "plymouth-quit.service" ];
      wantedBy = [ "graphical.target" ];
      conflicts = [ "getty@tty1.service" ];

      restartIfChanged = false;
      unitConfig.ConditionPathExists = "/dev/tty1";
      serviceConfig = {
        ExecStart = ''
          ${pkgs.sway}/bin/sway \
            -c /etc/sway.conf \
            ${escapeShellArgs cfg.extraArguments}
        '';
        User = cfg.user;

        IgnoreSIGPIPE = "no";

        # Log this user with utmp, letting it show up with commands 'w' and
        # 'who'. This is needed since we replace (a)getty.
        UtmpIdentifier = "%n";
        UtmpMode = "user";
        # A virtual terminal is needed.
        TTYPath = "/dev/tty1";
        TTYReset = "yes";
        TTYVHangup = "yes";
        TTYVTDisallocate = "yes";
        # Fail to start if not controlling the virtual terminal.
        StandardInput = "tty-fail";
        StandardOutput = "journal";
        StandardError = "journal";
        # Set up a full (custom) user session for the user, required by sway.
        PAMName = "sway";
      };
    };

    security.polkit.enable = true;

    security.pam.services.sway.text = ''
      auth    required pam_unix.so nullok
      account required pam_unix.so
      session required pam_unix.so
      session required pam_env.so conffile=/etc/pam/environment readenv=0
      session required ${config.systemd.package}/lib/security/pam_systemd.so
    '';

    hardware.opengl.enable = mkDefault true;

    systemd.targets.graphical.wants = [ "sway-tty1.service" ];

    systemd.defaultUnit = "graphical.target";
  };

  meta.maintainers = with lib.maintainers; [ matthewbauer ];

}
