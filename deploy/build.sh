#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! docker info >/dev/null 2>&1; then
  echo "Docker isn't running. Start Docker Desktop with 'open -a Docker', wait for it to finish starting, then try again." >&2
  exit 1
fi

docker run --rm --platform linux/arm64 \
  --volume "$REPO_DIR":/src \
  --volume led-lantern-cargo-registry:/usr/local/cargo/registry \
  --workdir /src \
  --env CARGO_TARGET_DIR=/src/target/lantern \
  rust:1.95-bookworm \
  cargo build --release --locked

echo "Built $REPO_DIR/target/lantern/release/led-lantern"
