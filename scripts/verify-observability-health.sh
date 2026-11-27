#!/usr/bin/env bash
# Verifies that a component's checked-in observability configuration is present.
# Offline counterpart of the deploy step in observability-integration.yml.
set -euo pipefail

component=""
environment="staging"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --component) component="${2:-}"; shift 2 ;;
    --environment) environment="${2:-}"; shift 2 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [[ -z "$component" ]]; then
  echo "::error::--component is required" >&2
  exit 2
fi

case "$component" in
  prometheus)   config="observability/prometheus/prometheus.yml" ;;
  alertmanager) config="observability/alertmanager/alertmanager.yml" ;;
  logstash)     config="observability/logstash/logstash.conf" ;;
  grafana)      config="observability/grafana/provisioning/dashboards/provider.yml" ;;
  jaeger|elasticsearch|kibana) config="docker-compose.observability.yml" ;;
  *)
    echo "::error::Unknown observability component: $component" >&2
    exit 1
    ;;
esac

if [[ ! -f "$config" ]]; then
  echo "::error::$component configuration is missing: $config" >&2
  exit 1
fi

echo "OK: $component configuration present ($config) for environment '$environment'"
