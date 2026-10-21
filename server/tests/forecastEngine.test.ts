import { describe, it, expect } from "vitest";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { generateForecast, defaultTermsFor } from "../src/forecasting/forecastEngine.js";
import { handleLoanForecastRequest } from "../src/http/forecastRoutes.js";

describe("loan forecast", () => {
  it("returns the same forecast for the same loan history", () => {
    const terms = defaultTermsFor("loan-fixed");
    expect(generateForecast(terms)).toEqual(generateForecast(terms));
  });

  it("rejects a non-GET request to the forecast route", async () => {
    const server: Server = createServer((req, res) =>
      handleLoanForecastRequest(req, res, "loan-1", new URL(req.url ?? "/", "http://internal"))
    );
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", () => resolve()));
    try {
      const { port } = server.address() as AddressInfo;
      const res = await fetch(`http://127.0.0.1:${port}/loans/loan-1/forecast`, { method: "POST" });
      expect(res.status).toBe(405);
    } finally {
      await new Promise<void>((resolve) => server.close(() => resolve()));
    }
  });
});
