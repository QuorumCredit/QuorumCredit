# Quorum Policy Guide

`server/src/quorum/quorumPolicy.ts` contains lightweight server-side helpers for
operator-facing quorum configuration and analysis.

- Endpoint rate-limit configuration: validates and stores per-endpoint request
  limits that can be surfaced by a dashboard.
- Quorum slice difficulty scoring: combines stake, reputation, slice size, and
  region diversity into a 0-100 score.
- Attestor reputation decay: applies a basis-point daily decay with a configured
  floor for inactive attestors.
- Slice composition evolution: merges current and candidate attestors and keeps
  the highest-scoring members for the target slice size.

These helpers are storage-agnostic. A future persistence adapter or dashboard
route can wrap the exported `quorumPolicyStore` without changing the scoring
rules.
