{
  pkgs,
  lib,
  modulesPath,
  # CA bundle for the host, from its own nixpkgs pin (see flake.nix).
  cacert,
  raindexEnv ? { },
  ...
}:

let
  inherit (import ./keys.nix) roles;

  env = {
    name = "prod";
    virtualHost = "api.raindex.finance";
    dataDir = "/mnt/data/raindex-api";
    dataVolumeName = "raindex-api-data";
  }
  // raindexEnv;

  serviceDefinitions = import ./services.nix;
  enabledServices = lib.filterAttrs (_: value: value.enabled) serviceDefinitions;

  mkService =
    name: cfg:
    let
      executable = "/nix/var/nix/profiles/per-service/${name}/bin/${cfg.bin}";
    in
    {
      description = "Raindex ${cfg.bin} (${env.name}/${name})";
      wantedBy = [ "multi-user.target" ];
      wants = [ "network-online.target" ];
      after = [ "network-online.target" ];
      restartIfChanged = false;
      stopIfChanged = false;

      unitConfig = {
        "X-OnlyManualStart" = true;
        ConditionPathExists = executable;
        RequiresMountsFor = env.dataDir;
      };

      environment = {
        ROCKET_ADDRESS = "127.0.0.1";
        ROCKET_PORT = "8000";
        RAINDEX_LOCAL_DB_PATH = "${env.dataDir}/market-data.sqlite";
        RAINDEX_LOG_DIR = "${env.dataDir}/logs";
        RAINDEX_TRUSTED_PROXY_IP_HEADER = "X-Real-IP";
        RUST_LOG = "raindex_rest_api=info,raindex_common=info,raindex_quote=info,rocket=warn,warn";
      };

      serviceConfig = {
        User = "raindex-api";
        Group = "raindex";
        ExecStart = executable;
        Restart = "always";
        RestartSec = 5;
        NoNewPrivileges = true;
        PrivateTmp = true;
        ProtectHome = true;
        ProtectSystem = "strict";
        ReadWritePaths = [ env.dataDir ];
      };
    };
