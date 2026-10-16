/**
 * Issue #1782: Smart Contract State Monitoring
 *
 * Tracks named contract state values (e.g. total staked, outstanding loans),
 * records every change, flags anomalies and notifies alert subscribers.
 *
 * Observations are ingested in-process by trusted server code (the indexer or
 * RPC poller). They are deliberately NOT writable through the public API.
 *
 * Anomaly rules, applied to each observed change:
 *   - type_change:     the value changed type (e.g. number -> string).
 *   - negative_value:  a key registered as non-negative went below zero.
 *   - large_change:    a numeric value moved by more than `maxRelativeChange`
 *                      of its previous magnitude.
 */
import { metrics } from "../http/metricsRegistry.js";

export type StateValue = number | string | boolean;

export interface StateChange {
  key: string;
  previous: StateValue | undefined;
  current: StateValue;
  changedAt: number;
}

export type AnomalyKind = "type_change" | "negative_value" | "large_change";

export interface StateAnomaly {
  id: string;
  key: string;
  kind: AnomalyKind;
  detail: string;
  previous: StateValue | undefined;
  current: StateValue;
  detectedAt: number;
}

export interface MonitorKeyOptions {
  /** Reject negative values for this key. */
  nonNegative?: boolean;
  /** Flag numeric moves larger than this fraction of the previous value. */
  maxRelativeChange?: number;
}

export interface ContractStateMonitorOptions {
  /** Most recent anomalies kept in memory. Defaults to 100. */
  maxAlerts?: number;
  /** Most recent changes kept per key. Defaults to 50. */
  maxHistoryPerKey?: number;
  /** Default relative-change threshold when a key does not set one. */
  defaultMaxRelativeChange?: number;
  now?: () => number;
}

export interface ObservationResult {
  change?: StateChange;
  anomalies: StateAnomaly[];
}

type AlertListener = (anomaly: StateAnomaly) => void;

const DEFAULT_MAX_ALERTS = 100;
const DEFAULT_HISTORY = 50;
const DEFAULT_MAX_RELATIVE_CHANGE = 0.5;

export class ContractStateMonitor {
  private values = new Map<string, StateValue>();
  private keyOptions = new Map<string, MonitorKeyOptions>();
  private history = new Map<string, StateChange[]>();
  private alerts: StateAnomaly[] = [];
  private listeners = new Set<AlertListener>();
  private alertCounter = 0;
  private readonly maxAlerts: number;
  private readonly maxHistoryPerKey: number;
  private readonly defaultMaxRelativeChange: number;
  private readonly now: () => number;

  constructor(options: ContractStateMonitorOptions = {}) {
    this.maxAlerts = options.maxAlerts ?? DEFAULT_MAX_ALERTS;
    this.maxHistoryPerKey = options.maxHistoryPerKey ?? DEFAULT_HISTORY;
    this.defaultMaxRelativeChange =
      options.defaultMaxRelativeChange ?? DEFAULT_MAX_RELATIVE_CHANGE;
    this.now = options.now ?? (() => Math.floor(Date.now() / 1000));
  }

  /** Configure how a key is checked. Can be called before or after first observation. */
  watch(key: string, options: MonitorKeyOptions = {}): void {
    this.keyOptions.set(key, options);
  }

  observe(key: string, value: StateValue): ObservationResult {
    const previous = this.values.get(key);
    this.values.set(key, value);
    metrics.setGauge("qc_state_tracked_keys", this.values.size);

    if (previous === undefined) return { anomalies: this.checkSingle(key, value) };
    if (previous === value) return { anomalies: [] };

    const change: StateChange = {
      key,
      previous,
      current: value,
      changedAt: this.now(),
    };
    this.recordHistory(key, change);
    metrics.incCounter("qc_state_changes_total");

    const anomalies = this.detect(key, previous, value);
    for (const anomaly of anomalies) this.raise(anomaly);
    return { change, anomalies };
  }

  /** Current value of every tracked key. */
  snapshot(): Record<string, StateValue> {
    return Object.fromEntries(this.values);
  }

  /** Recent changes for a key, oldest first. */
  getHistory(key: string): StateChange[] {
    return [...(this.history.get(key) ?? [])];
  }

  /** Most recent anomalies, newest first. */
  getAlerts(limit = this.maxAlerts): StateAnomaly[] {
    return this.alerts.slice(-limit).reverse();
  }

  /** Subscribe to new anomalies. Returns an unsubscribe function. */
  onAlert(listener: AlertListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private checkSingle(key: string, value: StateValue): StateAnomaly[] {
    const options = this.keyOptions.get(key);
    if (options?.nonNegative && typeof value === "number" && value < 0) {
      return [this.anomaly(key, "negative_value", `value ${value} is below zero`, undefined, value)];
    }
    return [];
  }

  private detect(key: string, previous: StateValue, current: StateValue): StateAnomaly[] {
    if (typeof previous !== typeof current) {
      return [
        this.anomaly(
          key,
          "type_change",
          `type changed from ${typeof previous} to ${typeof current}`,
          previous,
          current
        ),
      ];
    }
    const found: StateAnomaly[] = this.checkSingle(key, current);

    if (typeof previous === "number" && typeof current === "number") {
      const options = this.keyOptions.get(key);
      const limit = options?.maxRelativeChange ?? this.defaultMaxRelativeChange;
      const base = Math.abs(previous);
      const relative = base === 0 ? (current === 0 ? 0 : Infinity) : Math.abs(current - previous) / base;
      if (relative > limit) {
        found.push(
          this.anomaly(
            key,
            "large_change",
            `moved ${formatPercent(relative)} (limit ${formatPercent(limit)})`,
            previous,
            current
          )
        );
      }
    }
    return found;
  }

  private anomaly(
    key: string,
    kind: AnomalyKind,
    detail: string,
    previous: StateValue | undefined,
    current: StateValue
  ): StateAnomaly {
    return {
      id: `anom_${++this.alertCounter}`,
      key,
      kind,
      detail,
      previous,
      current,
      detectedAt: this.now(),
    };
  }

  private raise(anomaly: StateAnomaly): void {
    this.alerts.push(anomaly);
    if (this.alerts.length > this.maxAlerts) {
      this.alerts.splice(0, this.alerts.length - this.maxAlerts);
    }
    metrics.incLabeledCounter("qc_state_anomalies_total", "kind", anomaly.kind);
    for (const listener of this.listeners) {
      try {
        listener(anomaly);
      } catch (error) {
        console.error("[contractStateMonitor] alert listener failed", error);
      }
    }
  }

  private recordHistory(key: string, change: StateChange): void {
    const list = this.history.get(key) ?? [];
    list.push(change);
    if (list.length > this.maxHistoryPerKey) list.splice(0, list.length - this.maxHistoryPerKey);
    this.history.set(key, list);
  }
}

function formatPercent(fraction: number): string {
  if (!Number.isFinite(fraction)) return "infinite";
  return `${(fraction * 100).toFixed(1)}%`;
}

export const contractStateMonitor = new ContractStateMonitor();
