{
  pkgs,
  ragenix,
  rainix,
  system,
}:

let
  buildInputs = [
    pkgs.opentofu
    pkgs.rage
    pkgs.jq
    ragenix.packages.${system}.default
  ];

  tfState = "infra/terraform.tfstate";
  tfVars = "infra/terraform.tfvars";
  tfSecretVars = "infra/zz_secret.auto.tfvars";
  tfPlanFile = "infra/tfplan";

  parseIdentity = ''
    set -eo pipefail
    umask 077

    identity=~/.ssh/id_ed25519
    if [ "''${1:-}" = "-i" ]; then
      identity="$2"
      shift 2
    fi
  '';

  decryptState = ''
    if [ -f ${tfState}.age ]; then
      rage -d -i "$identity" ${tfState}.age > ${tfState}
    fi
  '';

  encryptState = ''
    if [ -f ${tfState} ]; then
      nix eval --raw --file ${../.}/keys.nix roles.state \
        --apply 'builtins.concatStringsSep "\n"' \
        | rage -e -R /dev/stdin -o ${tfState}.age ${tfState}
    fi
  '';

  decryptVars = ''
    if [ -f ${tfVars}.age ]; then
      rage -d -i "$identity" ${tfVars}.age > ${tfVars}
    elif [ ! -f ${tfVars} ]; then
      echo "${tfVars}.age is missing; run nix run .#tfEditVars first" >&2
      exit 1
    fi
    if [ -n "''${TF_VAR_do_token:-}" ]; then
      printf 'do_token = "%s"\n' "$TF_VAR_do_token" > ${tfSecretVars}
    fi
  '';

  encryptVars = ''
    nix eval --raw --file ${../.}/keys.nix roles.vars \
      --apply 'builtins.concatStringsSep "\n"' \
      | rage -e -R /dev/stdin -o ${tfVars}.age ${tfVars}
  '';

  cleanup = "rm -f ${tfState} ${tfState}.backup ${tfVars} ${tfSecretVars}";
  cleanupWithPlan = "${cleanup} ${tfPlanFile}";

  preamble = ''
    ${parseIdentity}
    on_exit() { ${cleanupWithPlan}; }
    trap on_exit EXIT
    ${decryptVars}
  '';

  preambleWithEncrypt = ''
    ${parseIdentity}
    on_exit() {
      ${encryptState}
      ${cleanupWithPlan}
    }
    trap on_exit EXIT
    ${decryptVars}
  '';

  resolveIp = ''
    ${parseIdentity}
    trap 'rm -f ${tfState}' EXIT
    ${decryptState}
    if [ ! -s ${tfState} ]; then
      echo "encrypted Terraform state is missing; provision infrastructure first" >&2
      exit 1
    fi
    host_ip=$(jq -r '.outputs.reserved_ip.value // empty' ${tfState})
    if [ -z "$host_ip" ] || [ "$host_ip" = "null" ]; then
      echo "production infrastructure is not present in Terraform state" >&2
      exit 1
    fi
    rm -f ${tfState}
  '';
in
{
  inherit buildInputs parseIdentity resolveIp;

  tfInit = rainix.mkTask.${system} {
    name = "tf-init";
    additionalBuildInputs = buildInputs;
    body = ''
      ${preamble}
      tofu -chdir=infra init "$@"
    '';
  };

  tfPlan = rainix.mkTask.${system} {
    name = "tf-plan";
    additionalBuildInputs = buildInputs;
    body = ''
      ${preamble}
      ${decryptState}
      tofu -chdir=infra plan "$@"
    '';
  };

  tfApply = rainix.mkTask.${system} {
    name = "tf-apply";
    additionalBuildInputs = buildInputs;
    body = ''
      ${preambleWithEncrypt}
      ${decryptState}
      tofu -chdir=infra plan -out=tfplan
      tofu -chdir=infra apply "$@" tfplan
    '';
  };

  tfImport = rainix.mkTask.${system} {
    name = "tf-import";
    additionalBuildInputs = buildInputs;
    body = ''
      ${preambleWithEncrypt}
      ${decryptState}
      tofu -chdir=infra import "$@"
    '';
  };

  tfDestroy = rainix.mkTask.${system} {
    name = "tf-destroy";
    additionalBuildInputs = buildInputs;
    body = ''
      ${preambleWithEncrypt}
      ${decryptState}
      tofu -chdir=infra destroy "$@"
    '';
  };

  tfEditVars = rainix.mkTask.${system} {
    name = "tf-edit-vars";
    additionalBuildInputs = buildInputs;
    body = ''
      ${parseIdentity}
      on_exit() { rm -f ${tfVars}; }
      trap on_exit EXIT

      if [ -f ${tfVars}.age ]; then
        rage -d -i "$identity" ${tfVars}.age > ${tfVars}
      else
        cp ${tfVars}.example ${tfVars}
      fi
      ''${EDITOR:-vi} ${tfVars}
      ${encryptVars}
    '';
  };
}
