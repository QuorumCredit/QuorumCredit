import { describe, it, expect, vi, afterEach } from "vitest";
import { CredentialStore, credentialStore, freshId } from "../src/credentials/credentialStore.js";
import { CredentialLinkingService } from "../src/credentials/credentialLinkingService.js";

const FUTURE = Math.floor(Date.now() / 1000) + 86_400;

describe("credential and link ID generation (#2085)", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("issues credential IDs that do not repeat across a simulated restart", () => {
    const beforeRestart = new CredentialStore();
    const first = beforeRestart.issueCredential("holder-1", "identity", "issuer-a", FUTURE);

    // A restarted process starts with a fresh store; a counter-based ID would repeat here.
    const afterRestart = new CredentialStore();
    const second = afterRestart.issueCredential("holder-1", "identity", "issuer-a", FUTURE);

    expect(second.id).not.toBe(first.id);
    expect(second.id).toMatch(/^cred_[0-9a-f]{24}$/);
  });

  it("issues verification IDs from randomness, not a counter", () => {
    const store = new CredentialStore();
    const cred = store.issueCredential("holder-1", "identity", "issuer-a", FUTURE);
    store.recordVerification(cred.id, "holder-1", "passport", FUTURE);
    const [firstKey] = (store as unknown as { verifications: Map<string, unknown> }).verifications.keys();

    const restarted = new CredentialStore();
    const cred2 = restarted.issueCredential("holder-1", "identity", "issuer-a", FUTURE);
    restarted.recordVerification(cred2.id, "holder-1", "passport", FUTURE);
    const [secondKey] = (restarted as unknown as { verifications: Map<string, unknown> }).verifications.keys();

    expect(secondKey).not.toBe(firstKey);
    expect(firstKey).toMatch(/^ver_[0-9a-f]{24}$/);
  });

  it("issues link IDs that do not repeat across a simulated restart", () => {
    const c1 = credentialStore.issueCredential("holder-link", "education", "issuer-l", FUTURE);
    const c2 = credentialStore.issueCredential("holder-link", "education", "issuer-l", FUTURE);
    const c3 = credentialStore.issueCredential("holder-link", "education", "issuer-l", FUTURE);
    const c4 = credentialStore.issueCredential("holder-link", "education", "issuer-l", FUTURE);

    const first = new CredentialLinkingService();
    const a = first.linkCredential({
      sourceId: c1.id,
      targetId: c2.id,
      type: "derived_from",
      createdBy: "tester",
    });
    expect(a.ok).toBe(true);

    const restarted = new CredentialLinkingService();
    const b = restarted.linkCredential({
      sourceId: c3.id,
      targetId: c4.id,
      type: "derived_from",
      createdBy: "tester",
    });
    expect(b.ok).toBe(true);
    if (a.ok && b.ok) {
      expect(b.link.id).not.toBe(a.link.id);
      expect(b.link.id).toMatch(/^link_[0-9a-f]{24}$/);
    }
  });

  it("freshId refuses to hand out a key that already exists", () => {
    const taken = new Set<string>();
    // Force the RNG-driven candidate to collide with an existing key on every attempt.
    const existing = { has: (key: string) => { taken.add(key); return true; } };
    expect(() => freshId("cred", existing)).toThrow(/could not allocate a unique cred id/);
    expect(taken.size).toBeGreaterThan(0);
  });

  it("freshId returns an ID that is not already present", () => {
    const existing = new Set<string>(["cred_taken"]);
    const id = freshId("cred", existing);
    expect(existing.has(id)).toBe(false);
    expect(id.startsWith("cred_")).toBe(true);
  });
});
