import { describe, expect, it } from "vitest";
import { AttestorLoadBalancer } from "../src/quorum/attestorLoadBalancer.js";

function balancer(capacities: Record<string, number>) {
  const lb = new AttestorLoadBalancer();
  for (const [id, capacity] of Object.entries(capacities)) lb.register(id, capacity);
  return lb;
}

describe("AttestorLoadBalancer", () => {
  it("validates registration and assignment inputs", () => {
    const lb = new AttestorLoadBalancer();
    expect(() => lb.register("", 1)).toThrow(/attestorId/);
    expect(() => lb.register("a", 0)).toThrow(/capacity/);
    expect(() => lb.register("a", 1.5)).toThrow(/capacity/);
    lb.register("a", 1);
    expect(() => lb.assign(["a"], 0)).toThrow(/count/);
  });

  it("selects the least-loaded attestors, spreading work instead of piling onto one", () => {
    const lb = balancer({ a: 10, b: 10, c: 10 });
    const first = lb.assign(["a", "b", "c"], 1);
    const second = lb.assign(["a", "b", "c"], 1);
    const third = lb.assign(["a", "b", "c"], 1);
    expect(new Set([...first, ...second, ...third]).size).toBe(3);
  });

  it("breaks ties deterministically by lifetime assignments, then by ID", () => {
    const lb = balancer({ b: 5, a: 5 });
    expect(lb.assign(["b", "a"], 1)).toEqual(["a"]);
    expect(lb.assign(["b", "a"], 1)).toEqual(["b"]);
  });

  it("never assigns beyond capacity and reports when too few attestors have spare room", () => {
    const lb = balancer({ a: 1, b: 1 });
    lb.assign(["a", "b"], 2);
    expect(() => lb.assign(["a", "b"], 1)).toThrow(/spare capacity/);
  });

  it("ignores unregistered candidates and duplicate candidate IDs", () => {
    const lb = balancer({ a: 3 });
    expect(lb.assign(["ghost", "a", "a"], 1)).toEqual(["a"]);
    expect(() => lb.assign(["ghost", "a", "a"], 2)).toThrow(/only 1 available/);
  });

  it("frees capacity on release and never drops load below zero", () => {
    const lb = balancer({ a: 1 });
    lb.assign(["a"], 1);
    lb.release("a");
    lb.release("a");
    expect(lb.snapshot()[0]).toMatchObject({ active: 0, load: 0, totalAssigned: 1 });
    expect(lb.assign(["a"], 1)).toEqual(["a"]);
  });

  it("reports per-attestor load and share of all lifetime assignments", () => {
    const lb = balancer({ a: 4, b: 4 });
    lb.assign(["a"], 1);
    lb.assign(["a"], 1);
    lb.assign(["b"], 1);
    const [a, b] = lb.snapshot();
    expect(a).toMatchObject({ attestorId: "a", active: 2, load: 0.5, totalAssigned: 2, share: 0.6667 });
    expect(b).toMatchObject({ attestorId: "b", active: 1, load: 0.25, totalAssigned: 1, share: 0.3333 });
  });

  it("rebalances by moving work from the most to the least loaded attestor", () => {
    const lb = balancer({ a: 4, b: 4 });
    lb.assign(["a"], 1);
    lb.assign(["a"], 1);
    lb.assign(["a"], 1);
    const moves = lb.rebalance(0.25);
    expect(moves.length).toBeGreaterThan(0);
    expect(moves.every((m) => m.from === "a" && m.to === "b")).toBe(true);
    const loads = lb.snapshot().map((entry) => entry.active);
    expect(Math.abs(loads[0]! - loads[1]!)).toBeLessThanOrEqual(1);
  });

  it("leaves a balanced distribution alone and respects the move cap", () => {
    const even = balancer({ a: 4, b: 4 });
    even.assign(["a"], 1);
    even.assign(["b"], 1);
    expect(even.rebalance(0.25)).toEqual([]);

    const skewed = balancer({ a: 20, b: 20 });
    for (let i = 0; i < 10; i++) skewed.assign(["a"], 1);
    expect(skewed.rebalance(0, 2)).toHaveLength(2);
  });

  it("deregisters attestors", () => {
    const lb = balancer({ a: 1 });
    expect(lb.deregister("a")).toBe(true);
    expect(lb.deregister("a")).toBe(false);
    expect(lb.snapshot()).toEqual([]);
  });
});
