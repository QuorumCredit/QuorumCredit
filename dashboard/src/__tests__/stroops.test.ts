/**
 * Tests for dashboard/src/stroops.ts — Issue #1512
 *
 * Coverage:
 *  - STROOPS_PER_XLM constant
 *  - stroopsToXlm: zero, typical values, boundary values, bigint precision,
 *    values beyond Number.MAX_SAFE_INTEGER, near-i128-max, negative values
 *  - xlmToStroops: zero, typical values, rounding, negative values,
 *    round-trip consistency with stroopsToXlm
 */

import { describe, it, expect } from "vitest";
import { STROOPS_PER_XLM, stroopsToXlm, xlmToStroops } from "../stroops";

// ---------------------------------------------------------------------------
// STROOPS_PER_XLM constant
// ---------------------------------------------------------------------------

describe("STROOPS_PER_XLM", () => {
  // Test 1
  it("equals 10_000_000", () => {
    expect(STROOPS_PER_XLM).toBe(10_000_000);
  });

  // Test 2
  it("is a number (not bigint)", () => {
    expect(typeof STROOPS_PER_XLM).toBe("number");
  });
});

// ---------------------------------------------------------------------------
// stroopsToXlm — basic / typical values
// ---------------------------------------------------------------------------

describe("stroopsToXlm — basic values", () => {
  // Test 3
  it("converts 0 stroops to '0.0000000'", () => {
    expect(stroopsToXlm(0)).toBe("0.0000000");
  });

  // Test 4
  it("converts 0n (bigint) to '0.0000000'", () => {
    expect(stroopsToXlm(0n)).toBe("0.0000000");
  });

  // Test 5
  it("converts exactly 1 XLM (10_000_000 stroops) to '1.0000000'", () => {
    expect(stroopsToXlm(10_000_000)).toBe("1.0000000");
  });

  // Test 6
  it("converts 1 stroop to '0.0000001'", () => {
    expect(stroopsToXlm(1)).toBe("0.0000001");
  });

  // Test 7
  it("converts 50 stroops (minimum yield stake) to '0.0000050'", () => {
    expect(stroopsToXlm(50)).toBe("0.0000050");
  });

  // Test 8
  it("converts 100_000 stroops (DEFAULT_MIN_LOAN_AMOUNT = 0.01 XLM) to '0.0100000'", () => {
    expect(stroopsToXlm(100_000)).toBe("0.0100000");
  });

  // Test 9
  it("converts 1_000_000 stroops to '0.1000000'", () => {
    expect(stroopsToXlm(1_000_000)).toBe("0.1000000");
  });

  // Test 10
  it("converts 1_234_567_890 stroops (123.456789 XLM) to '123.4567890'", () => {
    expect(stroopsToXlm(1_234_567_890)).toBe("123.4567890");
  });

  // Test 11
  it("always returns exactly 7 decimal places", () => {
    const result = stroopsToXlm(12_345_678);
    const [, frac] = result.split(".");
    expect(frac).toHaveLength(7);
  });
});

// ---------------------------------------------------------------------------
// stroopsToXlm — bigint input
// ---------------------------------------------------------------------------

describe("stroopsToXlm — bigint input", () => {
  // Test 12
  it("converts 10_000_000n to '1.0000000'", () => {
    expect(stroopsToXlm(10_000_000n)).toBe("1.0000000");
  });

  // Test 13
  it("converts 1n stroop to '0.0000001'", () => {
    expect(stroopsToXlm(1n)).toBe("0.0000001");
  });

  // Test 14
  it("converts a typical loan principal (5_000_000_000n = 500 XLM) correctly", () => {
    expect(stroopsToXlm(5_000_000_000n)).toBe("500.0000000");
  });

  // Test 15
  it("converts a typical treasury balance (1_000_000_000_000n = 100,000 XLM) correctly", () => {
    expect(stroopsToXlm(1_000_000_000_000n)).toBe("100000.0000000");
  });
});

// ---------------------------------------------------------------------------
// stroopsToXlm — precision beyond Number.MAX_SAFE_INTEGER
// ---------------------------------------------------------------------------

