import { connect, type Socket } from "node:net";
import { createInterface } from "node:readline";
import type { MessageHandler, PubSubBus } from "./PubSubBus.js";

type LockResult = { op: "lock_result"; reqId: number; ok: boolean };
type RelayMsg = { op: "message"; channel: string; message: string } | LockResult;

/** Client for the test-only relay server (see relayServer.ts) — same PubSubBus contract
 * as RedisBus, used only by the multi-instance integration test. */
export class RelayBus implements PubSubBus {
  private readonly socket: Socket;
  private readonly ready: Promise<void>;
  private readonly handlers = new Map<string, Set<MessageHandler>>();
  private readonly pendingLocks = new Map<number, (ok: boolean) => void>();
  private reqCounter = 0;

  constructor(host: string, port: number) {
    this.socket = connect(port, host);
    this.ready = new Promise((resolve, reject) => {
      this.socket.once("connect", () => resolve());
      this.socket.once("error", reject);
    });

    const rl = createInterface({ input: this.socket });
    rl.on("line", (line) => {
      if (!line.trim()) return;
      const msg: RelayMsg = JSON.parse(line);
      if (msg.op === "message") {
        const set = this.handlers.get(msg.channel);
        if (set) for (const handler of set) handler(msg.message);
      } else if (msg.op === "lock_result") {
        const resolve = this.pendingLocks.get(msg.reqId);
        if (resolve) {
          this.pendingLocks.delete(msg.reqId);
          resolve(msg.ok);
        }
      }
    });
  }

  private async send(payload: unknown): Promise<void> {
    await this.ready;
    this.socket.write(JSON.stringify(payload) + "\n");
  }

  async publish(channel: string, message: string): Promise<void> {
    await this.send({ op: "pub", channel, message });
  }

  async subscribe(channel: string, handler: MessageHandler): Promise<void> {
    let set = this.handlers.get(channel);
    if (!set) {
      set = new Set();
      this.handlers.set(channel, set);
      await this.send({ op: "sub", channel });
    }
    set.add(handler);
  }

  async unsubscribe(channel: string, handler: MessageHandler): Promise<void> {
    const set = this.handlers.get(channel);
    if (!set) return;
    set.delete(handler);
    if (set.size === 0) {
      this.handlers.delete(channel);
      await this.send({ op: "unsub", channel });
    }
  }

  async tryAcquireLock(key: string, ttlMs: number, holder: string): Promise<boolean> {
    await this.ready;
    const reqId = ++this.reqCounter;
    const result = new Promise<boolean>((resolve) => this.pendingLocks.set(reqId, resolve));
    this.socket.write(JSON.stringify({ op: "lock_acquire", reqId, key, holder, ttlMs }) + "\n");
    return result;
  }

  async releaseLock(key: string, holder: string): Promise<void> {
    await this.ready;
    const reqId = ++this.reqCounter;
    const result = new Promise<void>((resolve) => this.pendingLocks.set(reqId, () => resolve()));
    this.socket.write(JSON.stringify({ op: "lock_release", reqId, key, holder }) + "\n");
    await result;
  }

  async close(): Promise<void> {
    this.socket.destroy();
  }
}
