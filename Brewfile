# Brewfile — managed by chezmoi, applied via `brew bundle`
# Regenerate a snapshot with: brew bundle dump --describe --force --file=~/.local/share/chezmoi/home/Brewfile
# Curated by hand — grouped, commented. Do not blindly overwrite with a raw dump.

# ── Taps ──────────────────────────────────────────────────────────────────────
tap "hashicorp/tap"
tap "aquasecurity/trivy"
tap "go-task/tap"
tap "goreleaser/tap"
tap "azure/kubelogin"
tap "carvel-dev/carvel"
tap "cloudflare/cloudflare"
tap "siderolabs/tap"
tap "turbot/tap"
tap "wpscanteam/tap"
tap "mike-engel/jwt-cli"
tap "ankitpokhrel/jira-cli"
tap "auth0/auth0-cli"
tap "stripe/stripe-cli"
tap "netbirdio/tap"
tap "sass/sass"
tap "acmeco/tap"
tap "globex/tap"

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
brew "gitleaks"                     # secret scanner (pre-commit hook)
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
brew "grpcurl"

# ── Version managers (consolidated) ───────────────────────────────────────────
brew "mise"                         # polyglot runtime manager (python/node/go/…)
brew "tenv"                         # tofu/terraform/terragrunt version manager (replaces tfenv+tofuenv)
brew "direnv"                       # per-directory env
# NOTE: tfenv and tofuenv intentionally removed — replaced by tenv.

# ── IaC / Terraform-OpenTofu ──────────────────────────────────────────────────
brew "terraform-docs"
brew "terraform-inventory"
brew "terraformer"


# ── Containers / Kubernetes ───────────────────────────────────────────────────
brew "kubernetes-cli", link: false  # kubectl — link: false avoids clashing with Docker Desktop's kubectl
brew "k9s"
brew "kubectx"                      # kubectx/kubens
brew "kubie"                        # per-shell kube context isolation
brew "kubeseal"
brew "kustomize"
brew "helm"
brew "helmfile"
brew "helmsman"
brew "krew"
brew "stern"                        # multi-pod log tailing
brew "kubespy"
brew "kube-linter"
brew "skaffold"
brew "kompose"
brew "kind"
brew "talosctl"
brew "cilium-cli"
brew "velero"
brew "argo"
brew "argocd"
brew "crane"
brew "ollama"
brew "opencode"
brew "skopeo"
brew "oras"
brew "dive"                         # inspect image layers
brew "podman"

# ── Cloud CLIs ────────────────────────────────────────────────────────────────
brew "awscli"
brew "awsume"                       # AWS role assumption
brew "awslogs"
brew "aws-sam-cli"
brew "aws-cdk"
brew "aws-nuke"
brew "awsweeper"
brew "azure-cli"
brew "cloudflared"
brew "cloudflare-wrangler"                     # Cloudflare Workers
brew "coder"

# ── Security / supply-chain / DevSecOps ───────────────────────────────────────
brew "trivy"                        # vuln/misconfig scanner
brew "grype"                        # vuln scanner
brew "syft"                         # SBOM generator
brew "trufflehog"                   # secret scanner
brew "cosign"                       # sign/verify artifacts & images
brew "osv-scanner"                  # dependency vuln scanner (OSV)
brew "checkov"                      # IaC static analysis
brew "kics"                         # IaC static analysis
brew "tfsec"                        # terraform security scanner
brew "terrascan"                    # IaC security scanner
brew "kube-bench"                   # CIS Kubernetes benchmark
brew "kubescape"                    # k8s security posture
brew "semgrep"                      # multi-language SAST
brew "hadolint"                     # Dockerfile linter
brew "shellcheck"                   # shell linter
brew "shfmt"                        # shell formatter
brew "actionlint"                   # GitHub Actions linter
brew "zizmor"                       # GitHub Actions security auditor
brew "yamllint"
brew "golangci-lint"
brew "sops"                         # encrypted secrets in files
brew "age"                          # modern file encryption
brew "age-plugin-yubikey"           # age identities held in a YubiKey PIV slot (hats secrets envelope)
brew "ssh-audit"                    # SSH server/client auditing
brew "mkcert"                       # local trusted TLS certs
brew "step"                         # smallstep CA / cert tooling
brew "sslscan"
brew "nmap"
brew "rustscan"
brew "sqlmap"
brew "jwt-cli"
brew "gnupg"
brew "opensc"
brew "softhsm"
brew "bitwarden-cli"                # `bw` — chezmoi secret source
brew "rbw"                          # Rust Bitwarden agent (offline-capable)

# ── Languages / runtimes (managed loosely; mise handles versions) ─────────────
brew "go"
brew "rust"


# ── Media / docs / misc utilities ─────────────────────────────────────────────
brew "imagemagick"
brew "ghostscript"
brew "poppler"
brew "exiftool"
brew "p7zip"
brew "magic-wormhole"               # secure file transfer
brew "wget"
brew "httrack"
brew "yt-dlp"
brew "structurizr-cli"              # C4 architecture diagrams
brew "just"                         # command runner (Makefile alternative)
brew "watchexec"                    # run commands on file change
brew "goreleaser"
brew "cowsay"
brew "lolcat"
brew "fortune"

# ── Homebrew management ───────────────────────────────────────────────────────
brew "mas"                          # Mac App Store CLI (needed for `mas` entries below)
brew "dockutil"                     # scriptable Dock management (used by run_onchange_after_45-dock)

# ── Nerd Fonts (glyphs for starship / eza / lazygit) ──────────────────────────
cask "font-jetbrains-mono-nerd-font"
cask "font-meslo-lg-nerd-font"

# ── GUI apps (casks) ──────────────────────────────────────────────────────────
cask "visual-studio-code"
cask "cursor"                       # AI code editor (pinned to Dock)
cask "ghostty"                      # modern GPU terminal (recommended for Nerd Font glyphs)
cask "docker-desktop"               # kubernetes IDE
cask "dbeaver-community"
cask "postman"
cask "burp-suite"
cask "zenmap"
cask "drawio"
cask "firefox"
cask "bitwarden"
cask "keybase"
cask "tailscale-app"
cask "ngrok"
cask "nextcloud"
cask "slack"
cask "signal"
cask "telegram"
cask "microsoft-teams"
cask "spotify"
cask "vlc"
