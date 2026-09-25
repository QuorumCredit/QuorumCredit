# Container Image Security Scanning

## Overview

QuorumCredit implements automated container image scanning to identify vulnerabilities, secrets, and security misconfigurations in Docker images before deployment.

## Scanning Strategy

### Tools Used

1. **Trivy** - Primary vulnerability scanner by Aqua Security
   - Scans OS packages (apt, pip, npm, etc.)
   - Identifies known CVEs
   - Detects container misconfigurations
   - Detects embedded secrets

2. **Grype** - Secondary vulnerability scanner by Anchore
   - Provides additional vulnerability matching
   - Cross-validation of findings
   - Detailed dependency resolution

3. **GitHub Security** - Centralized vulnerability tracking
   - SARIF format results upload
   - GitHub Dependabot integration
   - Security alerts dashboard

### Scanning Scope

| Component | Trigger | Frequency | Action |
|-----------|---------|-----------|--------|
| Server Image | Push + PR | On commit | Scan + Report |
| Indexer Image | Push + PR | On commit | Scan + Report |
| Base Images | Weekly | Sunday 2 AM UTC | Full scan |
| Dependencies | On update | Package.json change | Vulnerability check |

---

## CI/CD Integration

### GitHub Actions Workflow

The workflow `.github/workflows/container-scan.yml` automatically:

1. **Builds images** from Dockerfile
2. **Scans with Trivy** - OS and application vulnerabilities
3. **Scans with Grype** - Vulnerability verification
4. **Reports results** to GitHub Security tab
5. **Comments on PRs** with summary
6. **Uploads artifacts** for audit trail
7. **Blocks deployment** if CRITICAL vulnerabilities found

### Workflow Triggers

```yaml
on:
  push:
    branches: [main, develop]
    paths:
      - 'server/Dockerfile'
      - 'services/indexer/Dockerfile'
      - 'package.json'
  pull_request:
    branches: [main, develop]
  schedule:
    - cron: '0 2 * * 0'  # Weekly
```

---

## Vulnerability Severity Levels

| Level | CVSS Score | Action | Timeline |
|-------|-----------|--------|----------|
| CRITICAL | 9.0-10.0 | Immediate patch required | < 24 hours |
| HIGH | 7.0-8.9 | Patch before next release | < 7 days |
| MEDIUM | 4.0-6.9 | Plan in next cycle | < 30 days |
| LOW | 0.1-3.9 | Monitor and assess | < 90 days |

---

## Local Scanning

### Prerequisites

```bash
# Install Trivy
curl -sfL https://raw.githubusercontent.com/aquasecurity/trivy/main/contrib/install.sh | sh -s -- -b /usr/local/bin

# Install Grype (optional)
curl -sSfL https://raw.githubusercontent.com/anchore/grype/main/install.sh | sh -s -- -b /usr/local/bin
```

### Scan Server Image

```bash
# Build image
cd server
docker build -t quorum-credit-server:latest .

# Scan with Trivy
trivy image quorum-credit-server:latest

# Detailed JSON output
trivy image --format json --output results.json quorum-credit-server:latest

# Parse results
python3 ../scripts/parse-vulnerabilities.py results.json
```

### Scan Indexer Image

```bash
# Build image
cd services/indexer
docker build -t quorum-credit-indexer:latest .

# Scan
trivy image --severity CRITICAL,HIGH quorum-credit-indexer:latest
```

### Scan with Specific Severity

```bash
# Only CRITICAL and HIGH
trivy image --severity CRITICAL,HIGH quorum-credit-server:latest

# JSON for all severities
trivy image --format json quorum-credit-server:latest > vulnerabilities.json
```

---

## Interpreting Results

### Trivy Output Examples

#### Example: OS Package Vulnerability

```
quorum-credit-server:latest (debian 12.0)
============================================
Total: 15 (CRITICAL: 2, HIGH: 5, MEDIUM: 8)

CRITICAL (2)
  openssl (Debian)
  ├─ CVE-2023-12345
  │  └─ Fixed Version: 3.0.8-1~deb12
  └─ CVE-2023-54321
     └─ Fixed Version: 3.0.8-1~deb12

HIGH (5)
  curl (Debian)
  ├─ CVE-2023-11111
  │  └─ Fixed Version: 7.88.1-2~deb12
  ...
```

