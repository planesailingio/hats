# devops — running and deploying infrastructure.
#
# Rule for this file: it targets infrastructure you operate — clusters, cloud
# accounts, images, IaC — rather than source code you write. Scanners live here
# when what they scan is an infra artifact (manifests, images, HCL, clusters);
# code-level linters live in dev.
#
# Installed on top of core. `hats brew install devops` == core + this file.

# ── Taps ──────────────────────────────────────────────────────────────────────
# NOTE: verified 2026-09-09 — every formula in these bundles resolves from
# homebrew/core; none of these taps is required. Deletion candidates.
tap "hashicorp/tap"
tap "aquasecurity/trivy"
tap "azure/kubelogin"
tap "carvel-dev/carvel"
tap "cloudflare/cloudflare"
tap "siderolabs/tap"
tap "turbot/tap"
tap "netbirdio/tap"
tap "globex/tap"

# ── IaC / Terraform-OpenTofu ──────────────────────────────────────────────────
brew "tenv"                         # tofu/terraform/terragrunt version manager (replaces tfenv+tofuenv)
brew "terraform-docs"
brew "terraform-inventory"
brew "terraformer"

# ── Kubernetes ────────────────────────────────────────────────────────────────
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

# ── Containers & registries ───────────────────────────────────────────────────
brew "podman"
brew "skopeo"
brew "crane"
brew "oras"
brew "dive"                         # inspect image layers

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
brew "cloudflare-wrangler"          # Cloudflare Workers
brew "coder"                        # remote dev environments

# ── Infra & supply-chain scanning ─────────────────────────────────────────────
brew "trivy"                        # vuln/misconfig scanner
brew "grype"                        # vuln scanner
brew "syft"                         # SBOM generator
brew "trufflehog"                   # secret scanner (repos, images, buckets)
brew "cosign"                       # sign/verify artifacts & images
brew "osv-scanner"                  # dependency vuln scanner (OSV)
brew "checkov"                      # IaC static analysis
brew "kics"                         # IaC static analysis
brew "tfsec"                        # terraform security scanner
brew "terrascan"                    # IaC security scanner
brew "kube-bench"                   # CIS Kubernetes benchmark
brew "kubescape"                    # k8s security posture
brew "hadolint"                     # Dockerfile linter
brew "step"                         # smallstep CA / cert tooling

# ── GUI ───────────────────────────────────────────────────────────────────────
cask "docker-desktop"
