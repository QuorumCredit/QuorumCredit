import { describe, it, expect, vi } from "vitest";
import type { IncomingMessage, ServerResponse } from "node:http";
import { handleSearchRequest } from "../src/http/searchRoutes.js";

function run(url: string) {
  const search = vi.fn(() => ({ results: [], total: 0 }));
  const captured = { status: 0, body: "" };
  const res = {
    writeHead(code: number) {
      captured.status = code;
      return res;
    },
    end(body: string) {
      captured.body = body;
    },
  } as unknown as ServerResponse;
  const req = { url, method: "GET", headers: {} } as unknown as IncomingMessage;
  handleSearchRequest(req, res, { searchService: { search } as never });
  return { search, ...captured };
}

describe("search route numeric parameter validation (#2087)", () => {
  it("returns 400 for non-numeric limit instead of passing NaN", () => {
    const r = run("/api/search?limit=abc");
    expect(r.status).toBe(400);
    expect(r.body).toContain("limit");
    expect(r.search).not.toHaveBeenCalled();
  });

  it("returns 400 for NaN-producing startLedger, endLedger and offset", () => {
    for (const key of ["startLedger", "endLedger", "offset"]) {
      const r = run(`/api/search?${key}=xyz`);
      expect(r.status, key).toBe(400);
      expect(r.body, key).toContain(key);
      expect(r.search, key).not.toHaveBeenCalled();
    }
  });

  it("rejects fractional and exponent forms rather than truncating them", () => {
    expect(run("/api/search?limit=1e3").status).toBe(400);
    expect(run("/api/search?offset=2.5").status).toBe(400);
  });

  it("accepts valid integers and forwards them to the service", () => {
    const r = run("/api/search?startLedger=100&endLedger=200&limit=25&offset=5");
    expect(r.status).toBe(200);
    expect(r.search).toHaveBeenCalledWith({
      q: undefined,
      category: undefined,
      action: undefined,
      startLedger: 100,
      endLedger: 200,
      limit: 25,
      offset: 5,
    });
  });

  it("keeps defaults when numeric parameters are absent or blank", () => {
    const r = run("/api/search?limit=&offset=");
    expect(r.status).toBe(200);
    expect(r.search).toHaveBeenCalledWith(
      expect.objectContaining({ limit: 100, offset: 0, startLedger: undefined, endLedger: undefined })
    );
  });

  it("still clamps limit to 1000", () => {
    const r = run("/api/search?limit=5000");
    expect(r.search).toHaveBeenCalledWith(expect.objectContaining({ limit: 1000 }));
  });
});
