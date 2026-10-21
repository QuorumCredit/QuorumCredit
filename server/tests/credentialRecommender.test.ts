import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { afterEach, describe, expect, it } from "vitest";
import { issueToken } from "../src/auth/tokens.js";
import { LocalApiRateLimiter } from "../src/auth/apiRateLimiter.js";
import { handleApiV1Request } from "../src/http/apiV1Routes.js";
import { CredentialStore, credentialStore } from "../src/credentials/credentialStore.js";
import { CredentialRecommender, type RecommendationList } from "../src/recommendations/credentialRecommender.js";

const FUTURE = Math.floor(Date.now() / 1000) + 86_400;

function ids(list: RecommendationList): string[] {
  return list.recommendations.map((r) => r.itemId);
}

/**
 * Holders:
 *   h1: education@X, professional@W
 *   h2: education@X
 *   target: education@X   (already holds issuer:X and type:education)
 */
function seeded() {
  const store = new CredentialStore();
  store.issueCredential("h1", "education", "X", FUTURE);
  store.issueCredential("h1", "professional", "W", FUTURE);
  store.issueCredential("h2", "education", "X", FUTURE);
  store.issueCredential("target", "education", "X", FUTURE);
  return { store, recommender: new CredentialRecommender(store) };
}

describe("CredentialRecommender", () => {
  it("recommends items similar holders have, and never items the holder already has", () => {
    const { recommender } = seeded();
    const list = recommender.recommend("target");
    expect(ids(list)).toEqual(expect.arrayContaining(["issuer:W", "type:professional"]));
    expect(ids(list)).not.toContain("issuer:X");
    expect(ids(list)).not.toContain("type:education");
    expect(list.recommendations[0]).toMatchObject({ reason: "similar_holders", personalized: false });
  });

  it("falls back to popularity for a holder with no credentials", () => {
    const { recommender } = seeded();
    const list = recommender.recommend("newcomer");
    expect(list.recommendations[0]).toMatchObject({ itemId: "issuer:X", reason: "popular" });
    expect(list.recommendations[0]!.score).toBe(3);
  });

  it("ignores revoked credentials when building holder profiles", () => {
    const store = new CredentialStore();
    const revoked = store.issueCredential("h9", "identity", "Gov", FUTURE);
    store.revokeCredential(revoked.id);
    store.issueCredential("h8", "identity", "Gov", FUTURE);
    const list = new CredentialRecommender(store).recommend("h7");
    // Only the active holder's credential counts, so the popularity of issuer:Gov is 1, not 2.
    expect(list.recommendations[0]!.score).toBe(1);
  });

  it("clamps the limit", () => {
    const { recommender } = seeded();
    expect(recommender.recommend("target", 0).recommendations.length).toBeLessThanOrEqual(1);
    expect(recommender.recommend("target", 10_000).recommendations.length).toBeLessThanOrEqual(50);
  });

  it("removes items the holder dismissed", () => {
    const { recommender } = seeded();
    const first = recommender.recommend("target");
    expect(ids(first)).toContain("issuer:W");
    expect(recommender.recordFeedback("target", "issuer:W", "dismissed")).toMatchObject({ outcome: "dismissed" });
    expect(ids(recommender.recommend("target"))).not.toContain("issuer:W");
  });

  it("boosts candidates of a kind the holder accepted", () => {
    const { recommender } = seeded();
    const before = recommender.recommend("target").recommendations.find((r) => r.itemId === "type:professional")!;
    recommender.recordFeedback("target", "type:professional", "accepted");
    const after = recommender.recommend("target").recommendations.find((r) => r.itemId === "type:professional")!;
    expect(after.personalized).toBe(true);
    expect(after.score).toBeCloseTo(before.score * 1.25, 4);
  });

  it("only accepts feedback for items actually shown, once", () => {
    const { recommender } = seeded();
    expect(recommender.recordFeedback("target", "issuer:W", "accepted")).toEqual({
      error: "feedback is only accepted for items recommended to this holder",
      status: 400,
    });
    recommender.recommend("target");
    expect(recommender.recordFeedback("target", "issuer:W", "accepted")).toMatchObject({ outcome: "accepted" });
    expect(recommender.recordFeedback("target", "issuer:W", "dismissed")).toEqual({
      error: "feedback already recorded as accepted",
      status: 409,
    });
  });

  it("measures effectiveness from unique impressions and accepted feedback", () => {
    const { recommender } = seeded();
    expect(recommender.effectiveness()).toEqual({ impressions: 0, accepted: 0, dismissed: 0, acceptanceRate: 0 });

    recommender.recommend("target");
    recommender.recommend("target");
    recommender.recordFeedback("target", "issuer:W", "accepted");
    recommender.recordFeedback("target", "type:professional", "dismissed");

    expect(recommender.effectiveness()).toEqual({
      impressions: 2,
      accepted: 1,
      dismissed: 1,
      acceptanceRate: 0.5,
    });
  });
});

