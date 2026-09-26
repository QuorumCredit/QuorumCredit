import { describe, expect, it } from "vitest";
import { CredentialStore } from "../src/credentials/credentialStore.js";
import {
  CredentialImportStore,
  parseCredentialCsv,
  parseCredentialJson,
} from "../src/batch/credentialImport.js";

const future = Math.floor(Date.now() / 1_000) + 60;

describe("credential batch import", () => {
  it("parses JSON records and returns row-level validation errors", () => {
    const rows = parseCredentialJson([
      {
        holderId: "holder-a",
        type: "identity",
        issuer: "issuer-a",
        expiresAt: future,
        metadata: { source: "test" },
      },
      { holderId: "", type: "unknown", issuer: "", expiresAt: 1 },
    ]);

    expect(rows[0]?.input?.holderId).toBe("holder-a");
    expect(rows[1]?.errors).toContain("holderId is required");
    expect(rows[1]?.errors).toContain("issuer is required");
    expect(rows[1]?.errors).toContain(
      "type must be identity, education, professional, or financial"
    );
  });

  it("parses quoted CSV and reports invalid rows without rejecting valid records", () => {
    const rows = parseCredentialCsv(
      `holderId,type,issuer,expiresAt,metadata\r\n"holder, one",identity,issuer,${future},"{""source"":""csv""}"\r\n,education,issuer,${future},{}`
    );

    expect(rows).toHaveLength(2);
    expect(rows[0]?.input?.holderId).toBe("holder, one");
    expect(rows[0]?.input?.metadata).toEqual({ source: "csv" });
    expect(rows[1]?.errors).toContain("holderId is required");
  });

  it("tracks completion, individual outcomes, and owner-only status access", async () => {
    const store = new CredentialImportStore();
    const credentials = new CredentialStore();
    const batch = store.startImport(
      parseCredentialJson([
        {
          holderId: "holder-import",
          type: "education",
          issuer: "issuer",
          expiresAt: future,
        },
        { holderId: "", type: "identity", issuer: "issuer", expiresAt: future },
      ]),
      credentials,
      "owner-a"
    );

    expect(batch.total).toBe(2);
    expect(store.getBatch(batch.importId, "owner-b")).toBeUndefined();
    await new Promise<void>((resolve) => setImmediate(resolve));
    const completed = store.getBatch(batch.importId, "owner-a");
    expect(completed).toMatchObject({
      status: "completed",
      processed: 2,
      imported: 1,
      failed: 1,
      progress: 100,
    });
    expect(credentials.getAllCredentials()).toHaveLength(1);
  });

  it("rejects malformed CSV input", () => {
    expect(() => parseCredentialCsv('holderId,type,issuer,expiresAt\n"unterminated')).toThrow(
      "unterminated quoted field"
    );
  });
});
