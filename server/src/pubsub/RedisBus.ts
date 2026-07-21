import { Redis } from "ioredis";
import type { MessageHandler, PubSubBus } from "./PubSubBus.js";

const RELEASE_LOCK_SCRIPT = `
if redis.call("get", KEYS[1]) == ARGV[1] then
  return redis.call("del", KEYS[1])
else
  return 0
end
`;

/**
 * Production pub/sub backbone. Redis requires a dedicated connection once it enters
 * subscribe mode, so we keep a separate publisher and subscriber connection (the
 * standard ioredis pattern). Any server instance publishing a message is received by
 * every instance subscribed to the same channel, which is what makes horizontal
 * scaling of the WS/socket.io layer safe (a client on instance A sees an event whose
 * underlying write was only ever observed by instance B).
 */
export class RedisBus implements PubSubBus {
  private readonly pub: Redis;
  private readonly sub: Redis;
  private readonly handlers = new Map<string, Set<MessageHandler>>();

  constructor(url: string) {
    this.pub = new Redis(url, { lazyConnect: false });
    this.sub = new Redis(url, { lazyConnect: false });
    this.sub.on("message", (channel: string, message: string) => {
      const set = this.handlers.get(channel);
      if (!set) return;
      for (const handler of set) handler(message);
    });
  }

  async publish(channel: string, message: string): Promise<void> {
    await this.pub.publish(channel, message);
  }

  async subscribe(channel: string, handler: MessageHandler): Promise<void> {
    let set = this.handlers.get(channel);
    if (!set) {
      set = new Set();
      this.handlers.set(channel, set);
      await this.sub.subscribe(channel);
    }
    set.add(handler);
  }

  async unsubscribe(channel: string, handler: MessageHandler): Promise<void> {
    const set = this.handlers.get(channel);
    if (!set) return;
    set.delete(handler);
    if (set.size === 0) {
      this.handlers.delete(channel);
      await this.sub.unsubscribe(channel);
    }
  }

  async tryAcquireLock(key: string, ttlMs: number, holder: string): Promise<boolean> {
    const result = await this.pub.set(key, holder, "PX", ttlMs, "NX");
    if (result === "OK") return true;
    // Already held — allow the existing holder to renew its own lock.
    const current = await this.pub.get(key);
    if (current === holder) {
      await this.pub.set(key, holder, "PX", ttlMs);
      return true;
    }
    return false;
  }

  async releaseLock(key: string, holder: string): Promise<void> {
    await this.pub.eval(RELEASE_LOCK_SCRIPT, 1, key, holder);
  }

  async close(): Promise<void> {
    this.handlers.clear();
    await Promise.all([this.pub.quit(), this.sub.quit()]);
  }
}
