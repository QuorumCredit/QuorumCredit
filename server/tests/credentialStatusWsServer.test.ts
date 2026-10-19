import { describe, it, expect, afterEach } from "vitest";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { WebSocket } from "ws";
import { attachCredentialStatusWsServer } from "../src/ws/credentialStatusWsServer.js";

describe("credential status socket authentication", () => {
  let server: Server | undefined;

  afterEach(async () => {
    if (server) await new Promise<void>((resolve) => server!.close(() => resolve()));
    server = undefined;
  });

  it("refuses a connection from an unauthenticated client", async () => {
    server = createServer();
    attachCredentialStatusWsServer({ httpServer: server, authSecret: "x".repeat(32) });
    await new Promise<void>((resolve) => server!.listen(0, "127.0.0.1", () => resolve()));
    const { port } = server.address() as AddressInfo;

    const outcome = await new Promise<string>((resolve) => {
      const ws = new WebSocket(`ws://127.0.0.1:${port}/ws/credentials/status`);
      ws.on("open", () => {
        resolve("open");
        ws.close();
      });
      ws.on("error", () => resolve("refused"));
      ws.on("close", () => resolve("closed"));
    });
    expect(outcome).not.toBe("open");
  });
});
