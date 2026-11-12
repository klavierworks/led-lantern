#!/usr/bin/env bash
set -euo pipefail

HOST=${1:-random@led-lantern.local}
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

"$REPO_DIR/deploy/build.sh"

rsync -az --delete \
  "$REPO_DIR/target/lantern/release/led-lantern" \
  "$REPO_DIR/deploy" \
  "$HOST:led-lantern/"

ssh -t "$HOST" 'led-lantern/deploy/setup-lantern.sh'
