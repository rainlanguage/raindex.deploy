rec {
  keys = {
    raindex-op = builtins.replaceStrings [ "\n" ] [ "" ] (builtins.readFile ./infra/raindex-op.pub);
    ci = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPTd2zKSwHgWegi290EiK5nYp1Wp4+x2fDYqFxbd0WLN";

    # Replaced by `nix run .#bootstrap` after nixos-anywhere provisions the
    # production host. Deploys trust this committed key and fail before then.
    host = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBiL05XFbmXMcwoNBhndBHb69QVDMIhAGJkLALzsblCM";
  };

  roles = with keys; {
    state = [
      raindex-op
      ci
    ];
    vars = [ raindex-op ];
    # Add another person's public key above, then add its name here to grant
    # root SSH access. Add it to `state` only if they also need to deploy, and
    # to `vars` only if they must operate the DigitalOcean infrastructure.
    ssh = [
      raindex-op
      ci
    ];
  };
}
