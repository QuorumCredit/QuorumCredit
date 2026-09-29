/**
 * Issue #1752: API Response Transformation Pipeline
 *
 * Response formats cannot be customised. This module implements a pluggable
 * transformation pipeline that lets callers request a named transformation via
 * the `?transform=<rule_id>` query parameter.
 *
 * Capabilities:
 *   1. Transformation rule engine — each rule is a named function
 *      `(body: unknown, ctx: TransformContext) => unknown`.
 *   2. `?transform=rule_id` query parameter support — middleware reads this
 *      param and applies the matching rule before the response is flushed.
 *   3. Predefined transformations — snake_case → camelCase, camelCase →
 *      snake_case, minimal (strip null/undefined), enveloped, paginated,
 *      and flat-array variants.
 *   4. Custom transformation registration — callers can register their own
 *      rules at startup or at runtime.
 */

import type { IncomingMessage, ServerResponse } from "node:http";
import { URL } from "node:url";
import { metrics } from "./metricsRegistry.js";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/** Context injected into every transform function. */
export interface TransformContext {
  /** The original request. */
  req: IncomingMessage;
  /** HTTP status code that will be sent. */
  statusCode: number;
  /** The rule ID that was requested by the caller. */
  ruleId: string;
}

/**
 * A transformation rule: takes the raw response body and returns the
 * transformed body.  The function MUST be pure with respect to the input
 * (do not mutate `body` in-place; return a new value).
 */
export type TransformFn = (body: unknown, ctx: TransformContext) => unknown;

export interface TransformRule {
  /** Stable identifier, used in the `?transform=` query parameter. */
  id: string;
  /** Human-readable description surfaced on `GET /api/v1/transforms`. */
  description: string;
  /** The transformation function. */
  fn: TransformFn;
}

// ---------------------------------------------------------------------------
// Rule engine
// ---------------------------------------------------------------------------

/**
 * Manages the catalogue of registered transformation rules and applies them
 * to response bodies.
 */
export class ResponseTransformer {
  private readonly rules = new Map<string, TransformRule>();

  constructor() {
    // Register all predefined transformations at construction time.
    this._registerDefaults();
  }

  // ---------------------------------------------------------------------------
  // Registration
  // ---------------------------------------------------------------------------

  /**
   * Register a custom transformation rule.
   *
   * @throws if `id` is empty.
   * @throws if a rule with the same `id` is already registered and `replace`
   *   is not set to `true`.
   */
  register(rule: TransformRule, replace = false): void {
    if (!rule.id) throw new Error("[responseTransformer] Rule id must not be empty.");
    if (this.rules.has(rule.id) && !replace) {
      throw new Error(
        `[responseTransformer] Rule "${rule.id}" is already registered. ` +
          "Pass replace=true to overwrite it."
      );
    }
    this.rules.set(rule.id, rule);
  }

  /** Remove a transformation rule. */
  unregister(id: string): boolean {
    return this.rules.delete(id);
  }

  /** List all registered rules (id + description). */
  listRules(): Array<{ id: string; description: string }> {
    return Array.from(this.rules.values()).map(({ id, description }) => ({
      id,
      description,
    }));
  }

  // ---------------------------------------------------------------------------
  // Application
  // ---------------------------------------------------------------------------

  /**
   * Apply the rule identified by `ruleId` to `body`.
   *
   * Returns the transformed body, or the original body when:
   *   - `ruleId` is `"none"` or empty.
   *   - No rule with that id is registered (also emits a warning).
   */
  apply(body: unknown, ruleId: string, ctx: TransformContext): unknown {
    if (!ruleId || ruleId === "none") return body;

    const rule = this.rules.get(ruleId);
    if (!rule) {
      console.warn(`[responseTransformer] Unknown rule "${ruleId}". Returning body unchanged.`);
      metrics.incLabeledCounter("qc_response_transform_unknown_rule_total", "rule_id", ruleId);
      return body;
    }

    try {
      const result = rule.fn(body, ctx);
      metrics.incLabeledCounter("qc_response_transforms_total", "rule_id", ruleId);
      return result;
    } catch (err) {
      console.error(`[responseTransformer] Rule "${ruleId}" threw:`, err);
      metrics.incLabeledCounter("qc_response_transform_errors_total", "rule_id", ruleId);
      return body; // fall back to untransformed body
    }
  }

