import { describe, it, expect, vi, afterEach } from "vitest";
import type { IncomingMessage, ServerResponse } from "node:http";
import { parseBoundedInt } from "../src/http/queryParams.js";
import { handleHttpRequest, type RouteContext } from "../src/http/routes.js";
import { auditReportGenerator } from "../src/batch/auditReportGenerator.js";
import { batchVerificationStore } from "../src/batch/batchVerificationStore.js";
import { analyticsService } from "../src/credentials/analyticsService.js";
import { tokenExpirationMonitor } from "../src/auth/tokenExpirationMonitor.js";

describe("parseBoundedInt (#2086)", () => {
  it("returns the fallback when the parameter is missing or blank", () => {
    expect(parseBoundedInt(null, 1, 200, 50)).toBe(50);
    expect(parseBoundedInt(undefined, 1, 200, 50)).toBe(50);
    expect(parseBoundedInt("", 1, 200, 50)).toBe(50);
    expect(parseBoundedInt("   ", 1, 200, 50)).toBe(50);
  });

  it("returns the fallback for non-numeric input instead of NaN", () => {
    expect(parseBoundedInt("abc", 1, 200, 50)).toBe(50);
    expect(parseBoundedInt("12abc-", 1, 200, 50)).toBe(50);
    expect(parseBoundedInt("Infinity", 1, 200, 50)).toBe(50);
    expect(Number.isNaN(parseBoundedInt("abc", 1, 200, 50))).toBe(false);
  });

  it("clamps values above max and below min", () => {
    expect(parseBoundedInt("1e9", 1, 200, 50)).toBe(200);
    expect(parseBoundedInt("999999", 1, 200, 50)).toBe(200);
    expect(parseBoundedInt("-5", 1, 365, 30)).toBe(1);
    expect(parseBoundedInt("0", 1, 200, 50)).toBe(1);
  });

  it("passes in-range values through, truncating fractions", () => {
    expect(parseBoundedInt("25", 1, 200, 50)).toBe(25);
    expect(parseBoundedInt(" 7 ", 1, 200, 50)).toBe(7);
    expect(parseBoundedInt("3.9", 1, 200, 50)).toBe(3);
  });
});

function fakeReq(url: string): IncomingMessage {
  return { url, method: "GET", headers: { host: "internal" } } as unknown as IncomingMessage;
}

function fakeRes() {
  const res = {
    statusCode: 200,
    writeHead(code: number) {
      res.statusCode = code;
      return res;
    },
    setHeader() {},
    getHeader() {
      return undefined;
    },
    end() {},
  };
  return res as unknown as ServerResponse & { statusCode: number };
}

const ctx = {
  authSecret: "test-secret",
  tokenTtlSeconds: 60,
} as unknown as RouteContext;

describe("routes apply bounded query parameters (#2086)", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("/audit-report/history clamps limit=1e9 and survives limit=abc", () => {
    const spy = vi.spyOn(auditReportGenerator, "getReportHistory").mockReturnValue([]);
    handleHttpRequest(fakeReq("/audit-report/history?limit=1e9"), fakeRes(), ctx);
    expect(spy).toHaveBeenLastCalledWith(200);

    handleHttpRequest(fakeReq("/audit-report/history?limit=abc"), fakeRes(), ctx);
    expect(spy).toHaveBeenLastCalledWith(50);
  });

  it("/auth/tokens/warnings/recent bounds limit", () => {
    const spy = vi.spyOn(tokenExpirationMonitor, "getRecentWarnings").mockReturnValue([]);
    handleHttpRequest(fakeReq("/auth/tokens/warnings/recent?limit=1e9"), fakeRes(), ctx);
    expect(spy).toHaveBeenLastCalledWith(200);
  });

  it("analytics trends clamp negative days to the minimum", () => {
    const spy = vi.spyOn(analyticsService, "getTrendData").mockReturnValue({} as never);
    handleHttpRequest(
      fakeReq("/analytics/credentials/holder-1/trends?days=-5"),
      fakeRes(),
      ctx
    );
    expect(spy).toHaveBeenCalled();
    expect(spy.mock.calls[0]?.[1]).toBe(1);
  });
});
