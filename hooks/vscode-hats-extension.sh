#!/bin/sh
# Installs the hats VS Code extension from vscode/hats/ in this repo.
#
# The extension tells you when a folder's hat and the window's profile disagree.
# There is nothing to build: a .vsix is a zip with the extension under
# extension/, and VS Code reads its manifest from extension/package.json.
set -eu

SRC="${HATS_REPO}/vscode/hats"
LIST="${HOME}/.config/hats/vscode-extensions.list"

# The same gate as the extension list hook: this is only meaningful once the
# editor group is on.
[ -f "${LIST}" ] || { echo "==> editor group is off; skipping the hats extension."; exit 0; }
[ -d "${SRC}" ] || { echo "==> no extension at ${SRC}; skipping."; exit 0; }

if ! command -v code >/dev/null 2>&1; then
  echo "==> the 'code' command is not on PATH; skipping the hats extension."
  echo "    In VS Code: Cmd-Shift-P -> Shell Command: Install 'code' command in PATH"
  exit 0
fi
if ! command -v zip >/dev/null 2>&1; then
  echo "==> zip is not installed; skipping the hats extension."
  exit 0
fi

work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT INT TERM

mkdir -p "${work}/extension"
cp "${SRC}/package.json" "${SRC}/extension.js" "${SRC}/README.md" "${work}/extension/"

vsix="${work}/hats.vsix"
(cd "${work}" && zip -qr "${vsix}" extension)

# --force so the same version reinstalls when the extension changes.
if code --install-extension "${vsix}" --force >/dev/null 2>&1; then
  echo "==> installed the hats VS Code extension."
else
  echo "==> could not install the hats VS Code extension; skipping."
fi
