#!/bin/sh
# Installs the pinned VS Code extensions. Re-runs when the list changes.
#
# Under chezmoi this list sat in the repo root and nothing read it. Here it is a
# managed file with a hook behind it, so editing the list actually does
# something on the next apply.
set -eu

LIST="${HOME}/.config/hats/vscode-extensions.list"
[ -f "${LIST}" ] || { echo "==> no extension list at ${LIST}; skipping."; exit 0; }

if ! command -v code >/dev/null 2>&1; then
  echo "==> the 'code' command is not on PATH; skipping extensions."
  echo "    In VS Code: Cmd-Shift-P -> Shell Command: Install 'code' command in PATH"
  exit 0
fi

installed="$(code --list-extensions 2>/dev/null | tr '[:upper:]' '[:lower:]')"
count=0
while IFS= read -r ext; do
  # Skip blanks and comments.
  case "${ext}" in ''|\#*) continue ;; esac
  if printf '%s\n' "${installed}" | grep -qxF "$(printf '%s' "${ext}" | tr '[:upper:]' '[:lower:]')"; then
    continue
  fi
  echo "   + ${ext}"
  code --install-extension "${ext}" --force >/dev/null 2>&1 || echo "   ! failed: ${ext}"
  count=$((count + 1))
done < "${LIST}"

echo "==> ${count} extension(s) installed."
