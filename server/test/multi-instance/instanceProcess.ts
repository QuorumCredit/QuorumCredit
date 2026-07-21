// Runs as a forked child process representing one server instance for the
// multi-instance integration test. It only exercises the PubSubBus contract (the
// exact mechanism loanSocketServer.ts/metricsWsServer.ts rely on to fan out events
// across instances) — see test/multi-instance/multiInstance.test.ts for the full
// rationale and how this differs from a real Redis-backed deployment.
import { RelayBus } from "../../src/pubsub/RelayBus.js";
import { EVENTS_CHANNEL } from "../../src/types.js";

const port = Number(process.env.RELAY_PORT);
const bus = new RelayBus("127.0.0.1", port);

type ParentMessage = { type: "publish"; msg: string };

await bus.subscribe(EVENTS_CHANNEL, (msg) => {
  process.send?.({ type: "received", msg, receivedAt: Date.now() });
});

process.on("message", (cmd: ParentMessage) => {
  if (cmd?.type === "publish") {
    void bus.publish(EVENTS_CHANNEL, cmd.msg).then(() => {
      process.send?.({ type: "published" });
    });
  }
});

process.send?.({ type: "ready" });
