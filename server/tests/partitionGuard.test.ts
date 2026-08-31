import { describe, it, expect, vi } from 'vitest';
import { PartitionGuard } from '../src/resilience/partitionGuard.js';

describe('PartitionGuard', () => {
  it('enters partitioned state after failureThreshold consecutive failures', () => {
    const guard = new PartitionGuard({ failureThreshold: 3, maxQueuedWrites: 10 });
    expect(guard.isPartitioned()).toBe(false);
    guard.recordFailure();
    guard.recordFailure();
    expect(guard.isPartitioned()).toBe(false);
    guard.recordFailure();
    expect(guard.isPartitioned()).toBe(true);
  });

  it('recovers on the first success after partition', async () => {
    const guard = new PartitionGuard({ failureThreshold: 2, maxQueuedWrites: 10 });
    guard.recordFailure();
    guard.recordFailure();
    expect(guard.isPartitioned()).toBe(true);
    guard.recordSuccess();
    // Give flush a tick
    await new Promise(r => setTimeout(r, 0));
    expect(guard.isPartitioned()).toBe(false);
  });

  it('replays enqueued writes on recovery', async () => {
    const guard = new PartitionGuard({ failureThreshold: 2, maxQueuedWrites: 10 });
    guard.recordFailure();
    guard.recordFailure();
    const write = vi.fn().mockResolvedValue(undefined);
    guard.enqueue(write);
    guard.recordSuccess();
    await new Promise(r => setTimeout(r, 10));
    expect(write).toHaveBeenCalledTimes(1);
  });

  it('drops oldest writes when queue exceeds maxQueuedWrites', () => {
    const guard = new PartitionGuard({ failureThreshold: 2, maxQueuedWrites: 2 });
    guard.recordFailure();
    guard.recordFailure();
    const w1 = vi.fn();
    const w2 = vi.fn();
    const w3 = vi.fn();
    guard.enqueue(w1);
    guard.enqueue(w2);
    guard.enqueue(w3); // w1 should be dropped
    expect(guard.status().droppedWrites).toBe(1);
    expect(guard.status().queueDepth).toBe(2);
  });

  it('logs a warning and calls reconcile when droppedWrites > 0 on recovery', async () => {
    const reconcile = vi.fn().mockResolvedValue(undefined);
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const guard = new PartitionGuard({ failureThreshold: 2, maxQueuedWrites: 2, reconcile });
    guard.recordFailure();
    guard.recordFailure();
    guard.enqueue(vi.fn());
    guard.enqueue(vi.fn());
    guard.enqueue(vi.fn()); // forces a drop
    guard.recordSuccess();
    await new Promise(r => setTimeout(r, 20));
    expect(reconcile).toHaveBeenCalledTimes(1);
    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('ALERT'));
    warnSpy.mockRestore();
  });

  it('resetDroppedWrites clears the counter', () => {
    const guard = new PartitionGuard({ failureThreshold: 2, maxQueuedWrites: 2 });
    guard.recordFailure();
    guard.recordFailure();
    guard.enqueue(vi.fn());
    guard.enqueue(vi.fn());
    guard.enqueue(vi.fn());
    expect(guard.status().droppedWrites).toBe(1);
    guard.resetDroppedWrites();
    expect(guard.status().droppedWrites).toBe(0);
  });
});
