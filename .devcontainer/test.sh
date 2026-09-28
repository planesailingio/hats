#!/bin/sh
# Linux smoke test for hats, run inside the dev container.
#
# Builds hats from the mounted source, sets it up unattended against a
# throwaway HOME (no network: hats clones nothing), and runs `hats test` —
# which makes a real commit and checks two shells get separate kubeconfigs.
set -eu

WORKSPACE="${WORKSPACE:-/workspace}"
FAKE_HOME="$(mktemp -d)"
HATS_HOME="$(mktemp -d)"
ANSWERS="$(mktemp)"

echo "==> building hats"
cd "${WORKSPACE}"
cargo build --release --locked -p hats
HATS="${WORKSPACE}/target/release/hats"

cat > "${ANSWERS}" <<'ANSWERS_EOF'
answers:
  identity.name: Test User
  identity.email: test@example.com
  features.ssh: true
  features.vscode: false
  hat.1.name: personal
  hat.1.kube_context: ""
  hat.add.2: true
  hat.2.name: client
  hat.2.git_name: Test Client
  hat.2.git_email: test@client.example
  hat.2.kube_context: client
  hat.add.3: false
  secrets.enabled: false
ANSWERS_EOF

# A shared kubeconfig for the isolation check to seed from.
mkdir -p "${FAKE_HOME}/.kube"
printf 'apiVersion: v1\nkind: Config\ncurrent-context: shared\n' > "${FAKE_HOME}/.kube/config"

echo "==> hats init"
HOME="${FAKE_HOME}" "${HATS}" --hats-home "${HATS_HOME}" --no-color \
    --answers "${ANSWERS}" init

echo "==> the base skeletons are in place"
for f in .gitconfig .ssh/config .terraformrc .tofurc \
         .ssh/config.d/personal.conf .ssh/config.d/client.conf \
         .gitconfig.d/personal .gitconfig.d/client; do
  [ -f "${FAKE_HOME}/${f}" ] || { echo "FAIL: ${f} was not scaffolded"; exit 1; }
done

echo "==> hats hat sync is a no-op after init"
HOME="${FAKE_HOME}" "${HATS}" --hats-home "${HATS_HOME}" --no-color hat sync \
  | grep -q "already in place" || { echo "FAIL: sync was not settled"; exit 1; }

echo "==> hats doctor"
HOME="${FAKE_HOME}" "${HATS}" --hats-home "${HATS_HOME}" --no-color doctor

echo "==> hats test"
HOME="${FAKE_HOME}" "${HATS}" --hats-home "${HATS_HOME}" --no-color test

echo "==> all checks passed"
