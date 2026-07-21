# quorum-credit-broadcast-server

Real-time, multi-instance-safe event broadcast backend for the QuorumCredit dashboard
(issue #1142). Tails the SQLite event log written by
[`tools/indexer`](../tools/indexer) and fans events out to `dashboard/src/useLoanSocket.ts`
(socket.io, `/`) and `dashboard/src/useMetricsSocket.ts` (raw WebSocket, `/ws/metrics`),
resumable from any point, across as many replicas as you run.

```
Soroban RPC → tools/indexer (Rust) → indexer.db (SQLite `events` table)
                                              │
                                    bridge (tails table, leader-elected)
                                              │  publish(BroadcastEvent)
                                        PubSubBus (RedisBus in prod)
                                         /                        \
                          instance A (socket.io + ws)    instance B (socket.io + ws)  ...
                                 │                                │
                          dashboard clients                dashboard clients
```

## Why this exists

Before this service, `useLoanSocket`/`useMetricsSocket` had nothing real to connect
to — no server anywhere in the repo emitted the events they expect, and a naive
single-process implementation would break the moment the API layer runs as more than
one replica. This service is that server, built so any instance can serve any client
regardless of which instance actually observed the underlying event.

## Running locally

```bash
npm install
INDEXER_DB_PATH=../tools/indexer/indexer.db npm run dev
```

Without `REDIS_URL` set, it falls back to an in-process `LocalBus` — fine for a single
instance locally, **not** safe for more than one replica (a loud warning is logged).
For real multi-instance behavior:

```bash
docker compose up   # redis + two broadcast-server replicas, ports 4000 and 4001
```

## Config (env vars)

| Var | Default | Meaning |
|---|---|---|
| `PORT` | `4000` | HTTP/WS listen port |
| `REDIS_URL` | unset | Redis pub/sub backbone; required for >1 instance |
| `INDEXER_DB_PATH` | `indexer.db` | Path to the indexer's SQLite file (read-only) |
| `AUTH_SECRET` | dev default — **override in production** | HMAC signing key for tokens |
| `TOKEN_TTL_SECONDS` | `300` | Issued-token lifetime |
| `CONN_QUEUE_MAX` | `500` | Per-connection outgoing queue capacity (see Backpressure) |
| `BRIDGE_POLL_INTERVAL_MS` | `250` | How often the bridge polls the indexer DB for new rows |
| `LEADER_LOCK_TTL_MS` | `5000` | Bridge leader-lock lease duration |
| `INSTANCE_ID` | random | Identifies this replica in logs/leader election |

## Protocol

**Auth**: `POST /api/auth/token` with `{ "apiKey": "...", "borrower": "..." }` returns
`{ token, expiresAt }`. socket.io clients send it as `auth: { token }`; the raw WS
client sends it as `?token=...` on connect. The server sends `auth_expiring` ahead of
hard expiry (30s default warning window) so clients can refresh in-band —
`auth:refresh` (socket.io) / `{type:"refresh_auth", token}` (raw WS) — without
dropping the connection. If a client never refreshes, the server sends `auth_expired`
and disconnects.

**Resume cursor**: every broadcast event carries the underlying indexer row's `id`
(monotonic). Loan-stream clients pass `since` on `subscribe({borrower, since})` to
replay everything they missed before going live. Metrics is a cumulative gauge, not a
list, so a reconnecting client just gets the current cumulative snapshot rather than a
replay of every intermediate one.

**Backpressure / drop policy**: each connection has a bounded outgoing queue
(`CONN_QUEUE_MAX`, `src/ws/connectionQueue.ts`). When it's full, the *oldest* queued
message is dropped to make room for the newest (favor current state over stale state —
every message here is a snapshot-able update, not an irreversible command), and the
client receives a `resync_required{resumeFrom}` control frame so it can request a
replay instead of silently sitting on a gap. Drops increment
`qc_broadcast_messages_dropped_total`, exposed at `GET /metrics`.

## Known gap: loan record fidelity

`tools/indexer`'s event decoder (`indexer.rs::simplify_value`) doesn't currently
extract a real on-chain loan id, deadline, or voucher list from event values — only
borrower/amount/purpose for `loan/request` and borrower/payment for `loan/repay|slash`.
`src/bridge/loanProjector.ts` builds the best `LoanRecord` it can from what's actually
decoded (a borrower-keyed synthetic id, cumulative repay tracking, no fabricated
deadline/voucher data), documented in that file. Full fidelity requires teaching the
indexer's decoder about the complete LoanRecord ABI — a Rust change to
`tools/indexer` that deserves its own review and is out of scope here.

## Tests

```bash
npm test                    # fast, hermetic unit tests (LocalBus, no external services)
npm run test:multi-instance # spawns two real OS processes, proves cross-instance
                             # delivery + asserts p99 < 200ms (kept out of `npm test`
                             # since it's heavier; see multiInstance.test.ts docstring
                             # for why it uses a relay stand-in instead of real Redis
                             # in this sandbox, and how to re-run against real Redis)
npm run loadtest -- --connections 200   # smoke-scale; see scripts/loadtest.ts for
                                         # how to run the full 5k-10k target
```

## Relation to `tools/indexer`

This service only *reads* the indexer's SQLite file (`better-sqlite3`, `readonly:
true`) — it never writes to it, so there's no contention with the indexer process.
Run them side by side, pointed at the same `--db-path`/`INDEXER_DB_PATH`.
