#!/bin/sh
# Creates the provider cache directory that ~/.terraformrc and ~/.tofurc name.
# Neither Terraform nor OpenTofu creates it, and both silently skip caching
# when it is missing.
#
# The path is read back from the managed files rather than repeated here, so a
# change to plugin_cache_dir cannot leave this script behind. Exits cleanly
# when the terraform group is off and neither file exists.
set -eu

found=0
for rc in "${HOME}/.terraformrc" "${HOME}/.tofurc"; do
  [ -f "${rc}" ] || continue
  dir="$(sed -n 's/^[[:space:]]*plugin_cache_dir[[:space:]]*=[[:space:]]*"\(.*\)".*/\1/p' "${rc}" | head -n 1)"
  [ -n "${dir}" ] || continue
  # Both tools expand environment variables in this setting; $HOME is the only
  # one the managed files use.
  case "${dir}" in
    '$HOME'*) dir="${HOME}${dir#\$HOME}" ;;
  esac
  found=1
  [ -d "${dir}" ] && continue
  mkdir -p "${dir}"
  echo "==> created ${dir}"
done

[ "${found}" -eq 1 ] || echo "==> no plugin_cache_dir in ~/.terraformrc or ~/.tofurc; skipping."
