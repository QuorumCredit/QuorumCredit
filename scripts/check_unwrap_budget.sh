#!/usr/bin/env bash
# Ratchet for panicking calls in contract code. Fails if the count of
# non-test `.unwrap()` / `.expect(` calls rises above scripts/unwrap-budget.txt.
# When the count falls, lower the budget file in the same change to lock in the gain.
set -euo pipefail
cd "$(dirname "$0")/.."

budget=$(tr -d '[:space:]' < scripts/unwrap-budget.txt)
count=$(bash scripts/count_unwrap_expect.sh)

if (( count > budget )); then
  echo "error: unwrap/expect count rose to $count (budget $budget)." >&2
  echo "Return a typed ContractError instead of panicking." >&2
  exit 1
fi

if (( count < budget )); then
  echo "notice: count fell to $count (budget $budget). Set scripts/unwrap-budget.txt to $count."
fi

echo "unwrap/expect in contract code: $count (budget $budget)"