  /**
   * Extract the `transform` query parameter from a request URL.
   * Returns an empty string when the parameter is absent or `"none"`.
   */
  getRuleIdFromRequest(req: IncomingMessage): string {
    try {
      const url = new URL(req.url ?? "", "http://internal");
      return url.searchParams.get("transform") ?? "";
    } catch {
      return "";
    }
  }

  /**
   * Convenience method: wrap a JSON response body through the pipeline and
   * write the transformed response.
   *
   * @param req         - Incoming request (used to extract `?transform=`).
   * @param res         - Server response to write to.
   * @param statusCode  - HTTP status code.
   * @param body        - The raw response body.
   */
  writeTransformed(
    req: IncomingMessage,
    res: ServerResponse,
    statusCode: number,
    body: unknown
  ): void {
    const ruleId = this.getRuleIdFromRequest(req);
    const ctx: TransformContext = { req, statusCode, ruleId };
    const transformed = this.apply(body, ruleId, ctx);
    const payload = JSON.stringify(transformed);
    res.writeHead(statusCode, {
      "content-type": "application/json",
      "x-transform-applied": ruleId || "none",
    });
    res.end(payload);
  }

  // ---------------------------------------------------------------------------
  // Predefined transformations
  // ---------------------------------------------------------------------------

  private _registerDefaults(): void {
    // -----------------------------------------------------------------------
    // identity — pass-through (useful as an explicit "no-op")
    // -----------------------------------------------------------------------
    this.rules.set("identity", {
      id: "identity",
      description: "Return the response body unchanged.",
      fn: (body) => body,
    });

    // -----------------------------------------------------------------------
    // camel_to_snake — convert all object keys from camelCase to snake_case
    // -----------------------------------------------------------------------
    this.rules.set("camel_to_snake", {
      id: "camel_to_snake",
      description: "Recursively convert all object keys from camelCase to snake_case.",
      fn: (body) => deepMapKeys(body, camelToSnake),
    });

    // -----------------------------------------------------------------------
    // snake_to_camel — convert all object keys from snake_case to camelCase
    // -----------------------------------------------------------------------
    this.rules.set("snake_to_camel", {
      id: "snake_to_camel",
      description: "Recursively convert all object keys from snake_case to camelCase.",
      fn: (body) => deepMapKeys(body, snakeToCamel),
    });

    // -----------------------------------------------------------------------
    // minimal — strip null and undefined values from objects
    // -----------------------------------------------------------------------
    this.rules.set("minimal", {
      id: "minimal",
      description: "Strip null and undefined fields from all objects in the response.",
      fn: (body) => stripNullish(body),
    });

    // -----------------------------------------------------------------------
    // enveloped — wrap body in { data: ..., meta: { transformedAt, ruleId } }
    // -----------------------------------------------------------------------
    this.rules.set("enveloped", {
      id: "enveloped",
      description:
        "Wrap the response in a standard envelope: { data, meta: { transformedAt, ruleId } }.",
      fn: (body, ctx) => ({
        data: body,
        meta: {
          transformedAt: new Date().toISOString(),
          ruleId: ctx.ruleId,
        },
      }),
    });

    // -----------------------------------------------------------------------
    // paginated — expects an array; wraps it in { items, count, total }
    // -----------------------------------------------------------------------
    this.rules.set("paginated", {
      id: "paginated",
      description:
        "Wrap an array response in { items, count } for pagination-style consumers.",
      fn: (body) => {
        const items = Array.isArray(body) ? body : [body];
        return { items, count: items.length };
      },
    });

    // -----------------------------------------------------------------------
    // flatten — if body has a single top-level array field, promote it
    // -----------------------------------------------------------------------
    this.rules.set("flatten", {
      id: "flatten",
      description:
        "If the response is an object with one array field, promote that array to the top level.",
      fn: (body) => {
        if (!body || typeof body !== "object" || Array.isArray(body)) return body;
        const entries = Object.entries(body as Record<string, unknown>);
        if (entries.length === 1 && Array.isArray(entries[0]![1])) {
          return entries[0]![1];
        }
        return body;
      },
    });

    // -----------------------------------------------------------------------
    // stringify_bigint — replace BigInt values with string representations
    // -----------------------------------------------------------------------
    this.rules.set("stringify_bigint", {
      id: "stringify_bigint",
      description:
        "Replace BigInt values with their string representations for JSON serialisation safety.",
      fn: (body) => stringifyBigInt(body),
    });

    // -----------------------------------------------------------------------
    // stroops_to_xlm — divide any field ending in _stroops / Amount by 1e7
    // -----------------------------------------------------------------------
    this.rules.set("stroops_to_xlm", {
      id: "stroops_to_xlm",
      description:
        "Convert numeric stroop fields (keys ending in 'Stroops', 'Amount', 'stake', 'yield') " +
        "to XLM by dividing by 10_000_000.",
      fn: (body) => deepMapValues(body, (key, value) => {
        if (typeof value !== "number" && typeof value !== "bigint") return value;
        const k = key.toLowerCase();
        if (
          k.endsWith("stroops") ||
          k.endsWith("amount") ||
          k === "stake" ||
          k === "stakeamount" ||
          k.endsWith("yield") ||
          k.endsWith("payment")
        ) {
          return typeof value === "bigint"
            ? Number(value) / 10_000_000
            : (value as number) / 10_000_000;
        }
        return value;
      }),
    });
  }
}