describe("credential recommendation HTTP routes", () => {
  const AUTH_SECRET = "test-secret-with-at-least-32-characters";
  const servers: Array<ReturnType<typeof createServer>> = [];

  afterEach(async () => {
    await Promise.all(
      servers.splice(0).map((server) => new Promise<void>((resolve) => server.close(() => resolve())))
    );
  });

  async function start() {
    const limiter = new LocalApiRateLimiter({ windowMs: 60_000, tiers: { free: 500, pro: 500, enterprise: 500 } });
    const server = createServer((req, res) => {
      handleApiV1Request(req, res, new URL(req.url ?? "/", "http://localhost"), {
        authSecret: AUTH_SECRET,
        apiRateLimiter: limiter,
      });
    });
    servers.push(server);
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address() as AddressInfo;
    const auth = (sub: string) => ({ authorization: `Bearer ${issueToken(AUTH_SECRET, sub, 60).token}` });
    return { baseUrl: `http://127.0.0.1:${address.port}`, auth };
  }

  it("serves recommendations for the authenticated holder and records feedback", async () => {
    credentialStore.issueCredential("rec-http-peer-1", "education", "RecHttpUni", FUTURE);
    credentialStore.issueCredential("rec-http-peer-1", "financial", "RecHttpBank", FUTURE);
    credentialStore.issueCredential("rec-http-peer-2", "education", "RecHttpUni", FUTURE);
    credentialStore.issueCredential("rec-http-holder", "education", "RecHttpUni", FUTURE);
    const { baseUrl, auth } = await start();

    const list = await fetch(`${baseUrl}/api/v1/credentials/recommendations?limit=5`, {
      headers: auth("rec-http-holder"),
    });
    expect(list.status).toBe(200);
    const body = (await list.json()) as RecommendationList;
    expect(body.holderId).toBe("rec-http-holder");
    expect(body.recommendations.map((r) => r.itemId)).toContain("issuer:RecHttpBank");

    const feedback = (itemId: string, outcome: string) =>
      fetch(`${baseUrl}/api/v1/credentials/recommendations/feedback`, {
        method: "POST",
        headers: { ...auth("rec-http-holder"), "content-type": "application/json" },
        body: JSON.stringify({ itemId, outcome }),
      });
    expect((await feedback("issuer:NeverShown", "accepted")).status).toBe(400);
    expect((await feedback("issuer:RecHttpBank", "accepted")).status).toBe(201);
    expect((await feedback("issuer:RecHttpBank", "accepted")).status).toBe(409);
    expect((await feedback("issuer:RecHttpBank", "maybe")).status).toBe(400);

    const effectiveness = await fetch(`${baseUrl}/api/v1/credentials/recommendations/effectiveness`, {
      headers: auth("rec-http-holder"),
    });
    expect(effectiveness.status).toBe(200);
    expect(await effectiveness.json()).toMatchObject({ accepted: expect.any(Number) });

    const wrong = await fetch(`${baseUrl}/api/v1/credentials/recommendations`, {
      method: "POST",
      headers: auth("rec-http-holder"),
    });
    expect(wrong.status).toBe(405);
  });
});
