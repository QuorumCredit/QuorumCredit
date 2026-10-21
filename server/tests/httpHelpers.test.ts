import { describe, it, expect } from "vitest";
import { Readable } from "node:stream";
import type { IncomingMessage } from "node:http";
import { ResponseTransformer } from "../src/http/responseTransformer.js";
import { readRequestBody, RequestBodyTooLargeError } from "../src/http/requestBody.js";
import { FieldAliasRegistry } from "../src/http/fieldAliasRegistry.js";
import { DeprecationRegistry } from "../src/http/deprecation.js";

const ctx = { req: {} as IncomingMessage, statusCode: 200, ruleId: "wrap" };

describe("response transformer", () => {
  it("transforms nested payloads without mutating the input", () => {
    const transformer = new ResponseTransformer();
    transformer.register({
      id: "wrap",
      description: "wrap the body in data",
      fn: (body) => ({ data: body }),
    });
    const nested = { loan: { id: 1, borrower: { name: "Ada" } } };
    expect(transformer.apply(nested, "wrap", ctx)).toEqual({ data: nested });
    expect(nested).toEqual({ loan: { id: 1, borrower: { name: "Ada" } } });
  });

  it("passes an empty payload through the transform", () => {
    const transformer = new ResponseTransformer();
    transformer.register({ id: "wrap", description: "wrap", fn: (body) => ({ data: body }) });
    expect(transformer.apply({}, "wrap", ctx)).toEqual({ data: {} });
  });
});

describe("request body reader", () => {
  it("reads a body under the limit", async () => {
    const req = Readable.from([Buffer.from("hello")]) as unknown as IncomingMessage;
    const body = await readRequestBody(req, 100);
    expect(body.toString("utf8")).toBe("hello");
  });

  it("rejects a body over the limit", async () => {
    const req = Readable.from([Buffer.from("x".repeat(50))]) as unknown as IncomingMessage;
    await expect(readRequestBody(req, 10)).rejects.toBeInstanceOf(RequestBodyTooLargeError);
  });
});

describe("field aliases", () => {
  it("still resolve the deprecated field name on a response", () => {
    const registry = new FieldAliasRegistry();
    registry.register({
      oldField: "borrower_name",
      newField: "borrower",
      deprecatedAt: new Date("2026-01-01T00:00:00Z"),
      direction: "response",
    } as never);
    const out = registry.applyToResponse({ borrower: "Ada" });
    expect(out.borrower_name).toBe("Ada");
    expect(out.borrower).toBe("Ada");
  });
});

describe("deprecation", () => {
  it("matches a deprecated endpoint and sends the deprecation and sunset headers", () => {
    const registry = new DeprecationRegistry();
    registry.register({
      id: "old-loans",
      method: "GET",
      path: "/api/v1/old-loans",
      deprecatedAt: new Date("2026-01-01T00:00:00Z"),
      sunsetAt: new Date("2026-12-31T00:00:00Z"),
      replacement: "/api/v1/loans",
    });
    const endpoint = registry.match("GET", "/api/v1/old-loans");
    expect(endpoint?.id).toBe("old-loans");
    const headers = registry.headersFor(endpoint!);
    expect(headers.deprecation).toMatch(/^@\d+$/);
    expect(headers.sunset).toBeDefined();
  });
});
