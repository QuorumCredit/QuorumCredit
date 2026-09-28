/**
 * Dead Letter Queue (DLQ) for Failed Async Jobs
 *
 * #1759: Failed async jobs are lost. This module captures them, enables
 * replay, and exposes monitoring metrics.
 *
 * Design:
 * - Failed jobs are routed here from any async worker via `enqueue`.
 * - Jobs carry the original payload, failure reason, and retry history.
 * - `replay` re-runs a specific job through a caller-supplied handler.
 * - `replayAll` replays every non-permanently-failed job (oldest-first).
 * - `getStats` surfaces DLQ depth, failure categories, and replay counts.
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type DlqJobStatus =
  | "pending"    // sitting in the DLQ, not yet replayed
  | "replaying"  // currently being replayed
  | "replayed"   // successfully replayed
  | "dead";      // exceeded max replay attempts — permanently failed

export interface DlqJob<T = unknown> {
  /** Unique job identifier (copied from the original job). */
  id: string;
  /** Original job type / queue name. */
  type: string;
  /** Original job payload. */
  payload: T;
  /** Error message or serialised error from the original failure. */
  failureReason: string;
  /** Timestamp when the job was first enqueued into the DLQ. */
  enqueuedAt: number;
  /** Timestamp of the most recent replay attempt (undefined if never replayed). */
  lastReplayAt?: number;
  /** Number of replay attempts made so far. */
  replayAttempts: number;
  /** Maximum replay attempts before status transitions to "dead". */
  maxReplayAttempts: number;
  status: DlqJobStatus;
}

export type JobHandler<T = unknown> = (job: DlqJob<T>) => Promise<void>;

export interface DlqStats {
  total: number;
  pending: number;
  replaying: number;
  replayed: number;
  dead: number;
  /** Counts per job type. */
  byType: Record<string, number>;
  /** Total successful replays since process start. */
  successfulReplays: number;
  /** Total failed replays since process start. */
  failedReplays: number;
}

// ---------------------------------------------------------------------------
// DeadLetterQueue
// ---------------------------------------------------------------------------

/**
 * In-memory dead letter queue.
 *
 * In production this would be backed by Redis or a database so jobs survive
 * restarts. The interface is intentionally thin so the backing store can be
 * swapped without changing callers.
 */
export class DeadLetterQueue {
  private readonly jobs = new Map<string, DlqJob>();
  private successfulReplays = 0;
  private failedReplays = 0;

  /**
   * Enqueue a failed job into the DLQ.
   *
   * @param id - Original job ID (must be unique per logical job).
   * @param type - Queue / job type name.
   * @param payload - Original job payload.
   * @param failureReason - Error message from the worker that failed.
   * @param maxReplayAttempts - How many replay tries before the job is marked "dead".
   */
  enqueue<T>(
    id: string,
    type: string,
    payload: T,
    failureReason: string,
    maxReplayAttempts: number = 3
  ): DlqJob<T> {
    if (!id.trim()) throw new Error("job id must not be empty");
    if (!type.trim()) throw new Error("job type must not be empty");

    const existing = this.jobs.get(id);
    if (existing) {
      // Update the failure reason and increment replay budget if the same job
      // fails again (e.g. after a successful enqueue but failed delivery).
      existing.failureReason = failureReason;
      existing.status = "pending";
      return existing as DlqJob<T>;
    }

    const job: DlqJob<T> = {
      id,
      type,
      payload,
      failureReason,
      enqueuedAt: Date.now(),
      replayAttempts: 0,
      maxReplayAttempts,
      status: "pending",
    };
    this.jobs.set(id, job as DlqJob);
    return job;
  }

  /**
   * Replay a single job by ID using the supplied handler.
   *
   * - Sets status to "replaying" while the handler runs.
   * - On success: status → "replayed".
   * - On failure and budget remaining: status → "pending".
   * - On failure and budget exhausted: status → "dead".
   *
   * @returns true if the handler succeeded, false otherwise.
   */
  async replay<T>(id: string, handler: JobHandler<T>): Promise<boolean> {
    const job = this.jobs.get(id) as DlqJob<T> | undefined;
    if (!job) throw new Error(`DLQ job not found: ${id}`);
    if (job.status === "replayed") return true; // idempotent
    if (job.status === "dead") return false;

    job.status = "replaying";
    job.lastReplayAt = Date.now();
    job.replayAttempts += 1;

    try {
      await handler(job);
      job.status = "replayed";
      this.successfulReplays += 1;
      return true;
    } catch (err) {
      job.failureReason = err instanceof Error ? err.message : String(err);
      this.failedReplays += 1;

      if (job.replayAttempts >= job.maxReplayAttempts) {
        job.status = "dead";
      } else {
        job.status = "pending";
      }
      return false;
    }
  }

  /**
   * Replay all pending jobs using the supplied handler.
   *
   * Jobs are replayed oldest-first. The handler receives each job in turn;
   * failures do not stop subsequent jobs from being attempted.
   *
   * @returns Summary of the bulk replay.
   */
  async replayAll<T>(handler: JobHandler<T>): Promise<{ attempted: number; succeeded: number; failed: number }> {
    const pending = Array.from(this.jobs.values())
      .filter((j) => j.status === "pending")
      .sort((a, b) => a.enqueuedAt - b.enqueuedAt);

    let succeeded = 0;
    let failed = 0;

    for (const job of pending) {
      const ok = await this.replay(job.id, handler as JobHandler);
      if (ok) succeeded += 1;
      else failed += 1;
    }

    return { attempted: pending.length, succeeded, failed };
  }

  /**
   * Retrieve a job by ID.
   */
  getJob(id: string): DlqJob | undefined {
    return this.jobs.get(id);
  }

  /**
   * List jobs, optionally filtered by status or type.
   */
  listJobs(filter?: { status?: DlqJobStatus; type?: string }): DlqJob[] {
    let jobs = Array.from(this.jobs.values());
    if (filter?.status) jobs = jobs.filter((j) => j.status === filter.status);
    if (filter?.type) jobs = jobs.filter((j) => j.type === filter.type);
    return jobs.sort((a, b) => a.enqueuedAt - b.enqueuedAt);
  }

  /**
   * Remove a job from the DLQ (e.g. after it has been investigated).
   */
  remove(id: string): boolean {
    return this.jobs.delete(id);
  }

  /**
   * Remove all jobs with the given status.
   */
  purge(status: DlqJobStatus): number {
    let count = 0;
    for (const [id, job] of this.jobs) {
      if (job.status === status) {
        this.jobs.delete(id);
        count += 1;
      }
    }
    return count;
  }

  /**
   * DLQ monitoring snapshot.
   */
  getStats(): DlqStats {
    const byType: Record<string, number> = {};
    let pending = 0;
    let replaying = 0;
    let replayed = 0;
    let dead = 0;

    for (const job of this.jobs.values()) {
      byType[job.type] = (byType[job.type] ?? 0) + 1;
      if (job.status === "pending") pending += 1;
      else if (job.status === "replaying") replaying += 1;
      else if (job.status === "replayed") replayed += 1;
      else if (job.status === "dead") dead += 1;
    }

    return {
      total: this.jobs.size,
      pending,
      replaying,
      replayed,
      dead,
      byType,
      successfulReplays: this.successfulReplays,
      failedReplays: this.failedReplays,
    };
  }
}

export const deadLetterQueue = new DeadLetterQueue();
