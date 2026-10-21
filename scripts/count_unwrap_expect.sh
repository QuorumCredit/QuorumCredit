#!/usr/bin/env bash
# Counts `.unwrap()` and `.expect(` calls in non-test contract code under src/.
#
# Skipped: `#[cfg(test)]` items (tracked by brace depth), comment-only lines,
# and dedicated test files (`*_test.rs`, `tests.rs`).
#
# This is a text count, so it needs no compile. It is a close approximation
# of the clippy::unwrap_used / clippy::expect_used hits in the lib build.
set -euo pipefail
cd "$(dirname "$0")/.."

# Optional file arguments count only those files (useful for spot checks).
if (( $# > 0 )); then
  files=("$@")
else
  mapfile -d '' files < <(find src -name '*.rs' ! -name '*_test.rs' ! -name 'tests.rs' -print0 | sort -z)
fi

printf '%s\0' "${files[@]}" \
  | xargs -0 awk '
    FNR == 1 { skip = 0; pending = 0; depth = 0 }

    # Inside a test block: follow brace depth until it closes.
    skip {
      line = $0
      depth += gsub(/\{/, "{", line) - gsub(/\}/, "}", line)
      if (depth <= 0) skip = 0
      next
    }

    # `#[cfg(test)]` applies to the next item.
    /^[[:space:]]*#\[cfg\(test\)\]/ { pending = 1; next }
    pending {
      pending = 0
      if ($0 ~ /;[[:space:]]*$/) next          # single-line item such as a `use`
      if ($0 ~ /\{/) {
        line = $0
        depth = gsub(/\{/, "{", line) - gsub(/\}/, "}", line)
        skip = depth > 0
        next
      }
    }

    /^[[:space:]]*\/\// { next }
    { total += gsub(/\.unwrap\(\)|\.expect\(/, "&") }

    END { print total + 0 }
  ' \
  | awk '{ sum += $1 } END { print sum + 0 }'
