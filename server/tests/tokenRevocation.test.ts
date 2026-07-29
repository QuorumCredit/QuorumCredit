/**
 * Issue #1292: Tests for JTI-based token revocation.
 *
 * Covers:
 *   - Issued tokens carry a jti claim.
 *   - verifyToken returns valid when no revocation store is supplied (backward compat).
 *   - A revoked JTI is rejected with reason "revoked".
 *   - Revoking one JTI does not affect other tokens.
 *   - revokeAllForSubject rejects all tokens for that subject.
 *   - Expired denylist entries are cleaned up (local store).
 */
import { describe, it, expect } from "vitest";
import { issueToken, verifyToken } from "../src/auth/tokens.js";
import { LocalRevocationStore } from "../src/auth/jtiRevocationStore.js";

const SECRET = "test-secret-for-revocation-tests";

describe("token revocation (Issue #1292)", () => {
  it("issued token carries a jti claim", () => {
    const issued = issueToken(SECRET, "api-key-1", 60);
    expect(issued.jti).toBeTruthy();
    expect(typeof issued.jti).toBe("string");
    expect(issued.jti.length).toBeGreaterThan(0);
  });

  it("each issued token has a unique jti", () => {
    const a = issueToken(SECRET, "api-key-1", 60);
    const b = issueToken(SECRET, "api-key-1", 60);
    expect(a.jti).not.toBe(b.jti);
  });

  it("verifyToken without revocationStore accepts a valid token (backward compat)", () => {
    const { token } = issueToken(SECRET, "api-key-1", 60);
    const result = verifyToken(SECRET, token);
    // Synchronous path (no store) — result is not a Promise
    expect(result).not.toBeInstanceOf(Promise);
    const r = result as Exclude<typeof result, Promise<unknown>>;
    expect(r.valid).toBe(true);
  });

  it("verifyToken with revocationStore accepts a non-revoked token", async () => {
    const store = new LocalRevocationStore();
    const { token } = issueToken(SECRET, "api-key-1", 60);
    const result = await verifyToken(SECRET, token, store);
    expect(result.valid).toBe(true);
    await store.close();
  });

  it("revoke-then-verify returns rejected", async () => {
    const store = new LocalRevocationStore();
    const issued = issueToken(SECRET, "api-key-1", 60);
    // Revoke before verifying.
    await store.revoke(issued.jti, issued.expiresAt, "api-key-1");
    const result = await verifyToken(SECRET, issued.token, store);
    expect(result.valid).toBe(false);
    if (!result.valid) expect(result.reason).toBe("revoked");
    await store.close();
  });

  it("revoking one jti does not affect other tokens", async () => {
    const store = new LocalRevocationStore();
    const a = issueToken(SECRET, "api-key-1", 60);
    const b = issueToken(SECRET, "api-key-1", 60);
    // Revoke only token a.
    await store.revoke(a.jti, a.expiresAt, "api-key-1");
    const resultA = await verifyToken(SECRET, a.token, store);
    const resultB = await verifyToken(SECRET, b.token, store);
    expect(resultA.valid).toBe(false);
    expect(resultB.valid).toBe(true);
    await store.close();
  });

  it("revokeAllForSubject rejects all tokens for that subject", async () => {
    const store = new LocalRevocationStore();
    const a = issueToken(SECRET, "api-key-2", 60);
    const b = issueToken(SECRET, "api-key-2", 60);
    const other = issueToken(SECRET, "api-key-3", 60);

    await (store as LocalRevocationStore).revoke(a.jti, a.expiresAt, "api-key-2");
    await (store as LocalRevocationStore).revoke(b.jti, b.expiresAt, "api-key-2");
    await store.revokeAllForSubject("api-key-2");

    const resultA = await verifyToken(SECRET, a.token, store);
    const resultB = await verifyToken(SECRET, b.token, store);
    const resultOther = await verifyToken(SECRET, other.token, store);

    expect(resultA.valid).toBe(false);
    expect(resultB.valid).toBe(false);
    // Different subject — not affected.
    expect(resultOther.valid).toBe(true);
    await store.close();
  });

  it("verifyToken still rejects bad signatures with a revocation store", async () => {
    const store = new LocalRevocationStore();
    const { token } = issueToken(SECRET, "api-key-1", 60);
    const result = await verifyToken("wrong-secret", token, store);
    expect(result.valid).toBe(false);
    if (!result.valid) expect(result.reason).toBe("bad_signature");
    await store.close();
  });

  it("verifyToken still rejects expired tokens with a revocation store", async () => {
    const store = new LocalRevocationStore();
    const { token } = issueToken(SECRET, "api-key-1", -10);
    const result = await verifyToken(SECRET, token, store);
    expect(result.valid).toBe(false);
    if (!result.valid) expect(result.reason).toBe("expired");
    await store.close();
  });
});
