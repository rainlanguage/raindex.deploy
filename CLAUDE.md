Read and follow @AGENTS.md.

# Merge / Publish / Deploy runbook

Deploy repo: a source change regenerates `src/generated/candidate/` and merges
without deploying. Deploying and publishing happen together at a release
(below). `LibRaindexDeploy` is the candidate. The start-block, `networks.json`
and `subgraph.yaml` tests read the released RaindexV7 from the newest entry of
the generated `LibRaindexV7Released`, else `LibRaindexV6Released` (releases cut
before the rename). Address/codehash churn is routine; don't stop to ask about
it.

## Merging a PR

- Run `gh` via `nix shell nixpkgs#gh --command gh` (not on global PATH).
- Required checks (`main` ruleset): `test`, `subgraph-test`, `test-js-bindings`,
  `wasm-artifacts`, `wasm-browser-test`, `wasm-test`. `rainix-sol / *`,
  `git-clean`, `Deploy-Preview-Push` and the ~30-min vercel preview do NOT gate
  a merge. Still understand every red before merging; never `--admin` over an
  unexplained failure.
- A merge-gate hook requires a
  `Reviewed <9-char-head-sha>: <substantive
  review>` PR comment bound to the
  **current** head; re-post after any new commit.
- raindex is `REVIEW_REQUIRED` and the author can't self-approve →
  `gh pr merge <n> --merge --admin` (needs explicit owner authorization per-PR).
  rainix merges with plain `gh pr merge <n> --merge`.
- Merge with the default merge commit. Re-trigger CI with a fresh empty commit +
  push. Move a branch with `git checkout -f -B <branch> <ref>`. Commit with
  `git -c commit.gpgsign=false commit --no-verify`. (Squash/rebase merges, CLI
  workflow re-runs and history rewrites are hook-blocked — these are the allowed
  equivalents; don't spell the blocked forms out or you'll trip those hooks.)

## Source changes

Regenerate artifacts — run the `rainix-copy-artifacts` step sequence (see that
workflow) in `nix develop github:rainlanguage/rainix#sol-shell`. Stage ALL
changed artifacts or `git-clean` drifts red. Nothing is deployed.

## Releasing (deploy + publish)

1. Deploy each changed suite:
   `gh workflow run manual-sol-artifacts.yaml --ref main -f suite=<x>`. One
   dispatch deploys all seven chains. **Never run two deploys concurrently** —
   they share one deployer EOA and race the nonce; serialize dispatches. A
   `completed/failure` run still deployed if the log has
   `ONCHAIN EXECUTION COMPLETE & SUCCESSFUL` (only `--verify` failed).
2. One PR: bump `foundry.toml [external.package].version`, run
   `forge script ./script/Build.sol --sig 'cutRelease()'` (freezes `candidate/`
   as `src/generated/<tag>/`), and if RaindexV7 moved run
   `script/build-start-blocks.sh` (`LibRaindexV7StartBlock`, `networks.json`,
   `subgraph.yaml`).
3. Merge, then push tag `sol-v<version>`. Via rainix `rainix-tag-release.yaml`
   (`package-release.yaml`) it re-runs the fork suite and `release-guard`
   (version == tag, frozen dir byte-identical to `candidate/`, clean tree, no
   later tag frozen), then publishes `raindex-deploy~<version>`.

`src/generated/<tag>/` snapshots are append-only — the
`frozen-snapshots-append-only` gate reds any PR that edits or deletes one.
