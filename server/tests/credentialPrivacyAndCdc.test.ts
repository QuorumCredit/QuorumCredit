import { describe, it, expect } from "vitest";
import { commitHolderName } from "../src/credentials/privacyPolicy.js";
import { CredentialCdc } from "../src/credentials/credentialCdc.js";

describe("credential privacy", () => {
  it("does not expose the holder's name in the commitment", () => {
    const commitment = commitHolderName("Ada Lovelace", "salt-1");
    expect(JSON.stringify(commitment)).not.toContain("Ada");
    expect(JSON.stringify(commitment)).not.toContain("Lovelace");
  });
});

describe("credential change data capture ordering", () => {
  it("lists changes newest first with increasing sequence numbers", () => {
    const store = { onStatusChange: () => () => {} };
    const cdc = new CredentialCdc(store as never);
    cdc.captureInsert({ id: "cred-1", holderId: "holder-1" } as never);
    cdc.captureInsert({ id: "cred-2", holderId: "holder-1" } as never);
    const changes = cdc.listChanges();
    expect(changes).toHaveLength(2);
    expect(changes[0]!.seq).toBeGreaterThan(changes[1]!.seq);
    expect(changes[0]!.credentialId).toBe("cred-2");
    cdc.close();
  });
});
