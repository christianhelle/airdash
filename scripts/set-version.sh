#!/bin/bash
# Sets the version of every crate in the workspace, the version requirements between them and their
# entries in Cargo.lock. The release workflow runs it before building, so the artifacts carry the version.

set -euo pipefail

if [[ $# -ne 1 ]] || [[ ! "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Usage: $0 MAJOR.MINOR.PATCH" >&2
  exit 1
fi

# Perl rather than sed, because the macOS runners have BSD sed.
export VERSION="$1"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

for manifest in "$root"/src/{airdash,airdash-core,airthings-client}/Cargo.toml; do
  # The first version line is the one in [package].
  perl -0pi -e '
    s/^version = "[^"]*"/version = "$ENV{VERSION}"/m;
    s/^((?:airdash-core|airthings-client) = \{ path = "[^"]*", version = )"[^"]*"/$1"$ENV{VERSION}"/mg;
  ' "$manifest"
done

perl -0pi -e '
  s/^(name = "(?:airdash|airdash-core|airthings-client)"\nversion = )"[^"]*"/$1"$ENV{VERSION}"/mg;
' "$root/Cargo.lock"

echo "Set the version to $VERSION"
