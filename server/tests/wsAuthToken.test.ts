/**
 * Issue #2156: unit tests for the shared WebSocket token resolver.
 *
 * The integration behaviour (server refuses/accepts real handshakes) is covered in
 * metricsWsAuth.test.ts and credentialStatusWsServer.test.ts; here we pin the pure
 * parsing and deprecation logic directly.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import type { IncomingMessage } from "node:http";
import {
  parseBearerSubprotocol,
  selectAuthSubprotocol,
  extractWsToken,
  DEFAULT_WS_QUERY_TOKEN_SUNSET_DATE,
} from "../src/ws/wsAuthToken.js";
import { logger } from "../src/logging/logger.js";

function fakeReq(
  headers: Record<string, string | undefined>,
  url = "/ws/metrics"
): IncomingMessage {
  return { headers, url } as unknown as IncomingMessage;
}

describe("parseBearerSubprotocol", () => {
  it("extracts the token from a single bearer subprotocol", () => {
    expect(parseBearerSubprotocol("bearer.abc.def")).toBe("abc.def");
  });

  it("extracts the token from a multi-value header, picking the bearer entry", () => {
    expect(parseBearerSubprotocol("bearer, bearer.s3cr3t")).toBe("s3cr3t");
  });

  it("ignores non-bearer subprotocols", () => {
    expect(parseBearerSubprotocol("chat, json")).toBeNull();
  });

  it("returns null when the header is absent", () => {
    expect(parseBearerSubprotocol(undefined)).toBeNull();
  });

  it("returns null for a bare 'bearer' with no token after the dot", () => {
    expect(parseBearerSubprotocol("bearer.")).toBeNull();
  });
});

describe("selectAuthSubprotocol", () => {
  it("acks the token-less 'bearer' so the token is never echoed back", () => {
    expect(selectAuthSubprotocol(new Set(["bearer", "bearer.tok"]))).toBe("bearer");
  });

  it("echoes the offered token subprotocol when 'bearer' was not offered", () => {
    // Single-subprotocol client fallback — still completes the handshake.
    expect(selectAuthSubprotocol(new Set(["bearer.tok"]))).toBe("bearer.tok");
  });

  it("returns false when no subprotocols were offered", () => {
    expect(selectAuthSubprotocol(new Set())).toBe(false);
  });
});

describe("extractWsToken", () => {
  let warnSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    // Suppress real log output while still recording the call.
    warnSpy = vi.spyOn(logger, "warn").mockImplementation(() => undefined);
  });

  afterEach(() => {
    warnSpy.mockRestore();
  });

  it("prefers the Sec-WebSocket-Protocol header and emits no deprecation warning", () => {
    const res = extractWsToken(fakeReq({ "sec-websocket-protocol": "bearer, bearer.tok" }));
    expect(res).toEqual({ token: "tok", source: "subprotocol", refused: false });
    expect(warnSpy).not.toHaveBeenCalled();
  });

  it("uses the query string and warns before the sunset date", () => {
    const far = new Date(Date.now() + 365 * 24 * 60 * 60 * 1000);
    const res = extractWsToken(fakeReq({}, "/ws/metrics?token=q"), {
      queryTokenSunsetDate: far,
    });
    expect(res).toEqual({ token: "q", source: "query", refused: false });
    expect(warnSpy).toHaveBeenCalledTimes(1);
  });

  it("refuses the query string once the sunset date has passed (token is not returned)", () => {
    const res = extractWsToken(fakeReq({}, "/ws/metrics?token=q"), {
      queryTokenSunsetDate: new Date(0),
    });
    expect(res).toEqual({ token: null, source: "query", refused: true });
    // Still warned — the use of the deprecated mechanism is what triggers it.
    expect(warnSpy).toHaveBeenCalledTimes(1);
  });

  it("returns no token (and no warning) when nothing is supplied", () => {
    const res = extractWsToken(fakeReq({}, "/ws/metrics"));
    expect(res).toEqual({ token: null, source: null, refused: false });
    expect(warnSpy).not.toHaveBeenCalled();
  });

  it("defaults to the shared sunset date when no option is given", () => {
    // Sanity: the default is a real, fixed future date.
    expect(DEFAULT_WS_QUERY_TOKEN_SUNSET_DATE.getTime()).toBeGreaterThan(Date.now());
    const res = extractWsToken(fakeReq({}, "/ws/metrics?token=q"));
    expect(res.source).toBe("query");
    expect(res.refused).toBe(false);
  });

  it("prefers the header even when a query token is also present", () => {
    const res = extractWsToken(
      fakeReq({ "sec-websocket-protocol": "bearer.hdr" }, "/ws/metrics?token=q")
    );
    expect(res).toEqual({ token: "hdr", source: "subprotocol", refused: false });
    expect(warnSpy).not.toHaveBeenCalled();
  });
});
