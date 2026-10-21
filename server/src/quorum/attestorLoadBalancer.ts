/**
 * Attestor load balancing (issue #1774).
 *
 * Distributes verification work across attestors so no single attestor carries a
 * disproportionate share:
 * - `assign` picks the least-loaded attestors (active / capacity), breaking ties by
 *   lifetime assignments and then by ID, so selection is deterministic;
 * - `release` returns capacity when an assignment completes;
 * - `snapshot` exposes per-attestor load and each attestor's share of all work;
 * - `rebalance` moves assignments from overloaded attestors to underloaded ones, up
 *   to a bounded number of moves per call, so the distribution converges without thrash.
 */

export const DEFAULT_REBALANCE_THRESHOLD = 0.25;
export const DEFAULT_MAX_REBALANCE_MOVES = 100;

export interface AttestorLoad {
  attestorId: string;
  capacity: number;
  active: number;
  load: number;
  totalAssigned: number;
  share: number;
}

export interface RebalanceMove {
  from: string;
  to: string;
}

interface AttestorState {
  capacity: number;
  active: number;
  totalAssigned: number;
}

export class AttestorLoadBalancer {
  private readonly attestors = new Map<string, AttestorState>();

  /** Register an attestor or update its capacity. Existing assignments are kept. */
  register(attestorId: string, capacity: number): void {
    requireId(attestorId);
    if (!Number.isInteger(capacity) || capacity < 1) {
      throw new Error("capacity must be a positive integer");
    }
    const existing = this.attestors.get(attestorId);
    if (existing) {
      existing.capacity = capacity;
    } else {
      this.attestors.set(attestorId, { capacity, active: 0, totalAssigned: 0 });
    }
  }

  /** Remove an attestor. Returns false if it was not registered. */
  deregister(attestorId: string): boolean {
    return this.attestors.delete(attestorId);
  }

  /**
   * Choose `count` distinct attestors from `candidates` that still have spare capacity,
   * preferring the least loaded. Records each selected assignment.
   */
  assign(candidates: string[], count: number): string[] {
    if (!Number.isInteger(count) || count < 1) throw new Error("count must be a positive integer");
    const unique = Array.from(new Set(candidates));
    const available = unique
      .map((id) => ({ id, state: this.attestors.get(id) }))
      .filter((entry): entry is { id: string; state: AttestorState } =>
        entry.state !== undefined && entry.state.active < entry.state.capacity
      )
      .sort((a, b) => compareLoad(a.state, b.state) || a.id.localeCompare(b.id));

    if (available.length < count) {
      throw new Error(`need ${count} attestors with spare capacity, only ${available.length} available`);
    }
    const chosen = available.slice(0, count).map((entry) => entry.id);
    for (const id of chosen) {
      const state = this.attestors.get(id)!;
      state.active += 1;
      state.totalAssigned += 1;
    }
    return chosen;
  }

  /** Mark one assignment on `attestorId` as complete. Load never drops below zero. */
  release(attestorId: string): void {
    const state = this.attestors.get(attestorId);
    if (state && state.active > 0) state.active -= 1;
  }

  snapshot(): AttestorLoad[] {
    const totalAssigned = Array.from(this.attestors.values()).reduce(
      (sum, state) => sum + state.totalAssigned,
      0
    );
    return Array.from(this.attestors.entries())
      .map(([attestorId, state]) => ({
        attestorId,
        capacity: state.capacity,
        active: state.active,
        load: round(state.active / state.capacity),
        totalAssigned: state.totalAssigned,
        share: totalAssigned === 0 ? 0 : round(state.totalAssigned / totalAssigned),
      }))
      .sort((a, b) => a.attestorId.localeCompare(b.attestorId));
  }

  /**
   * Move active assignments from the most loaded attestor to the least loaded one
   * while their load gap exceeds `threshold`. Stops after `maxMoves` moves.
   */
  rebalance(
    threshold: number = DEFAULT_REBALANCE_THRESHOLD,
    maxMoves: number = DEFAULT_MAX_REBALANCE_MOVES
  ): RebalanceMove[] {
    if (!Number.isFinite(threshold) || threshold < 0) throw new Error("threshold must be non-negative");
    const moves: RebalanceMove[] = [];
    while (moves.length < maxMoves) {
      const ids = Array.from(this.attestors.keys());
      const withSpare = ids.filter((id) => this.attestors.get(id)!.active < this.attestors.get(id)!.capacity);
      const withWork = ids.filter((id) => this.attestors.get(id)!.active > 0);
      if (withWork.length === 0 || withSpare.length === 0) break;

      const from = withWork.sort((a, b) => compareLoadDesc(this.attestors.get(a)!, this.attestors.get(b)!) || a.localeCompare(b))[0]!;
      const to = withSpare.sort((a, b) => compareLoad(this.attestors.get(a)!, this.attestors.get(b)!) || a.localeCompare(b))[0]!;
      if (from === to) break;

      const fromLoad = this.attestors.get(from)!.active / this.attestors.get(from)!.capacity;
      const toLoad = this.attestors.get(to)!.active / this.attestors.get(to)!.capacity;
      if (fromLoad - toLoad <= threshold) break;

      this.attestors.get(from)!.active -= 1;
      const target = this.attestors.get(to)!;
      target.active += 1;
      target.totalAssigned += 1;
      moves.push({ from, to });
    }
    return moves;
  }
}

function compareLoad(a: AttestorState, b: AttestorState): number {
  const byLoad = a.active / a.capacity - b.active / b.capacity;
  return byLoad !== 0 ? byLoad : a.totalAssigned - b.totalAssigned;
}

function compareLoadDesc(a: AttestorState, b: AttestorState): number {
  return compareLoad(b, a);
}

function requireId(attestorId: string): void {
  if (typeof attestorId !== "string" || attestorId.trim().length === 0) {
    throw new Error("attestorId is required");
  }
}

function round(value: number): number {
  return Math.round(value * 10_000) / 10_000;
}

export const attestorLoadBalancer = new AttestorLoadBalancer();
