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
  amount: number;           // stroops
  amount_repaid: number;    // stroops
  total_yield: number;      // stroops
  status: LoanStatus;
  created_at: number;       // unix timestamp
  deadline: number;         // unix timestamp
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
  lastUpdated: number | null;
}

const initialState: LoanState = {
  loans: [],
  reputation: null,
  connected: false,
  lastUpdated: null,
};

const loanSlice = createSlice({
  name: "loans",
  initialState,
  reducers: {
    setConnected(state, action: PayloadAction<boolean>) {
      state.connected = action.payload;
    },
    upsertLoan(state, action: PayloadAction<LoanRecord>) {
      const idx = state.loans.findIndex((l) => l.id === action.payload.id);
      if (idx >= 0) {
        state.loans[idx] = action.payload;
      } else {
        state.loans.push(action.payload);
      }
      state.lastUpdated = Date.now();
    },
    setLoans(state, action: PayloadAction<LoanRecord[]>) {
      state.loans = action.payload;
      state.lastUpdated = Date.now();
    },
    setReputation(state, action: PayloadAction<ReputationInfo>) {
      state.reputation = action.payload;
    },
  },
});

export const { setConnected, upsertLoan, setLoans, setReputation } = loanSlice.actions;
export default loanSlice.reducer;
