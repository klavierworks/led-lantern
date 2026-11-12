#!/usr/bin/env bash
set -euo pipefail

HOST=${1:-random@led-lantern.local}
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

rsync -az --delete \
  --exclude target \
  --exclude .git \
  --exclude .claude \
  --exclude .DS_Store \
  "$REPO_DIR/" "$HOST:led-lantern/"

ssh -t "$HOST" 'led-lantern/deploy/setup-lantern.sh'
