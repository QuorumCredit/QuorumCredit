import type { PubSubBus } from "../pubsub/PubSubBus.js";
import type { EventStore } from "./eventStore.js";
import { MetricsAggregator } from "./metricsAggregator.js";
import { EVENTS_CHANNEL, type BroadcastEvent } from "../types.js";

const LEADER_LOCK_KEY = "qc:bridge:leader";

export interface BridgeOptions {
  bus: PubSubBus;
  store: EventStore;
  instanceId: string;
  pollIntervalMs: number;
  leaderLockTtlMs: number;
  onPublish?: (event: BroadcastEvent) => void;
}

/**
 * Tails the indexer's `events` table and republishes new rows onto the pub/sub bus so
 * every server instance's connected clients see them, regardless of which instance
 * they're attached to. Only one instance actually runs the tail loop at a time — the
 * others hold off via a bus-mediated lease so events aren't published N times for N
 * instances.
 *
 * Cursor note: the bridge does NOT persist its publish cursor across restarts. A newly
 * elected leader replays from event id 0 and republishes everything; this is safe
 * because (a) client hooks track their own lastEventId and ignore anything they've
 * already applied, and (b) a client's *initial* sync always comes from a direct
 * EventStore.getEventsSince(since) replay in the WS/socket.io layer, not from bus
 * traffic. The cost is a burst of already-seen messages on leader handover, which is
 * cheap at this protocol's event volume — documented here rather than adding a second
 * persistence mechanism for a cursor whose loss has no correctness impact.
 */
export class Bridge {
  private readonly opts: BridgeOptions;
  private readonly aggregator = new MetricsAggregator();
  private timer: ReturnType<typeof setTimeout> | undefined;
  private stopped = false;
  private isLeader = false;
  private lastPublishedId = 0;

  constructor(opts: BridgeOptions) {
    this.opts = opts;
  }

  start(): void {
    this.stopped = false;
    void this.tick();
  }

  async stop(): Promise<void> {
    this.stopped = true;
    if (this.timer) clearTimeout(this.timer);
    if (this.isLeader) {
      await this.opts.bus.releaseLock(LEADER_LOCK_KEY, this.opts.instanceId);
      this.isLeader = false;
    }
  }

  private async tick(): Promise<void> {
    if (this.stopped) return;

    try {
      this.isLeader = await this.opts.bus.tryAcquireLock(
        LEADER_LOCK_KEY,
        this.opts.leaderLockTtlMs,
        this.opts.instanceId
      );

      if (this.isLeader) {
        const rows = this.opts.store.getEventsSince(this.lastPublishedId);
        for (const event of rows) {
          const metrics = this.aggregator.applyEvent(event);
          const broadcast: BroadcastEvent = { eventId: event.id, event, metrics };
          await this.opts.bus.publish(EVENTS_CHANNEL, JSON.stringify(broadcast));
          this.opts.onPublish?.(broadcast);
          this.lastPublishedId = event.id;
        }
      }
    } catch {
      // Transient bus/store error — next tick retries; leadership lease expiring
      // naturally hands off to another instance if this one is unhealthy.
    }

    if (!this.stopped) {
      this.timer = setTimeout(() => void this.tick(), this.opts.pollIntervalMs);
    }
  }
}
