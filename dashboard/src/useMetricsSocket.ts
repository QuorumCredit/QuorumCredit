import { useEffect, useRef, useState } from "react";
import { ProtocolMetrics } from "./analytics";

interface UseMetricsSocketResult {
  latest: ProtocolMetrics | null;
  connected: boolean;
  /**
   * True once the hook has exhausted `maxAttempts` reconnect attempts without
   * a successful connection. The hook stops retrying at this point so the
   * metrics server is not hammered indefinitely.
   *
   * UI should surface this as a human-readable error (e.g. the give-up variant
   * of EmptyState) rather than a silent spinner.
   */
  gaveUp: boolean;
}

/**
 * Opens a WebSocket to `url` and streams the latest metrics snapshot.
 *
 * Fix #1298: incoming frames are now inspected for `type` before being treated
 * as a ProtocolMetrics snapshot — a `resync_required` control frame triggers a
 * reconnect from the server-supplied `resumeFrom` cursor instead of being
 * misinterpreted as metrics data.
 *
 * Fix #1299: the last successfully applied event id is tracked in a ref and
 * appended as `?since=<id>` on every reconnect so the server replays any
 * events the client missed during the gap.
 *
 * Fix #1510: reconnect now uses exponential backoff with full jitter so the
 * client does not hammer a down server.
 *
 *   delay = rand(0, min(baseDelayMs * 2^attempt, maxDelayMs))
 *
 * After `maxAttempts` consecutive failed connections the hook gives up and
 * surfaces `gaveUp: true` so the UI can prompt the user to reload instead of
 * looping forever.  A successful connect resets the attempt counter.
 *
 * @param url          WebSocket server URL
 * @param baseDelayMs  Base reconnect delay (default 1 000 ms)
 * @param maxDelayMs   Upper cap on any single delay (default 30 000 ms)
 * @param maxAttempts  Number of consecutive failures before giving up (default 10)
 * @param resetKey     Increment this value to discard the give-up state and
 *                     restart the connection cycle from scratch (e.g. on a
 *                     user-triggered "Try again" action).
 */
export function useMetricsSocket(
  url: string,
  baseDelayMs = 1000,
  maxDelayMs = 30_000,
  maxAttempts = 10,
  resetKey = 0,
): UseMetricsSocketResult {
  const [latest, setLatest] = useState<ProtocolMetrics | null>(null);
  const [connected, setConnected] = useState(false);
  const [gaveUp, setGaveUp] = useState(false);
  const wsRef = useRef<WebSocket | null>(null);
  const unmountedRef = useRef(false);
  /** Last event id successfully applied from a snapshot frame. */
  const lastEventIdRef = useRef<number>(0);
  /** Consecutive failed connection attempts (reset to 0 on a successful open). */
  const attemptsRef = useRef<number>(0);
  /** setTimeout handle for the pending reconnect, so we can cancel on unmount. */
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    unmountedRef.current = false;
    attemptsRef.current = 0;
    setGaveUp(false);

    function scheduleReconnect(since?: number) {
      if (unmountedRef.current) return;

      const attempt = attemptsRef.current;

      if (attempt >= maxAttempts) {
        // Exhausted all retries — stop hammering the server.
        setGaveUp(true);
        return;
      }

      // Exponential backoff with full jitter:
      //   delay = rand(0, min(base * 2^attempt, cap))
      const ceiling = Math.min(baseDelayMs * Math.pow(2, attempt), maxDelayMs);
      const delay = Math.random() * ceiling;

      attemptsRef.current = attempt + 1;
      retryTimerRef.current = setTimeout(() => connect(since), delay);
    }

    function connect(since?: number) {
      if (unmountedRef.current) return;

      // Append the cursor so the server replays any events missed since the
      // last successfully applied snapshot (fixes #1299).
      const cursor = since ?? lastEventIdRef.current;
      const wsUrl = cursor > 0 ? `${url}${url.includes("?") ? "&" : "?"}since=${cursor}` : url;

      const ws = new WebSocket(wsUrl);
      wsRef.current = ws;

      ws.onopen = () => {
        if (!unmountedRef.current) {
          // Successful connection — reset the backoff counter.
          attemptsRef.current = 0;
          setConnected(true);
          setGaveUp(false);
        }
      };

      ws.onmessage = (ev) => {
        try {
          // #1298: parse the raw frame and check `type` before treating it as
          // a ProtocolMetrics snapshot. Unknown future frame types are ignored
          // rather than forwarded to setLatest.
          const frame = JSON.parse(ev.data as string) as Record<string, unknown>;

          if (frame.type === "snapshot") {
            // Happy path: a real metrics snapshot.
            const id = typeof frame.id === "number" ? frame.id : 0;
            const metrics = frame.metrics as ProtocolMetrics;
            if (!unmountedRef.current) {
              // Advance the cursor so the next reconnect resumes from here.
              if (id > lastEventIdRef.current) lastEventIdRef.current = id;
              setLatest(metrics);
            }
            return;
          }

          if (frame.type === "resync_required") {
            // #1298: control frame — do NOT forward to setLatest. Instead,
            // reconnect from the server-supplied cursor so we recover without
            // corrupting the displayed metrics (#1299 cursor tracking applies
            // here too: we jump to resumeFrom rather than our stale cursor).
            const resumeFrom =
              typeof frame.resumeFrom === "number"
                ? frame.resumeFrom
                : lastEventIdRef.current;
            if (!unmountedRef.current) {
              lastEventIdRef.current = resumeFrom;
              ws.close(); // triggers onclose → schedules reconnect with updated cursor
            }
            return;
          }

          // auth_expiring / auth_expired / unknown control frames: ignore metrics.
        } catch {
          // malformed frame — ignore
        }
      };

      ws.onclose = () => {
        if (unmountedRef.current) return;
        setConnected(false);
        // Schedule next reconnect using backoff. Pass the current cursor so the
        // server replays events missed during this disconnect window.
        scheduleReconnect();
      };

      ws.onerror = () => ws.close();
    }

    connect();

    return () => {
      unmountedRef.current = true;
      if (retryTimerRef.current !== null) {
        clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
      wsRef.current?.close();
    };
  }, [url, baseDelayMs, maxDelayMs, maxAttempts, resetKey]);

  return { latest, connected, gaveUp };
}
