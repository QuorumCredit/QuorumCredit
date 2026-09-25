# Security Scanning for Dependencies Guide

## Overview

This guide describes the comprehensive security scanning infrastructure for QuorumCredit, including automated dependency vulnerability detection, reporting, and tracking.

## Scanning Tools

### 1. Cargo Audit (Rust)

**Purpose**: Scans Rust dependencies for known security vulnerabilities

**Configuration**:
```bash
# Run manual scan
cargo audit

# Output JSON report
cargo audit --json > audit.json
```

**What it checks**:
- Known CVEs in Rust crates
- Unmaintained dependencies
- Security advisories from RustSec Advisory Database

### 2. npm Audit (Node.js)

**Purpose**: Identifies vulnerabilities in npm package dependencies

**Configuration**:
```bash
# Run manual scan
npm audit

# Fix vulnerabilities automatically
npm audit fix
```

**What it checks**:
- Known CVEs in npm packages
- Metadata about packages
- Severity levels and fix availability

### 3. Snyk

**Purpose**: Comprehensive vulnerability detection and remediation

**Setup**:
```bash
# Install Snyk CLI
npm install -g snyk

# Authenticate
snyk auth

# Scan project
snyk test --severity-threshold=high
```

**Features**:
- Real-time vulnerability detection
- Fix pull request generation
- Continuous monitoring
- Remediation guidance

### 4. Trivy

**Purpose**: Container image and filesystem scanning

**Configuration**:
```bash
# Install Trivy
curl -sfL https://raw.githubusercontent.com/aquasecurity/trivy/main/contrib/install.sh | sh

# Scan filesystem
trivy fs .

# Scan Docker image
trivy image myimage:tag
```

### 5. OSV Scanner (Google)

**Purpose**: Open Source Vulnerabilities database scanning

**Features**:
- Scans multiple package managers
- GitHub advisory integration
- CycloneDX SBOM support

## Automated Scanning

### CI/CD Pipeline

The `dependency-security-scan.yml` workflow automatically:

1. **Runs on schedule**: Daily at 2 AM UTC
2. **Triggers on changes**: When dependency files are modified
3. **Multiple scanners**: Cargo, npm, Snyk, Trivy, OSV
4. **Generates reports**: JSON, SARIF, SBOM formats
5. **Blocks merges**: Fails on critical vulnerabilities in PRs

### Running Manually

```bash
# Trigger workflow manually
gh workflow run dependency-security-scan.yml

# Check results
gh run list --workflow=dependency-security-scan.yml
```

## Software Bill of Materials (SBOM)

### What is SBOM?

A Software Bill of Materials lists all components and dependencies in your application.

### Format: CycloneDX

```json
{
  "bomFormat": "CycloneDX",
  "specVersion": "1.4",
  "components": [
    {
      "type": "library",
      "name": "dependency-name",
      "version": "1.2.3",
      "purl": "pkg:npm/dependency-name@1.2.3"
    }
  ]
}
```

### Generating SBOMs

```bash
# Cargo SBOM
./scripts/generate-sbom-cargo.sh

# npm SBOM
./scripts/generate-sbom-npm.sh
```

### Using SBOMs

- **Supply chain compliance**: Track all dependencies
- **Vulnerability correlation**: Cross-reference against databases
- **License tracking**: Identify license obligations
- **Component reporting**: Export to tools like Black Duck

## Vulnerability Tracking

### Trend Analysis

Vulnerabilities are tracked over time to identify patterns:

```
security/trends/
├── vulnerability-history.jsonl   # Daily snapshots
├── trends-report.json            # Current status
└── trends-visualization.md       # Human-readable report
```

### Accessing Trends

```bash
# View current vulnerability count
jq . security/trends/trends-report.json

# Analyze historical data
tail -30 security/trends/vulnerability-history.jsonl
```

## Remediation Workflow

### When Vulnerabilities Are Found

1. **Automatic Report Generation**
   - Security scan creates summary report
   - Severity levels clearly marked
   - Fix recommendations included

2. **PR Comments (on pull requests)**
   - Scan results posted as comments
   - Actionable remediation steps
   - Links to vulnerability details

3. **GitHub Security Tab**
   - Vulnerabilities appear in Security → Dependabot alerts
   - Recommended version updates shown
   - Auto-fix pull requests (if Dependabot enabled)

### Remediation Steps

