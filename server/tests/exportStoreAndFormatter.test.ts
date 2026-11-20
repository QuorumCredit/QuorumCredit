import { describe, it, expect, vi } from "vitest";
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

describe("export cleanup and scheduling", () => {
  it("drops expired completed exports when pending exports are read", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-01T00:00:00Z"));
    const store = new ExportStore();
    const request = store.createExportRequest("cred-1", "json");
    // Internal map access: the store has no public way to complete an export.
    const stored = (store as any).exports.get(request.id);
    stored.status = "completed";
    stored.expiresAt = Date.now() + 1000;
    vi.setSystemTime(Date.now() + 5000);

    store.getPendingExports();

    expect((store as any).exports.has(request.id)).toBe(false);
    vi.useRealTimers();
  });

  it("schedules the next run from the previous due time and counts missed periods", () => {
    const day = 24 * 60 * 60 * 1000;
    const start = Date.parse("2026-01-01T00:00:00Z");
    vi.useFakeTimers();
    vi.setSystemTime(start);
    const store = new ExportStore();
    const scheduled = store.createScheduledExport("cred-1", "json", "daily", start);

    // The first run is due at start + 1 day but happens 1.5 days late.
    vi.setSystemTime(start + day + day / 2);
    store.updateNextExportTime(scheduled.id);
    expect(store.getScheduledExport(scheduled.id)?.nextExportAt).toBe(start + 2 * day);

    // Running 3.5 days after start skips the period that ended at start + 3 days.
    vi.setSystemTime(start + 3 * day + day / 2);
    store.updateNextExportTime(scheduled.id);
    expect(store.getScheduledExport(scheduled.id)?.nextExportAt).toBe(start + 4 * day);
    expect(store.getScheduledExport(scheduled.id)?.missedRuns).toBe(1);
    vi.useRealTimers();
  });
});
