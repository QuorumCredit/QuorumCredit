export type MessageHandler = (message: string) => void;

/**
 * Shared pub/sub backbone contract. Every server instance publishes and subscribes
 * through the same channel name; an implementation must guarantee that a message
 * published from any one instance is delivered to subscribers on every other
 * instance (not just within the publishing process). `RedisBus` is the production
 * implementation; `LocalBus` is an in-process stand-in for single-instance dev/tests;
 * `RelayBus` is a test-only stand-in used to prove the multi-instance contract in
 * environments without a Redis server available (see server/README.md).
 */
export interface PubSubBus {
  publish(channel: string, message: string): Promise<void>;
  subscribe(channel: string, handler: MessageHandler): Promise<void>;
  unsubscribe(channel: string, handler: MessageHandler): Promise<void>;
  /** Distributed lock used for leader election (the bridge only runs on one instance). */
  tryAcquireLock(key: string, ttlMs: number, holder: string): Promise<boolean>;
  releaseLock(key: string, holder: string): Promise<void>;
  close(): Promise<void>;
}
