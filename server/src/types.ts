/**
 * Wire types shared (by convention — this is a separate npm package from
 * dashboard/, not a shared workspace) with dashboard/src/useLoanSocket.ts and
 * useMetricsSocket.ts. Keep in sync with docs/realtime-broadcast.md.
 */

export type EventCategory = "vouch" | "loan" | "admin" | "contract";

/** A single decoded row from the indexer's `events` table (tools/indexer/src/db.rs). */
export interface IndexedEvent {
  id: number;
  ledger: number;
  ledgerClosedAt: string;
  txHash: string;
  contractId: string;
  category: string;
  action: string;
  value: Record<string, unknown>;
}

export interface LoanRecord {
  id: number;
  borrower: string;
  amount: number;
  amount_repaid: number;
  total_yield: number;
  status: "Active" | "Repaid" | "Defaulted" | "None";
  created_at: number;
  deadline: number;
  loan_purpose: string;
  vouchers: { voucher: string; stake: number; vouch_timestamp: number }[];
}

export interface ReputationInfo {
  tier: string;
  score: number;
}

export interface ProtocolMetrics {
  tvl: number;
  active_loans: number;
  total_loans: number;
  defaulted_loans: number;
  default_rate: number;
  total_yield_distributed: number;
  slash_count: number;
  fee_revenue: number;
  top_borrowers: [string, number][];
  top_vouchers: [string, number][];
  timestamp: number;
}

// ---------------------------------------------------------------------------
// socket.io (/loans) envelopes — cursor is the underlying indexer event id.
// ---------------------------------------------------------------------------

export interface LoanUpdateFrame {
  eventId: number;
  loan: LoanRecord;
}

export interface LoanListFrame {
  eventId: number;
  loans: LoanRecord[];
}

export interface ReputationFrame {
  eventId: number;
  reputation: ReputationInfo;
}

export interface ResyncRequiredFrame {
  reason: "queue_overflow";
  resumeFrom: number;
}

export interface SubscribePayload {
  borrower: string;
  /** Last eventId this client has already applied; omit/0 for a fresh subscription. */
  since?: number;
}

// ---------------------------------------------------------------------------
// raw WebSocket (/ws/metrics) envelopes
// ---------------------------------------------------------------------------

export type MetricsServerFrame =
  | { type: "snapshot"; id: number; metrics: ProtocolMetrics }
  | { type: "resync_required"; reason: "queue_overflow"; resumeFrom: number }
  | { type: "auth_expiring"; expiresAt: number }
  | { type: "auth_expired" };

export type MetricsClientFrame = { type: "refresh_auth"; token: string };

// ---------------------------------------------------------------------------
// Pub/sub payload — what actually travels over Redis / the relay bus.
// ---------------------------------------------------------------------------

export interface BroadcastEvent {
  eventId: number;
  event: IndexedEvent;
  metrics: ProtocolMetrics;
}

export const EVENTS_CHANNEL = "qc:events";
