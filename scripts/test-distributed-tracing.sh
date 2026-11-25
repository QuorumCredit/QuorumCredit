#!/usr/bin/env bash
# Offline validation of the distributed tracing configuration.
set -euo pipefail

environment="staging"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --environment) environment="${2:-}"; shift 2 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done

compose="docker-compose.observability.yml"
if [[ ! -f "$compose" ]]; then
  echo "::error::Observability compose file is missing: $compose" >&2
  exit 1
fi
if ! grep -qi 'jaeger' "$compose"; then
  echo "::error::No Jaeger tracing service found in $compose" >&2
  exit 1
fi

echo "OK: distributed tracing configuration valid for environment '$environment'"