```bash
# Step 1: Review vulnerability
# - Check security/reports/summary/README.md
# - Understand impact and affected components

# Step 2: Update affected packages
# For npm:
npm update package-name
npm audit fix

# For Cargo:
cargo update package-name

# Step 3: Verify updates
npm test
cargo test

# Step 4: Submit PR
git add -A
git commit -m "chore: update dependencies for security patches"
git push origin branch-name
```

## Configuration

### Environment Variables

```bash
# GitHub Actions
SNYK_TOKEN=xxxx              # Snyk API token
GITHUB_TOKEN=xxxx            # GitHub API token (provided automatically)

# Slack notifications
SLACK_WEBHOOK_URL=xxxx       # For alert notifications
```

### Threshold Settings

Modify severity thresholds in `dependency-security-scan.yml`:

```yaml
- name: Run Snyk scan
  run: snyk test --severity-threshold=high  # Can be: low, medium, high, critical
```

### Custom Rules

Create `.snyk` policy file for custom exclusions:

```json
{
  "version": "1.19.0",
  "ignore": {
    "CVE-2021-1234": [
      {
        "path": "node_modules/package-name",
        "expiry": "2024-12-31T00:00:00.000Z",
        "reason": "Issue is not applicable"
      }
    ]
  }
}
```

## Security Best Practices

### 1. Keep Dependencies Updated

```bash
# Regular updates (weekly)
npm update
cargo update

# Check for outdated packages
npm outdated
cargo outdated
```

### 2. Monitor Continuously

- Enable Dependabot alerts (GitHub Settings → Security & analysis)
- Review alerts weekly
- Subscribe to security mailing lists for critical frameworks

### 3. Lock Versions

```bash
# Commit lock files to version control
git add package-lock.json
git add Cargo.lock
git commit -m "chore: lock dependency versions"
```

### 4. Automate Updates

Use GitHub's Dependabot to automatically:
- Create PR for security updates
- Test updates in CI
- Auto-merge safe updates

### 5. Review Dependencies

Regularly audit all dependencies:

```bash
# See what's installed
npm list --depth=0

# Check for problematic licenses
npm audit --audit-level=moderate --format=json | grep -i license

# Identify security risks
cargo audit --json | jq '.vulnerabilities'
```

## Handling False Positives

### Whitelisting Vulnerabilities

If a vulnerability is not applicable:

```bash
# Create .snyk policy
echo '{"version":"1.19.0","ignore":{}}' > .snyk

# Add exception
snyk ignore --id=CVE-2021-1234 --reason="Not applicable"
```

### Documenting Exceptions

Always document why a vulnerability is accepted:

```bash
# Good example
snyk ignore --id=CVE-2021-1234 \
  --reason="Impact limited to optional feature not used in production" \
  --expiry="2024-12-31"
```

## Reporting and Compliance

### Security Scan Reports

Available in GitHub Actions artifacts:

- `cargo-audit-report` - Rust vulnerabilities
- `npm-audit-report-node-*` - Node.js vulnerabilities
- `snyk-report` - Comprehensive Snyk analysis
- `sbom-reports` - Software Bill of Materials
- `security-scan-summary` - Executive summary

### Export for Compliance

```bash
# Export SBOM for audit
jq . security/sbom/sbom-npm.json > audit-sbom-export.json

# Export vulnerabilities
cat security/reports/summary/README.md > security-audit-report.md

# Archive everything
tar czf security-audit-$(date +%Y-%m-%d).tar.gz \
  security/sbom/ \
  security/reports/
```

## Troubleshooting

### Cargo Audit Fails

```bash
# Update cargo-audit
cargo install cargo-audit --locked --force

# Check for outdated Rust toolchain
rustup update
```

### npm Audit Shows False Positives

```bash
# Update npm and dependencies
npm install -g npm@latest
npm ci

# Clear npm cache
npm cache clean --force
```

### Snyk Token Issues

```bash
# Verify Snyk token
snyk config get api

# Reauthenticate
snyk auth
```

## Additional Resources

- [RustSec Advisory Database](https://rustsec.org/)
- [npm Security Advisories](https://www.npmjs.com/advisories)
- [OWASP Dependency Check](https://owasp.org/www-project-dependency-check/)
- [CycloneDX SBOM Standard](https://cyclonedx.org/)
- [GitHub Dependabot](https://docs.github.com/en/code-security/dependabot)
