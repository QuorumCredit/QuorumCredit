#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== git status ==="
git status

echo "=== git branch ==="
git branch --show-current

echo "=== git diff stat ==="
git diff origin/main...HEAD --stat || true

echo "=== cargo-mutants check ==="
if ! command -v cargo-mutants &>/dev/null; then
  echo "Installing cargo-mutants..."
  cargo install cargo-mutants --locked
else
  cargo mutants --version
fi

echo "=== cargo mutants --list ==="
cargo mutants --list

echo "=== cargo mutants --jobs 4 ==="
cargo mutants --jobs 4

echo "=== outcomes.json ==="
if [[ -f mutants.out/outcomes.json ]]; then
  cat mutants.out/outcomes.json
fi

echo "=== cargo test ==="
cargo test
