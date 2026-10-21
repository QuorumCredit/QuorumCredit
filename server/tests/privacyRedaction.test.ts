import { describe, it, expect } from "vitest";
import { redactForViewer } from "../src/credentials/privacyPolicy.js";

const record = { holderName: "Ada Lovelace", holderEmail: "ada@example.com", status: "active" };

describe("privacy redaction for non-owners", () => {
  it("redacts each listed field for a viewer who is not the owner", () => {
    const seen = redactForViewer(record, "holder-1", "verifier-9", ["holderName", "holderEmail"]);
    expect(seen.holderName).toBeNull();
    expect(seen.holderEmail).toBeNull();
    expect(seen.status).toBe("active");
  });

  it("shows every field to the owner", () => {
    expect(redactForViewer(record, "holder-1", "holder-1", ["holderName", "holderEmail"])).toEqual(record);
  });
});
