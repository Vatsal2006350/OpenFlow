#!/bin/bash
# Run OpenFlow in development: build the local-LLM worker, then let Tauri's
# configured hook compile Jac to static assets before opening the native app.
# No frontend server is required; Tauri serves the bundled Jac assets.
set -e
cd "$(dirname "$0")"
if ! command -v jac >/dev/null 2>&1; then
  echo "Jac is required. Install it with: python3.13 -m pip install jaclang==0.13.5 jac-client==0.3.11" >&2
  exit 1
fi
echo "[dev] building the local-LLM worker…"
cargo build -p openflow-llm-worker
exec ui/node_modules/.bin/tauri dev "$@"