in
{
  imports = [
    (modulesPath + "/virtualisation/digital-ocean-config.nix")
    (modulesPath + "/profiles/qemu-guest.nix")
    ./disko.nix
  ];

  boot.loader.grub = {
    efiSupport = true;
    efiInstallAsRemovable = true;
  };

  networking.useDHCP = lib.mkForce false;

  services = {
    cloud-init = {
      enable = true;
      network.enable = true;
      settings = {
        datasource_list = [
          "ConfigDrive"
          "Digitalocean"
        ];
        datasource.ConfigDrive = { };
        datasource.Digitalocean = { };
        cloud_init_modules = [
          "seed_random"
          "bootcmd"
          "write_files"
          "growpart"
          "resizefs"
          "set_hostname"
          "update_hostname"
          "set_password"
        ];
        cloud_config_modules = [
          "ssh-import-id"
          "keyboard"
          "runcmd"
          "disable_ec2_metadata"
        ];
        cloud_final_modules = [
          "write_files_deferred"
          "puppet"
          "chef"
          "ansible"
          "mcollective"
          "salt_minion"
          "reset_rmc"
          "scripts_per_once"
          "scripts_per_boot"
          "scripts_user"
          "ssh_authkey_fingerprints"
          "keys_to_console"
          "install_hotplug"
          "phone_home"
          "final_message"
        ];
      };
    };

    openssh = {
      enable = true;
      settings = {
        PasswordAuthentication = false;
        PermitRootLogin = "prohibit-password";
      };
    };

    nginx = {
      enable = true;
      recommendedTlsSettings = true;
      recommendedProxySettings = true;
      recommendedOptimisation = true;
      recommendedGzipSettings = true;

      appendHttpConfig = ''
        limit_req_zone $binary_remote_addr zone=api:10m rate=10r/s;
      '';

      virtualHosts.${env.virtualHost} = {
        enableACME = true;
        forceSSL = true;

        extraConfig = ''
          add_header X-Content-Type-Options "nosniff" always;
          add_header X-Frame-Options "DENY" always;
          add_header Referrer-Policy "strict-origin-when-cross-origin" always;
          client_max_body_size 1m;
        '';

        locations = {
          "~* \\.(php|asp|aspx|jsp|cgi)$".return = "444";
          "~* ^/(containers|_ignition|vendor|public/index)".return = "444";
          "/" = {
            proxyPass = "http://127.0.0.1:8000";
            extraConfig = ''
              limit_req zone=api burst=20 nodelay;
              limit_req_status 429;
            '';
          };
        };
      };
    };

    journald.extraConfig = ''
      MaxRetentionSec=14day
      SystemMaxUse=512M
    '';
  };

  security.acme = {
    acceptTerms = true;
    defaults.email = "ops@raindex.finance";
  };

  # Serve the pinned CA bundle at the paths security.pki would populate from
  # the rainix nixpkgs cacert. Overriding the etc entries avoids an overlay,
  # which would rebuild everything that depends on cacert.
  environment.etc =
    lib.genAttrs
      [
        "ssl/certs/ca-certificates.crt"
        "ssl/certs/ca-bundle.crt"
        "pki/tls/certs/ca-bundle.crt"
      ]
      (_: {
        source = lib.mkForce "${cacert}/etc/ssl/certs/ca-bundle.crt";
      })
    // {
      "ssl/trust-source".source = lib.mkForce "${cacert.p11kit}/etc/ssl/trust-source";
    };

  assertions = [
    {
      assertion = lib.versionOlder pkgs.cacert.version cacert.version;
      message = "rainix nixpkgs cacert ${pkgs.cacert.version} has caught up with the pinned cacert ${cacert.version}; remove the nixpkgs-cacert input, the /etc CA overrides and the nix-daemon CURL_CA_BUNDLE override.";
    }
  ];

  networking.firewall = {
    enable = true;
    allowedTCPPorts = [
      22
      80
      443
    ];
  };

  fileSystems."/mnt/data" = {
    device = "/dev/disk/by-id/scsi-0DO_Volume_${env.dataVolumeName}";
    fsType = "ext4";
  };

  # Remote Rust builds can briefly exceed the production VM's physical RAM.
  # Keep swap on the replaceable root disk; application data remains on the
  # persistent volume above.
  swapDevices = [
    {
      device = "/var/lib/swapfile";
      size = 8192;
    }
  ];

  nix = {
    settings = {
      experimental-features = [
        "nix-command"
        "flakes"
      ];
      auto-optimise-store = true;
      download-buffer-size = 268435456;
    };
    gc = {
      automatic = true;
      dates = "weekly";
      options = "--delete-older-than 30d";
    };
  };

  users = {
    groups.raindex = { };
    users = {
      root.openssh.authorizedKeys.keys = roles.ssh;
      raindex-api = {
        isSystemUser = true;
        group = "raindex";
      };
    };
  };

  programs.bash.interactiveShellInit = "set -o vi";

  systemd.tmpfiles.rules = [
    "d ${env.dataDir} 0775 root raindex -"
    "d ${env.dataDir}/logs 0775 raindex-api raindex -"
  ];
  systemd.services = lib.mapAttrs mkService enabledServices // {
    # Nix itself trusts the /etc bundle above. nixpkgs also sets CURL_CA_BUNDLE
    # here from the read-only security.pki.caBundle (the rainix cacert); point
    # it at /etc too so the unit does not reference the old bundle.
    nix-daemon.environment.CURL_CA_BUNDLE = lib.mkForce "/etc/ssl/certs/ca-certificates.crt";
  };

  environment.systemPackages = with pkgs; [
    bat
    curl
    htop
    sqlite
    zellij
  ];

  system.activationScripts.per-service-profiles.text = "mkdir -p /nix/var/nix/profiles/per-service";

  system.stateVersion = "24.11";
}
