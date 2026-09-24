# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | ✅ Yes     |

---

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Please report vulnerabilities privately so they can be assessed and patched before public disclosure.

### How to Report

1. **Email**: Send a report to `security@quorumcredit.io` with the subject line `[SECURITY] <brief description>`.
2. **GitHub Private Advisory** *(preferred)*: Use [GitHub's private vulnerability reporting](https://github.com/your-org/QuorumCredit/security/advisories/new) to submit directly in the repository.

### What to Include

- Description of the vulnerability and its potential impact
- Steps to reproduce or a proof-of-concept (PoC)
- Affected contract functions or modules
- Suggested fix if you have one

---

## Disclosure Process

1. **Report received** — We acknowledge receipt within **48 hours**.
2. **Assessment** — We assess severity and scope within **5 business days**.
3. **Fix developed** — A patch is developed and reviewed privately.
4. **Coordinated disclosure** — We notify you before publishing the fix and credit you in the release notes (unless you prefer to remain anonymous).
5. **Public disclosure** — Details are published after the fix is deployed, typically within **90 days** of the initial report.

---

## Scope

The following are **in scope**:

- Smart contract logic in `QuorumCredit/src/`
- Authentication and authorization bypasses (`require_auth`, admin multisig)
- Fund loss or theft (voucher stakes, loan principal, yield reserve)
- Reentrancy or state corruption vulnerabilities
- Denial-of-service attacks that permanently brick the contract

The following are **out of scope**:

- Issues in third-party dependencies (report to the upstream maintainer)
- Theoretical attacks with no practical exploit path
- Issues already publicly known or previously reported

---

## Contract Security Audit Framework

This repository ships an automated security audit framework that scans contract
source for common vulnerability classes, suggests remediations, and records an
audit history across runs.

### Vulnerability Detection Rules

The framework evaluates each contract source against a set of detection rules.
Each rule has an `id`, a `severity`, a `pattern` (regular expression matched
against the source), and a `remediation` hint.

| Rule ID | Severity | Detects |
|---------|----------|---------|
| `AUTH-001` | High | State-mutating entrypoints missing a `require_auth` guard |
| `ARITH-001` | High | Unchecked arithmetic (`+`, `-`, `*`) on balances/amounts |
| `REENTRANCY-001` | Critical | External calls made before state is committed |
| `ACCESS-001` | High | Admin-only functions without an admin/threshold check |
| `PANIC-001` | Medium | `unwrap()`/`expect()` on fallible contract calls |
| `OVERFLOW-001` | High | Casts or arithmetic without overflow protection |
| `STORAGE-001` | Medium | Unbounded iteration over persistent storage |

### Pattern Matching

Rules are applied by matching their `pattern` against the contract source. A
finding is produced for every match and carries the rule id, severity, matched
line, and the remediation hint. Findings are sorted by severity so the most
critical issues surface first.

### Auto-Remediation Suggestions

Every finding includes a remediation suggestion derived from its rule. For
example, `AUTH-001` suggests adding a `require_auth` guard to the entrypoint,
and `ARITH-001` suggests using checked arithmetic (`checked_add`, `checked_sub`,
`checked_mul`). Suggestions are advisory and should be reviewed before applying.

### Audit History

Each audit run is recorded with a timestamp, the scanned target, and the set of
findings. The history is retained so regressions and resolved findings can be
tracked across runs.

### Running an Audit

Run the audit framework against the contract source before every deployment and
review any High or Critical findings before proceeding.

---

## Security Best Practices for Deployers

- Never commit `.env` files or secret keys — add `.env` to `.gitignore`
- Use hardware wallets or multisig for admin keys
- Set `admin_threshold > 1` in production to require M-of-N signatures
- Run the contract security audit framework and `cargo audit` before every deployment: `cargo install cargo-audit && cargo audit`
- Follow the required deployment sequence: build → deploy → initialize (same keypair)

---

## Contact

| Channel | Address |
|---------|---------|
| Security email | `security@quorumcredit.io` |
| GitHub advisories | [Submit advisory](https://github.com/your-org/QuorumCredit/security/advisories/new) |
| General contact | [Stellar Developer Discord](https://discord.gg/stellardev) |