// ---------------------------------------------------------------------------
// Pure utility functions used by the predefined transforms
// ---------------------------------------------------------------------------

function camelToSnake(key: string): string {
  return key.replace(/([A-Z])/g, (c) => `_${c.toLowerCase()}`);
}

function snakeToCamel(key: string): string {
  return key.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase());
}

/** Recursively remap all object keys using `mapKey`. */
function deepMapKeys(value: unknown, mapKey: (k: string) => string): unknown {
  if (Array.isArray(value)) return value.map((v) => deepMapKeys(v, mapKey));
  if (value && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      out[mapKey(k)] = deepMapKeys(v, mapKey);
    }
    return out;
  }
  return value;
}

/** Recursively strip null / undefined values from objects. */
function stripNullish(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(stripNullish);
  if (value && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      if (v == null) continue;
      out[k] = stripNullish(v);
    }
    return out;
  }
  return value;
}

/** Recursively convert BigInt values to strings. */
function stringifyBigInt(value: unknown): unknown {
  if (typeof value === "bigint") return value.toString();
  if (Array.isArray(value)) return value.map(stringifyBigInt);
  if (value && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      out[k] = stringifyBigInt(v);
    }
    return out;
  }
  return value;
}

/**
 * Recursively map leaf values in an object using `mapValue(key, value)`.
 * Non-object/array leaves are passed to `mapValue` with their parent key.
 */
function deepMapValues(
  value: unknown,
  mapValue: (key: string, value: unknown) => unknown,
  _key = ""
): unknown {
  if (Array.isArray(value)) {
    return value.map((v) => deepMapValues(v, mapValue, _key));
  }
  if (value && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      out[k] = deepMapValues(v, mapValue, k);
    }
    return out;
  }
  return _key ? mapValue(_key, value) : value;
}

// ---------------------------------------------------------------------------
// Singleton
// ---------------------------------------------------------------------------

export const responseTransformer = new ResponseTransformer();
