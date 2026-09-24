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

## Webhook Signature Verification Standard

All inbound webhooks must be authenticated with an HMAC-SHA256 signature so that
receivers can verify the payload originated from QuorumCredit and was not tampered
with in transit.

### Signature Header

Every webhook request includes the following header:

| Header | Description |
|--------|-------------|
| `X-Webhook-Signature` | Hex-encoded HMAC-SHA256 digest of the raw request body, keyed with the shared webhook secret |

### Signing Algorithm

1. Compute `HMAC-SHA256(secret, raw_request_body)` over the **exact** raw bytes of the request body (do not re-serialize or re-order JSON).
2. Hex-encode the resulting digest (lowercase).
3. Send the value in the `X-Webhook-Signature` header.

### Verification Requirements

- Recompute the HMAC-SHA256 digest on the receiver side using the shared secret and the raw body.
- Compare the recomputed digest against the header value using a **constant-time** comparison to avoid timing side channels.
- Reject the request with `401 Unauthorized` when the header is missing, malformed, or does not match.
- Reject requests whose timestamp is outside the allowed tolerance window to mitigate replay attacks.

### Failure Tracking

Signature verification failures must be tracked so that repeated or anomalous
failures can be detected and investigated:

- Log each failure with the request source, timestamp, and reason (missing header, malformed signature, digest mismatch).
- Emit a metric/counter for verification failures to enable alerting on spikes.
- Never log the shared secret or the full signature value.

---

## Scope

The following are **in scope**:

- Smart contract logic in `QuorumCredit/src/`
- Authentication and authorization bypasses (`require_auth`, admin multisig)
- Fund loss or theft (voucher stakes, loan principal, yield reserve)
- Reentrancy or state corruption vulnerabilities
- Denial-of-service attacks that permanently brick the contract
- Webhook signature verification bypasses or replay attacks

The following are **out of scope**:

- Issues in third-party dependencies (report to the upstream maintainer)
- Theoretical attacks with no practical exploit path
- Issues already publicly known or previously reported

---

## Security Best Practices for Deployers

- Never commit `.env` files or secret keys — add `.env` to `.gitignore`
- Use hardware wallets or multisig for admin keys
- Set `admin_threshold > 1` in production to require M-of-N signatures
- Run `cargo audit` before every deployment: `cargo install cargo-audit && cargo audit`
- Follow the required deployment sequence: build → deploy → initialize (same keypair)
- Rotate webhook secrets regularly and store them in a secret manager, never in source control

---

## Contact

| Channel | Address |
|---------|---------|
| Security email | `security@quorumcredit.io` |
| GitHub advisories | [Submit advisory](https://github.com/your-org/QuorumCredit/security/advisories/new) |
| General contact | [Stellar Developer Discord](https://discord.gg/stellardev) |
