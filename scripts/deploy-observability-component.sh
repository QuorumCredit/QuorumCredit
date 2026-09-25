#!/bin/bash

# Issue #1578: Deploy Observability Component
# Deploys and configures observability stack components

set -euo pipefail

COMPONENT="${1:-}"
ENVIRONMENT="${2:-staging}"
VERSION="${3:-latest}"

log_info() { echo "ℹ️  $*" >&2; }
log_success() { echo "✅ $*" >&2; }
log_error() { echo "❌ $*" >&2; }

if [[ -z "$COMPONENT" ]]; then
  log_error "Usage: $0 <component> <environment> <version>"
  exit 1
fi

log_info "Deploying $COMPONENT ($VERSION) to $ENVIRONMENT"

case "$COMPONENT" in
  prometheus)
    log_info "Deploying Prometheus..."
    docker-compose -f docker-compose.observability.yml up -d prometheus
    sleep 10
    curl -s http://localhost:9090/-/healthy > /dev/null && log_success "Prometheus deployed"
    ;;
  grafana)
    log_info "Deploying Grafana..."
    docker-compose -f docker-compose.observability.yml up -d grafana
    sleep 20
    curl -s http://localhost:3000/api/health > /dev/null && log_success "Grafana deployed"
    ;;
  jaeger)
    log_info "Deploying Jaeger..."
    docker-compose -f docker-compose.observability.yml up -d jaeger
    sleep 10
    curl -s http://localhost:16686/ > /dev/null && log_success "Jaeger deployed"
    ;;
  elasticsearch)
    log_info "Deploying Elasticsearch..."
    docker-compose -f docker-compose.observability.yml up -d elasticsearch
    sleep 30
    curl -s http://localhost:9200/_cluster/health > /dev/null && log_success "Elasticsearch deployed"
    ;;
  logstash)
    log_info "Deploying Logstash..."
    docker-compose -f docker-compose.observability.yml up -d logstash
    sleep 15
    curl -s http://localhost:9600/ > /dev/null && log_success "Logstash deployed"
    ;;
  kibana)
    log_info "Deploying Kibana..."
    docker-compose -f docker-compose.observability.yml up -d kibana
    sleep 20
    curl -s http://localhost:5601/api/status > /dev/null && log_success "Kibana deployed"
    ;;
  alertmanager)
    log_info "Deploying AlertManager..."
    docker-compose -f docker-compose.observability.yml up -d alertmanager
    sleep 10
    curl -s http://localhost:9093/-/healthy > /dev/null && log_success "AlertManager deployed"
    ;;
  *)
    log_error "Unknown component: $COMPONENT"
    exit 1
    ;;
esac

log_success "$COMPONENT deployment completed"
