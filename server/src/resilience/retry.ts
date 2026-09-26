export interface RetryPolicy {
  /** Number of retries after the initial attempt. */
  maxRetries: number;
  baseDelayMs: number;
  maxDelayMs: number;
  jitter: boolean;
}

export interface RetryStatistics {
  attempts: number;
  retries: number;
  succeeded: number;
  failed: number;
  totalDelayMs: number;
}

export interface RetryExecution<T> {
  value: T;
  attempts: number;
  exhaustedRetries: boolean;
}

const DEFAULT_POLICY: RetryPolicy = {
  maxRetries: 3,
  baseDelayMs: 250,
  maxDelayMs: 10_000,
  jitter: true,
};

const defaultSleep = (ms: number): Promise<void> =>
  new Promise((resolve) => setTimeout(resolve, ms));

/** Configurable exponential retry executor with per-operation metrics. */
export class RetryExecutor {
  private readonly policies = new Map<string, RetryPolicy>();
  private readonly stats = new Map<string, RetryStatistics>();

  constructor(
    private readonly random: () => number = Math.random,
    private readonly sleep: (ms: number) => Promise<void> = defaultSleep
  ) {}

  setPolicy(operation: string, policy: RetryPolicy): void {
    if (!operation.trim()) throw new Error("retry operation must not be empty");
    if (
      !Number.isInteger(policy.maxRetries) ||
      policy.maxRetries < 0 ||
      !Number.isFinite(policy.baseDelayMs) ||
      policy.baseDelayMs < 0 ||
      !Number.isFinite(policy.maxDelayMs) ||
      policy.maxDelayMs < policy.baseDelayMs
    ) {
      throw new Error("invalid retry policy");
    }
    this.policies.set(operation, { ...policy });
  }

  getStatistics(): Record<string, RetryStatistics> {
    return Object.fromEntries(
      [...this.stats.entries()].map(([operation, value]) => [operation, { ...value }])
    );
  }

  async execute<T>(
    operation: string,
    task: (attempt: number) => Promise<T>,
    shouldRetry: (value: T) => boolean,
    options: {
      sleep?: (ms: number) => Promise<void>;
      onAttempt?: (attempt: number, value: T) => void;
      retryOnError?: (error: unknown) => boolean;
    } = {}
  ): Promise<RetryExecution<T>> {
    const policy = this.policies.get(operation) ?? DEFAULT_POLICY;
    const stats = this.stats.get(operation) ?? {
      attempts: 0,
      retries: 0,
      succeeded: 0,
      failed: 0,
      totalDelayMs: 0,
    };
    this.stats.set(operation, stats);

    for (let attempt = 1; ; attempt += 1) {
      stats.attempts += 1;
      let value: T;
      try {
        value = await task(attempt);
      } catch (error) {
        if (
          attempt > policy.maxRetries ||
          !(options.retryOnError?.(error) ?? true)
        ) {
          stats.failed += 1;
          throw error;
        }
        const delay = this.delayFor(policy, attempt);
        stats.retries += 1;
        stats.totalDelayMs += delay;
        await (options.sleep ?? this.sleep)(delay);
        continue;
      }

      options.onAttempt?.(attempt, value);
      if (!shouldRetry(value)) {
        stats.succeeded += 1;
        return { value, attempts: attempt, exhaustedRetries: false };
      }
      if (attempt > policy.maxRetries) {
        stats.failed += 1;
        return { value, attempts: attempt, exhaustedRetries: true };
      }

      const delay = this.delayFor(policy, attempt);
      stats.retries += 1;
      stats.totalDelayMs += delay;
      await (options.sleep ?? this.sleep)(delay);
    }
  }

  private delayFor(policy: RetryPolicy, retryNumber: number): number {
    const exponential = Math.min(
      policy.maxDelayMs,
      policy.baseDelayMs * 2 ** (retryNumber - 1)
    );
    return policy.jitter ? Math.floor(this.random() * (exponential + 1)) : exponential;
  }
}

export const retryExecutor = new RetryExecutor();
