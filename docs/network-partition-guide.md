# Network Partition Handling and Recovery (Issue #1229)

The `quorum-credit-broadcast-server` (`server/`) previously had no explicit handling for losing connectivity to its validator/indexer network — a stalled `Bridge` tick loop (see `server/src/bridge/bridge.ts`) just retried silently forever with no visible state change and no protection for in-flight writes. `server/src/resilience/partitionGuard.ts` adds detection, a read-only mode, and a bounded write-replay queue.

## Detection

`Bridge.tick()` already wraps its bus/store access in a try/catch (needed regardless, since a leader-lock acquisition or an indexer DB read can transiently fail). `PartitionGuard` piggybacks on that same success/failure signal:

- `PARTITION_FAILURE_THRESHOLD` (default `5`) consecutive failed ticks flips the instance into partitioned state.
- The very next successful tick clears it immediately (no threshold on recovery — a single good tick is enough evidence connectivity is back).

This means partition detection is per-instance, not cluster-wide: in a multi-replica deployment behind Redis, each instance independently decides whether *it* can reach the bus/indexer DB.

## Guarantees while partitioned

**Reads are never blocked.** Every `GET` endpoint keeps serving whatever state this instance last had from the indexer DB — there's no reason to fail a read just because outbound connectivity is degraded.

**Writes are queued, not rejected, and not silently dropped-per-request.** Mutating endpoints (`POST /loans/:id/expenses`, `POST`/`DELETE /loans/:id/recurring-payment`, `POST /loans/:id/recurring-payment/execute`) check `PartitionGuard.isPartitioned()` before acting:

- If healthy: applied immediately, exactly as before this issue.
- If partitioned: the write is enqueued and the endpoint responds `202 Accepted` with `{ queued: true, reason: "..." }` instead of the normal success payload — callers get an explicit signal that their write hasn't landed yet rather than a misleading 201/200.

**Recovery replays the queue automatically**, in FIFO order, as soon as the next successful bridge tick clears partitioned state — no operator action required.

## What this does NOT guarantee

- **The queue is in-memory only.** A process restart while partitioned loses whatever was queued. This service has no durable write-ahead log; documenting the limitation here is deliberate rather than silently accepting data loss on restart without saying so.
- **The queue is bounded** (`PARTITION_MAX_QUEUED_WRITES`, default `500`). A partition that outlasts that many accepted writes drops the *oldest* queued write to make room for new ones — counted in `droppedWrites` (`GET /status/partition`) and the `qc_partition_writes_dropped_total` metric, so an operator can see it happened instead of it failing silently.
- **`/loans/:id/recurring-payment/execute` queues the retry attempt, not an assumed outcome.** On-chain fund movement can't be pre-computed while we can't reach the chain, so replay re-runs the real `executeRecurringPayment` retry-with-backoff path once connectivity is back, rather than fabricating a result now.
- **Detection is per-instance state**, not shared across replicas — there's no cluster-wide partition flag today.

## Observability

- `GET /status/partition` → `{ partitioned, partitionedSince, consecutiveFailures, queueDepth, droppedWrites }`
- Metrics (via `GET /metrics`): `qc_partition_detected_total`, `qc_partition_recovered_total`, `qc_partition_writes_queued_total`, `qc_partition_writes_replayed_total`, `qc_partition_writes_replay_failed_total`, `qc_partition_writes_dropped_total`, `qc_partition_queue_depth` (gauge).

## Testing partition recovery scenarios

`scripts/test_partition_recovery.sh` drives a running instance's HTTP surface to verify the queue/recovery lifecycle: it checks `/status/partition`, attempts a write and confirms it's applied or queued consistently with the reported state, confirms reads always succeed, and (when already partitioned) polls until recovery and confirms the queue drains. Inducing an actual partition requires stopping the instance's Redis/bus dependency in the target environment — the script documents the exact steps to do that and re-run the drill across the before/during/after states.

## Configuration

| Env var | Default | Meaning |
|---|---|---|
| `PARTITION_FAILURE_THRESHOLD` | `5` | Consecutive failed bridge ticks before entering read-only mode |
| `PARTITION_MAX_QUEUED_WRITES` | `500` | Bounded write-replay queue capacity; oldest dropped beyond this |

## Post-Recovery Reconciliation

When `droppedWrites > 0` after a partition clears, the `PartitionGuard` automatically:

1. **Logs a `WARN`-level alert** (not just a metric increment) with the exact count of dropped writes and a pointer to this document.
2. **Increments `qc_partition_dropped_writes_on_recovery_total`** so the alert is also visible in Prometheus.
3. **Calls the optional `reconcile` callback** (if configured) to diff local projected state against the indexer.

### Configuring the reconcile callback

```typescript
const guard = new PartitionGuard({
  failureThreshold: 5,
  maxQueuedWrites: 500,
  reconcile: async () => {
    // Diff local LoanProjector state against the indexer DB
    // and flag any loans whose status diverged during the partition.
    const localLoans = projector.getAll();
    const indexerLoans = await indexerStore.getActiveLoans();
    for (const loan of indexerLoans) {
      const local = localLoans.find(l => l.id === loan.id);
      if (!local || local.status !== loan.status) {
        console.warn(`[Reconcile] Loan ${loan.id} status mismatch: local=${local?.status}, indexer=${loan.status}`);
      }
    }
  },
});
```

### Manual Recovery Procedure

If `droppedWrites > 0` is reported after recovery:

1. Check `GET /status/partition` — note the `droppedWrites` count.
2. Query the indexer DB directly for the affected time window:
   ```sql
   SELECT * FROM events WHERE ledger_closed_at >= '<partition_start>' ORDER BY ledger ASC;
   ```
3. Compare indexer state against the in-memory projector state (`GET /status/loans` or socket snapshot).
4. For any diverged loans, the safest path is a full projector rebuild from the indexer event log.
5. Call `guard.resetDroppedWrites()` once reconciliation is confirmed complete.
6. Monitor `qc_partition_dropped_writes_on_recovery_total` — it should not increment again if the root cause is fixed.

### New Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `qc_partition_dropped_writes_on_recovery_total` | Counter | Incremented each time recovery finds dropped writes — each increment means diverged state is possible |
| `qc_partition_reconcile_triggered_total` | Counter | How many times the reconcile callback was invoked |
| `qc_partition_reconcile_failed_total` | Counter | How many times the reconcile callback threw an error |
