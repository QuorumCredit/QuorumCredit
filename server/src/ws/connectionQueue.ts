/**
 * Bounded outgoing queue for a single connection.
 *
 * Backpressure/drop policy (documented per issue #1142's requirement): each
 * connection gets a fixed-capacity FIFO queue. When a new message arrives and the
 * queue is already at capacity, the OLDEST queued message is dropped to make room —
 * we favor delivering recent state over stale state, since every message here is a
 * snapshot-able event (loan/metrics updates), not an irreversible command. The first
 * drop in a batch flips `droppedSinceResync`; callers are expected to check
 * `takeDropFlag()` after draining and, if it was set, send a single
 * `resync_required` control frame carrying the lowest surviving cursor so the client
 * can request a replay instead of silently operating on a gap.
 */
export class ConnectionQueue<T> {
  private readonly items: T[] = [];
  private droppedSinceResync = false;

  constructor(private readonly capacity: number) {
    if (capacity < 1) throw new Error("ConnectionQueue capacity must be >= 1");
  }

  get size(): number {
    return this.items.length;
  }

  /** Enqueues `item`, dropping the oldest queued item if at capacity. Returns true if a drop occurred. */
  push(item: T): boolean {
    this.items.push(item);
    if (this.items.length > this.capacity) {
      this.items.shift();
      this.droppedSinceResync = true;
      return true;
    }
    return false;
  }

  /** Removes and returns every currently-queued item, oldest first. */
  drainAll(): T[] {
    return this.items.splice(0, this.items.length);
  }

  /** Returns whether a drop happened since the last call, clearing the flag. */
  takeDropFlag(): boolean {
    const had = this.droppedSinceResync;
    this.droppedSinceResync = false;
    return had;
  }
}
