/**
 * Issue #2156: WebSocket bearer tokens must not travel in the URL query string.
 *
 * `?token=` is recorded by proxies, load balancers and browser history, leaking
 * the credential to anyone with access to those logs. Instead the token is carried
 * in the `Sec-WebSocket-Protocol` handshake header, which is not part of the request
 * URL and is not written to standard access logs.
 *
 * Clients offer a pair of subprotocols — `["bearer", "bearer.<token>"]` — and the
 * server acks the token-less `bearer` entry (see {@link selectAuthSubprotocol}) so
 * the token is never echoed back in the response header either.
 *
 * The legacy `?token=` query parameter is deprecated rather than removed outright:
 * it still authenticates during the migration window so existing clients keep
 * working, but each use logs a warning, and after {@link queryTokenSunsetDate} the
 * connection is refused outright. This module is shared by the raw-`ws` servers
 * (metrics + credential-status); the socket.io loan server already authenticates
 * via the in-handshake `auth` payload, which is likewise not part of the URL.
 */
import type { IncomingMessage } from "node:http";
import { logger } from "../logging/logger.js";

/** Subprotocol scheme prefix clients use to carry the bearer token. */
export const BEARER_SUBPROTOCOL_PREFIX = "bearer.";

/**
 * Date after which `?token=` query-string auth is refused (not merely warned).
 * Configurable per server via options; this default leaves a generous migration
 * window and is intentionally shared by both raw-`ws` servers.
 */
export const DEFAULT_WS_QUERY_TOKEN_SUNSET_DATE = new Date("2027-03-31T00:00:00Z");

export type WsTokenSource = "subprotocol" | "query";

export interface ExtractWsTokenOptions {
  /** Refuse `?token=` on or after this date. Defaults to the standard sunset. */
  queryTokenSunsetDate?: Date;
}

export interface ExtractWsTokenResult {
  /** Token string to verify, or null when none was supplied. */
  token: string | null;
  /** Where the token came from; null when none was supplied. */
  source: WsTokenSource | null;
  /**
   * True when a deprecated `?token=` was used past the sunset date. Callers must
   * refuse the handshake (401) and must NOT verify the token. The deprecation
   * warning has already been logged by this point.
   */
  refused: boolean;
}

/**
 * Parse a `Sec-WebSocket-Protocol` header value and return the bearer token
 * carried by the first `bearer.<token>` entry, or null when none is present.
 */
export function parseBearerSubprotocol(headerValue: string | undefined): string | null {
  if (!headerValue) return null;
  // A client may offer several subprotocols; pick the bearer one.
  for (const raw of headerValue.split(",")) {
    const proto = raw.trim();
    if (proto.startsWith(BEARER_SUBPROTOCOL_PREFIX)) {
      const token = proto.slice(BEARER_SUBPROTOCOL_PREFIX.length);
      if (token) return token;
    }
  }
  return null;
}

/**
 * `handleProtocols` implementation for `ws`. When the client offered the
 * token-less `bearer` subprotocol, ack that one so the token is never echoed in
 * the response header; otherwise echo the first offered protocol so a
 * single-subprotocol client still completes the handshake.
 */
export function selectAuthSubprotocol(protocols: Set<string>): string | false {
  if (protocols.has("bearer")) return "bearer";
  const first = protocols.values().next().value;
  return first ?? false;
}

/**
 * Resolve the bearer token for an incoming WebSocket upgrade, preferring the
 * `Sec-WebSocket-Protocol` header and falling back to the deprecated `?token=`
 * query parameter. Emits a deprecation warning whenever the query parameter is
 * used, and marks the result as refused once the sunset date has passed.
 */
export function extractWsToken(
  req: IncomingMessage,
  opts: ExtractWsTokenOptions = {}
): ExtractWsTokenResult {
  const sunset = opts.queryTokenSunsetDate ?? DEFAULT_WS_QUERY_TOKEN_SUNSET_DATE;

  // Preferred: token carried in the Sec-WebSocket-Protocol handshake header.
  const subproto = parseBearerSubprotocol(req.headers["sec-websocket-protocol"]);
  if (subproto !== null) {
    return { token: subproto, source: "subprotocol", refused: false };
  }

  // Deprecated fallback: token in the URL query string.
  const url = new URL(req.url ?? "", "http://internal");
  const queryToken = url.searchParams.get("token");
  if (queryToken) {
    const pastSunset = Date.now() >= sunset.getTime();
    logger.warn(
      "WebSocket auth via ?token= query parameter is deprecated; pass the token in the Sec-WebSocket-Protocol header instead",
      { pastSunset, sunset: sunset.toISOString(), path: url.pathname }
    );
    if (pastSunset) {
      return { token: null, source: "query", refused: true };
    }
    return { token: queryToken, source: "query", refused: false };
  }

  return { token: null, source: null, refused: false };
}
