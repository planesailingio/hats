#!/bin/sh
# Installs the Brewfile. Re-runs whenever the Brewfile or this script changes:
# the trigger inputs are declared in hats.yaml, so no hash comment is needed here.
set -eu

# Homebrew prefix and OS come from hats as HATS_BREW_PREFIX / HATS_OS, so this
# script needs no templating: it is plain sh that runs identically everywhere.
BREW_PREFIX="${HATS_BREW_PREFIX:-/opt/homebrew}"
[ -x "${BREW_PREFIX}/bin/brew" ] && eval "$(${BREW_PREFIX}/bin/brew shellenv)"

BREWFILE="${HATS_REPO}/Brewfile"
[ -f "${BREWFILE}" ] || { echo "no Brewfile at ${BREWFILE}"; exit 1; }

if [ "${HATS_OS:-}" = "darwin" ]; then
  echo "==> brew bundle (${BREWFILE})"
  # --no-upgrade keeps existing installs fast; drop it to also upgrade each time.
  brew bundle install --no-upgrade --file="${BREWFILE}"
else
  # Linux (dev container): casks and a few formulae are macOS-only. Filter them
  # so `brew bundle` does not abort. The macOS Brewfile stays the one source of
  # truth rather than being forked per platform.
  echo "==> brew bundle on Linux — filtering macOS-only entries"
  LINUX_BREWFILE="$(mktemp)"
  trap 'rm -f "${LINUX_BREWFILE}"' EXIT
  grep -vE '^[[:space:]]*(cask|mas|cask_args)\b' "${BREWFILE}" \
    | grep -vE '^[[:space:]]*brew "(dockutil|mas|reattach-to-user-namespace|trash|m-cli)"' \
    > "${LINUX_BREWFILE}"
  brew bundle install --no-upgrade --file="${LINUX_BREWFILE}" || {
    echo "!! brew bundle had failures (some Linux formulae may be unavailable) — continuing"
  }
fi
