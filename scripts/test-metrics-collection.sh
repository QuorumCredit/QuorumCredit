#!/usr/bin/env bash
# Offline validation of the metrics collection configuration.
set -euo pipefail

environment="staging"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --environment) environment="${2:-}"; shift 2 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done

config="observability/prometheus/prometheus.yml"
if [[ ! -f "$config" ]]; then
  echo "::error::Metrics configuration is missing: $config" >&2
  exit 1
fi
if ! grep -q 'scrape_configs' "$config"; then
  echo "::error::$config has no scrape_configs section" >&2
  exit 1
fi

echo "OK: metrics collection configuration valid for environment '$environment'"
