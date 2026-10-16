import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { afterEach, describe, expect, it } from "vitest";
import { issueToken } from "../src/auth/tokens.js";
import { LocalApiRateLimiter } from "../src/auth/apiRateLimiter.js";
import { handleApiV1Request } from "../src/http/apiV1Routes.js";
import {
  OnboardingError,
  OnboardingStore,
  ONBOARDING_STEPS,
  progressOf,
} from "../src/onboarding/onboardingStore.js";

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

function expectCode(fn: () => unknown, code: string): void {
  try {
    fn();
  } catch (error) {
    expect(error).toBeInstanceOf(OnboardingError);
    expect((error as OnboardingError).code).toBe(code);
    return;
  }
  throw new Error(`expected OnboardingError with code ${code}`);
}

describe("OnboardingStore", () => {
  it("starts one in-progress flow per holder and reuses it", () => {
    const store = new OnboardingStore();
    const first = store.start("holder-1");
    const second = store.start("holder-1");
    expect(second.id).toBe(first.id);
    expect(first.status).toBe("in_progress");
    expect(store.start("holder-2").id).not.toBe(first.id);
  });

  it("only exposes a flow to its holder", () => {
    const store = new OnboardingStore();
    const flow = store.start("holder-1");
    expect(store.getFlow(flow.id, "holder-1")?.id).toBe(flow.id);
    expect(store.getFlow(flow.id, "someone-else")).toBeUndefined();
    expect(store.completeStep(flow.id, "someone-else", "profile")).toBeUndefined();
  });

  it("enforces the step order and rejects unknown steps", () => {
    const store = new OnboardingStore();
    const flow = store.start("holder-1");

    expectCode(() => store.completeStep(flow.id, "holder-1", "identity"), "step_out_of_order");
    expectCode(() => store.completeStep(flow.id, "holder-1", "bogus"), "unknown_step");

    store.completeStep(flow.id, "holder-1", "profile");
    // Re-completing a finished step is a harmless no-op.
    expect(store.completeStep(flow.id, "holder-1", "profile")?.completedSteps).toEqual([
      "profile",
    ]);
  });

  it("refuses to complete until every step is done", () => {
    const store = new OnboardingStore();
    const flow = store.start("holder-1");
    store.completeStep(flow.id, "holder-1", "profile");
    store.completeStep(flow.id, "holder-1", "identity");

    let caught: unknown;
    try {
      store.complete(flow.id, "holder-1");
    } catch (error) {
      caught = error;
    }
    expect(caught).toBeInstanceOf(OnboardingError);
    expect((caught as OnboardingError).missingSteps).toEqual(["documents", "consent"]);
  });

  it("completes once all steps are done and blocks further step changes", () => {
    const store = new OnboardingStore();
    const flow = store.start("holder-1");
    for (const step of ONBOARDING_STEPS) store.completeStep(flow.id, "holder-1", step);

    const completed = store.complete(flow.id, "holder-1", 1_000);
    expect(completed).toMatchObject({ status: "completed", completedAt: 1_000 });
    expectCode(() => store.completeStep(flow.id, "holder-1", "profile"), "flow_completed");
    // Completing again is idempotent.
    expect(store.complete(flow.id, "holder-1")?.status).toBe("completed");
    // A new onboarding can start once the previous one is finished.
    expect(store.start("holder-1").id).not.toBe(flow.id);
  });
});

describe("progressOf", () => {
  it("reports completed count, percent and the next step", () => {
    const store = new OnboardingStore();
    const flow = store.start("holder-1");
    store.completeStep(flow.id, "holder-1", "profile");
    const progress = progressOf(store.getFlow(flow.id, "holder-1")!);
    expect(progress).toMatchObject({
      completed: 1,
      total: ONBOARDING_STEPS.length,
      percent: 25,
      nextStep: "identity",
    });
    expect(progress.steps[0]).toEqual({ step: "profile", completed: true });
  });
});

