import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// Scaffold for #1947: checks the documented contract itself. Matching each route in
// server/src/http/routes.ts to an openapi.yaml entry is the next step.
const spec = readFileSync(resolve(__dirname, "../../openapi.yaml"), "utf8");
const paths = [...spec.matchAll(/^ {2}(\/[^\s:]+):\s*$/gm)].map((m) => m[1]!);

describe("openapi.yaml contract", () => {
  it("documents at least one path", () => {
    expect(paths.length).toBeGreaterThan(0);
  });

  it("does not list the same path twice", () => {
    expect(new Set(paths).size).toBe(paths.length);
  });

  it("gives every path at least one operation with responses", () => {
    for (const path of paths) {
      const start = spec.indexOf(`\n  ${path}:`);
      const block = spec.slice(start + 1).split(/\n {2}\//)[0] ?? "";
      expect(block, path).toMatch(/responses:/);
    }
  });
});
