import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { afterEach, describe, expect, it } from "vitest";
import { issueToken } from "../src/auth/tokens.js";
import { LocalApiRateLimiter } from "../src/auth/apiRateLimiter.js";
import { handleApiV1Request } from "../src/http/apiV1Routes.js";
import { credentialStore } from "../src/credentials/credentialStore.js";
import {
  FieldSelectionError,
  parseFieldsParam,
  projectCredential,
  type Credential,
} from "../src/credentials/fieldProjection.js";

const AUTH_SECRET = "test-secret-with-at-least-32-characters";
const servers: Array<ReturnType<typeof createServer>> = [];

afterEach(async () => {
  await Promise.all(
    servers.splice(0).map(
      (server) =>
        new Promise<void>((resolve, reject) => {
          server.close((error) => (error ? reject(error) : resolve()));
        })
    )
  );
});

const sample: Credential = {
  id: "cred_1",
  holderId: "holder-a",
  type: "identity",
  issuedAt: 100,
  expiresAt: 200,
  issuer: "issuer-x",
  status: "active",
  metadata: {
    score: 720,
    level: { tier: "gold", region: "eu" },
    notes: "private",
  },
};

describe("parseFieldsParam", () => {
  it("returns undefined when no selection is requested", () => {
    expect(parseFieldsParam(null)).toBeUndefined();
    expect(parseFieldsParam("")).toBeUndefined();
    expect(parseFieldsParam("  ,  ")).toBeUndefined();
  });

  it("parses, trims and de-duplicates comma-separated paths", () => {
    expect(parseFieldsParam("id, status ,id")).toEqual(["id", "status"]);
    expect(parseFieldsParam("metadata.score,metadata.level.tier")).toEqual([
      "metadata.score",
      "metadata.level.tier",
    ]);
  });

  it("rejects unknown top-level fields", () => {
    expect(() => parseFieldsParam("id,secret")).toThrow(FieldSelectionError);
    expect(() => parseFieldsParam("id,secret")).toThrow("unknown field: secret");
  });

  it("rejects nested selection on non-container fields", () => {
    expect(() => parseFieldsParam("status.value")).toThrow(
      "field status does not support nested selection"
    );
  });

  it("rejects malformed paths", () => {
    expect(() => parseFieldsParam("metadata..score")).toThrow("invalid field path");
    expect(() => parseFieldsParam("metadata.1bad")).toThrow("invalid field path");
  });

  it("caps the number of requested paths", () => {
    const many = Array.from({ length: 33 }, (_, i) => `metadata.k${i}`).join(",");
    expect(() => parseFieldsParam(many)).toThrow("fields cannot exceed 32 paths");
  });
});

describe("projectCredential", () => {
  it("returns the full credential when no paths are given", () => {
    expect(projectCredential(sample)).toEqual(sample);
  });

  it("returns only the requested top-level fields", () => {
    expect(projectCredential(sample, ["id", "status"])).toEqual({
      id: "cred_1",
      status: "active",
    });
  });

  it("selects nested metadata keys and implies the parent container", () => {
    expect(projectCredential(sample, ["metadata.score"])).toEqual({
      metadata: { score: 720 },
    });
    expect(projectCredential(sample, ["metadata.level.tier"])).toEqual({
      metadata: { level: { tier: "gold" } },
    });
  });

  it("returns the whole metadata object when it is selected alongside a nested key", () => {
    expect(projectCredential(sample, ["metadata.score", "metadata"])).toEqual({
      metadata: sample.metadata,
    });
    expect(projectCredential(sample, ["metadata", "metadata.score"])).toEqual({
      metadata: sample.metadata,
    });
  });

  it("omits nested keys that do not exist without failing", () => {
    expect(projectCredential(sample, ["metadata.missing"])).toEqual({});
  });
});

async function startServer(): Promise<string> {
  const server = createServer((req, res) => {
    handleApiV1Request(
      req,
      res,
      new URL(req.url ?? "/", "http://localhost"),
      {
        authSecret: AUTH_SECRET,
        apiRateLimiter: new LocalApiRateLimiter({
          windowMs: 60_000,
          tiers: { free: 1000, pro: 1000, enterprise: 1000 },
        }),
      }
    );
  });
  servers.push(server);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  return `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
}

describe("GET /api/v1/credentials/:id", () => {
  it("returns only the requested fields to the credential holder", async () => {
    const holder = `holder-${Date.now()}`;
    const credential = credentialStore.issueCredential(
      holder,
      "identity",
      "issuer-x",
      Math.floor(Date.now() / 1000) + 3600,
      { score: 99, notes: "private" }
    );
    const baseUrl = await startServer();
    const token = issueToken(AUTH_SECRET, holder, 60).token;

    const response = await fetch(
      `${baseUrl}/api/v1/credentials/${credential.id}?fields=id,metadata.score`,
      { headers: { authorization: `Bearer ${token}` } }
    );
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({
      id: credential.id,
      metadata: { score: 99 },
    });
  });

  it("returns the full record when fields is omitted", async () => {
    const holder = `holder-full-${Date.now()}`;
    const credential = credentialStore.issueCredential(
      holder,
      "education",
      "issuer-y",
      Math.floor(Date.now() / 1000) + 3600
    );
    const baseUrl = await startServer();
    const token = issueToken(AUTH_SECRET, holder, 60).token;

    const response = await fetch(`${baseUrl}/api/v1/credentials/${credential.id}`, {
      headers: { authorization: `Bearer ${token}` },
    });
    expect(response.status).toBe(200);
    expect(await response.json()).toMatchObject({
      id: credential.id,
      holderId: holder,
      issuer: "issuer-y",
    });
  });

  it("returns 400 for an invalid fields parameter", async () => {
    const baseUrl = await startServer();
    const token = issueToken(AUTH_SECRET, "anyone", 60).token;
    const response = await fetch(
      `${baseUrl}/api/v1/credentials/whatever?fields=nope`,
      { headers: { authorization: `Bearer ${token}` } }
    );
    expect(response.status).toBe(400);
    expect(await response.json()).toEqual({ error: "unknown field: nope" });
  });

  it("returns 404 for a credential owned by another holder", async () => {
    const owner = `owner-${Date.now()}`;
    const credential = credentialStore.issueCredential(
      owner,
      "financial",
      "issuer-z",
      Math.floor(Date.now() / 1000) + 3600
    );
    const baseUrl = await startServer();
    const intruder = issueToken(AUTH_SECRET, `intruder-${Date.now()}`, 60).token;
    const response = await fetch(`${baseUrl}/api/v1/credentials/${credential.id}`, {
      headers: { authorization: `Bearer ${intruder}` },
    });
    expect(response.status).toBe(404);
  });
});
