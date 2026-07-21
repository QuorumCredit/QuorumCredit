import { describe, it, expect } from "vitest";
import { ConnectionQueue } from "../src/ws/connectionQueue.js";

describe("ConnectionQueue", () => {
  it("drains items in FIFO order", () => {
    const q = new ConnectionQueue<number>(5);
    q.push(1);
    q.push(2);
    q.push(3);
    expect(q.drainAll()).toEqual([1, 2, 3]);
    expect(q.size).toBe(0);
  });

  it("drops the oldest item once capacity is exceeded", () => {
    const q = new ConnectionQueue<number>(3);
    expect(q.push(1)).toBe(false);
    expect(q.push(2)).toBe(false);
    expect(q.push(3)).toBe(false);
    expect(q.push(4)).toBe(true); // over capacity — 1 is dropped
    expect(q.drainAll()).toEqual([2, 3, 4]);
  });

  it("reports the drop flag exactly once per overflow batch", () => {
    const q = new ConnectionQueue<number>(2);
    q.push(1);
    q.push(2);
    q.push(3); // drop
    expect(q.takeDropFlag()).toBe(true);
    expect(q.takeDropFlag()).toBe(false);
  });

  it("rejects a capacity below 1", () => {
    expect(() => new ConnectionQueue<number>(0)).toThrow();
  });
});
