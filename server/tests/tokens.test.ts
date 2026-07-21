import { describe, it, expect } from "vitest";
import { issueToken, verifyToken, isExpiringSoon } from "../src/auth/tokens.js";

const SECRET = "test-secret";

describe("tokens", () => {
  it("issues a token that verifies successfully", () => {
    const { token } = issueToken(SECRET, "borrower-1", 60);
    const result = verifyToken(SECRET, token);
    expect(result.valid).toBe(true);
    if (result.valid) expect(result.payload.sub).toBe("borrower-1");
  });

  it("rejects a token signed with a different secret", () => {
    const { token } = issueToken(SECRET, "borrower-1", 60);
    const result = verifyToken("wrong-secret", token);
    expect(result.valid).toBe(false);
    if (!result.valid) expect(result.reason).toBe("bad_signature");
  });

  it("rejects a malformed token", () => {
    const result = verifyToken(SECRET, "not-a-real-token");
    expect(result.valid).toBe(false);
    if (!result.valid) expect(result.reason).toBe("malformed");
  });

  it("rejects an expired token", () => {
    const { token } = issueToken(SECRET, "borrower-1", -10);
    const result = verifyToken(SECRET, token);
    expect(result.valid).toBe(false);
    if (!result.valid) expect(result.reason).toBe("expired");
  });

  it("flags a token as expiring soon within the warning window", () => {
    const { token } = issueToken(SECRET, "borrower-1", 5);
    const result = verifyToken(SECRET, token);
    expect(result.valid).toBe(true);
    if (result.valid) {
      expect(isExpiringSoon(result.payload, 30_000)).toBe(true);
      expect(isExpiringSoon(result.payload, 1_000)).toBe(false);
    }
  });

  it("carries an optional borrower claim through", () => {
    const { token } = issueToken(SECRET, "api-key-1", 60, "GABC...BORROWER");
    const result = verifyToken(SECRET, token);
    expect(result.valid).toBe(true);
    if (result.valid) expect(result.payload.borrower).toBe("GABC...BORROWER");
  });
});
