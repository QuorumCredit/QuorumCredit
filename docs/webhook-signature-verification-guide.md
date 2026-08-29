# Webhook Signature Verification & Subscription Guide

This document covers:
1. How to register, manage, and authenticate webhook subscriptions in QuorumCredit.
2. How to verify webhook delivery signatures on the receiving end.
3. Subscription limits and rate controls (Issue #111).

---

## Overview

QuorumCredit emits contract events for key lifecycle actions (loans, vouches, slashes). Off-chain systems subscribe to these events via **webhook subscriptions** managed by the `WebhookRegistry` module (`src/webhook_registry.rs`). Each registered URL receives a signed HTTP POST payload for every matching event.

---

## Subscription Limits (Issue #111)

### Default Limit

> **`MAX_WEBHOOKS_PER_SUBJECT = 10`**

Each `(subject, caller)` pair is capped at **10 active webhook subscriptions** by default. Attempting to register beyond this limit returns a `LimitExceeded` error.

| Dimension      | Limit              | Notes |
|----------------|--------------------|-------|
| Subscriptions per (subject, caller) | `MAX_WEBHOOKS_PER_SUBJECT` = **10** | Default; admin-configurable |
| Minimum limit  | 1                  | Cannot be set to 0 |
| Maximum limit  | No hard cap        | Governed by admin via `set_limit()` |

### Why This Limit Exists

1. **Storage protection**: Soroban persistent storage has metered costs. Unbounded subscriptions would allow a single caller to inflate contract storage costs.
2. **Dispatch cost**: Event dispatch iterates over all active subscriptions. A bounded count prevents runaway compute costs at event emission time.
3. **Spam prevention**: Limits the ability of a malicious actor to register thousands of endpoints to overwhelm delivery infrastructure.

### Changing the Limit (Admin Only)

Admins can override the global per-subject limit:

```rust
// In contract admin handler
WebhookRegistry::set_limit(&env, 20); // raise to 20
```

The updated limit takes effect immediately for all new registration attempts. Existing subscriptions are not retroactively invalidated.

---

## Registering a Webhook Subscription

### Function Signature

```rust
pub fn register(
    env: &Env,
    owner: Address,     // must sign the transaction
    subject: String,    // event topic / borrower address
    url: String,        // HTTPS delivery endpoint
) -> Result<(), WebhookRegistryError>
```

### Constraints

- `owner` must authenticate (`require_auth()` is called internally).
- `url` must be unique within the same `(subject, owner)` pair — duplicate URLs return `DuplicateUrl`.
- Registration is rejected with `LimitExceeded` when the caller already has `get_limit()` active subscriptions for `subject`.
- Soft-deleted (inactive) subscriptions do **not** count toward the limit.

### Example

```typescript
// JavaScript / TypeScript — using stellar-sdk
await contract.invoke('register_webhook', {
  owner: callerKeypair.publicKey(),
  subject: borrowerAddress,   // receive events for this borrower
  url: 'https://myapp.com/webhooks/quorum',
});
```

---

## Unregistering a Subscription

```rust
pub fn unregister(
    env: &Env,
    owner: Address,
    subject: String,
    url: String,
) -> Result<(), WebhookRegistryError>
```

Marks the subscription inactive (soft-delete). The audit trail is preserved; the slot is freed for future registrations.

---

## Querying Active Subscriptions

```rust
pub fn get_subscriptions(
    env: &Env,
    owner: Address,
    subject: String,
) -> Vec<WebhookSubscription>
```

Returns only **active** subscriptions. Inactive (unregistered) entries are filtered out.

---

## Webhook Payload Format

Each delivery is a JSON POST with the following envelope:

```json
{
  "event_topic": "loan/repaid",
  "contract_id": "CABC...XYZ",
  "timestamp": 1700000000,
  "ledger_sequence": 12345678,
  "payload": {
    "borrower": "GXXX...YYY",
    "amount": 5000000000
  },
  "signature": "base64-encoded-HMAC-SHA256"
}
```

| Field             | Type     | Description |
|-------------------|----------|-------------|
| `event_topic`     | `string` | Matches the contract event topic (e.g. `"loan/repaid"`) |
| `contract_id`     | `string` | Soroban contract address that emitted the event |
| `timestamp`       | `u64`    | Unix timestamp of the Stellar ledger close |
| `ledger_sequence` | `u32`    | Ledger sequence number |
| `payload`         | `object` | Event-specific data (see Event Reference below) |
| `signature`       | `string` | HMAC-SHA256 of the raw request body, Base64-encoded |

---

## Signature Verification

QuorumCredit signs outbound webhooks using **HMAC-SHA256** with a shared secret. The shared secret is stored securely off-chain — never in contract storage.

### Verification Steps

1. Extract the `signature` field from the JSON body **before** parsing (or use the raw body bytes).
2. Compute `HMAC-SHA256(secret_key, raw_request_body)`.
3. Base64-encode the digest.
4. Compare with the received `signature` using a **constant-time comparison** to prevent timing attacks.

### Node.js Example

```typescript
import crypto from 'crypto';

function verifyWebhookSignature(
  rawBody: Buffer,
  receivedSig: string,
  secret: string
): boolean {
  const expected = crypto
    .createHmac('sha256', secret)
    .update(rawBody)
    .digest('base64');
  
  // Use timingSafeEqual to prevent timing attacks
  const a = Buffer.from(expected, 'base64');
  const b = Buffer.from(receivedSig, 'base64');
  if (a.length !== b.length) return false;
  return crypto.timingSafeEqual(a, b);
}

// Express.js handler
app.post('/webhooks/quorum', express.raw({ type: 'application/json' }), (req, res) => {
  const body = JSON.parse(req.body.toString());
  const isValid = verifyWebhookSignature(
    req.body,            // raw Buffer from express.raw()
    body.signature,
    process.env.WEBHOOK_SECRET!
  );
  
  if (!isValid) {
    return res.status(401).json({ error: 'Invalid signature' });
  }
  
  // Process event
  console.log('Received event:', body.event_topic, body.payload);
  res.status(200).send('OK');
});
```

### Python Example

```python
import hmac
import hashlib
import base64

def verify_webhook_signature(raw_body: bytes, received_sig: str, secret: str) -> bool:
    expected = hmac.new(
        secret.encode('utf-8'),
        raw_body,
        hashlib.sha256
    ).digest()
    expected_b64 = base64.b64encode(expected).decode()
    # Constant-time comparison
    return hmac.compare_digest(expected_b64, received_sig)
```

> [!IMPORTANT]
> Always use **constant-time comparison** (`crypto.timingSafeEqual` / `hmac.compare_digest`). Standard string equality is vulnerable to timing attacks that could allow an attacker to forge signatures.

---

## Retry Policy

Failed deliveries are retried with exponential backoff (see `src/webhook_retry.rs`):

| Attempt | Delay |
|---------|-------|
| 1       | 1s    |
| 2       | 2s    |
| 3       | 4s    |
| 4       | 8s    |
| 5       | 16s   |

After 5 failed attempts, the delivery is marked exhausted. If the circuit breaker opens (3 consecutive failures), deliveries to that endpoint are paused for 60 seconds before a probe attempt.

---

## Circuit Breaker Behaviour

The `CircuitBreaker` (Issue #110) protects the delivery infrastructure from repeatedly attempting to deliver to a permanently-failed endpoint:

| State    | Behaviour |
|----------|-----------|
| `Closed` | Deliveries proceed normally. |
| `Open`   | Deliveries skipped for `cooldown_secs` (default: 60s). |
| `HalfOpen` | One probe attempt allowed. Success → Closed. Failure → Open. |

The circuit opens after `DEFAULT_FAILURE_THRESHOLD = 3` consecutive failures.

---

## Event Reference

| Topic             | Trigger               | Key Payload Fields |
|-------------------|-----------------------|--------------------|
| `vouch/create`    | New vouch             | `voucher`, `borrower`, `stake`, `token` |
| `vouch/increase`  | Stake increased       | `voucher`, `borrower`, `additional_stake`, `token` |
| `vouch/decrease`  | Stake decreased       | `voucher`, `borrower`, `reduced_stake`, `token` |
| `vouch/withdraw`  | Vouch withdrawn       | `voucher`, `borrower`, `returned_stake`, `token` |
| `loan/request`    | Loan disbursed        | `borrower`, `amount`, `threshold`, `loan_purpose`, `token` |
| `loan/repay`      | Loan repaid           | `borrower`, `payment` |
| `loan/slash`      | Default slashed       | `borrower`, `slashed_amount` |
| `contract/init`   | Contract initialized  | `deployer`, `admins`, `admin_threshold`, `token` |
| `admin/pause`     | Contract paused       | `admin` |
| `admin/unpause`   | Contract unpaused     | `admin` |

---

## Error Reference

| Error Code | Variant        | Meaning |
|------------|----------------|---------|
| `LimitExceeded` | —         | Caller has reached `MAX_WEBHOOKS_PER_SUBJECT` active subscriptions for this subject. |
| `DuplicateUrl`  | —         | The same URL is already actively registered for this subject/owner. |
| `NotFound`      | —         | No matching active subscription found for the given URL. |

---

## Security Considerations

- **HTTPS only**: Only register endpoints with `https://` URLs. Plain HTTP endpoints risk exposing payload data in transit.
- **Validate Content-Type**: Reject requests that don't carry `Content-Type: application/json`.
- **Idempotency**: Webhook payloads include `ledger_sequence` + `event_topic`. Use this as an idempotency key to deduplicate retries.
- **Timeout**: Your endpoint should respond with HTTP 2xx within **5 seconds**. Timeouts count as delivery failures.
- **Firewall**: Consider allowlisting the Stellar event indexer's IP range if your infrastructure supports it.

---

## See Also

- [Circuit Breaker implementation](../src/webhook_retry.rs)
- [Webhook Registry implementation](../src/webhook_registry.rs)
- [Event Indexing Guide](./event-indexing-guide.md)
- [Monitoring Guide](./monitoring-guide.md)
