import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { afterEach, describe, expect, it } from "vitest";
import { issueToken } from "../src/auth/tokens.js";
import { LocalApiRateLimiter } from "../src/auth/apiRateLimiter.js";
import { handleApiV1Request } from "../src/http/apiV1Routes.js";
import { credentialStore } from "../src/credentials/credentialStore.js";
import {
  AttestationLedger,
  DisputeService,
  isDisputeError,
  type Dispute,
} from "../src/disputes/attestorDisputes.js";

const FUTURE = Math.floor(Date.now() / 1000) + 86_400;
const REASON = "Attestor A verified the document but B found it forged.";

function setup() {
  const ledger = new AttestationLedger();
  const service = new DisputeService(ledger);
  return { ledger, service };
}

function expectDispute(result: Dispute | { error: string; status: number }): Dispute {
  if (isDisputeError(result)) throw new Error(`${result.status}: ${result.error}`);
  return result;
}

describe("AttestationLedger", () => {
  it("allows one verdict per attestor per credential", () => {
    const { ledger } = setup();
    expect(isDisputeError(ledger.record("c1", "a1", "verified"))).toBe(false);
    expect(ledger.record("c1", "a1", "rejected")).toEqual({
      error: "attestor has already recorded a verdict for this credential",
      status: 409,
    });
  });

  it("detects only opposing-verdict pairs", () => {
    const { ledger } = setup();
    ledger.record("c1", "a1", "verified");
    ledger.record("c1", "a2", "rejected");
    ledger.record("c1", "a3", "verified");
    const conflicts = ledger.detectConflicts("c1");
    expect(conflicts.map((c) => [c.attestorA, c.attestorB])).toEqual([
      ["a1", "a2"],
      ["a2", "a3"],
    ]);
    expect(ledger.detectConflicts("c1", ["a1", "a3"])).toEqual([]);
    expect(ledger.detectConflicts("unknown")).toEqual([]);
  });
});

describe("DisputeService", () => {
  it("opens a dispute only against a detected conflict", () => {
    const { ledger, service } = setup();
    ledger.record("c1", "a1", "verified");
    ledger.record("c1", "a2", "verified");
    expect(service.open({ credentialId: "c1", raisedBy: "h1", attestors: ["a1", "a2"], reason: REASON })).toEqual({
      error: "no attestor conflict detected for these attestors",
      status: 409,
    });

    ledger.record("c1", "a3", "rejected");
    const dispute = expectDispute(
      service.open({ credentialId: "c1", raisedBy: "h1", attestors: ["a1", "a3"], reason: REASON })
    );
    expect(dispute).toMatchObject({ status: "open", credentialId: "c1", raisedBy: "h1" });
    expect(dispute.conflicts).toHaveLength(1);
    expect(dispute.history).toEqual([expect.objectContaining({ type: "opened", actor: "h1" })]);
  });

  it("validates reason length and attestor count", () => {
    const { ledger, service } = setup();
    ledger.record("c1", "a1", "verified");
    ledger.record("c1", "a2", "rejected");
    expect(isDisputeError(service.open({ credentialId: "c1", raisedBy: "h", attestors: ["a1", "a2"], reason: "short" }))).toBe(true);
    expect(isDisputeError(service.open({ credentialId: "c1", raisedBy: "h", attestors: ["a1", "a1"], reason: REASON }))).toBe(true);
  });

  it("allows only one open dispute per credential", () => {
    const { ledger, service } = setup();
    ledger.record("c1", "a1", "verified");
    ledger.record("c1", "a2", "rejected");
    expectDispute(service.open({ credentialId: "c1", raisedBy: "h", attestors: ["a1", "a2"], reason: REASON }));
    const second = service.open({ credentialId: "c1", raisedBy: "h", attestors: ["a1", "a2"], reason: REASON });
    expect(second).toEqual({ error: "an open dispute already exists for this credential", status: 409 });
  });

  it("resolves once, records history, and blocks parties from resolving", () => {
    const { ledger, service } = setup();
    ledger.record("c1", "a1", "verified");
    ledger.record("c1", "a2", "rejected");
    const dispute = expectDispute(
      service.open({ credentialId: "c1", raisedBy: "h", attestors: ["a1", "a2"], reason: REASON })
    );

    expect(service.resolve(dispute.id, { resolver: "a1", outcome: "upheld" })).toEqual({
      error: "a party to the dispute cannot resolve it",
      status: 403,
    });
    expect(service.resolve(dispute.id, { resolver: "h", outcome: "upheld" })).toMatchObject({ status: 403 });
    expect(service.resolve("missing", { resolver: "arbiter", outcome: "upheld" })).toMatchObject({ status: 404 });
    expect(service.resolve(dispute.id, { resolver: "arbiter", outcome: "maybe" as "upheld" })).toMatchObject({
      status: 400,
    });

    const resolved = expectDispute(
      service.resolve(dispute.id, { resolver: "arbiter", outcome: "dismissed", note: "evidence insufficient" })
    );
    expect(resolved).toMatchObject({ status: "resolved", outcome: "dismissed", resolvedBy: "arbiter" });
    expect(resolved.history.map((e) => e.type)).toEqual(["opened", "resolved"]);
    expect(resolved.history[1]).toMatchObject({ actor: "arbiter", note: "evidence insufficient" });

    expect(service.resolve(dispute.id, { resolver: "arbiter2", outcome: "upheld" })).toMatchObject({ status: 409 });
    expect(service.list({ status: "open" })).toEqual([]);
    expect(service.list({ credentialId: "c1" })).toHaveLength(1);
  });
});

