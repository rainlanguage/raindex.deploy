# Raindex API infrastructure

This is the production deployment path for the Raindex market API: DigitalOcean
resources managed by OpenTofu, encrypted local state, NixOS installed with
nixos-anywhere, separate deploy-rs system and service profiles, and a persistent
block volume.

The NixOS configuration provides:

- nginx TLS termination, security headers, and a 10 request/second per-IP
  ingress limit with a burst of 20;
- a systemd-managed API that starts after reboot and restarts on failure;
- SQLite and daily JSON logs under `/mnt/data/raindex-api` so deployments do not
  discard indexed data;
- 14 retained application log files, 14-day/512 MiB journald bounds, a host
  firewall, automatic ACME renewal, and weekly Nix garbage collection.

## One-time production provisioning

The default hostname is `api.raindex.finance`. Export `RAINDEX_API_HOSTNAME` for
every Nix command if production uses another name.

OpenTofu creates the DigitalOcean SSH key named `raindex-op` from
`infra/raindex-op.pub` and installs it on the new host. `keys.nix` reads the
same file, keeping provisioning and NixOS authorization in sync. The private key
is kept outside the repository at `~/.ssh/raindex-op`; pass
`-i ~/.ssh/raindex-op` to the commands below.

### Operator access

The production host accepts only keys listed in `roles.ssh` in `keys.nix`. To
grant another person access:

1. Add their public key as a named entry under `keys`.
2. Add that name to `roles.ssh`.
3. Run `nix run .#deployNixos -- -i ~/.ssh/raindex-op`.

Removing the name from `roles.ssh` and deploying NixOS revokes access. Add the
name to `roles.state` when that person must deploy, and to `roles.vars` only
when they must operate the DigitalOcean infrastructure.

1. Run `nix run .#tfEditVars`, set the DigitalOcean token, and commit the
   generated `infra/terraform.tfvars.age`.
2. Run `nix run .#tfInit`, `nix run .#tfPlan`, and `nix run .#tfApply`. The plan
   command is a preview; apply creates and consumes a fresh ephemeral plan.
   Commit the generated `infra/terraform.tfstate.age`; plaintext variables and
   state are deleted by the wrappers and ignored by Git.
3. Point the API hostname at `nix run .#resolveIp`.
4. Run `nix run .#bootstrap`. This converts the temporary Ubuntu image to the
   declared NixOS system and replaces the fail-closed placeholder in `keys.nix`
   with the host's Ed25519 key. Verify and commit that change.
5. Run `nix run .#deployAll`. Subsequent API-only releases use
   `nix run .#deployService -- rest-api`.

The operator commands accept `-i /path/to/private-key` before other arguments;
otherwise they use `~/.ssh/id_ed25519`.

## GitHub deployment

Configure the production environment with:

- `RAINDEX_API_SSH_KEY`: the private CI key already listed in `keys.nix` and
  able to decrypt the committed encrypted Terraform state;
- optional `RAINDEX_API_HOSTNAME`: only when the hostname differs from
  `api.raindex.finance`;
- optional `CACHIX_AUTH_TOKEN`.

The manual `Deploy Raindex API` workflow validates the flake and OpenTofu,
resolves the reserved IP from encrypted state, checks the committed SSH host
key, deploys through deploy-rs, and requires `/health/detailed` to succeed.

DNS is deliberately managed outside this stack. Neither a DigitalOcean API token
nor plaintext Terraform state is needed by the deployment workflow.
