import { describe, it, expect, afterEach } from "vitest";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { attachLoanSocketServer } from "../src/ws/loanSocketServer.js";

describe("loan socket CORS origins", () => {
  let server: Server | undefined;

  afterEach(async () => {
    if (server) await new Promise<void>((resolve) => server!.close(() => resolve()));
    server = undefined;
  });

  async function start(allowedOrigins: string[]): Promise<string> {
    server = createServer();
    attachLoanSocketServer({
      httpServer: server,
      bus: {} as never,
      store: {} as never,
      authSecret: "x".repeat(32),
      connectionQueueMax: 10,
      allowedOrigins,
    });
    await new Promise<void>((resolve) => server!.listen(0, "127.0.0.1", () => resolve()));
    const { port } = server.address() as AddressInfo;
    return `http://127.0.0.1:${port}`;
  }

  it("refuses a cross-origin request from an origin that is not listed", async () => {
    const base = await start(["https://app.example.com"]);
    const res = await fetch(`${base}/socket.io/?EIO=4&transport=polling`, {
      headers: { origin: "https://evil.example" },
    });
    expect(res.headers.get("access-control-allow-origin")).toBeNull();
  });

  it("grants CORS to a listed origin", async () => {
    const base = await start(["https://app.example.com"]);
    const res = await fetch(`${base}/socket.io/?EIO=4&transport=polling`, {
      headers: { origin: "https://app.example.com" },
    });
    expect(res.headers.get("access-control-allow-origin")).toBe("https://app.example.com");
  });
});
