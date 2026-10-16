import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { afterEach, describe, expect, it } from "vitest";
import { issueToken } from "../src/auth/tokens.js";
import { LocalApiRateLimiter } from "../src/auth/apiRateLimiter.js";
import { handleApiV1Request } from "../src/http/apiV1Routes.js";
import { CredentialStore, credentialStore } from "../src/credentials/credentialStore.js";
import {
  MetadataSearchIndex,
  extractMetadataTerms,
  tokenize,
} from "../src/credentials/metadataSearchIndex.js";

const FUTURE = Math.floor(Date.now() / 1000) + 86_400;

describe("tokenize / extractMetadataTerms", () => {
  it("lowercases and splits on non-alphanumeric characters", () => {
    expect(tokenize("Advanced-Rust, Level_2!")).toEqual(["advanced", "rust", "level", "2"]);
    expect(tokenize("   ")).toEqual([]);
  });

  it("indexes nested keys and leaf values but not structural noise", () => {
    const terms = extractMetadataTerms({
      course: { level: "Advanced", tags: ["rust", "soroban"] },
      score: 92,
      verified: true,
      missing: null,
    });
    expect(terms).toEqual(
      expect.arrayContaining(["course", "level", "advanced", "tags", "rust", "soroban", "score", "92", "verified", "true"])
    );
    expect(terms).not.toContain("null");
  });

  it("stops descending past the maximum depth", () => {
    let deep: Record<string, unknown> = { leaf: "bottomterm" };
    for (let i = 0; i < 10; i++) deep = { [`k${i}`]: deep };
    expect(extractMetadataTerms(deep)).not.toContain("bottomterm");
  });
});

describe("MetadataSearchIndex", () => {
  function setup() {
    const store = new CredentialStore();
    const index = new MetadataSearchIndex(store);
    return { store, index };
  }

  it("finds credentials whose metadata contains every query term", () => {
    const { store, index } = setup();
    const a = store.issueCredential("alice", "education", "Uni", FUTURE, { course: "Rust Basics" });
    const b = store.issueCredential("bob", "education", "Uni", FUTURE, { course: "Soroban Advanced" });
    const c = store.issueCredential("carol", "education", "Uni", FUTURE, { course: "Rust Advanced" });
    index.attach();
    store.issueCredential("dan", "education", "Uni", FUTURE, { course: "Other" });

    const result = index.search("rust advanced");
    if ("error" in result) throw new Error(result.error);
    expect(result.credentials.map((cred) => cred.id)).toEqual([c.id]);
    const rust = index.search("rust");
    if ("error" in rust) throw new Error(rust.error);
    expect(rust.total).toBe(2);
    expect(rust.credentials.map((cred) => cred.id)).toEqual(expect.arrayContaining([a.id, c.id]));
    expect(rust.credentials.map((cred) => cred.id)).not.toContain(b.id);
    index.dispose();
  });

  it("indexes credentials issued after attach", () => {
    const { store, index } = setup();
    index.attach();
    const cred = store.issueCredential("erin", "professional", "Firm", FUTURE, { role: "auditor" });
    const result = index.search("auditor");
    if ("error" in result) throw new Error(result.error);
    expect(result.credentials.map((c) => c.id)).toEqual([cred.id]);
    index.dispose();
  });

  it("applies status and type filters against the live store", () => {
    const { store, index } = setup();
    index.attach();
    const active = store.issueCredential("frank", "identity", "Gov", FUTURE, { kind: "passport" });
    const revoked = store.issueCredential("gina", "financial", "Bank", FUTURE, { kind: "passport" });
    store.revokeCredential(revoked.id);

    const onlyActive = index.search("passport", { status: "active" });
    if ("error" in onlyActive) throw new Error(onlyActive.error);
    expect(onlyActive.credentials.map((c) => c.id)).toEqual([active.id]);

    const onlyFinancial = index.search("passport", { type: "financial" });
    if ("error" in onlyFinancial) throw new Error(onlyFinancial.error);
    expect(onlyFinancial.credentials.map((c) => c.id)).toEqual([revoked.id]);
    index.dispose();
  });

  it("paginates results newest first", () => {
    const { store, index } = setup();
    index.attach();
    for (let i = 0; i < 5; i++) {
      store.issueCredential(`holder${i}`, "education", "Uni", FUTURE, { badge: "gold" });
    }
    const page = index.search("gold", { limit: 2, offset: 2 });
    if ("error" in page) throw new Error(page.error);
    expect(page.total).toBe(5);
    expect(page.credentials).toHaveLength(2);
    expect(page.limit).toBe(2);
    expect(page.offset).toBe(2);
    index.dispose();
  });

  it("rejects queries with no searchable terms or too many terms", () => {
    const { index } = setup();
    expect(index.search("  !!  ")).toEqual({ error: "q must contain at least one searchable term" });
    const many = Array.from({ length: 17 }, (_, i) => `t${i}`).join(" ");
    expect(index.search(many)).toEqual({ error: "q cannot contain more than 16 terms" });
  });

  it("returns no results for an unknown term without scanning", () => {
    const { store, index } = setup();
    index.attach();
    store.issueCredential("h", "education", "Uni", FUTURE, { course: "Rust" });
    const result = index.search("nonexistentterm");
    if ("error" in result) throw new Error(result.error);
    expect(result.total).toBe(0);
    index.dispose();
  });

  it("maintenance prunes postings for credentials removed from the source", () => {
    const { store, index } = setup();
    index.attach();
    const cred = store.issueCredential("ida", "education", "Uni", FUTURE, { unique: "zebra" });
    expect(index.stats().indexedCredentials).toBe(1);

    // Simulate the credential disappearing from the store without a removal event.
    const ghost = new CredentialStore();
    const ghostIndex = new MetadataSearchIndex(ghost);
    ghostIndex.indexCredential(cred);
    const report = ghostIndex.runMaintenance();
    expect(report.removedCredentials).toBe(1);
    // Removing a credential already drops terms left without postings, so the sweep finds none.
    expect(report.removedTokens).toBe(0);
    expect(ghostIndex.stats()).toMatchObject({ indexedCredentials: 0, distinctTokens: 0, postingEntries: 0 });
    expect(ghostIndex.stats().lastMaintenance).toMatchObject({ removedCredentials: 1 });
    index.dispose();
    ghostIndex.dispose();
  });

  it("reports performance statistics after queries", () => {
    const { store, index } = setup();
    index.attach();
    store.issueCredential("j", "education", "Uni", FUTURE, { course: "Rust" });
    index.search("rust");
    index.search("rust");
    const stats = index.stats();
    expect(stats.queryCount).toBe(2);
    expect(stats.avgLatencyMs).toBeGreaterThanOrEqual(0);
    expect(stats.p95LatencyMs).toBeGreaterThanOrEqual(0);
    expect(stats.lastRebuildAt).toBeTypeOf("number");
    index.dispose();
  });

  it("rejects non-positive maintenance intervals and stops its timer on request", () => {
    const { index } = setup();
    expect(() => index.startMaintenance(0)).toThrow(RangeError);
    const stop = index.startMaintenance(60_000);
    stop();
    index.dispose();
  });
});

