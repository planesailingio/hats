# dev — writing and shipping application code.
#
# Rule for this file: languages, the clients you poke your own services with,
# code-level linters/SAST, and release tooling. Infra tooling is devops.
#
# Installed on top of core. `hats brew install dev` == core + this file.

# ── Taps ──────────────────────────────────────────────────────────────────────
# NOTE: verified 2026-09-09 — every formula in these bundles resolves from
# homebrew/core; none of these taps is required. Deletion candidates.
tap "go-task/tap"
tap "goreleaser/tap"
tap "ankitpokhrel/jira-cli"
tap "auth0/auth0-cli"
tap "stripe/stripe-cli"
tap "sass/sass"

# ── Languages / runtimes (mise handles versions; these are the toolchains) ────
brew "go"
brew "rust"

# ── Service clients & local endpoints ─────────────────────────────────────────
brew "grpcurl"                      # call gRPC services from the shell
brew "mkcert"                       # local trusted TLS certs

# ── Code-level linting & SAST ─────────────────────────────────────────────────
brew "semgrep"                      # multi-language SAST
brew "golangci-lint"
brew "actionlint"                   # GitHub Actions linter
brew "zizmor"                       # GitHub Actions security auditor

# ── Build, release & docs ─────────────────────────────────────────────────────
brew "goreleaser"
brew "structurizr-cli"              # C4 architecture diagrams

# ── Local AI ──────────────────────────────────────────────────────────────────
brew "ollama"                       # local model runtime
brew "opencode"                     # terminal coding agent

# ── GUI ───────────────────────────────────────────────────────────────────────
cask "dbeaver-community"
cask "postman"
cask "ngrok"                        # expose a local port for webhook testing
