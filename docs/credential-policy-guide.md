# Credential Policy Guide

This guide documents the server-side credential policy helpers in
`server/src/credentials/credentialPolicy.ts`.

## Capabilities

- Transactional batching: `issueCredentialBatch` validates every request before
  committing any credential. If one request fails, the whole batch is rejected.
- Holder lockout: repeated holder failures are tracked with a rolling window and
  temporary lockout period.
- Conditional issuance: each request can require a minimum score, an allowlist
  of issuers, a `notBefore` timestamp, and an expiry timestamp.
- Tiering: issued credentials are assigned `bronze`, `silver`, `gold`, or
  `platinum` from the holder credit score.

## Tier Thresholds

| Tier | Minimum score |
| --- | ---: |
| bronze | 0 |
| silver | 550 |
| gold | 700 |
| platinum | 850 |

## Minimal Usage

```ts
import { credentialPolicyStore } from "../credentials/credentialPolicy.js";

const result = credentialPolicyStore.issueCredentialBatch([
  {
    id: "cred-1",
    holder: "G...",
    issuer: "issuer-mainnet",
    creditScore: 740,
    condition: {
      minCreditScore: 700,
      allowedIssuers: ["issuer-mainnet"],
      expiresAfter: Date.now() + 30 * 24 * 60 * 60 * 1000,
    },
  },
]);

if (!result.accepted) {
  console.error(result.errors);
}
```

The module is intentionally storage-agnostic for now. A later route or database
adapter can wrap the same policy object without changing the validation rules.