describe("GET /api/v1/credentials/metadata/search", () => {
  const AUTH_SECRET = "test-secret-with-at-least-32-characters";
  const servers: Array<ReturnType<typeof createServer>> = [];

  afterEach(async () => {
    await Promise.all(
      servers.splice(0).map(
        (server) => new Promise<void>((resolve) => server.close(() => resolve()))
      )
    );
  });

  async function start(): Promise<{ baseUrl: string; token: string }> {
    const limiter = new LocalApiRateLimiter({ windowMs: 60_000, tiers: { free: 100, pro: 100, enterprise: 100 } });
    const server = createServer((req, res) => {
      handleApiV1Request(req, res, new URL(req.url ?? "/", "http://localhost"), {
        authSecret: AUTH_SECRET,
        apiRateLimiter: limiter,
      });
    });
    servers.push(server);
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address() as AddressInfo;
    return {
      baseUrl: `http://127.0.0.1:${address.port}`,
      token: issueToken(AUTH_SECRET, "search-user", 60).token,
    };
  }

  it("searches the live credential store and validates parameters", async () => {
    const issued = credentialStore.issueCredential("http-holder", "education", "HTTP Uni", FUTURE, {
      specialty: "zkproofs-http-test",
    });
    const server = await start();
    const headers = { authorization: `Bearer ${server.token}` };

    const ok = await fetch(`${server.baseUrl}/api/v1/credentials/metadata/search?q=zkproofs-http-test`, { headers });
    expect(ok.status).toBe(200);
    const body = (await ok.json()) as { credentials: Array<{ id: string }> };
    expect(body.credentials.map((c) => c.id)).toContain(issued.id);

    const missing = await fetch(`${server.baseUrl}/api/v1/credentials/metadata/search`, { headers });
    expect(missing.status).toBe(400);

    const badStatus = await fetch(`${server.baseUrl}/api/v1/credentials/metadata/search?q=x&status=nope`, { headers });
    expect(badStatus.status).toBe(400);

    const stats = await fetch(`${server.baseUrl}/api/v1/credentials/metadata/index/stats`, { headers });
    expect(stats.status).toBe(200);
    expect(await stats.json()).toMatchObject({ indexedCredentials: expect.any(Number) });

    const wrongMethod = await fetch(`${server.baseUrl}/api/v1/credentials/metadata/index/stats`, {
      method: "POST",
      headers,
    });
    expect(wrongMethod.status).toBe(405);
  });
});
