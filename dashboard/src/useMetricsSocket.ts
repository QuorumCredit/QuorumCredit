import { useEffect, useRef, useState } from "react";
import { ProtocolMetrics } from "./analytics";

interface UseMetricsSocketResult {
  latest: ProtocolMetrics | null;
  connected: boolean;
}

/**
 * Opens a WebSocket to `url` and streams the latest metrics snapshot.
 * Reconnects automatically after `reconnectDelayMs` on unexpected close,
 * resuming from the last-seen event id via the `?since=` query parameter so
 * events published during the disconnect window are not silently dropped.
 *
 * Fix #1298: incoming frames are now inspected for `type` before being treated
 * as a ProtocolMetrics snapshot — a `resync_required` control frame triggers a
 * reconnect from the server-supplied `resumeFrom` cursor instead of being
 * misinterpreted as metrics data.
 *
 * Fix #1299: the last successfully applied event id is tracked in a ref and
 * appended as `?since=<id>` on every reconnect so the server replays any
 * events the client missed during the gap.
 */
export function useMetricsSocket(
  url: string,
  reconnectDelayMs = 3000
): UseMetricsSocketResult {
  const [latest, setLatest] = useState<ProtocolMetrics | null>(null);
  const [connected, setConnected] = useState(false);
  const wsRef = useRef<WebSocket | null>(null);
  const unmountedRef = useRef(false);
  /** Last event id successfully applied from a snapshot frame. */
  const lastEventIdRef = useRef<number>(0);

  useEffect(() => {
    unmountedRef.current = false;

    function connect(since?: number) {
      if (unmountedRef.current) return;

      // Append the cursor so the server replays any events missed since the
      // last successfully applied snapshot (fixes #1299).
      const cursor = since ?? lastEventIdRef.current;
      const wsUrl = cursor > 0 ? `${url}${url.includes("?") ? "&" : "?"}since=${cursor}` : url;

      const ws = new WebSocket(wsUrl);
      wsRef.current = ws;

      ws.onopen = () => {
        if (!unmountedRef.current) setConnected(true);
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
            const resumeFrom = typeof frame.resumeFrom === "number" ? frame.resumeFrom : lastEventIdRef.current;
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
        // Reconnect with the current cursor (lastEventIdRef already updated
        // before close was triggered, e.g. by resync_required handling).
        setTimeout(() => connect(), reconnectDelayMs);
      };

      ws.onerror = () => ws.close();
    }

    connect();

    return () => {
      unmountedRef.current = true;
      wsRef.current?.close();
    };
  }, [url, reconnectDelayMs]);

  return { latest, connected };
}
