#!/bin/sh
# bootstrap.sh — cold-start a fresh Mac. Dependency-free POSIX sh.
#
# Usage on a new machine:
#   sh -c "$(curl -fsLS https://raw.githubusercontent.com/planesailingio/hats/main/bootstrap.sh)"
#
# Order: Xcode CLT (git) -> Homebrew -> hats -> hats init.
# Every step is idempotent, so re-running on a configured machine is a no-op.
#
# `hats init` takes it from there: it clones this repo into ~/.hats/repo, asks
# which groups of files to manage, walks you through your profiles, and asks how
# secrets should be fetched. Nothing is written to your home directory until you
# run `hats apply`, and `hats plan` shows exactly what that would change first.
set -eu

TAP="${HATS_TAP:-planesailingio/tools}"
log() { printf '\033[1;34m==>\033[0m %s\n' "$1"; }
warn() { printf '\033[1;33m==>\033[0m %s\n' "$1"; }

# 1. Xcode Command Line Tools (provides git, clang, make) ─────────────────────
if ! xcode-select -p >/dev/null 2>&1; then
  log "Installing Xcode Command Line Tools — accept the GUI prompt."
  xcode-select --install || true
  # Wait for the install to finish before continuing.
  until xcode-select -p >/dev/null 2>&1; do sleep 15; done
else
  log "Xcode Command Line Tools present."
fi

# 2. Homebrew ─────────────────────────────────────────────────────────────────
if ! command -v brew >/dev/null 2>&1; then
  log "Installing Homebrew."
  NONINTERACTIVE=1 /bin/bash -c \
    "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
else
  log "Homebrew present."
fi

# Put brew on PATH for this script (arch-aware: Apple Silicon vs Intel).
if [ -x /opt/homebrew/bin/brew ]; then
  eval "$(/opt/homebrew/bin/brew shellenv)"
elif [ -x /usr/local/bin/brew ]; then
  eval "$(/usr/local/bin/brew shellenv)"
else
  warn "brew not found on PATH after install — aborting."; exit 1
fi

# 3. hats ─────────────────────────────────────────────────────────────────────
if ! command -v hats >/dev/null 2>&1; then
  log "Installing hats from ${TAP}."
  brew install "${TAP}/hats"
else
  log "hats present ($(hats --version))."
fi

# 4. Set up ───────────────────────────────────────────────────────────────────
log "Running hats init."
hats init

cat <<'EOF'

Next:
  hats plan          see what would change in your home directory
  hats apply         write the files and run the setup hooks
  hats doctor        check this machine has everything hats needs

Then open a new terminal (set its font to a Nerd Font) and run `profile` to
switch client context. `hats secrets fetch` pulls tokens down if you configured
a secrets provider.
EOF
