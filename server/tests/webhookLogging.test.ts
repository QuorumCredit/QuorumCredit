import { describe, it, expect, afterEach, vi } from "vitest";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { handleWebhookRequest } from "../src/http/webhookRoutes.js";
import { signWebhookPayload } from "../src/webhooks/signature.js";

const SECRET = "test-webhook-secret";

describe("incoming webhook logging", () => {
  let server: Server | undefined;

  afterEach(async () => {
    vi.restoreAllMocks();
    if (server) await new Promise<void>((resolve) => server!.close(() => resolve()));
    server = undefined;
  });

  it("logs the event type and ID but not the payload body", async () => {
    const logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
    server = createServer((req, res) => handleWebhookRequest(req, res, { webhookSecret: SECRET }));
    await new Promise<void>((resolve) => server!.listen(0, "127.0.0.1", () => resolve()));
    const { port } = server.address() as AddressInfo;

    const payload = {
      event: "loan.repaid",
      webhookId: "wh_test_1",
      timestamp: 1700000000,
      data: { borrower: "GSECRETBORROWER" },
    };
    await fetch(`http://127.0.0.1:${port}/webhook`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "x-webhook-signature": signWebhookPayload(payload as never, SECRET),
      },
      body: JSON.stringify(payload),
    });

    const logged = JSON.stringify(logSpy.mock.calls);
    expect(logged).toContain("wh_test_1");
    expect(logged).not.toContain("GSECRETBORROWER");
  });
});
