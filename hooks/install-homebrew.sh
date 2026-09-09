#!/bin/sh
# Runs ONCE, BEFORE any file is written, on a machine that has never applied.
# Covers the path where someone ran `hats init` directly rather than the
# bootstrap script, so Homebrew may not exist yet. Idempotent.
set -eu

if ! command -v brew >/dev/null 2>&1; then
  echo "==> Installing Homebrew"
  NONINTERACTIVE=1 /bin/bash -c \
    "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
fi

# Homebrew prefix and OS come from hats as HATS_BREW_PREFIX / HATS_OS, so this
# script needs no templating: it is plain sh that runs identically everywhere.
BREW_PREFIX="${HATS_BREW_PREFIX:-/opt/homebrew}"
[ -x "${BREW_PREFIX}/bin/brew" ] && eval "$(${BREW_PREFIX}/bin/brew shellenv)"
