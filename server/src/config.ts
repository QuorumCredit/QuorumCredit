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
}

function envInt(name: string, fallback: number): number {
  const raw = process.env[name];
  if (!raw) return fallback;
  const parsed = Number.parseInt(raw, 10);
  return Number.isFinite(parsed) ? parsed : fallback;
}

export function loadConfig(env: NodeJS.ProcessEnv = process.env): ServerConfig {
  return {
    port: envInt("PORT", 4000),
    redisUrl: env.REDIS_URL,
    indexerDbPath: env.INDEXER_DB_PATH ?? "indexer.db",
    authSecret: env.AUTH_SECRET ?? "dev-insecure-secret-change-me",
    tokenTtlSeconds: envInt("TOKEN_TTL_SECONDS", 300),
    connectionQueueMax: envInt("CONN_QUEUE_MAX", 500),
    bridgePollIntervalMs: envInt("BRIDGE_POLL_INTERVAL_MS", 250),
    leaderLockTtlMs: envInt("LEADER_LOCK_TTL_MS", 5000),
    instanceId: env.INSTANCE_ID ?? `inst-${process.pid}-${Math.random().toString(36).slice(2, 8)}`,
  };
}
