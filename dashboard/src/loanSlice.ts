import { createSlice, PayloadAction } from "@reduxjs/toolkit";

export type LoanStatus = "Active" | "Repaid" | "Defaulted" | "None";

export interface VouchRecord {
  voucher: string;
  stake: number;
  vouch_timestamp: number;
}

export interface LoanRecord {
  id: number;
  borrower: string;
  amount: number;       // stroops
  amount_repaid: number; // stroops
  total_yield: number;  // stroops
  status: LoanStatus;
  created_at: number;   // unix timestamp
  deadline: number;     // unix timestamp
  loan_purpose: string;
  vouchers: VouchRecord[];
}

export interface ReputationInfo {
  tier: string;
  score: number;
}

/**
 * Discriminated union for socket connection errors.
 *
 * - `auth`      — server rejected the connection (bad/missing API key, expired
 *                 token, etc.).  Socket.IO will NOT retry automatically; the UI
 *                 should prompt the user to re-authenticate.
 * - `transient` — a network-level failure (ECONNREFUSED, timeout, …).
 *                 Socket.IO will keep retrying; the UI should show a reconnecting
 *                 spinner rather than an auth-error message.
 */
export type SocketError =
  | { kind: "auth"; message: string }
  | { kind: "transient"; message: string };

export interface LoanState {
  loans: LoanRecord[];
  reputation: ReputationInfo | null;
  connected: boolean;
  /** True from mount until the first loan:list or loan:update arrives */
  loading: boolean;
  lastUpdated: number | null;
  /**
   * Non-null when the most recent connection attempt failed.
   * Cleared automatically on a successful `connect` event.
   * `kind === "auth"` means the server rejected the credentials;
   * `kind === "transient"` means a network blip (socket is retrying).
   */
  socketError: SocketError | null;
}

const initialState: LoanState = {
  loans: [],
  reputation: null,
  connected: false,
  loading: true,
  lastUpdated: null,
  socketError: null,
};

const loanSlice = createSlice({
  name: "loans",
  initialState,
  reducers: {
    setConnected(state, action: PayloadAction<boolean>) {
      state.connected = action.payload;
      // Clear any previous error on a successful connect so stale error banners
      // disappear once the socket is healthy again.
      if (action.payload) {
        state.socketError = null;
      }
      // If we disconnected before ever receiving data, stay loading so the
      // component shows a spinner rather than a misleading empty state.
    },
    /**
     * Record a connection error with its kind so UI can distinguish auth
     * failures (need re-login) from transient disconnects (show spinner).
     */
    setSocketError(state, action: PayloadAction<SocketError | null>) {
      state.socketError = action.payload;
    },
    upsertLoan(state, action: PayloadAction<LoanRecord>) {
      const idx = state.loans.findIndex((l) => l.id === action.payload.id);
      if (idx >= 0) {
        state.loans[idx] = action.payload;
      } else {
        state.loans.push(action.payload);
      }
      state.lastUpdated = Date.now();
      state.loading = false;
    },
    setLoans(state, action: PayloadAction<LoanRecord[]>) {
      state.loans = action.payload;
      state.lastUpdated = Date.now();
      state.loading = false;
    },
    setReputation(state, action: PayloadAction<ReputationInfo>) {
      state.reputation = action.payload;
    },
    /** Explicitly mark loading done (e.g. server confirmed no loans exist) */
    setLoading(state, action: PayloadAction<boolean>) {
      state.loading = action.payload;
    },
  },
});

export const { setConnected, upsertLoan, setLoans, setReputation, setLoading, setSocketError } =
  loanSlice.actions;
export default loanSlice.reducer;
