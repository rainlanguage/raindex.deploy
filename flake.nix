{
  description = "Flake for development workflows.";

  inputs = {
    rainix.url = "github:rainlanguage/rainix";
    rain.url = "github:rainlanguage/rain.cli";
    flake-utils.url = "github:numtide/flake-utils";
    ragenix.url = "github:yaxitech/ragenix";
    deploy-rs.url = "github:serokell/deploy-rs";
    crane.url = "github:ipetkov/crane";

    # Pinned separately so the API host can take a newer CA bundle
    # (cacert >= 3.131.1) without moving the rainix nixpkgs input.
    nixpkgs-cacert.url = "github:NixOS/nixpkgs/f903b88037954e42a165c27b686201c2f242ae28";

    disko.url = "github:nix-community/disko";
    disko.inputs.nixpkgs.follows = "rainix/nixpkgs";

    nixos-anywhere.url = "github:nix-community/nixos-anywhere";
    nixos-anywhere.inputs.nixpkgs.follows = "rainix/nixpkgs";
  };

  outputs =
    {
      self,
      flake-utils,
      rainix,
      rain,
      ragenix,
      deploy-rs,
      crane,
      disko,
      nixos-anywhere,
      nixpkgs-cacert,
      ...
    }:
    let
      configuredHostname = builtins.getEnv "RAINDEX_API_HOSTNAME";
      apiHostname = if configuredHostname == "" then "api.raindex.finance" else configuredHostname;
    in
    {
      nixosConfigurations.raindex-api = rainix.inputs.nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs.cacert = nixpkgs-cacert.legacyPackages.x86_64-linux.cacert;
        specialArgs.raindexEnv = {
          name = "prod";
          virtualHost = apiHostname;
          dataDir = "/mnt/data/raindex-api";
          dataVolumeName = "raindex-api-data";
        };
        modules = [
          disko.nixosModules.disko
          ragenix.nixosModules.default
          ./os.nix
        ];
      };

      deploy = (import ./deploy.nix { inherit deploy-rs self; }).config;
      checks.x86_64-linux = deploy-rs.lib.x86_64-linux.deployChecks self.deploy;
    }
    // flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = rainix.pkgs.${system};
        craneLib = (crane.mkLib pkgs).overrideToolchain rainix.rust-toolchain.${system};
        infraPkgs = import ./infra {
          inherit
            pkgs
            ragenix
            rainix
            system
            ;
        };
        deployPkgs = (import ./deploy.nix { inherit deploy-rs self; }).wrappers {
          inherit pkgs infraPkgs;
          localSystem = system;
        };
        apiRust = pkgs.callPackage ./rust.nix { inherit craneLib; };
      in
      rec {
        packages = rec {

          raindex-prelude = rainix.mkTask.${system} {
            name = "raindex-prelude";
            body = ''
              set -euxo pipefail

              mkdir -p meta;
              forge script --silent ./script/BuildAuthoringMeta.sol;
              rain meta build \
                -i <(cat ./meta/RaindexV7SubParserAuthoringMeta.rain.meta) \
                -m authoring-meta-v2 \
                -t cbor \
                -e deflate \
                -l none \
                -o meta/RaindexV7SubParser.rain.meta \
                ;
            '';
          };

          raindex-rs-test = rainix.mkTask.${system} {
            name = "raindex-rs-test";
            body = ''
              set -euxo pipefail
              cargo test --workspace
            '';
          };

          raindex-ui-components-prelude = rainix.mkTask.${system} {
            name = "raindex-ui-components-prelude";
            body = ''
              set -euxo pipefail

              # Fix linting of generated types
              cd packages/ui-components && npm i && npm run lint
            '';
            additionalBuildInputs = [
              rainix.rust-toolchain.${system}
              rainix.rust-build-inputs.${system}
            ];
          };

          raindex-cli-artifact = rainix.mkTask.${system} {
            name = "raindex-cli-artifact";
            body = ''
              set -euxo pipefail

              OUTPUT_DIR=crates/cli/bin
              ARCHIVE_NAME=raindex-cli.tar.gz
              BINARY_NAME=raindex-cli

              TARGET_TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"

              case "$TARGET_TRIPLE" in
                aarch64-apple-darwin|x86_64-apple-darwin|x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu)
                  ;;
                *)
                  echo "Unsupported host target: $TARGET_TRIPLE" >&2
                  exit 1
                  ;;
              esac

              cargo build --release -p raindex_cli --target "$TARGET_TRIPLE"

              mkdir -p "$OUTPUT_DIR"
              rm -f "$OUTPUT_DIR/$ARCHIVE_NAME"

              cp "target/$TARGET_TRIPLE/release/raindex_cli" "$OUTPUT_DIR/$BINARY_NAME"
              chmod 755 "$OUTPUT_DIR/$BINARY_NAME"
              strip "$OUTPUT_DIR/$BINARY_NAME" || true

              tar -C "$OUTPUT_DIR" -czf "$OUTPUT_DIR/$ARCHIVE_NAME" "$BINARY_NAME"
            '';
          };

          rainix-wasm-artifacts = rainix.mkTask.${system} {
            name = "rainix-wasm-artifacts";
            body = ''
              set -euxo pipefail

              cargo build --profile release-wasm --target wasm32-unknown-unknown --lib -p raindex_js_api
            '';
          };

          rainix-wasm-test = rainix.mkTask.${system} {
            name = "rainix-wasm-test";
            body = ''
              set -euxo pipefail

              CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER='wasm-bindgen-test-runner' cargo test --target wasm32-unknown-unknown --lib -p raindex_quote -p raindex_bindings -p raindex_js_api -p raindex_common
            '';
          };

          rainix-wasm-browser-test = rainix.mkTask.${system} {
            name = "rainix-wasm-browser-test";
            body = ''
              set -euxo pipefail

              cd crates/common
              wasm-pack test --headless --chrome --features browser-tests -- leadership::wasm_tests
              wasm-pack test --headless --chrome --features browser-tests -- scheduler::wasm::wasm_tests
              wasm-pack test --headless --chrome --features browser-tests -- status::wasm::wasm_tests
              wasm-pack test --headless --chrome --features browser-tests -- retry::wasm_tests
              wasm-pack test --headless --chrome --features browser-tests -- raindex_client::local_db::wasm_tests
            '';
            additionalBuildInputs = [
              pkgs.wasm-pack
            ];
          };

          js-install = rainix.mkTask.${system} {
            name = "js-install";
            body = ''
              set -euxo pipefail
              cd packages/raindex
              npm install --no-check
            '';
          };

          build-js-bindings = rainix.mkTask.${system} {
            name = "build-js-bindings";
            body = ''
              set -euxo pipefail
              cd packages/raindex
              npm run build
            '';
          };

          test-js-bindings = rainix.mkTask.${system} {
            name = "test-js-bindings";
            body = ''
              set -euxo pipefail
              cd packages/raindex
              npm install --no-check
              npm run build
              npm test
            '';
          };

          # Re-export the rain CLI from the pinned `rain` flake input so
          # `nix shell .#rain-cli` resolves to the exact version recorded
          # in flake.lock (cached on the rainlanguage Cachix). build-meta.sh
          # uses this instead of `nix shell github:rainlanguage/rain.cli`
          # so a rain.cli main move can't race the cache push.
          rain-cli = rain.defaultPackage.${system};

          # Terraform-compatible infrastructure tooling from the flake-pinned
          # nixpkgs used by rainix.
          inherit (pkgs) opentofu actionlint;

          raindex-api = apiRust.package;
          raindex-api-clippy = apiRust.clippy;

          inherit (infraPkgs)
            tfInit
            tfPlan
            tfApply
            tfImport
            tfDestroy
            tfEditVars
            ;

          bootstrap = rainix.mkTask.${system} {
            name = "bootstrap-raindex-api-nixos";
            additionalBuildInputs = infraPkgs.buildInputs ++ [
              nixos-anywhere.packages.${system}.default
              pkgs.openssh
              pkgs.gnused
            ];
            body = ''
              ${infraPkgs.resolveIp}
              ssh_opts="-o IgnoreUnknown=UseKeychain -o StrictHostKeyChecking=no -o ConnectTimeout=5 -i $identity"

              nixos-anywhere --flake ".#raindex-api" \
                --option pure-eval false \
                --ssh-option "IgnoreUnknown=UseKeychain" \
                --ssh-option "IdentityFile=$identity" \
                --target-host "root@$host_ip" "$@"

              echo "Waiting for the NixOS host to return..."
              retries=0
              until ssh $ssh_opts "root@$host_ip" true 2>/dev/null; do
                retries=$((retries + 1))
                if [ "$retries" -ge 60 ]; then
                  echo "Host did not return after 5 minutes" >&2
                  exit 1
                fi
                sleep 5
              done

              new_key=$(ssh $ssh_opts "root@$host_ip" \
                cat /etc/ssh/ssh_host_ed25519_key.pub | awk '{print $1 " " $2}')
              if ! echo "$new_key" | grep -qE '^ssh-ed25519 [A-Za-z0-9+/=]+$'; then
                echo "Invalid SSH host key returned by provisioned host" >&2
                exit 1
              fi

              sed -i.bak -E '/host =/s|"ssh-ed25519 [A-Za-z0-9+/=_-]+"|"'"$new_key"'"|' keys.nix
              rm -f keys.nix.bak
              echo "Updated keys.nix with the provisioned host key; commit it before deploying."
            '';
          };

          resolveIp = pkgs.writeShellApplication {
            name = "resolve-ip";
            runtimeInputs = infraPkgs.buildInputs;
            text = ''
              ${infraPkgs.resolveIp}
              echo "$host_ip"
            '';
          };

          remote = pkgs.writeShellApplication {
            name = "raindex-api-remote";
            runtimeInputs = infraPkgs.buildInputs ++ [ pkgs.openssh ];
            text = ''
              ${infraPkgs.resolveIp}
              exec ssh -i "$identity" "root@$host_ip" "$@"
            '';
          };

        }
        // deployPkgs
        // rainix.packages.${system};

        devShells.default = pkgs.mkShell {
          packages = [
            packages.raindex-prelude
            packages.raindex-rs-test
            packages.rainix-wasm-artifacts
            packages.rainix-wasm-test
            packages.rainix-wasm-browser-test
            packages.js-install
            packages.build-js-bindings
            packages.test-js-bindings
            rain.defaultPackage.${system}
            packages.raindex-ui-components-prelude
            packages.raindex-cli-artifact
            packages.opentofu
            packages.actionlint
          ];

          inherit (rainix.devShells.${system}.default) shellHook buildInputs nativeBuildInputs;
        };
        devShells.webapp-shell = pkgs.mkShell {
          packages = with pkgs; [ nodejs_22 ];
          inherit (rainix.devShells.${system}.default) buildInputs nativeBuildInputs;
        };

        # Re-export rainix's slim devShells so workflows can reference them
        # via `.#X-shell` and pick up the flake.lock-pinned rainix rev
        # instead of live `github:rainlanguage/rainix#X-shell` (which
        # bypasses flake.lock and tracks rainix main).
        devShells.wasm-shell = rainix.devShells.${system}.wasm-shell;
        devShells.subgraph-shell = rainix.devShells.${system}.subgraph-shell;
        devShells.sol-shell = rainix.devShells.${system}.sol-shell;
        devShells.rust-shell = rainix.devShells.${system}.rust-shell;
      }
    );

}
