import { describe, it, expect } from "vitest";
import { QuorumPolicyStore } from "../src/quorum/quorumPolicy.js";

describe("quorum policy scoring", () => {
  it("scores an empty slice as zero", () => {
    const store = new QuorumPolicyStore();
    expect(store.scoreSlice([])).toEqual({
      memberCount: 0,
      totalStake: 0,
      averageReputation: 0,
      regionDiversity: 0,
      score: 0,
    });
  });

  it("scores a two-member slice from a fixed fixture", () => {
    const store = new QuorumPolicyStore();
    const score = store.scoreSlice([
      { attestor: "a", reputation: 500, stake: 500_000, region: "eu" },
      { attestor: "b", reputation: 500, stake: 500_000, region: "us" },
    ]);
    expect(score.memberCount).toBe(2);
    expect(score.totalStake).toBe(1_000_000);
    expect(score.averageReputation).toBe(500);
    expect(score.regionDiversity).toBe(1);
    expect(score.score).toBe(72.86);
  });
});
