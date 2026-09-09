#!/bin/sh
# Linux smoke test for the dotfiles, run inside the dev container.
#
# Builds hats from the mounted source, sets it up unattended against a throwaway
# HOME, applies the files, and runs `hats test` — which makes a real commit and
# checks two shells get separate kubeconfigs.
#
# The repo is mounted read-write but nothing here writes to it except cargo's
# target directory.
set -eu

WORKSPACE="${WORKSPACE:-/workspace}"
FAKE_HOME="$(mktemp -d)"
HATS_HOME="$(mktemp -d)"
ANSWERS="$(mktemp)"

echo "==> building hats"
cd "${WORKSPACE}"
cargo build --release --locked -p hats
HATS="${WORKSPACE}/target/release/hats"

# A tagged clone, because hats deliberately refuses a repo whose tag does not
# match the binary. Cloning also proves the manifest and files are committed.
echo "==> preparing a tagged clone"
SRC="$(mktemp -d)/dotfiles"
git clone --quiet "${WORKSPACE}" "${SRC}"
git -C "${SRC}" -c user.email=ci@example.com -c user.name=CI \
    tag -a "v$("${HATS}" --version | awk '{print $2}')" -m ci 2>/dev/null || true

cat > "${ANSWERS}" <<'ANSWERS_EOF'
answers:
  identity.name: Test User
  identity.email: test@example.com
  hat.1.name: personal
  hat.1.aws_profile: default
  hat.1.kube_context: ""
  hat.add.2: true
  hat.2.name: client
  hat.2.git_name: Test Client
  hat.2.git_email: test@client.example
  hat.2.aws_profile: client-account
  hat.2.kube_context: client
  hat.add.3: false
  secrets.enabled: false
ANSWERS_EOF

# A shared kubeconfig for the isolation check to seed from.
mkdir -p "${FAKE_HOME}/.kube"
printf 'apiVersion: v1\nkind: Config\ncurrent-context: shared\n' > "${FAKE_HOME}/.kube/config"

echo "==> hats init"
HOME="${FAKE_HOME}" "${HATS}" --hats-home "${HATS_HOME}" --no-color \
    --answers "${ANSWERS}" init --repo "${SRC}"

echo "==> hats plan"
# Exit code 2 means "there are changes", which is what a fresh home should say.
set +e
HOME="${FAKE_HOME}" "${HATS}" --hats-home "${HATS_HOME}" --no-color plan --skip-hooks >/dev/null
rc=$?
set -e
[ "${rc}" -eq 2 ] || { echo "FAIL: expected plan to report changes (exit 2), got ${rc}"; exit 1; }

echo "==> hats apply"
HOME="${FAKE_HOME}" "${HATS}" --hats-home "${HATS_HOME}" --no-color apply --yes --only-files

echo "==> hats plan is now clean"
set +e
HOME="${FAKE_HOME}" "${HATS}" --hats-home "${HATS_HOME}" --no-color plan --skip-hooks >/dev/null
rc=$?
set -e
[ "${rc}" -eq 0 ] || { echo "FAIL: expected a clean plan (exit 0), got ${rc}"; exit 1; }

echo "==> hats lint"
HOME="${FAKE_HOME}" "${HATS}" --hats-home "${HATS_HOME}" --no-color lint --no-external

echo "==> hats test"
HOME="${FAKE_HOME}" "${HATS}" --hats-home "${HATS_HOME}" --no-color test

echo "==> all checks passed"
