#!/usr/bin/env bash
# Offline validation of the alerting rules and receiver configuration.
set -euo pipefail

environment="staging"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --environment) environment="${2:-}"; shift 2 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done

rules="observability/prometheus/alert-rules.yml"
receiver="observability/alertmanager/alertmanager.yml"
for config in "$rules" "$receiver"; do
  if [[ ! -f "$config" ]]; then
    echo "::error::Alerting configuration is missing: $config" >&2
    exit 1
  fi
done
if ! grep -q 'groups' "$rules"; then
  echo "::error::$rules has no alert rule groups" >&2
  exit 1
fi

echo "OK: alerting configuration valid for environment '$environment'"
