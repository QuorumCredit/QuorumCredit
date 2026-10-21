import { describe, it, expect } from "vitest";
import { ExportStore } from "../src/exports/exportStore.js";
import { ExportFormatter } from "../src/exports/exportFormatter.js";

const data = {
  credentialId: "cred-1",
  holder: "Ada",
  issueDate: 1700000000,
  expiryDate: 1800000000,
  status: "active",
};

describe("export ownership", () => {
  it("returns an export to the credential that owns it", () => {
    const store = new ExportStore();
    const request = store.createExportRequest("cred-1", "json");
    expect(store.getExportFor(request.id, "cred-1")?.id).toBe(request.id);
  });

  it("refuses to return an export owned by another credential", () => {
    const store = new ExportStore();
    const request = store.createExportRequest("cred-1", "json");
    expect(store.getExportFor(request.id, "cred-2")).toBeUndefined();
  });
});

describe("export formatting", () => {
  it("writes JSON that parses back to the same data", () => {
    const out = new ExportFormatter().format("cred-1", data, "json");
    expect(JSON.parse(out.toString("utf8"))).toEqual(data);
  });

  it("writes CSV with a header row and the credential row", () => {
    const out = new ExportFormatter().format("cred-1", data, "csv").toString("utf8");
    expect(out).toContain("Credential ID");
    expect(out).toContain("cred-1");
  });
});
