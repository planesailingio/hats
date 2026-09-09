#!/bin/sh
# Runs after files change (trigger declared in hats.yaml). Sets brew zsh as the login shell, builds bat theme cache,
# installs pinned runtimes. Idempotent; safe to re-run.
set -eu

# Homebrew prefix and OS come from hats as HATS_BREW_PREFIX / HATS_OS, so this
# script needs no templating: it is plain sh that runs identically everywhere.
BREW_PREFIX="${HATS_BREW_PREFIX:-/opt/homebrew}"
[ -x "${BREW_PREFIX}/bin/brew" ] && eval "$(${BREW_PREFIX}/bin/brew shellenv)"

# 1. Make brew zsh the default shell (only if not already). ────────────────────
BREW_ZSH="${BREW_PREFIX}/bin/zsh"
if [ -x "${BREW_ZSH}" ]; then
  if ! grep -qx "${BREW_ZSH}" /etc/shells 2>/dev/null; then
    echo "==> Adding ${BREW_ZSH} to /etc/shells (needs sudo)"
    echo "${BREW_ZSH}" | sudo tee -a /etc/shells >/dev/null || true
  fi
  if [ "${SHELL:-}" != "${BREW_ZSH}" ]; then
    echo "==> chsh to ${BREW_ZSH}"
    chsh -s "${BREW_ZSH}" || true
  fi
fi

# 2. bat: build theme cache so "Catppuccin Mocha" resolves (also used by delta). ─
if command -v bat >/dev/null 2>&1; then
  echo "==> bat cache --build"
  bat cache --build >/dev/null 2>&1 || true
fi

# 3. mise: install the globally pinned runtimes. ───────────────────────────────
if command -v mise >/dev/null 2>&1; then
  echo "==> mise install"
  mise install || true
fi

# 4. tenv: install latest OpenTofu + Terraform so `tofu`/`terraform` are on PATH. ──
# (terramate is NOT a tenv-managed tool — it comes from brew via the Brewfile.)
if command -v tenv >/dev/null 2>&1; then
  echo "==> tenv install latest OpenTofu + Terraform"
  tenv tofu install latest >/dev/null 2>&1 || true
  tenv tofu use latest >/dev/null 2>&1 || true
fi

# 5. atuin: import existing shell history once. ────────────────────────────────
if command -v atuin >/dev/null 2>&1; then
  atuin import auto >/dev/null 2>&1 || true
fi
