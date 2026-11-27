#!/usr/bin/env bash
# Offline validation of the log aggregation configuration.
set -euo pipefail

environment="staging"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --environment) environment="${2:-}"; shift 2 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done

config="observability/logstash/logstash.conf"
if [[ ! -f "$config" ]]; then
  echo "::error::Log aggregation configuration is missing: $config" >&2
  exit 1
fi
if ! grep -q 'input' "$config" || ! grep -q 'output' "$config"; then
  echo "::error::$config must define both input and output" >&2
  exit 1
fi

echo "OK: log aggregation configuration valid for environment '$environment'"
