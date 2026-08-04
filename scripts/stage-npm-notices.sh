#!/bin/sh
# Stage the repo-root third-party notices into the npm package directory.
#
# npm tarballs cannot reference files above the package root, so the MIT
# attribution for the statically linked Tree-sitter components has to exist as a
# copy next to package.json. That copy is generated (git-ignored), like the
# tarball itself.
#
# Split out of update-npm-package.sh so this step is executable — and therefore
# testable — without the network fetches that script performs.
#
# Fails closed: publishing the npm channel without the notice would distribute
# the binary without its required attribution.
set -eu

notices="${AEGIS_NPM_NOTICES_SOURCE:-THIRD_PARTY_NOTICES.md}"
notices_out="${AEGIS_NPM_NOTICES:-packaging/npm/THIRD_PARTY_NOTICES.md}"

if [ ! -f "$notices" ]; then
  printf 'missing %s; cannot stage third-party notices for the npm package\n' "$notices" >&2
  exit 1
fi

mkdir -p "$(dirname "$notices_out")"
cp "$notices" "$notices_out"

printf '%s staged\n' "$notices_out"
