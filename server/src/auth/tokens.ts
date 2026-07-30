/**
 * Issue #1292: JTI-based token revocation.
 *
 * Every issued token now carries a `jti` (JWT ID) claim — a random 128-bit
 * hex string.  `verifyToken` accepts an optional `RevocationStore`; when
 * supplied it checks the denylist before accepting the token.  A revoked
 * token is rejected with reason `"revoked"` regardless of its signature or
 * expiry.
 *
 * Backward-compatible: tokens issued without a `jti` (old format) are still
 * accepted unless the subject's entire history has been revoked via
 * `revokeAllForSubject`.
 */
import { createHmac, randomBytes, timingSafeEqual } from "node:crypto";
import type { RevocationStore } from "./jtiRevocationStore.js";

export interface TokenPayload {
  sub: string;
  borrower?: string;
  iat: number;
  exp: number;
  /** Issue #1292: unique token identifier for targeted revocation. */
  jti?: string;
}

export interface IssuedToken {
  token: string;
  expiresAt: number;
  /** The JTI embedded in this token (use to revoke it individually). */
  jti: string;
}

function base64url(input: Buffer | string): string {
  return Buffer.from(input).toString("base64url");
}

function sign(secret: string, payload: string): string {
  return createHmac("sha256", secret).update(payload).digest("base64url");
}

/** Generate a cryptographically-random JTI (128-bit hex string). */
function generateJti(): string {
  return randomBytes(16).toString("hex");
}

/**
 * Issue a short-lived, HMAC-signed bearer token with a `jti` claim.
 *
 * Not a general JWT implementation — header-less, single-purpose, sufficient
 * for authenticating dashboard socket connections without an external library.
 */
export function issueToken(
  secret: string,
  subject: string,
  ttlSeconds: number,
  borrower?: string
): IssuedToken {
  const now = Math.floor(Date.now() / 1000);
  const jti = generateJti();
  const payload: TokenPayload = {
    sub: subject,
    borrower,
    iat: now,
    exp: now + ttlSeconds,
    jti,
  };
  const encodedPayload = base64url(JSON.stringify(payload));
  const signature = sign(secret, encodedPayload);
  return {
    token: `${encodedPayload}.${signature}`,
    expiresAt: payload.exp * 1000,
    jti,
  };
}

export type VerifyResult =
  | { valid: true; payload: TokenPayload }
  | { valid: false; reason: "malformed" | "bad_signature" | "expired" | "revoked" };

/**
 * Verify a token's signature and expiry.
 *
 * When `revocationStore` is provided, the JTI denylist is consulted after
 * signature verification — a revoked token is rejected with reason `"revoked"`.
 *
 * The function is intentionally synchronous for the common path (no Redis),
 * and returns a Promise only when a revocation store is supplied.
 */
export function verifyToken(
  secret: string,
  token: string,
  revocationStore?: RevocationStore
): VerifyResult | Promise<VerifyResult> {
  const parts = token.split(".");
  if (parts.length !== 2) return { valid: false, reason: "malformed" };
  const [encodedPayload, signature] = parts;
  const expected = sign(secret, encodedPayload);

  const a = Buffer.from(signature);
  const b = Buffer.from(expected);
  if (a.length !== b.length || !timingSafeEqual(a, b)) {
    return { valid: false, reason: "bad_signature" };
  }

  let payload: TokenPayload;
  try {
    payload = JSON.parse(Buffer.from(encodedPayload, "base64url").toString("utf8"));
  } catch {
    return { valid: false, reason: "malformed" };
  }

  if (typeof payload.exp !== "number" || payload.exp * 1000 < Date.now()) {
    return { valid: false, reason: "expired" };
  }

  // No revocation store — accept the token.
  if (!revocationStore) {
    return { valid: true, payload };
  }

  // Async denylist check.
  return checkRevocation(revocationStore, payload);
}

async function checkRevocation(
  store: RevocationStore,
  payload: TokenPayload
): Promise<VerifyResult> {
  // Check per-JTI revocation.
  if (payload.jti) {
    if (await store.isRevoked(payload.jti)) {
      return { valid: false, reason: "revoked" };
    }
  }

  // Check subject-level revocation (revokeAllForSubject).
  // Only RedisRevocationStore exposes isSubjectRevokedBefore; fall back
  // gracefully when using LocalRevocationStore.
  if ("isSubjectRevokedBefore" in store && payload.sub && payload.iat) {
    const redisStore = store as import("./jtiRevocationStore.js").RedisRevocationStore;
    if (await redisStore.isSubjectRevokedBefore(payload.sub, payload.iat)) {
      return { valid: false, reason: "revoked" };
    }
  }

  return { valid: true, payload };
}

/** True when the token is valid but will expire within `windowMs`. */
export function isExpiringSoon(payload: TokenPayload, windowMs: number): boolean {
  return payload.exp * 1000 - Date.now() < windowMs;
}
