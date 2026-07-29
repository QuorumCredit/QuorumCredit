import { useEffect, useRef } from "react";
import { useDispatch } from "react-redux";
import io from "socket.io-client";
import { AppDispatch } from "./store";
import { setConnected, upsertLoan, setLoans, setReputation } from "./loanSlice";
import type { LoanRecord, ReputationInfo } from "./loanSlice";

export interface UseLoanSocketOptions {
  /** socket.io server URL, e.g. "http://localhost:3000" */
  url: string;
  /** Borrower address to subscribe to */
  borrower: string;
  /** API key sent in socket auth header */
  apiKey?: string;
}

/**
 * Opens a socket.io connection to the loan-status server and dispatches
 * Redux actions as the server pushes events.
 *
 * Events handled:
 *  - "loan:update"       → upsertLoan
 *  - "loan:list"         → setLoans
 *  - "reputation"        → setReputation
 *  - "resync_required"   → re-subscribes with `since: resumeFrom` to recover
 *                          any state dropped by the server's connectionQueue
 *                          backpressure policy (see server/src/ws/connectionQueue.ts)
 */
export function useLoanSocket({ url, borrower, apiKey }: UseLoanSocketOptions): void {
  const dispatch = useDispatch<AppDispatch>();
  const socketRef = useRef<ReturnType<typeof io> | null>(null);

  useEffect(() => {
    const socket = io(url, {
      auth: apiKey ? { key: apiKey } : undefined,
      transports: ["websocket"],
    });

    socketRef.current = socket;

    socket.on("connect", () => dispatch(setConnected(true)));
    socket.on("disconnect", () => dispatch(setConnected(false)));

    // Subscribe to this borrower's updates
    socket.emit("subscribe", { borrower });

    socket.on("loan:update", (loan: LoanRecord) => dispatch(upsertLoan(loan)));
    socket.on("loan:list", (loans: LoanRecord[]) => dispatch(setLoans(loans)));
    socket.on("reputation", (rep: ReputationInfo) => dispatch(setReputation(rep)));

    // The server drops the oldest queued message on backpressure overflow and
    // emits this control frame instead of leaving the client to silently operate
    // on a gap. Re-subscribing with `since: resumeFrom` triggers a fresh
    // "loan:list" reply covering everything from that cursor forward.
    socket.on("resync_required", (payload: { reason: string; resumeFrom: number }) => {
      socket.emit("subscribe", { borrower, since: payload.resumeFrom });
    });

    return () => {
      socket.disconnect();
      dispatch(setConnected(false));
    };
  }, [url, borrower, apiKey, dispatch]);
}
