/**
 * Attestor feedback loop (issue #1771).
 *
 * Borrowers and peers rate attestors on a 1–5 scale. Each rater holds at most
 * one active rating per attestor (re-submitting replaces it), so one account
 * cannot inflate an attestor's reputation by repeating itself. A rolling trend
 * compares the most recent ratings against the window before them, and the
 * reputation adjustment is derived from the average once enough independent
 * ratings exist.
 */

export const FEEDBACK_MIN_SCORE = 1;
export const FEEDBACK_MAX_SCORE = 5;
export const FEEDBACK_NEUTRAL_SCORE = 3;
export const FEEDBACK_MAX_COMMENT_LENGTH = 500;

/** Reputation is expressed in basis points (10 000 = 100 %). */
export const BASE_REPUTATION_BPS = 5_000;
/** Largest reputation move in either direction, in basis points. */
export const MAX_REPUTATION_ADJUSTMENT_BPS = 2_000;
/** Ratings required before feedback moves an attestor's reputation at all. */
export const MIN_RATINGS_FOR_ADJUSTMENT = 3;
/** Number of most-recent ratings compared against the window before them. */
export const DEFAULT_TREND_WINDOW = 10;
/** Average-score change needed before a trend is reported as up or down. */
const TREND_THRESHOLD = 0.25;

export class FeedbackValidationError extends Error {}

export interface AttestorFeedback {
  attestorId: string;
  raterId: string;
  score: number;
  comment?: string;
  submittedAt: number;
}

export type FeedbackTrendDirection = "up" | "down" | "flat";

export interface FeedbackTrend {
  windowSize: number;
  recentAverage: number | null;
  previousAverage: number | null;
  direction: FeedbackTrendDirection;
}

export interface AttestorFeedbackSummary {
  attestorId: string;
  count: number;
  average: number | null;
  trend: FeedbackTrend;
  reputationBps: number;
}

export interface AttestorFeedbackStoreOptions {
  now?: () => number;
  trendWindow?: number;
}

export class AttestorFeedbackStore {
  private readonly ratings = new Map<string, Map<string, AttestorFeedback>>();
  private readonly now: () => number;
  private readonly trendWindow: number;

  constructor(options: AttestorFeedbackStoreOptions = {}) {
    this.now = options.now ?? Date.now;
    this.trendWindow = options.trendWindow ?? DEFAULT_TREND_WINDOW;
  }

  /** Record (or replace) a rater's feedback for an attestor and return the new summary. */
  submit(attestorId: string, raterId: string, score: unknown, comment?: unknown): AttestorFeedbackSummary {
    requireNonEmpty(attestorId, "attestorId");
    requireNonEmpty(raterId, "raterId");
    if (attestorId === raterId) {
      throw new FeedbackValidationError("attestors cannot rate themselves");
    }
    if (
      typeof score !== "number" ||
      !Number.isInteger(score) ||
      score < FEEDBACK_MIN_SCORE ||
      score > FEEDBACK_MAX_SCORE
    ) {
      throw new FeedbackValidationError(
        `score must be an integer between ${FEEDBACK_MIN_SCORE} and ${FEEDBACK_MAX_SCORE}`
      );
    }
    if (comment !== undefined && comment !== null) {
      if (typeof comment !== "string") {
        throw new FeedbackValidationError("comment must be a string");
      }
      if (comment.length > FEEDBACK_MAX_COMMENT_LENGTH) {
        throw new FeedbackValidationError(
          `comment cannot exceed ${FEEDBACK_MAX_COMMENT_LENGTH} characters`
        );
      }
    }

    let byRater = this.ratings.get(attestorId);
    if (!byRater) {
      byRater = new Map();
      this.ratings.set(attestorId, byRater);
    }
    const entry: AttestorFeedback = {
      attestorId,
      raterId,
      score,
      submittedAt: this.now(),
    };
    if (typeof comment === "string") entry.comment = comment;
    byRater.set(raterId, entry);

    return this.summary(attestorId);
  }

  summary(attestorId: string): AttestorFeedbackSummary {
    const entries = this.sortedEntries(attestorId);
    const average = mean(entries.map((entry) => entry.score));
    return {
      attestorId,
      count: entries.length,
      average,
      trend: this.trend(entries),
      reputationBps: reputationFromAverage(average, entries.length),
    };
  }

  /** Feedback for an attestor, oldest first. */
  list(attestorId: string): AttestorFeedback[] {
    return this.sortedEntries(attestorId).map((entry) => ({ ...entry }));
  }

  private sortedEntries(attestorId: string): AttestorFeedback[] {
    const byRater = this.ratings.get(attestorId);
    if (!byRater) return [];
    return Array.from(byRater.values()).sort((a, b) => a.submittedAt - b.submittedAt);
  }

  private trend(entries: AttestorFeedback[]): FeedbackTrend {
    const window = this.trendWindow;
    const recent = entries.slice(-window);
    const previous = entries.slice(Math.max(0, entries.length - 2 * window), Math.max(0, entries.length - window));
    const recentAverage = mean(recent.map((entry) => entry.score));
    const previousAverage = mean(previous.map((entry) => entry.score));

    let direction: FeedbackTrendDirection = "flat";
    if (recentAverage !== null && previousAverage !== null) {
      const delta = recentAverage - previousAverage;
      if (delta >= TREND_THRESHOLD) direction = "up";
      else if (delta <= -TREND_THRESHOLD) direction = "down";
    }
    return { windowSize: window, recentAverage, previousAverage, direction };
  }
}

/**
 * Map an average score onto a bounded reputation adjustment around the base.
 * An average of 5 adds the maximum, 1 subtracts it, and 3 is neutral.
 */
export function reputationFromAverage(average: number | null, count: number): number {
  if (average === null || count < MIN_RATINGS_FOR_ADJUSTMENT) return BASE_REPUTATION_BPS;
  const normalized = (average - FEEDBACK_NEUTRAL_SCORE) / (FEEDBACK_MAX_SCORE - FEEDBACK_NEUTRAL_SCORE);
  const adjustment = Math.round(normalized * MAX_REPUTATION_ADJUSTMENT_BPS);
  return Math.min(10_000, Math.max(0, BASE_REPUTATION_BPS + adjustment));
}

function mean(values: number[]): number | null {
  if (values.length === 0) return null;
  const total = values.reduce((sum, value) => sum + value, 0);
  return Math.round((total / values.length) * 100) / 100;
}

function requireNonEmpty(value: unknown, field: string): void {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new FeedbackValidationError(`${field} is required`);
  }
}

export const attestorFeedbackStore = new AttestorFeedbackStore();
