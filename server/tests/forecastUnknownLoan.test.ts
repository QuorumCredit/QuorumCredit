import { describe, it, expect } from "vitest";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { handleLoanForecastRequest } from "../src/http/forecastRoutes.js";

// Documents current behaviour: the route has no loan store, so an unknown loan gets
// default terms and a 200. Returning 404 needs a loan lookup, which is the next step.
describe("loan forecast for a loan that is not known", () => {
  it("falls back to default terms and reports no accuracy samples", async () => {
    const server: Server = createServer((req, res) =>
      handleLoanForecastRequest(req, res, "loan-unknown", new URL(req.url ?? "/", "http://internal"))
    );
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", () => resolve()));
    try {
      const { port } = server.address() as AddressInfo;
      const res = await fetch(`http://127.0.0.1:${port}/loans/loan-unknown/forecast`);
      expect(res.status).toBe(200);
      const body = (await res.json()) as { accuracy: { sampleCount: number } };
      expect(body.accuracy.sampleCount).toBe(0);
    } finally {
      await new Promise<void>((resolve) => server.close(() => resolve()));
    }
  });
});
