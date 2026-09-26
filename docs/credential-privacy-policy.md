# Credential Privacy Policy Helpers

`server/src/credentials/privacyPolicy.ts` provides small reusable primitives for
credential privacy workflows:

- holder-name commitments using a salted SHA-256 digest
- cross-credential chain linking without exposing full holder metadata
- anonymity-pool viability summaries
- credential expiration grace-period status calculation

The helpers are intentionally storage-agnostic and can be wired into REST routes,
contract indexers, or dashboard state without changing the privacy calculations.