#### Example: Application Vulnerability

```
Application (nodejs)
====================
Total: 8 (CRITICAL: 0, HIGH: 2, MEDIUM: 6)

HIGH (2)
  /app/node_modules/lodash
  ├─ CVE-2023-22222 - Prototype pollution
  │  └─ Fixed Version: 4.17.21
  └─ CVE-2021-23337 - Regular expression DoS
     └─ Fixed Version: 4.17.21
```

#### Example: Misconfigurations

```
Misconfigurations (0)
=====================
No misconfigurations detected
```

#### Example: Secrets Detected

```
Secrets (1)
===========
WARNING: Potential secrets detected!

  1. AWS Access Key
     Location: server/src/config.ts:42
     Rule: aws-access-key
```

---

## Remediation Process

### Step 1: Assess Finding

```bash
# Check CVE details
# Go to https://nvd.nist.gov/vuln/detail/CVE-2023-XXXXX

# Check if we're actually affected
grep -r "vulnerable-package" package.json

# Determine if it's a transitive dependency
npm ls vulnerable-package
```

### Step 2: Identify Fix

```bash
# Option 1: Update package directly
npm update vulnerable-package

# Option 2: Update package with specific version
npm install vulnerable-package@^4.0.0

# Option 3: Use dependency patch for transitive
npm install patch-package --save-dev
# Edit patches and commit
```

### Step 3: Verify Fix

```bash
# Rebuild image
docker build -t quorum-credit-server:patched .

# Scan again
trivy image quorum-credit-server:patched

# Should show CVE resolved or different version
```

### Step 4: Test and Deploy

```bash
# Run test suite
npm test

# Deploy to staging
kubectl set image deployment/api-server \
  api-server=quorum-credit-server:patched

# Monitor
kubectl logs deployment/api-server -f
```

---

## Base Image Updates

### Keeping Node.js Updated

```bash
# Check for new Node.js versions
curl -s https://nodejs.org/en/download/releases/ | grep -i "v22"

# Update Dockerfile
# FROM node:22-slim -> FROM node:22.1-slim

# Build and test
docker build -t quorum-credit-server:node-22.1 .
trivy image quorum-credit-server:node-22.1
npm test
```

### Scanning Base Images Independently

```bash
# Scan just the base image
trivy image node:22-slim

# Compare versions
trivy image node:21-slim
trivy image node:22-slim

# Choose the base with fewer vulnerabilities
```

---

## Automated Remediation

### Dependabot Configuration

Create `.github/dependabot.yml`:

```yaml
version: 2
updates:
  - package-ecosystem: "npm"
    directory: "/server"
    schedule:
      interval: "weekly"
    open-pull-requests-limit: 5
    reviewers:
      - "security-team"
    allow:
      - dependency-type: "production"
      - dependency-type: "direct"
    pull-request-branch-name:
      separator: "/"
      prefix: "dependabot"

  - package-ecosystem: "npm"
    directory: "/services/indexer"
    schedule:
      interval: "weekly"

  - package-ecosystem: "docker"
    directory: "/"
    schedule:
      interval: "weekly"
```

### Automated PR Creation

Dependabot automatically:
- Creates PRs for vulnerable dependencies
- Triggers container scan workflow
- Provides vulnerability details
- Allows auto-merge if tests pass

---

## Reporting and Monitoring

### GitHub Security Dashboard

View findings at: `https://github.com/QuorumCredit/QuorumCredit/security/vulnerability-alerts`

Shows:
- All detected vulnerabilities
- Severity breakdown
- Affected packages
- Recommended fixes

### Slack Notifications

```bash
# Send scan summary to Slack
curl -X POST https://hooks.slack.com/services/YOUR/WEBHOOK/URL \
  -H 'Content-Type: application/json' \
  -d '{
    "text": "🔴 CRITICAL vulnerabilities found in quorum-credit-server:latest",
    "blocks": [
      {"type": "section", "text": {"type": "mrkdwn", "text": "*Container Scan Report*"}},
      {"type": "section", "fields": [
        {"type": "mrkdwn", "text": "*CRITICAL*\n2"},
        {"type": "mrkdwn", "text": "*HIGH*\n5"}
      ]}
    ]
  }'
```