describe("dispute HTTP workflow", () => {
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
    const tokenFor = (sub: string) => `Bearer ${issueToken(AUTH_SECRET, sub, 60).token}`;
    return { baseUrl: `http://127.0.0.1:${address.port}`, tokenFor };
  }

  it("runs attest → conflict → dispute → resolve end to end", async () => {
    const credential = credentialStore.issueCredential("http-dispute-holder", "professional", "Board", FUTURE, {});
    const { baseUrl, tokenFor } = await start();
    const post = (path: string, sub: string, body: unknown) =>
      fetch(`${baseUrl}${path}`, {
        method: "POST",
        headers: { authorization: tokenFor(sub), "content-type": "application/json" },
        body: JSON.stringify(body),
      });

    const verdictA = await post(`/api/v1/credentials/${credential.id}/attestations`, "http-att-a", { verdict: "verified" });
    expect(verdictA.status).toBe(201);
    const verdictB = await post(`/api/v1/credentials/${credential.id}/attestations`, "http-att-b", { verdict: "rejected" });
    expect(verdictB.status).toBe(201);
    const duplicate = await post(`/api/v1/credentials/${credential.id}/attestations`, "http-att-b", { verdict: "verified" });
    expect(duplicate.status).toBe(409);

    const missingCredential = await post("/api/v1/credentials/no-such-cred/attestations", "http-att-a", { verdict: "verified" });
    expect(missingCredential.status).toBe(404);

    const opened = await post("/api/v1/disputes", "http-holder-2", {
      credentialId: credential.id,
      attestors: ["http-att-a", "http-att-b"],
      reason: REASON,
    });
    expect(opened.status).toBe(201);
    const dispute = (await opened.json()) as Dispute;

    const selfResolve = await post(`/api/v1/disputes/${dispute.id}/resolve`, "http-att-a", { outcome: "upheld" });
    expect(selfResolve.status).toBe(403);

    const resolved = await post(`/api/v1/disputes/${dispute.id}/resolve`, "http-arbiter", {
      outcome: "upheld",
      note: "B's rejection is supported by the issuer record",
    });
    expect(resolved.status).toBe(200);
    expect(await resolved.json()).toMatchObject({ status: "resolved", outcome: "upheld" });

    const fetched = await fetch(`${baseUrl}/api/v1/disputes/${dispute.id}`, {
      headers: { authorization: tokenFor("http-arbiter") },
    });
    expect(fetched.status).toBe(200);
    const listed = await fetch(`${baseUrl}/api/v1/disputes?credentialId=${credential.id}&status=resolved`, {
      headers: { authorization: tokenFor("http-arbiter") },
    });
    expect((await listed.json()) as { disputes: Dispute[] }).toMatchObject({
      disputes: [expect.objectContaining({ id: dispute.id })],
    });

    const badStatus = await fetch(`${baseUrl}/api/v1/disputes?status=weird`, {
      headers: { authorization: tokenFor("http-arbiter") },
    });
    expect(badStatus.status).toBe(400);
  });

  it("rejects malformed dispute bodies", async () => {
    const { baseUrl, tokenFor } = await start();
    const notObject = await fetch(`${baseUrl}/api/v1/disputes`, {
      method: "POST",
      headers: { authorization: tokenFor("http-malformed"), "content-type": "application/json" },
      body: "[1,2]",
    });
    expect(notObject.status).toBe(400);

    const badAttestors = await fetch(`${baseUrl}/api/v1/disputes`, {
      method: "POST",
      headers: { authorization: tokenFor("http-malformed"), "content-type": "application/json" },
      body: JSON.stringify({ credentialId: "c", attestors: "a,b", reason: REASON }),
    });
    expect(badAttestors.status).toBe(400);

    const wrongMethod = await fetch(`${baseUrl}/api/v1/disputes/anything`, {
      method: "DELETE",
      headers: { authorization: tokenFor("http-malformed") },
    });
    expect(wrongMethod.status).toBe(405);
  });
});