describe("stroopsToXlm — values beyond Number.MAX_SAFE_INTEGER", () => {
  // Number.MAX_SAFE_INTEGER = 9_007_199_254_740_991
  // That corresponds to ~900,719,925.474 XLM — well within on-chain range.
  // Values above this cannot be represented exactly as a JS number.

  // Test 16
  it("handles Number.MAX_SAFE_INTEGER stroops exactly as bigint", () => {
    const maxSafe = BigInt(Number.MAX_SAFE_INTEGER); // 9_007_199_254_740_991n
    const result = stroopsToXlm(maxSafe);
    // whole = 900_719_925, frac = 4_740_991
    expect(result).toBe("900719925.4740991");
  });

  // Test 17
  it("handles Number.MAX_SAFE_INTEGER + 1 correctly as bigint without precision loss", () => {
    const aboveMaxSafe = BigInt(Number.MAX_SAFE_INTEGER) + 1n; // 9_007_199_254_740_992n
    const result = stroopsToXlm(aboveMaxSafe);
    expect(result).toBe("900719925.4740992");
  });

  // Test 18
  it("handles a value 1 stroop apart from the previous without merging results", () => {
    const a = stroopsToXlm(9_007_199_254_740_991n);
    const b = stroopsToXlm(9_007_199_254_740_992n);
    expect(a).not.toBe(b);
  });

  // Test 19 — value representative of large on-chain loan principal
  it("handles 10^15 stroops (100,000,000 XLM) exactly", () => {
    expect(stroopsToXlm(1_000_000_000_000_000n)).toBe("100000000.0000000");
  });

  // Test 20
  it("handles 10^18 stroops (100,000,000,000 XLM) exactly", () => {
    expect(stroopsToXlm(1_000_000_000_000_000_000n)).toBe("100000000000.0000000");
  });
});

// ---------------------------------------------------------------------------
// stroopsToXlm — near i128 maximum
// ---------------------------------------------------------------------------

describe("stroopsToXlm — near i128 boundary", () => {
  // i128::MAX = 170_141_183_460_469_231_731_687_303_715_884_105_727
  // In XLM that is astronomically large but the bigint path should handle it.
  const I128_MAX = 170_141_183_460_469_231_731_687_303_715_884_105_727n;

  // Test 21
  it("does not throw for i128::MAX stroops", () => {
    expect(() => stroopsToXlm(I128_MAX)).not.toThrow();
  });

  // Test 22
  it("returns a string with exactly 7 decimal places for i128::MAX", () => {
    const result = stroopsToXlm(I128_MAX);
    const [, frac] = result.split(".");
    expect(frac).toHaveLength(7);
  });

  // Test 23
  it("whole-part of i128::MAX conversion is correct", () => {
    const result = stroopsToXlm(I128_MAX);
    const whole = result.split(".")[0];
    // i128::MAX / 10_000_000 = 17_014_118_346_046_923_173_168_730_371_588n
    expect(whole).toBe("17014118346046923173168730371588");
  });

  // Test 24
  it("fractional part of i128::MAX conversion is correct", () => {
    const result = stroopsToXlm(I128_MAX);
    const frac = result.split(".")[1];
    // i128::MAX % 10_000_000 = 4_105_727
    expect(frac).toBe("4105727");
  });
});

// ---------------------------------------------------------------------------
// stroopsToXlm — negative values (slash/penalty display)
// ---------------------------------------------------------------------------

describe("stroopsToXlm — negative values", () => {
  // Test 25
  it("converts -1 stroop to '-0.0000001'", () => {
    expect(stroopsToXlm(-1)).toBe("-0.0000001");
  });

  // Test 26
  it("converts -10_000_000 (−1 XLM) to '-1.0000000'", () => {
    expect(stroopsToXlm(-10_000_000)).toBe("-1.0000000");
  });

  // Test 27
  it("converts -1n bigint stroop to '-0.0000001'", () => {
    expect(stroopsToXlm(-1n)).toBe("-0.0000001");
  });

  // Test 28
  it("converts -10_000_000n to '-1.0000000'", () => {
    expect(stroopsToXlm(-10_000_000n)).toBe("-1.0000000");
  });

  // Test 29
  it("converts -100_000 (−0.01 XLM, min loan slash) to '-0.0100000'", () => {
    expect(stroopsToXlm(-100_000)).toBe("-0.0100000");
  });

  // Test 30 — large negative (e.g. 50% of a large stake slashed)
  it("converts -5_000_000_000n (-500 XLM) to '-500.0000000'", () => {
    expect(stroopsToXlm(-5_000_000_000n)).toBe("-500.0000000");
  });

  // Test 31 — negative beyond MAX_SAFE_INTEGER
  it("handles large negative bigint beyond Number.MAX_SAFE_INTEGER", () => {
    const big = -(BigInt(Number.MAX_SAFE_INTEGER) + 1n);
    const result = stroopsToXlm(big);
    expect(result.startsWith("-")).toBe(true);
    expect(result).toBe("-900719925.4740992");
  });

  // Test 32 — negative always has exactly 7 decimal places
  it("negative result always has exactly 7 decimal places", () => {
    const result = stroopsToXlm(-1_234_567_890);
    const [, frac] = result.split(".");
    expect(frac).toHaveLength(7);
  });
});