### Artifact Storage

Scan reports stored in GitHub Actions artifacts:
- `vulnerability-reports/` directory
- Retained for 30 days
- Available for audit trail
- Downloadable as ZIP

---

## Best Practices

### Dockerfile Hardening

```dockerfile
# ✅ Good: Scan for vulnerabilities
FROM node:22-slim AS build

# ❌ Bad: No vulnerability scanning
FROM node:latest

# ✅ Good: Pin specific version
FROM node:22.1.0-slim

# ✅ Good: Use slim/alpine variants
FROM node:22-alpine

# ✅ Good: Multi-stage build
FROM node:22-slim AS build
WORKDIR /app
COPY package*.json ./
RUN npm ci --only=production

FROM node:22-slim
COPY --from=build /app/node_modules ./node_modules

# ✅ Good: Don't run as root
RUN useradd -m appuser
USER appuser
```

### Package Security

```bash
# ✅ Use package-lock.json
git add package-lock.json

# ✅ Audit packages regularly
npm audit

# ✅ Update regularly
npm update

# ❌ Don't use * versions
# "lodash": "*"  <- BAD

# ✅ Use ranges
# "lodash": "^4.17.0"  <- GOOD
```

### Secrets Management

```bash
# ✅ Use environment variables
ENV NODE_ENV=production

# ✅ Use secrets management
COPY --from=secrets /run/secrets/api-key /app/.env

# ❌ Never commit secrets
# DATABASE_PASSWORD=secret123  <- BAD

# ✅ Use .gitignore
echo ".env" >> .gitignore
```

---

## Troubleshooting

### Scan Fails with Network Error

```bash
# Trivy tries to download vulnerability DB
# May fail behind corporate proxy

# Configure proxy
trivy config init
# Edit ~/.trivy/trivy.yaml
# proxy:
#   http: "http://proxy:8080"
#   https: "http://proxy:8080"
```

### False Positives

```bash
# Some CVEs don't affect your code

# Check if actually vulnerable
grep -r "vulnerable-function" src/

# If not used, document as accepted risk
# In .trivyignore:
# CVE-2023-12345

# Or in Dockerfile:
# RUN trivy vulnerability image \
#   --exit-code 0 \
#   --no-progress \
#   --format sarif \
#   --ignore-unfixed \
#   --severity HIGH,CRITICAL \
#   IMAGE
```

### Scanning Takes Too Long

```bash
# Trivy downloads vulnerability DB on first run

# Pre-download DB
trivy image --download-db-only

# Use cached DB
trivy image --skip-update quorum-credit-server:latest
```

---

## Incident Response

### Critical Vulnerability Found

1. **Alert** - GitHub Security alerts firing
2. **Assess** - Check if we deploy affected version
3. **Patch** - Update package immediately
4. **Test** - Run full test suite
5. **Deploy** - Push to production quickly
6. **Verify** - Confirm deployed version
7. **Monitor** - Watch for issues

### Process Workflow

```bash
#!/bin/bash
# Handle critical vulnerability

# 1. Create hotfix branch
git checkout -b hotfix/cve-2024-xxxxx

# 2. Update vulnerable package
npm install vulnerable-package@^5.0.0

# 3. Run tests
npm test

# 4. Build new image
docker build -t quorum-credit-server:hotfix .

# 5. Scan new image
trivy image quorum-credit-server:hotfix

# 6. Push and PR
git add package.json package-lock.json
git commit -m "fix: patch CVE-2024-xxxxx"
git push origin hotfix/cve-2024-xxxxx
gh pr create --title "Hotfix: CVE-2024-xxxxx" --label security
```

---

## References

- [Trivy Documentation](https://aquasecurity.github.io/trivy/)
- [Grype Documentation](https://github.com/anchore/grype)
- [NIST CVE Database](https://nvd.nist.gov/)
- [OWASP Container Security](https://owasp.org/www-community/vulnerabilities/Container_Security)
- [CIS Docker Benchmark](https://www.cisecurity.org/benchmark/docker/)
