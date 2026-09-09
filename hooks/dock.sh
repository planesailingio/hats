#!/bin/sh
# Rebuilds the Dock to an EXACT pinned set via dockutil.
#
# Declared in hats.yaml with `os: [darwin]` and an onchange trigger on this file,
# so editing the app list below re-applies it on the next `hats apply`. The
# runtime OS guard below is belt and braces, and keeps this script safe to run
# by hand with `hats hooks run dock --force`.
#
# Idempotent: a full --remove all followed by re-adding each app every time.
# Right-side stacks (Downloads) are left untouched. No sudo needed.
set -eu

if [ "${HATS_OS:-$(uname -s | tr '[:upper:]' '[:lower:]')}" != "darwin" ]; then
  echo "==> not macOS; skipping Dock rebuild."
  exit 0
fi

BREW_PREFIX="${HATS_BREW_PREFIX:-/opt/homebrew}"
[ -x "${BREW_PREFIX}/bin/brew" ] && eval "$(${BREW_PREFIX}/bin/brew shellenv)"

if ! command -v dockutil >/dev/null 2>&1; then
  echo "==> dockutil not installed yet (the brew-bundle hook runs first); skipping."
  exit 0
fi

# Pinned apps, in Dock order. Editing this list re-runs the hook next apply.
APPS="
/Applications/Visual Studio Code.app
/Applications/Firefox.app
"

echo "==> Rebuilding Dock"
dockutil --remove all --no-restart >/dev/null 2>&1 || true

# Add each app that actually exists; a missing app is skipped, not an error.
echo "${APPS}" | while IFS= read -r app; do
  [ -n "${app}" ] || continue
  if [ -d "${app}" ]; then
    dockutil --add "${app}" --no-restart >/dev/null 2>&1 \
      && echo "   + ${app}" \
      || echo "   ! failed to add ${app}"
  else
    echo "   - skipped (not installed): ${app}"
  fi
done

# Single restart at the end so the items do not revert.
killall Dock 2>/dev/null || true
echo "==> Dock rebuilt."
