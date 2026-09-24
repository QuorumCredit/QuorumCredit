/**
 * Per-connection sliding-window rate limiter for inbound WS messages.
 *
 * Each connection gets its own WsRateLimiter instance. On every inbound
 * message the caller invokes `check()`:
 *
 *   - Returns `"ok"` if the request is within the limit.
 *   - Returns `"throttled"` on the FIRST violation inside a window
 *     (warn the client; let the current message be silently dropped).
 *   - Returns `"disconnect"` once the number of throttle violations in a
 *     row reaches `maxViolations` — the caller should close the socket with
 *     close-code 4029 ("Too Many Requests") so the client knows why.
 *
 * Algorithm: sliding-window log. We keep a circular buffer of timestamps for
 * accepted messages; any timestamp older than `windowMs` is evicted before
 * counting. This gives true per-window semantics without a fixed-bucket
 * boundary artefact (where a burst split across two adjacent fixed buckets
 * appears to be under the limit).
 */
export type RateLimitDecision = "ok" | "throttled" | "disconnect";

export interface WsRateLimiterOptions {
  /** Width of the sliding window in milliseconds. Default: 1 000 ms. */
  windowMs?: number;
  /** Maximum accepted messages per window. Default: 30. */
  maxMessages?: number;
  /**
   * How many consecutive throttle hits before upgrading the decision to
   * "disconnect". Default: 3.
   */
  maxViolations?: number;
}

export class WsRateLimiter {
  private readonly windowMs: number;
  private readonly maxMessages: number;
  private readonly maxViolations: number;

  /** Timestamps (ms) of accepted messages within the current window. */
  private readonly log: number[] = [];
  /** Consecutive throttle violations since the last accepted message. */
  private violations = 0;

  constructor(opts: WsRateLimiterOptions = {}) {
    this.windowMs = opts.windowMs ?? 1_000;
    this.maxMessages = opts.maxMessages ?? 30;
    this.maxViolations = opts.maxViolations ?? 3;
  }

  /**
   * Call once per inbound message. Returns the decision the caller should
   * act on.
   */
  check(nowMs: number = Date.now()): RateLimitDecision {
    this.evict(nowMs);

    if (this.log.length < this.maxMessages) {
      // Accepted — reset violation streak.
      this.log.push(nowMs);
      this.violations = 0;
      return "ok";
    }

    // Over limit — count violation.
    this.violations += 1;
    if (this.violations >= this.maxViolations) {
      return "disconnect";
    }
    return "throttled";
  }

  /** Reset state — useful when a connection is recycled in tests. */
  reset(): void {
    this.log.length = 0;
    this.violations = 0;
  }

  // ── private ──────────────────────────────────────────────────────────────

  private evict(nowMs: number): void {
    const cutoff = nowMs - this.windowMs;
    // Log is chronological: drop from the front while stale.
    let i = 0;
    while (i < this.log.length && this.log[i] <= cutoff) i++;
    if (i > 0) this.log.splice(0, i);
  }
}
