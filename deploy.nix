{ deploy-rs, self }:

let
  system = "x86_64-linux";
  inherit (deploy-rs.lib.${system}) activate;
  profileBase = "/nix/var/nix/profiles/per-service";
  apiPackage = self.packages.${system}.raindex-api;
  serviceDefinitions = import ./services.nix;
  enabledServices = builtins.attrNames (
    builtins.removeAttrs serviceDefinitions (
      builtins.filter (name: !serviceDefinitions.${name}.enabled) (builtins.attrNames serviceDefinitions)
    )
  );

  mkServiceProfile =
    name:
    activate.custom apiPackage (
      builtins.concatStringsSep " && " [
        "systemctl stop ${name} || true"
        "systemctl restart ${name}"
      ]
    );

  mkProfile = name: {
    path = mkServiceProfile name;
    profilePath = "${profileBase}/${name}";
  };

  serviceProfiles = builtins.listToAttrs (
    map (name: {
      inherit name;
      value = mkProfile name;
    }) enabledServices
  );
in
{
  config.nodes.raindex-api = {
    hostname = builtins.getEnv "DEPLOY_HOST";
    sshUser = "root";
    user = "root";
    profilesOrder = [ "system" ] ++ enabledServices;
    profiles = {
      system.path = activate.nixos self.nixosConfigurations.raindex-api;
    }
    // serviceProfiles;
  };

  wrappers =
    {
      pkgs,
      infraPkgs,
      localSystem,
    }:
    let
      deployInputs = infraPkgs.buildInputs ++ [ deploy-rs.packages.${localSystem}.deploy-rs ];
      preamble = ''
        ${infraPkgs.resolveIp}
        export DEPLOY_HOST="$host_ip"
        export NIX_SSHOPTS="-o IgnoreUnknown=UseKeychain -i $identity"
        ssh_flag="--ssh-opts=-o IgnoreUnknown=UseKeychain -i $identity"
      '';
      deployFlags = if localSystem == "x86_64-linux" then "" else "--skip-checks --remote-build";
    in
    {
      deployNixos = pkgs.writeShellApplication {
        name = "deploy-nixos";
        runtimeInputs = deployInputs;
        text = ''
          ${preamble}
          deploy ${deployFlags} ''${ssh_flag:+"$ssh_flag"} .#raindex-api.system \
            -- --impure "$@"
        '';
      };

      deployService = pkgs.writeShellApplication {
        name = "deploy-service";
        runtimeInputs = deployInputs;
        text = ''
          ${preamble}
          profile="''${1:?usage: deploy-service <profile>}"
          shift
          deploy ${deployFlags} ''${ssh_flag:+"$ssh_flag"} ".#raindex-api.$profile" \
            -- --impure "$@"
        '';
      };

      deployAll = pkgs.writeShellApplication {
        name = "deploy-all";
        runtimeInputs = deployInputs;
        text = ''
          ${preamble}
          deploy ${deployFlags} ''${ssh_flag:+"$ssh_flag"} .#raindex-api \
            -- --impure "$@"
        '';
      };
    };
}
