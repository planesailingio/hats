# core — the base set. Every bundle includes it; no bundle is installed without it.
#
# Rule for this file: would you want it on ANY machine you sit at, whatever the
# job is that day? Shell, git, file/text/JSON wrangling, system inspection,
# secrets handling, comms, fonts. Nothing role-specific.
#
# Composed by `hats brew install <bundle>` — see brew/README.md.

# ── Taps ──────────────────────────────────────────────────────────────────────
# NOTE: verified 2026-09-09 — every formula in these bundles resolves from
# homebrew/core; none of these taps is required. Deletion candidates.
tap "mike-engel/jwt-cli"            # jwt-cli
tap "acmeco/tap"               # personal formulae

# ── Shell, prompt & modern CLI core ───────────────────────────────────────────
brew "zsh"                          # brew zsh, newer than system
brew "starship"                     # cross-shell prompt (replaces oh-my-zsh theme)
brew "zsh-autosuggestions"          # fish-style history suggestions
brew "zsh-syntax-highlighting"      # command syntax highlighting
brew "zsh-completions"              # extra completion definitions
brew "fzf"                          # fuzzy finder
brew "fzf-tab"                      # tab completion rendered through fzf
brew "zoxide"                       # smarter cd, frecency jumps
brew "atuin"                        # searchable, synced shell history
brew "eza"                          # ls replacement, icons + git
brew "bat"                          # cat replacement, syntax highlight
brew "ripgrep"                      # rg, grep replacement
brew "fd"                           # find replacement
brew "sd"                           # sed-like find/replace
brew "tree"                         # classic tree (eza --tree covers most cases)
brew "broot"                        # interactive tree navigator
brew "yazi"                         # TUI file manager
brew "superfile"                    # alt TUI file manager
brew "less"                         # newer pager than system
brew "tlrc"                         # tldr client (Rust) — replaces deprecated `tldr`
brew "glow"                         # render markdown in terminal
brew "viddy"                        # modern `watch` with history/diff
brew "watch"                        # classic watch

# ── Git tooling ───────────────────────────────────────────────────────────────
brew "git"
brew "git-lfs"
brew "git-delta"                    # syntax-highlighting diff pager
brew "difftastic"                   # structural (AST) diff
brew "lazygit"                      # git TUI
brew "tig"                          # git TUI/pager
brew "gh"                           # GitHub CLI
brew "gibo"                         # .gitignore boilerplate fetcher
brew "git-secrets"                  # prevent committing secrets
brew "gitleaks"                     # secret scanner (pre-commit hook; hats CI uses it)
brew "pre-commit"                   # git hook framework

# ── System / process / disk inspection ────────────────────────────────────────
brew "btop"                         # resource monitor (replaces htop day-to-day)
brew "htop"                         # kept, familiar fallback
brew "procs"                        # ps replacement
brew "dust"                         # du replacement
brew "duf"                          # df replacement
brew "ncdu"                         # ncurses disk usage
brew "gping"                        # ping with a graph
brew "hyperfine"                    # command-line benchmarking
brew "smartmontools"
brew "fio"

# ── Data wrangling (JSON/YAML/HTTP) ───────────────────────────────────────────
brew "jq"
brew "yq"
brew "yh"                           # YAML highlighter
brew "yj"                           # convert yaml/toml/json
brew "jless"                        # JSON/YAML pager
brew "gron"                         # greppable JSON
brew "jql"                          # JSON query
brew "json-table"
brew "httpie"                       # human HTTP client
brew "xh"                           # faster httpie-compatible client (Rust)
brew "jwt-cli"                      # decode/verify JWTs — pairs with jq day to day

# ── Linters that every bundle needs (config formats, not source code) ─────────
brew "yamllint"                     # here, not dev: devops lints k8s/CI YAML too
brew "shellcheck"                   # shell linter (hats lint shells out to it)
brew "shfmt"                        # shell formatter

# ── Runtime & env management ──────────────────────────────────────────────────
brew "mise"                         # polyglot runtime manager (python/node/go/…)
brew "direnv"                       # per-directory env
# NOTE: tenv moved to devops (terraform/tofu versions); tfenv+tofuenv gone.

# ── Secrets, keys & encryption (hats itself depends on these) ─────────────────
brew "sops"                         # encrypted secrets in files
brew "age"                          # modern file encryption
brew "age-plugin-yubikey"           # age identities in a YubiKey PIV slot (hats secrets envelope)
brew "gnupg"
brew "opensc"                       # PKCS#11 / PIV — YubiKey support
brew "softhsm"
brew "bitwarden-cli"                # `bw` — hats secret source
brew "rbw"                          # Rust Bitwarden agent (offline-capable)

# ── Media / docs / misc utilities ─────────────────────────────────────────────
brew "imagemagick"
brew "ghostscript"
brew "poppler"
brew "exiftool"
brew "p7zip"
brew "magic-wormhole"               # secure file transfer
brew "wget"
brew "yt-dlp"
brew "just"                         # command runner (Makefile alternative)
brew "watchexec"                    # run commands on file change
brew "cowsay"
brew "lolcat"
brew "fortune"

# ── Homebrew management ───────────────────────────────────────────────────────
brew "mas"                          # Mac App Store CLI
brew "dockutil"                     # scriptable Dock management (used by hooks/dock.sh)

# ── Nerd Fonts (glyphs for starship / eza / lazygit) ──────────────────────────
cask "font-jetbrains-mono-nerd-font"
cask "font-meslo-lg-nerd-font"

# ── GUI apps: editor, terminal, browser, comms ────────────────────────────────
cask "visual-studio-code"
cask "cursor"                       # AI code editor (pinned to Dock)
cask "ghostty"                      # modern GPU terminal (recommended for Nerd Font glyphs)
cask "firefox"
cask "drawio"
cask "bitwarden"
cask "keybase"
cask "tailscale-app"
cask "nextcloud"
cask "slack"
cask "signal"
cask "telegram"
cask "microsoft-teams"
cask "spotify"
cask "vlc"
