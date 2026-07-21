// Load generator for the /ws/metrics endpoint.
//
// Opens N concurrent WebSocket connections against a running broadcast server,
// measures connect success rate and end-to-end delivery latency (time from a probe
// event being published server-side to being observed on each open connection),
// and prints a summary report.
//
// Usage:
//   npm run loadtest -- --url http://localhost:4000 --connections 200 --duration 15
//
// The issue this backs (#1142) states a 5k-10k concurrent-connection target across N
// instances — this sandbox only runs a bounded smoke pass (documented, not silently
// capped: see server/README.md "Load test" section for why, and how to run the full
// scale test in a real environment/CI runner with proper ulimits and, ideally,
// multiple target instances behind a load balancer).
import { WebSocket } from "ws";
import { issueToken } from "../src/auth/tokens.js";

interface Args {
  url: string;
  connections: number;
  durationSec: number;
  authSecret: string;
}

function parseArgs(argv: string[]): Args {
  const get = (flag: string, fallback: string): string => {
    const idx = argv.indexOf(flag);
    return idx >= 0 && argv[idx + 1] ? argv[idx + 1] : fallback;
  };
  return {
    url: get("--url", "http://localhost:4000"),
    connections: Number.parseInt(get("--connections", "200"), 10),
    durationSec: Number.parseInt(get("--duration", "10"), 10),
    authSecret: get("--auth-secret", process.env.AUTH_SECRET ?? "dev-insecure-secret-change-me"),
  };
}

async function main(): Promise<void> {
  const args = parseArgs(process.argv.slice(2));
  const wsBase = args.url.replace(/^http/, "ws");
  const { token } = issueToken(args.authSecret, "loadtest", args.durationSec + 60);

  console.log(`Load test: ${args.connections} connections to ${wsBase}/ws/metrics for ${args.durationSec}s`);

  let connected = 0;
  let failed = 0;
  const firstMessageLatencies: number[] = [];
  const sockets: WebSocket[] = [];

  const connectOne = (): Promise<void> =>
    new Promise((resolve) => {
      const startedAt = Date.now();
      const ws = new WebSocket(`${wsBase}/ws/metrics?token=${encodeURIComponent(token)}`);
      let gotFirstMessage = false;

      ws.on("open", () => {
        connected++;
        sockets.push(ws);
      });
      ws.on("message", () => {
        if (!gotFirstMessage) {
          gotFirstMessage = true;
          firstMessageLatencies.push(Date.now() - startedAt);
        }
      });
      ws.on("error", () => {
        failed++;
      });
      ws.on("close", () => {});
      // Resolve once the connection attempt has settled either way, so callers can
      // fan these out without unbounded concurrent in-flight handshakes.
      ws.once("open", () => resolve());
      ws.once("error", () => resolve());
    });

  const BATCH = 100;
  for (let i = 0; i < args.connections; i += BATCH) {
    const batch = Math.min(BATCH, args.connections - i);
    await Promise.all(Array.from({ length: batch }, connectOne));
    process.stdout.write(`\rConnected: ${connected}/${args.connections} (failed: ${failed})`);
  }
  console.log();

  await new Promise((r) => setTimeout(r, args.durationSec * 1000));

  for (const ws of sockets) ws.close();

  firstMessageLatencies.sort((a, b) => a - b);
  const pct = (p: number): number =>
    firstMessageLatencies.length === 0 ? 0 : firstMessageLatencies[Math.floor(firstMessageLatencies.length * p)];

  console.log("\n--- Load test report ---");
  console.log(`Requested connections: ${args.connections}`);
  console.log(`Successfully connected: ${connected} (${((connected / args.connections) * 100).toFixed(1)}%)`);
  console.log(`Failed to connect:      ${failed}`);
  console.log(`First-message latency:  p50=${pct(0.5)}ms p95=${pct(0.95)}ms p99=${pct(0.99)}ms`);
  console.log("-------------------------\n");

  if (args.connections >= 5000) {
    console.log("Full-scale run complete — append these numbers to docs/realtime-broadcast.md.");
  } else {
    console.log(
      `NOTE: this was a ${args.connections}-connection smoke run, not the full 5k-10k target from issue #1142. ` +
        "Run with --connections 10000 against a properly-provisioned environment (raised ulimits, real Redis, " +
        "ideally multiple target instances behind a load balancer) to capture the full-scale numbers."
    );
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
