#!/bin/sh
# Installs a bundle from brew/. Re-runs whenever any bundle file or this script
# changes: the trigger inputs are declared in hats.yaml, so no hash comment is
# needed here.
#
# The bundle is chosen with HATS_BREW_BUNDLE; it defaults to `full`, which is
# what a single flat Brewfile used to mean. Keep the case below in step with
# BrewBundle::files() in cli/src/cli.rs — the hook composes for itself rather
# than shelling out to `hats brew`, because it must also filter for Linux.
set -eu

# Homebrew prefix and OS come from hats as HATS_BREW_PREFIX / HATS_OS, so this
# script needs no templating: it is plain sh that runs identically everywhere.
BREW_PREFIX="${HATS_BREW_PREFIX:-/opt/homebrew}"
[ -x "${BREW_PREFIX}/bin/brew" ] && eval "$(${BREW_PREFIX}/bin/brew shellenv)"

BUNDLE="${HATS_BREW_BUNDLE:-full}"
BREW_DIR="${HATS_REPO}/brew"

# Every bundle leads with core: no machine gets a role set without the base set.
case "${BUNDLE}" in
  core)    SETS="core" ;;
  devops)  SETS="core devops" ;;
  pentest) SETS="core pentest" ;;
  dev)     SETS="core dev" ;;
  full)    SETS="core devops pentest dev" ;;
  *)       echo "unknown bundle '${BUNDLE}' (core|devops|pentest|dev|full)"; exit 1 ;;
esac

COMPOSED="$(mktemp)"
trap 'rm -f "${COMPOSED}"' EXIT

for set in ${SETS}; do
  f="${BREW_DIR}/${set}.Brewfile"
  [ -f "${f}" ] || { echo "no bundle file at ${f}"; exit 1; }
  cat "${f}" >> "${COMPOSED}"
  printf '\n' >> "${COMPOSED}"
done

if [ "${HATS_OS:-}" = "darwin" ]; then
  echo "==> brew bundle (${BUNDLE}: ${SETS})"
  # --no-upgrade keeps existing installs fast; drop it to also upgrade each time.
  brew bundle install --no-upgrade --file="${COMPOSED}"
else
  # Linux (dev container): casks and a few formulae are macOS-only. Filter them
  # so `brew bundle` does not abort. The macOS bundles stay the one source of
  # truth rather than being forked per platform.
  echo "==> brew bundle on Linux (${BUNDLE}: ${SETS}) — filtering macOS-only entries"
  LINUX_BREWFILE="$(mktemp)"
  trap 'rm -f "${COMPOSED}" "${LINUX_BREWFILE}"' EXIT
  grep -vE '^[[:space:]]*(cask|mas|cask_args)\b' "${COMPOSED}" \
    | grep -vE '^[[:space:]]*brew "(dockutil|mas|reattach-to-user-namespace|trash|m-cli)"' \
    > "${LINUX_BREWFILE}"
  brew bundle install --no-upgrade --file="${LINUX_BREWFILE}" || {
    echo "!! brew bundle had failures (some Linux formulae may be unavailable) — continuing"
  }
fi
