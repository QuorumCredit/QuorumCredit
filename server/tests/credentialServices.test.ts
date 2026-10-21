import { describe, it, expect } from "vitest";
import { CredentialAnonymizationService } from "../src/credentials/credentialAnonymizationService.js";
import { CredentialArchivalService } from "../src/credentials/credentialArchival.js";

const DAY = 24 * 60 * 60;

describe("credential anonymization", () => {
  it("removes the direct identifiers and suppresses the listed metadata field", () => {
    const service = new CredentialAnonymizationService("test-salt");
    const credential = {
      id: "cred-secret-1",
      holderId: "holder-secret",
      issuer: "issuer-secret",
      type: "license",
      status: "active",
      issuedAt: 1700000000,
      expiresAt: 1800000000,
      metadata: { email: "ada@example.com", tier: "gold" },
    };

    const out = service.anonymize(credential as never, { suppressMetadataFields: ["email"] });
    const serialised = JSON.stringify(out);
    expect(serialised).not.toContain("cred-secret-1");
    expect(serialised).not.toContain("holder-secret");
    expect(serialised).not.toContain("issuer-secret");
    expect(serialised).not.toContain("ada@example.com");
    expect(serialised).toContain('"email":null');
  });
});

describe("credential archival", () => {
  it("archives a credential past the age limit and keeps its data recoverable", () => {
    const now = Math.floor(Date.now() / 1000);
    const old = {
      id: "cred-old",
      holderId: "holder-1",
      issuer: "issuer-1",
      type: "license",
      status: "active",
      issuedAt: now - 200 * DAY,
      expiresAt: now + DAY,
      metadata: {},
    };
    let active: unknown[] = [old];
    const service = new CredentialArchivalService(
      () => active as never,
      (id) => {
        active = active.filter((c) => (c as { id: string }).id !== id);
      },
      { rules: [{ name: "age-limit", maxAgeSeconds: 90 * DAY }] }
    );

    service.runSweepNow();
    expect(service.isArchived("cred-old")).toBe(true);
    expect(service.getArchivedCredential("cred-old")?.credential).toEqual(old);
    expect(service.getArchivedCredential("cred-old")?.archivedByRule).toBe("age-limit");
  });

  it("keeps a credential inside the age limit active", () => {
    const now = Math.floor(Date.now() / 1000);
    const recent = {
      id: "cred-new",
      holderId: "holder-2",
      issuer: "issuer-1",
      type: "license",
      status: "active",
      issuedAt: now - DAY,
      expiresAt: now + 30 * DAY,
      metadata: {},
    };
    const service = new CredentialArchivalService(
      () => [recent as never],
      () => {},
      { rules: [{ name: "age-limit", maxAgeSeconds: 90 * DAY }] }
    );

    service.runSweepNow();
    expect(service.isArchived("cred-new")).toBe(false);
  });
});
