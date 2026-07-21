import Database from "better-sqlite3";
import type { IndexedEvent } from "../types.js";

interface EventRow {
  id: number;
  ledger: number;
  ledger_closed_at: string;
  tx_hash: string;
  contract_id: string;
  category: string;
  action: string;
  value_json: string;
}

/**
 * Read-only view over the `events` table written by tools/indexer/src/db.rs. This
 * mirrors Store::get_events_since there — the indexer owns writes; this service only
 * ever reads, so there's no risk of write contention with the Rust indexer process.
 */
export class EventStore {
  private readonly db: Database.Database;
  private readonly stmt: Database.Statement<[number]>;

  constructor(dbPath: string) {
    this.db = new Database(dbPath, { readonly: true, fileMustExist: true });
    this.stmt = this.db.prepare(
      `SELECT id, ledger, ledger_closed_at, tx_hash, contract_id, category, action, value_json
       FROM events WHERE id > ? ORDER BY id ASC`
    );
  }

  /** Rows with id strictly greater than `sinceId`, oldest first. */
  getEventsSince(sinceId: number): IndexedEvent[] {
    const rows = this.stmt.all(sinceId) as EventRow[];
    return rows.map((row) => ({
      id: row.id,
      ledger: row.ledger,
      ledgerClosedAt: row.ledger_closed_at,
      txHash: row.tx_hash,
      contractId: row.contract_id,
      category: row.category,
      action: row.action,
      value: safeParse(row.value_json),
    }));
  }

  close(): void {
    this.db.close();
  }
}

function safeParse(json: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(json);
    return typeof parsed === "object" && parsed !== null ? parsed : {};
  } catch {
    return {};
  }
}
