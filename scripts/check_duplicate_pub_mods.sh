#!/usr/bin/env bash
# Fails if src/lib.rs declares the same `pub mod <name>;` more than once.
# A duplicate module declaration is a compile error (issue #2101).
set -euo pipefail

LIB="${1:-src/lib.rs}"

dupes=$(grep -E '^[[:space:]]*pub mod [A-Za-z0-9_]+;' "$LIB" \
  | sed -E 's/^[[:space:]]*pub mod ([A-Za-z0-9_]+);.*/\1/' \
  | sort | uniq -d)

if [ -n "$dupes" ]; then
  echo "Duplicate 'pub mod' declarations in $LIB:" >&2
  echo "$dupes" | sed 's/^/  - /' >&2
  exit 1
fi

echo "No duplicate 'pub mod' declarations in $LIB."
