import { useEffect, useRef } from "react";
import { useDispatch } from "react-redux";
import { io } from "socket.io-client";
import { AppDispatch } from "./store";
import { setConnected, upsertLoan, setLoans, setReputation } from "./loanSlice";
import type { LoanRecord, ReputationInfo } from "./loanSlice";
import type { LoanUpdateFrame, LoanListFrame } from "../../server/src/types";

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
 * Fix #1299: the last successfully applied event id is tracked so that when the
 * socket reconnects (after any network blip) the `subscribe` message includes a
 * `since` cursor. The server's EventStore.getEventsSince path then replays any
 * loan events published during the disconnect window instead of silently dropping
 * them.
 *
 * Events handled:
 *  - "loan:update"    → upsertLoan
 *  - "loan:list"      → setLoans (initial replay on subscribe)
 *  - "reputation"     → setReputation
 */
export function useLoanSocket({ url, borrower, apiKey }: UseLoanSocketOptions): void {
  const dispatch = useDispatch<AppDispatch>();
  const socketRef = useRef<ReturnType<typeof io> | null>(null);
  /** Last event id successfully applied from any server frame. */
  const lastEventIdRef = useRef<number>(0);

  useEffect(() => {
    const socket = io(url, {
      auth: apiKey ? { key: apiKey } : undefined,
      transports: ["websocket"],
    });

    socketRef.current = socket;

    socket.on("connect", () => {
      dispatch(setConnected(true));
      // Re-subscribe every time the transport connects (initial connect or any
      // automatic socket.io reconnect after a network blip). Pass the cursor so
      // the server replays events published during the gap (#1299).
      socket.emit("subscribe", {
        borrower,
        ...(lastEventIdRef.current > 0 ? { since: lastEventIdRef.current } : {}),
      });
    });

    socket.on("disconnect", () => dispatch(setConnected(false)));

    socket.on("loan:update", (frame: LoanUpdateFrame | LoanRecord) => {
      // Support both the raw LoanRecord shape (legacy) and the envelope shape
      // {eventId, loan} that the server emits after the #1299 server-side fix.
      if (frame && typeof frame === "object" && "eventId" in frame && "loan" in frame) {
        const { eventId, loan } = frame as LoanUpdateFrame;
        if (eventId > lastEventIdRef.current) lastEventIdRef.current = eventId;
        dispatch(upsertLoan(loan as LoanRecord));
      } else {
        dispatch(upsertLoan(frame as LoanRecord));
      }
    });

    socket.on("loan:list", (frame: LoanListFrame | LoanRecord[]) => {
      // Support both the raw LoanRecord[] shape (legacy) and the envelope shape
      // {eventId, loans} emitted after the server-side #1299 fix.
      if (frame && typeof frame === "object" && !Array.isArray(frame) && "loans" in frame) {
        const { eventId, loans } = frame as LoanListFrame;
        if (eventId > lastEventIdRef.current) lastEventIdRef.current = eventId;
        dispatch(setLoans(loans as LoanRecord[]));
      } else {
        dispatch(setLoans(frame as LoanRecord[]));
      }
    });

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
