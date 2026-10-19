// Runs as a forked child process representing one server instance for the
// multi-instance integration test. It only exercises the PubSubBus contract (the
// exact mechanism loanSocketServer.ts/metricsWsServer.ts rely on to fan out events
// across instances) plus, when REDIS_URL is set, the Redis-backed WS aggregate rate
// limiter — see test/multi-instance/multiInstance.test.ts for the full rationale.
//
// Backend selection: REDIS_URL set → real RedisBus (what CI runs, see
// .github/workflows/server-multi-instance.yml); unset → RelayBus against the
// test-only relay server on RELAY_PORT (local runs without Redis).
import { Redis } from "ioredis";
import type { PubSubBus } from "../../src/pubsub/PubSubBus.js";
import { RedisBus } from "../../src/pubsub/RedisBus.js";
import { RelayBus } from "../../src/pubsub/RelayBus.js";
import { RedisWsAggregateRateLimiter } from "../../src/ws/wsAggregateRateLimiter.js";
import { EVENTS_CHANNEL } from "../../src/types.js";

const redisUrl = process.env.REDIS_URL;
const bus: PubSubBus = redisUrl
  ? new RedisBus(redisUrl)
  : new RelayBus("127.0.0.1", Number(process.env.RELAY_PORT));

// Only meaningful against a shared Redis; the relay has no INCR/PEXPIRE equivalent.
const limiter = redisUrl
  ? new RedisWsAggregateRateLimiter(new Redis(redisUrl), {
      maxMessages: Number(process.env.WS_AGG_MAX_MESSAGES ?? 300),
    })
  : undefined;

type ParentMessage =
  | { type: "publish"; msg: string }
  | { type: "lock"; reqId: number; key: string; holder: string; ttlMs: number }
  | { type: "unlock"; reqId: number; key: string; holder: string }
  | { type: "hit"; reqId: number; key: string }
  | { type: "isBlocked"; reqId: number; key: string };

await bus.subscribe(EVENTS_CHANNEL, (msg) => {
  process.send?.({ type: "received", msg, receivedAt: Date.now() });
});

process.on("message", (cmd: ParentMessage) => {
  switch (cmd?.type) {
    case "publish":
      void bus.publish(EVENTS_CHANNEL, cmd.msg).then(() => {
        process.send?.({ type: "published" });
      });
      break;
    case "lock":
      void bus.tryAcquireLock(cmd.key, cmd.ttlMs, cmd.holder).then((ok) => {
        process.send?.({ type: "result", reqId: cmd.reqId, value: ok });
      });
      break;
    case "unlock":
      void bus.releaseLock(cmd.key, cmd.holder).then(() => {
        process.send?.({ type: "result", reqId: cmd.reqId, value: true });
      });
      break;
    case "hit":
      void limiter?.recordHit(cmd.key).then((blocked) => {
        process.send?.({ type: "result", reqId: cmd.reqId, value: blocked });
      });
      break;
    case "isBlocked":
      void limiter?.isBlocked(cmd.key).then((blocked) => {
        process.send?.({ type: "result", reqId: cmd.reqId, value: blocked });
      });
      break;
  }
});

process.send?.({ type: "ready" });
