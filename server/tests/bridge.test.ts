import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import Database from "better-sqlite3";
import { EventStore } from "../src/bridge/eventStore.js";
import { Bridge } from "../src/bridge/bridge.js";
import { LocalBus } from "../src/pubsub/LocalBus.js";
import { EVENTS_CHANNEL, type BroadcastEvent } from "../src/types.js";

/** Builds a throwaway SQLite file with the same `events` table shape the Rust
 * indexer writes (tools/indexer/src/db.rs::run_migrations), so EventStore/Bridge
 * are exercised against a realistic fixture without depending on the indexer binary. */
function makeIndexerFixture(): {
  path: string;
  dir: string;
  insert: (row: Partial<EventFixtureRow>) => void;
  close: () => void;
} {
  const dir = mkdtempSync(join(tmpdir(), "qc-broadcast-test-"));
  const path = join(dir, "indexer.db");
  const db = new Database(path);
  db.exec(`
    CREATE TABLE events (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      ledger INTEGER NOT NULL,
      ledger_closed_at TEXT NOT NULL,
      tx_hash TEXT NOT NULL,
      contract_id TEXT NOT NULL,
      category TEXT NOT NULL,
      action TEXT NOT NULL,
      value_json TEXT NOT NULL
    );
  `);
  const stmt = db.prepare(
    `INSERT INTO events (ledger, ledger_closed_at, tx_hash, contract_id, category, action, value_json)
     VALUES (@ledger, @ledger_closed_at, @tx_hash, @contract_id, @category, @action, @value_json)`
  );
  const insert = (row: Partial<EventFixtureRow>): void => {
    stmt.run({
      ledger: 1,
      ledger_closed_at: new Date().toISOString(),
      tx_hash: "tx",
      contract_id: "C...",
      category: "loan",
      action: "request",
      value_json: JSON.stringify({ borrower: "B1", amount_stroops: 1000 }),
      ...row,
    });
  };
  return { path, dir, insert, close: () => db.close() };
}

interface EventFixtureRow {
  ledger: number;
  ledger_closed_at: string;
  tx_hash: string;
  contract_id: string;
  category: string;
  action: string;
  value_json: string;
}

const cleanupDirs: string[] = [];
const cleanupDbs: Array<() => void> = [];
afterEach(() => {
  while (cleanupDbs.length) cleanupDbs.pop()!();
  while (cleanupDirs.length) rmSync(cleanupDirs.pop()!, { recursive: true, force: true });
});

describe("EventStore", () => {
  it("returns only rows with id greater than the cursor, in order", () => {
    const fixture = makeIndexerFixture();
    cleanupDirs.push(fixture.dir);
    cleanupDbs.push(fixture.close);
    fixture.insert({});
    fixture.insert({});
    fixture.insert({});

    const store = new EventStore(fixture.path);
    expect(store.getEventsSince(0)).toHaveLength(3);
    expect(store.getEventsSince(1).map((e) => e.id)).toEqual([2, 3]);
    expect(store.getEventsSince(3)).toHaveLength(0);
    store.close();
  });

  it("decodes value_json into an object", () => {
    const fixture = makeIndexerFixture();
    cleanupDirs.push(fixture.dir);
    cleanupDbs.push(fixture.close);
    fixture.insert({ value_json: JSON.stringify({ borrower: "B1", amount_stroops: 42 }) });

    const store = new EventStore(fixture.path);
    const [row] = store.getEventsSince(0);
    expect(row.value).toEqual({ borrower: "B1", amount_stroops: 42 });
    store.close();
  });
});

describe("Bridge", () => {
  it("publishes new rows onto the bus exactly once when it holds the leader lock", async () => {
    const fixture = makeIndexerFixture();
    cleanupDirs.push(fixture.dir);
    cleanupDbs.push(fixture.close);
    fixture.insert({ category: "loan", action: "request", value_json: JSON.stringify({ borrower: "B1", amount_stroops: 100 }) });

    const store = new EventStore(fixture.path);
    const bus = new LocalBus();
    const received: BroadcastEvent[] = [];
    await bus.subscribe(EVENTS_CHANNEL, (msg) => received.push(JSON.parse(msg)));

    const bridge = new Bridge({
      bus,
      store,
      instanceId: "leader-1",
      pollIntervalMs: 20,
      leaderLockTtlMs: 5000,
    });
    bridge.start();

    await waitFor(() => received.length === 1);
    expect(received[0].event.category).toBe("loan");
    expect(received[0].metrics.total_loans).toBe(1);

    await bridge.stop();
    store.close();
  });

  it("does not publish from a non-leader instance while another holds the lock", async () => {
    const fixture = makeIndexerFixture();
    cleanupDirs.push(fixture.dir);
    cleanupDbs.push(fixture.close);
    fixture.insert({});

    const store = new EventStore(fixture.path);
    const bus = new LocalBus();
    // Simulate another instance already holding the lock.
    expect(await bus.tryAcquireLock("qc:bridge:leader", 60_000, "other-instance")).toBe(true);

    const received: string[] = [];
    await bus.subscribe(EVENTS_CHANNEL, (msg) => received.push(msg));

    const bridge = new Bridge({ bus, store, instanceId: "follower-1", pollIntervalMs: 20, leaderLockTtlMs: 5000 });
    bridge.start();

    await new Promise((r) => setTimeout(r, 100));
    expect(received).toHaveLength(0);

    await bridge.stop();
    store.close();
  });
});

async function waitFor(predicate: () => boolean, timeoutMs = 2000): Promise<void> {
  const start = Date.now();
  while (!predicate()) {
    if (Date.now() - start > timeoutMs) throw new Error("waitFor timed out");
    await new Promise((r) => setTimeout(r, 10));
  }
}
