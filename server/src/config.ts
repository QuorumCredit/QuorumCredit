export interface ServerConfig {
  port: number;
  redisUrl: string | undefined;
  indexerDbPath: string;
  authSecret: string;
  tokenTtlSeconds: number;
  /** Bounded per-connection outgoing queue capacity before drop-oldest kicks in. */
  connectionQueueMax: number;
  /** How often the bridge polls the indexer DB for newly-inserted rows. */
  bridgePollIntervalMs: number;
  /** How long a bridge leader lock is held before it must be renewed. */
  leaderLockTtlMs: number;
  instanceId: string;
  /** Deployed version identifier, surfaced on /health for canary release monitoring
   * (issue #1231) — e.g. a git SHA or semver tag set by the deploy pipeline. */
  serviceVersion: string;
  /**
   * Application-level WebSocket heartbeat / idle-timeout settings.
   * Both loanSocketServer (socket.io) and metricsWsServer (raw ws) use these.
   */
  wsHeartbeat: {
    /** How often the server sends a ping to each connection (ms). Default: 30 000. */
    intervalMs: number;
    /**
     * How long after the last pong (or initial connect) before the server tears
     * down the connection as a half-open/idle zombie (ms). Default: 60 000.
     */
    idleTimeoutMs: number;
  };
  /** Cost allocation inputs (issue #1227) — see server/src/costs/costAllocator.ts. */
  costAllocation: {
    contractFeeStroopsPerTx: number;
    apiServerMonthlyCostCents: number;
    storageMonthlyCostCents: number;
    stroopsToCentsRate: number | undefined;
  };
  /** Network partition detection/recovery (issue #1229) — see server/src/resilience/partitionGuard.ts. */
  partitionGuard: {
    failureThreshold: number;
    maxQueuedWrites: number;
  };
}

function envInt(name: string, fallback: number): number {
  const raw = process.env[name];
  if (!raw) return fallback;
  const parsed = Number.parseInt(raw, 10);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function envFloat(name: string): number | undefined {
  const raw = process.env[name];
  if (!raw) return undefined;
  const parsed = Number.parseFloat(raw);
  return Number.isFinite(parsed) ? parsed : undefined;
}

/** Known insecure default that must never reach production. */
const INSECURE_DEFAULT_SECRET = "dev-insecure-secret-change-me";
/** Minimum acceptable AUTH_SECRET length (32 characters). */
const MIN_AUTH_SECRET_LENGTH = 32;

/**
 * Fail-fast guard for security-critical environment variables (Issue #1291).
 *
 * In production (`NODE_ENV=production`) the server must not start if:
 *   - AUTH_SECRET is unset or equals the well-known insecure default.
 *   - AUTH_SECRET is shorter than MIN_AUTH_SECRET_LENGTH characters.
 *
 * In non-production environments a warning is printed instead so local
 * development and test runs are not broken by missing configuration.
 */
function validateAuthSecret(secret: string): void {
  const isProduction = process.env.NODE_ENV === "production";

  const problems: string[] = [];
  if (secret === INSECURE_DEFAULT_SECRET) {
    problems.push(
      "AUTH_SECRET equals the publicly-known insecure default " +
        `("${INSECURE_DEFAULT_SECRET}").`
    );
  } else if (secret.length < MIN_AUTH_SECRET_LENGTH) {
    problems.push(
      `AUTH_SECRET is only ${secret.length} character(s) long; ` +
        `minimum required is ${MIN_AUTH_SECRET_LENGTH}.`
    );
  }

  if (problems.length === 0) return;

  const message =
    "[quorum-credit] FATAL — insecure AUTH_SECRET configuration detected:\n" +
    problems.map((p) => `  • ${p}`).join("\n") +
    "\n  Set AUTH_SECRET to a strong, randomly-generated secret of at least " +
    `${MIN_AUTH_SECRET_LENGTH} characters before starting the server.`;

  if (isProduction) {
    console.error(message);
    process.exit(1);
  } else {
    console.warn(
      "[quorum-credit] WARNING — " +
        "insecure AUTH_SECRET in use (this would abort in NODE_ENV=production):\n" +
        problems.map((p) => `  • ${p}`).join("\n")
    );
  }
}

export function loadConfig(env: NodeJS.ProcessEnv = process.env): ServerConfig {
  const authSecret = env.AUTH_SECRET ?? INSECURE_DEFAULT_SECRET;
  validateAuthSecret(authSecret);

  return {
    port: envInt("PORT", 4000),
    redisUrl: env.REDIS_URL,
    indexerDbPath: env.INDEXER_DB_PATH ?? "indexer.db",
    authSecret,
    tokenTtlSeconds: envInt("TOKEN_TTL_SECONDS", 300),
    connectionQueueMax: envInt("CONN_QUEUE_MAX", 500),
    bridgePollIntervalMs: envInt("BRIDGE_POLL_INTERVAL_MS", 250),
    leaderLockTtlMs: envInt("LEADER_LOCK_TTL_MS", 5000),
    instanceId: env.INSTANCE_ID ?? `inst-${process.pid}-${Math.random().toString(36).slice(2, 8)}`,
    serviceVersion: env.SERVICE_VERSION ?? "dev",
    costAllocation: {
      contractFeeStroopsPerTx: envInt("CONTRACT_FEE_STROOPS_PER_TX", 100_000),
      apiServerMonthlyCostCents: envInt("API_SERVER_MONTHLY_COST_CENTS", 0),
      storageMonthlyCostCents: envInt("STORAGE_MONTHLY_COST_CENTS", 0),
      stroopsToCentsRate: envFloat("STROOPS_TO_CENTS_RATE"),
    },
    partitionGuard: {
      failureThreshold: envInt("PARTITION_FAILURE_THRESHOLD", 5),
      maxQueuedWrites: envInt("PARTITION_MAX_QUEUED_WRITES", 500),
    },
    wsHeartbeat: {
      intervalMs: envInt("WS_HEARTBEAT_INTERVAL_MS", 30_000),
      idleTimeoutMs: envInt("WS_IDLE_TIMEOUT_MS", 60_000),
    },
  };
}
