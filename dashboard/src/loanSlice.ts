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

export interface LoanState {
  loans: LoanRecord[];
  reputation: ReputationInfo | null;
  connected: boolean;
  /** True from mount until the first loan:list or loan:update arrives */
  loading: boolean;
  lastUpdated: number | null;
}

const initialState: LoanState = {
  loans: [],
  reputation: null,
  connected: false,
  loading: true,
  lastUpdated: null,
};

const loanSlice = createSlice({
  name: "loans",
  initialState,
  reducers: {
    setConnected(state, action: PayloadAction<boolean>) {
      state.connected = action.payload;
      // If we disconnected before ever receiving data, stay loading so the
      // component shows a spinner rather than a misleading empty state.
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

export const { setConnected, upsertLoan, setLoans, setReputation, setLoading } =
  loanSlice.actions;
export default loanSlice.reducer;
