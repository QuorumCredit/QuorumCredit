import { createHmac, timingSafeEqual } from "node:crypto";

export interface TokenPayload {
  sub: string;
  borrower?: string;
  iat: number;
  exp: number;
}

export interface IssuedToken {
  token: string;
  expiresAt: number;
}

function base64url(input: Buffer | string): string {
  return Buffer.from(input).toString("base64url");
}

function sign(secret: string, payload: string): string {
  return createHmac("sha256", secret).update(payload).digest("base64url");
}

/** Issues a short-lived, HMAC-signed bearer token (header-less, single-purpose — not a
 * general JWT implementation; sufficient for authenticating dashboard socket connections
 * without pulling in an external JWT dependency). */
export function issueToken(
  secret: string,
  subject: string,
  ttlSeconds: number,
  borrower?: string
): IssuedToken {
  const now = Math.floor(Date.now() / 1000);
  const payload: TokenPayload = { sub: subject, borrower, iat: now, exp: now + ttlSeconds };
  const encodedPayload = base64url(JSON.stringify(payload));
  const signature = sign(secret, encodedPayload);
  return { token: `${encodedPayload}.${signature}`, expiresAt: payload.exp * 1000 };
}

export type VerifyResult =
  | { valid: true; payload: TokenPayload }
  | { valid: false; reason: "malformed" | "bad_signature" | "expired" };

export function verifyToken(secret: string, token: string): VerifyResult {
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

  return { valid: true, payload };
}

/** True when the token is valid but will expire within `windowMs` — used to proactively
 * push an auth_expiring/auth_expired frame so clients refresh before a hard disconnect. */
export function isExpiringSoon(payload: TokenPayload, windowMs: number): boolean {
  return payload.exp * 1000 - Date.now() < windowMs;
}
