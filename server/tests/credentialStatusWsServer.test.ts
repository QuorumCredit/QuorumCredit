import { describe, it, expect, afterEach } from "vitest";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { WebSocket } from "ws";
import { attachCredentialStatusWsServer } from "../src/ws/credentialStatusWsServer.js";
import { issueToken } from "../src/auth/tokens.js";

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

describe("credential status socket subscriptions", () => {
  const SECRET = "x".repeat(32);
  let server: Server | undefined;

  afterEach(async () => {
    if (server) await new Promise<void>((resolve) => server!.close(() => resolve()));
    server = undefined;
  });

  it("confirms a subscription from an authenticated client", async () => {
    server = createServer();
    attachCredentialStatusWsServer({ httpServer: server, authSecret: SECRET });
    await new Promise<void>((resolve) => server!.listen(0, "127.0.0.1", () => resolve()));
    const { port } = server.address() as AddressInfo;
    const { token } = issueToken(SECRET, "holder-subscriber", 300);

    const reply = await new Promise<{ type: string; credentialIds?: string[] }>((resolve, reject) => {
      const ws = new WebSocket(`ws://127.0.0.1:${port}/ws/credentials/status?token=${token}`);
      ws.on("open", () => ws.send(JSON.stringify({ type: "subscribe", credentialIds: ["cred-1"] })));
      ws.on("message", (data) => {
        const frame = JSON.parse(data.toString());
        if (frame.type === "subscribed") {
          resolve(frame);
          ws.close();
        }
      });
      ws.on("error", reject);
    });
    expect(reply.credentialIds).toContain("cred-1");
  });
});
