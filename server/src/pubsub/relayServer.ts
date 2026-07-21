import { createServer, type Socket, type Server } from "node:net";
import { createInterface } from "node:readline";

/**
 * Minimal newline-delimited-JSON pub/sub + lock relay. This is NOT Redis — it exists
 * solely to let the multi-instance integration test (test/multi-instance/) prove that
 * two separate OS processes talking to the same PubSubBus implementation correctly
 * fan out events to each other, in sandboxes where a real Redis server binary isn't
 * installable. Production deployments must set REDIS_URL and use RedisBus; see
 * server/README.md for how to re-run the same test against a real Redis instance.
 */

type ClientMsg =
  | { op: "sub"; channel: string }
  | { op: "unsub"; channel: string }
  | { op: "pub"; channel: string; message: string }
  | { op: "lock_acquire"; reqId: number; key: string; holder: string; ttlMs: number }
  | { op: "lock_release"; reqId: number; key: string; holder: string };

interface LockEntry {
  holder: string;
  expiresAt: number;
}

export interface RelayServerHandle {
  port: number;
  close(): Promise<void>;
}

export function startRelayServer(port = 0): Promise<RelayServerHandle> {
  return new Promise((resolve, reject) => {
    const subscriptions = new Map<Socket, Set<string>>();
    const locks = new Map<string, LockEntry>();

    const server: Server = createServer((socket) => {
      subscriptions.set(socket, new Set());
      const rl = createInterface({ input: socket });

      rl.on("line", (line) => {
        if (!line.trim()) return;
        let msg: ClientMsg;
        try {
          msg = JSON.parse(line);
        } catch {
          return;
        }

        switch (msg.op) {
          case "sub":
            subscriptions.get(socket)?.add(msg.channel);
            break;
          case "unsub":
            subscriptions.get(socket)?.delete(msg.channel);
            break;
          case "pub": {
            const payload = JSON.stringify({ op: "message", channel: msg.channel, message: msg.message }) + "\n";
            for (const [peer, channels] of subscriptions) {
              if (channels.has(msg.channel)) peer.write(payload);
            }
            break;
          }
          case "lock_acquire": {
            const now = Date.now();
            const existing = locks.get(msg.key);
            const free = !existing || existing.expiresAt < now || existing.holder === msg.holder;
            if (free) {
              locks.set(msg.key, { holder: msg.holder, expiresAt: now + msg.ttlMs });
            }
            socket.write(JSON.stringify({ op: "lock_result", reqId: msg.reqId, ok: free }) + "\n");
            break;
          }
          case "lock_release": {
            const existing = locks.get(msg.key);
            if (existing && existing.holder === msg.holder) locks.delete(msg.key);
            socket.write(JSON.stringify({ op: "lock_result", reqId: msg.reqId, ok: true }) + "\n");
            break;
          }
        }
      });

      socket.on("close", () => subscriptions.delete(socket));
      socket.on("error", () => subscriptions.delete(socket));
    });

    server.on("error", reject);
    server.listen(port, "127.0.0.1", () => {
      const addr = server.address();
      const boundPort = typeof addr === "object" && addr ? addr.port : port;
      resolve({
        port: boundPort,
        close: () =>
          new Promise<void>((res) => {
            for (const socket of subscriptions.keys()) socket.destroy();
            server.close(() => res());
          }),
      });
    });
  });
}