// ---------------------------------------------------------------------------
// stroopsToXlm — fractional stroop counts (number input)
// ---------------------------------------------------------------------------

describe("stroopsToXlm — fractional number input truncation", () => {
  // The implementation calls Math.trunc on number inputs, so fractional
  // stroops are truncated (not rounded) since stroops are indivisible.

  // Test 33
  it("truncates 1.9 stroops to 1 stroop ('0.0000001')", () => {
    expect(stroopsToXlm(1.9)).toBe("0.0000001");
  });

  // Test 34
  it("truncates -1.9 stroops toward zero to -1 stroop ('-0.0000001')", () => {
    expect(stroopsToXlm(-1.9)).toBe("-0.0000001");
  });
});

// ---------------------------------------------------------------------------
// xlmToStroops — basic / typical values
// ---------------------------------------------------------------------------

describe("xlmToStroops — basic values", () => {
  // Test 35
  it("converts 0 XLM to 0 stroops", () => {
    expect(xlmToStroops(0)).toBe(0);
  });

  // Test 36
  it("converts 1 XLM to 10_000_000 stroops", () => {
    expect(xlmToStroops(1)).toBe(10_000_000);
  });

  // Test 37
  it("converts 0.01 XLM (min loan) to 100_000 stroops", () => {
    expect(xlmToStroops(0.01)).toBe(100_000);
  });

  // Test 38
  it("converts 0.0000001 XLM (1 stroop) to 1", () => {
    expect(xlmToStroops(0.0000001)).toBe(1);
  });

  // Test 39
  it("converts 500 XLM to 5_000_000_000 stroops", () => {
    expect(xlmToStroops(500)).toBe(5_000_000_000);
  });

  // Test 40
  it("converts 1000 XLM to 10_000_000_000 stroops", () => {
    expect(xlmToStroops(1000)).toBe(10_000_000_000);
  });

  // Test 41
  it("returns a number (not bigint)", () => {
    expect(typeof xlmToStroops(1)).toBe("number");
  });
});

// ---------------------------------------------------------------------------
// xlmToStroops — rounding
// ---------------------------------------------------------------------------

describe("xlmToStroops — rounding behaviour", () => {
  // Test 42
  it("rounds 0.00000005 XLM (0.5 stroops) up to 1 stroop", () => {
    expect(xlmToStroops(0.00000005)).toBe(1);
  });

  // Test 43
  it("rounds 0.000000049 XLM (0.49 stroops) down to 0 stroops", () => {
    expect(xlmToStroops(0.000000049)).toBe(0);
  });

  // Test 44
  it("rounds 1.00000005 XLM down correctly", () => {
    // 1.00000005 * 10_000_000 = 10_000_000.5 → rounds to 10_000_001
    expect(xlmToStroops(1.00000005)).toBe(10_000_001);
  });
});

// ---------------------------------------------------------------------------
// xlmToStroops — negative values
// ---------------------------------------------------------------------------

describe("xlmToStroops — negative values", () => {
  // Test 45
  it("converts -1 XLM to -10_000_000 stroops", () => {
    expect(xlmToStroops(-1)).toBe(-10_000_000);
  });

  // Test 46
  it("converts -0.0000001 XLM to -1 stroop", () => {
    expect(xlmToStroops(-0.0000001)).toBe(-1);
  });

  // Test 47
  it("converts -500 XLM to -5_000_000_000 stroops", () => {
    expect(xlmToStroops(-500)).toBe(-5_000_000_000);
  });
});

// ---------------------------------------------------------------------------
// Round-trip consistency
// ---------------------------------------------------------------------------

describe("round-trip: xlmToStroops → stroopsToXlm", () => {
  // Test 48
  it("round-trips 1 XLM exactly", () => {
    const stroops = xlmToStroops(1);
    expect(stroopsToXlm(stroops)).toBe("1.0000000");
  });

  // Test 49
  it("round-trips 0.01 XLM exactly", () => {
    const stroops = xlmToStroops(0.01);
    expect(stroopsToXlm(stroops)).toBe("0.0100000");
  });

  // Test 50
  it("round-trips 123.4567890 XLM exactly via bigint", () => {
    const stroops = BigInt(xlmToStroops(123.456789));
    expect(stroopsToXlm(stroops)).toBe("123.4567890");
  });

  // Test 51
  it("round-trips negative -1 XLM exactly", () => {
    const stroops = xlmToStroops(-1);
    expect(stroopsToXlm(stroops)).toBe("-1.0000000");
  });
});
