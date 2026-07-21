import type { MessageHandler, PubSubBus } from "./PubSubBus.js";

/**
 * In-process pub/sub — messages published on this instance are only ever seen by
 * subscribers on this same instance. This is NOT multi-instance-safe; it exists for
 * local dev (`REDIS_URL` unset) and unit tests that don't need cross-process delivery.
 * Anything that must prove cross-instance behavior (the multi-instance integration
 * test, production deployments) must use RedisBus or RelayBus instead.
 */
export class LocalBus implements PubSubBus {
  private readonly channels = new Map<string, Set<MessageHandler>>();
  private readonly locks = new Map<string, { holder: string; expiresAt: number }>();

  async publish(channel: string, message: string): Promise<void> {
    const handlers = this.channels.get(channel);
    if (!handlers) return;
    for (const handler of handlers) handler(message);
  }

  async subscribe(channel: string, handler: MessageHandler): Promise<void> {
    let handlers = this.channels.get(channel);
    if (!handlers) {
      handlers = new Set();
      this.channels.set(channel, handlers);
    }
    handlers.add(handler);
  }

  async unsubscribe(channel: string, handler: MessageHandler): Promise<void> {
    this.channels.get(channel)?.delete(handler);
  }

  async tryAcquireLock(key: string, ttlMs: number, holder: string): Promise<boolean> {
    const now = Date.now();
    const existing = this.locks.get(key);
    if (existing && existing.expiresAt > now && existing.holder !== holder) {
      return false;
    }
    this.locks.set(key, { holder, expiresAt: now + ttlMs });
    return true;
  }

  async releaseLock(key: string, holder: string): Promise<void> {
    const existing = this.locks.get(key);
    if (existing && existing.holder === holder) {
      this.locks.delete(key);
    }
  }

  async close(): Promise<void> {
    this.channels.clear();
    this.locks.clear();
  }
}
