#!/usr/bin/env bash
# Render one harness scene to a PNG using a real headless Chromium.
#
# Provisions Chromium + DejaVu fonts + Node via nix, then runs the driver
# (screenshot.mjs). This is the one-command entry point:
#
#   packages/ui-components/harness/screenshot.sh <scene> <out.png> [width] [height]
#
# Examples:
#   harness/screenshot.sh toast-error-decoded /tmp/toast.png
#   harness/screenshot.sh toast-error-decoded /tmp/toast.png 760 240
#
# Scene names are the keys in scenes/index.ts. List them by rendering an unknown
# scene (the page prints the available scenes).
set -euo pipefail

SCENE="${1:?usage: screenshot.sh <scene> <out.png> [width] [height]}"
OUT="${2:?usage: screenshot.sh <scene> <out.png> [width] [height]}"
WIDTH="${3:-900}"
HEIGHT="${4:-360}"

HARNESS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$HARNESS_DIR/../../.." && pwd)"

# Chromium: prefer an explicit override, then a binary already on PATH, then
# nix-provision one. `nixpkgs#chromium` on a cold machine is a large first-time
# download; a pre-set $CHROMIUM or a system chromium avoids it.
if [ -z "${CHROMIUM:-}" ]; then
	if command -v chromium >/dev/null 2>&1; then
		CHROMIUM="$(command -v chromium)"
	elif command -v chromium-browser >/dev/null 2>&1; then
		CHROMIUM="$(command -v chromium-browser)"
	elif command -v google-chrome-stable >/dev/null 2>&1; then
		CHROMIUM="$(command -v google-chrome-stable)"
	else
		echo "[harness] resolving chromium via nix (first run downloads it)..."
		CHROMIUM="$(nix build --no-link --print-out-paths "nixpkgs#chromium")/bin/chromium"
	fi
fi
export CHROMIUM

# DejaVu fonts so text renders in headless Chromium (this closure is small).
if [ -z "${HARNESS_FONT_DIR:-}" ]; then
	HARNESS_FONT_DIR="$(nix build --no-link --print-out-paths "nixpkgs#dejavu_fonts")/share/fonts/truetype"
fi
export HARNESS_FONT_DIR

# Node/npm come from the repo's webapp devShell. Run the driver
# there so `vite` resolves from the repo-root node_modules.
exec nix develop "$REPO_ROOT#webapp-shell" --command \
	node "$HARNESS_DIR/screenshot.mjs" \
	--scene "$SCENE" --out "$OUT" --width "$WIDTH" --height "$HEIGHT"
