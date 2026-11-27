#!/usr/bin/env bash
# Generates the observability integration report uploaded by CI.
set -euo pipefail

environment="staging"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --environment) environment="${2:-}"; shift 2 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done

outdir="observability/reports"
mkdir -p "$outdir"
report="$outdir/${environment}-report.md"

{
  echo "# Observability integration report"
  echo
  echo "- Environment: $environment"
  echo "- Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo
  echo "## Checked-in configuration"
  for config in \
    observability/prometheus/prometheus.yml \
    observability/prometheus/alert-rules.yml \
    observability/alertmanager/alertmanager.yml \
    observability/logstash/logstash.conf \
    observability/grafana/provisioning/dashboards/provider.yml \
    docker-compose.observability.yml
  do
    if [[ -f "$config" ]]; then
      echo "- OK: $config"
    else
      echo "- MISSING: $config"
    fi
  done
} > "$report"

echo "Report written to $report"
