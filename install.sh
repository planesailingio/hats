#!/bin/sh
# install.sh — get `hats` onto a macOS or Linux machine. Dependency-free POSIX sh.
#
#   curl -fsSL https://planesailingio.github.io/hats/install.sh | sh
#
# Installs Homebrew if it is missing, asking first, then installs hats from the
# tap. That is all it does: no `hats init`, no files written to your home
# directory. Run `hats init` yourself when you are ready.
#
# Set CI=1 to install Homebrew without asking — for containers and pipelines.
#
# For a cold-start Mac that has no Xcode Command Line Tools either, use
# bootstrap.sh instead: it handles those, then runs the wizard.
set -eu

TAP="${HATS_TAP:-planesailingio/tools}"

log()  { printf '\033[1;34m==>\033[0m %s\n' "$1"; }
warn() { printf '\033[1;33m==>\033[0m %s\n' "$1" >&2; }
die()  { printf '\033[1;31m==>\033[0m %s\n' "$1" >&2; exit 1; }

# 1. Platform ─────────────────────────────────────────────────────────────────
# Homebrew supports macOS and Linux and nothing else, so neither do we. Naming
# the platform we saw makes a WSL or BSD run fail with something actionable.
OS="$(uname -s)"
case "${OS}" in
  Darwin) PLATFORM=macos ;;
  Linux)  PLATFORM=linux ;;
  *)      die "unsupported platform: ${OS}. hats installs on macOS and Linux only." ;;
esac
log "Platform: ${PLATFORM} ($(uname -m))."

# 2. Homebrew ─────────────────────────────────────────────────────────────────
# `confirm` reads from the terminal rather than stdin: this script is meant to
# be piped into sh, which leaves stdin holding the script itself, so a plain
# `read` would silently consume the rest of the file.
confirm() {
  if [ "${CI:-}" = "1" ]; then
    log "CI=1, installing without asking."
    return 0
  fi
  # Opening it, not testing it with -r: in a container /dev/tty exists and looks
  # readable but fails to open, and `read < /dev/tty` would then die untidily
  # under `set -e` instead of printing this. The open runs in a subshell because
  # a redirection failure on a special built-in like `:` is fatal to the shell
  # itself in POSIX sh -- the subshell absorbs that and just reports non-zero.
  if ! (: < /dev/tty) 2>/dev/null; then
    die "no terminal to ask on. Re-run with CI=1 to install Homebrew unattended."
  fi
  printf '%s [y/N] ' "$1"
  read -r reply < /dev/tty
  case "${reply}" in
    [yY] | [yY][eE][sS]) return 0 ;;
    *) return 1 ;;
  esac
}

brew_shellenv() {
  # The three prefixes brew uses: Apple Silicon, Intel Mac, and Linux.
  for candidate in /opt/homebrew/bin/brew /usr/local/bin/brew /home/linuxbrew/.linuxbrew/bin/brew; do
    if [ -x "${candidate}" ]; then
      eval "$("${candidate}" shellenv)"
      return 0
    fi
  done
  return 1
}

if command -v brew >/dev/null 2>&1; then
  log "Homebrew present ($(brew --version | head -n 1))."
elif brew_shellenv; then
  log "Homebrew present but off PATH; using $(command -v brew)."
else
  warn "Homebrew is not installed. hats is distributed as a Homebrew formula."
  if [ "${PLATFORM}" = linux ] && [ "$(id -u)" = 0 ]; then
    die "Homebrew refuses to install as root. Re-run as an ordinary user with sudo rights."
  fi
  if ! confirm "Install Homebrew now?"; then
    die "Homebrew is required. Nothing was installed."
  fi

  log "Installing Homebrew."
  NONINTERACTIVE=1 /bin/bash -c \
    "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

  brew_shellenv || die "brew is still not on PATH after installing. Aborting."
  log "Homebrew installed ($(brew --version | head -n 1))."
fi

# 3. hats ─────────────────────────────────────────────────────────────────────
# Homebrew 6 refuses to auto-load formulae from an untrusted third-party tap and
# warns on every operation. Installing from the tap is already a decision to
# trust it, so say so once and keep the output clean. Guarded because `brew
# trust` does not exist on older Homebrew, and non-fatal because the install
# works without it.
if brew trust --help >/dev/null 2>&1; then
  log "Trusting the ${TAP} tap."
  brew trust "${TAP}" >/dev/null 2>&1 || warn "could not trust ${TAP}; continuing."
fi

if command -v hats >/dev/null 2>&1; then
  log "hats present ($(hats --version)). Upgrading if the tap is ahead."
  brew upgrade "${TAP}/hats" 2>/dev/null || log "Already at the latest release."
else
  log "Installing hats from ${TAP}."
  brew install "${TAP}/hats"
fi

log "Installed: $(hats --version)."

cat <<'EOF'

Next:
  hats init          clone the dotfiles, define your hats, pick a secrets backend
  hats plan          see what that would change in your home directory
  hats apply         write the files and run the setup hooks

`hats init` is interactive and writes nothing until you run `hats apply`.
EOF