async function startServer(): Promise<string> {
  const server = createServer((req, res) => {
    handleApiV1Request(req, res, new URL(req.url ?? "/", "http://localhost"), {
      authSecret: AUTH_SECRET,
      apiRateLimiter: new LocalApiRateLimiter({
        windowMs: 60_000,
        tiers: { free: 1000, pro: 1000, enterprise: 1000 },
      }),
    });
  });
  servers.push(server);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  return `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
}

describe("onboarding API routes", () => {
  it("walks a holder through the flow via HTTP", async () => {
    const baseUrl = await startServer();
    const holder = `holder-http-${Date.now()}`;
    const headers = {
      authorization: `Bearer ${issueToken(AUTH_SECRET, holder, 60).token}`,
    };

    const started = await fetch(`${baseUrl}/api/v1/onboarding/start`, {
      method: "POST",
      headers,
    });
    expect(started.status).toBe(200);
    const flow = (await started.json()) as { id: string; progress: { nextStep: string } };
    expect(flow.progress.nextStep).toBe("profile");

    const outOfOrder = await fetch(
      `${baseUrl}/api/v1/onboarding/${flow.id}/steps/identity`,
      { method: "POST", headers }
    );
    expect(outOfOrder.status).toBe(409);
    expect(await outOfOrder.json()).toMatchObject({ code: "step_out_of_order" });

    for (const step of ONBOARDING_STEPS) {
      const res = await fetch(`${baseUrl}/api/v1/onboarding/${flow.id}/steps/${step}`, {
        method: "POST",
        headers,
      });
      expect(res.status).toBe(200);
    }

    const completed = await fetch(`${baseUrl}/api/v1/onboarding/${flow.id}/complete`, {
      method: "POST",
      headers,
    });
    expect(completed.status).toBe(200);
    expect(await completed.json()).toMatchObject({
      status: "completed",
      progress: { percent: 100, nextStep: null },
    });
  });

  it("returns 409 with missing steps when completing early", async () => {
    const baseUrl = await startServer();
    const headers = {
      authorization: `Bearer ${issueToken(AUTH_SECRET, `early-${Date.now()}`, 60).token}`,
    };
    const flow = (await (
      await fetch(`${baseUrl}/api/v1/onboarding/start`, { method: "POST", headers })
    ).json()) as { id: string };

    const res = await fetch(`${baseUrl}/api/v1/onboarding/${flow.id}/complete`, {
      method: "POST",
      headers,
    });
    expect(res.status).toBe(409);
    expect(await res.json()).toMatchObject({
      code: "steps_incomplete",
      missingSteps: [...ONBOARDING_STEPS],
    });
  });

  it("hides another holder's flow and rejects unknown steps and wrong methods", async () => {
    const baseUrl = await startServer();
    const owner = `owner-${Date.now()}`;
    const ownerHeaders = {
      authorization: `Bearer ${issueToken(AUTH_SECRET, owner, 60).token}`,
    };
    const otherHeaders = {
      authorization: `Bearer ${issueToken(AUTH_SECRET, `other-${Date.now()}`, 60).token}`,
    };
    const flow = (await (
      await fetch(`${baseUrl}/api/v1/onboarding/start`, {
        method: "POST",
        headers: ownerHeaders,
      })
    ).json()) as { id: string };

    const hidden = await fetch(`${baseUrl}/api/v1/onboarding/${flow.id}`, {
      headers: otherHeaders,
    });
    expect(hidden.status).toBe(404);

    const unknown = await fetch(`${baseUrl}/api/v1/onboarding/${flow.id}/steps/nope`, {
      method: "POST",
      headers: ownerHeaders,
    });
    expect(unknown.status).toBe(400);

    const wrongMethod = await fetch(`${baseUrl}/api/v1/onboarding/start`, {
      headers: ownerHeaders,
    });
    expect(wrongMethod.status).toBe(405);
  });
});
