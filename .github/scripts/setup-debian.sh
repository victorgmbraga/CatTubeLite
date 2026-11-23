#!/bin/sh
# Prepare a Debian container to build and test CatTubeLite. Building on
# Debian 12 keeps the binary compatible with systems from glibc 2.36 on.
set -eu
RUST_VERSION="${RUST_VERSION:-1.100.0}"
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
  build-essential ca-certificates curl git pkg-config python3 >/dev/null
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
  sh -s -- -y --profile minimal --default-toolchain "$RUST_VERSION" \
    --component clippy --component rustfmt
if [ -n "${GITHUB_PATH:-}" ]; then
  echo "$HOME/.cargo/bin" >>"$GITHUB_PATH"
fi
