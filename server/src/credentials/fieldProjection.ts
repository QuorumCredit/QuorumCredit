/**
 * Issue #1779: Progressive Disclosure API
 *
 * Lets callers request only the credential fields they need via `?fields=`
 * instead of always receiving the full credential record.
 *
 * Grammar:
 *   - Comma-separated list of paths, e.g. `id,status,metadata.score`.
 *   - Top-level paths must be one of DISCLOSABLE_FIELDS.
 *   - Only `metadata` is a nested container; `metadata.<key>[.<key>...]` selects
 *     a nested value from the free-form metadata object.
 *
 * Field dependencies:
 *   - Selecting a nested path (`metadata.score`) implicitly includes its parent
 *     container (`metadata`) in the projection, with only the selected keys.
 *   - Selecting `metadata` together with `metadata.<key>` returns the whole
 *     `metadata` object, because the broader selection subsumes the narrower one.
 *   - Omitting `fields` (or passing an empty value) returns every field.
 */
import type { Credential } from "./credentialStore.js";

export const DISCLOSABLE_FIELDS = [
  "id",
  "holderId",
  "type",
  "issuedAt",
  "expiresAt",
  "issuer",
  "status",
  "metadata",
] as const;

export const MAX_FIELD_PATHS = 32;

const SEGMENT = /^[A-Za-z_][A-Za-z0-9_]*$/;
const NESTED_CONTAINERS = new Set<string>(["metadata"]);

export class FieldSelectionError extends Error {}

/**
 * Parse and validate a `fields` query value.
 * Returns `undefined` when the caller asked for everything.
 */
export function parseFieldsParam(raw: string | null | undefined): string[] | undefined {
  if (raw === null || raw === undefined || raw.trim() === "") return undefined;

  const paths = Array.from(
    new Set(
      raw
        .split(",")
        .map((part) => part.trim())
        .filter((part) => part.length > 0)
    )
  );
  if (paths.length === 0) return undefined;
  if (paths.length > MAX_FIELD_PATHS) {
    throw new FieldSelectionError(`fields cannot exceed ${MAX_FIELD_PATHS} paths`);
  }

  for (const path of paths) {
    const segments = path.split(".");
    if (!segments.every((segment) => SEGMENT.test(segment))) {
      throw new FieldSelectionError(`invalid field path: ${path}`);
    }
    const [top, ...rest] = segments as [string, ...string[]];
    if (!(DISCLOSABLE_FIELDS as readonly string[]).includes(top)) {
      throw new FieldSelectionError(`unknown field: ${top}`);
    }
    if (rest.length > 0 && !NESTED_CONTAINERS.has(top)) {
      throw new FieldSelectionError(`field ${top} does not support nested selection`);
    }
  }

  return paths;
}

/**
 * Project a credential down to the requested paths.
 * With no paths, the full credential is returned unchanged.
 */
export function projectCredential(
  credential: Credential,
  paths?: string[]
): Record<string, unknown> {
  const full = credential as unknown as Record<string, unknown>;
  if (!paths || paths.length === 0) return { ...full };

  const out: Record<string, unknown> = {};
  // Group nested selections by top-level field, collapsing to the broadest selection.
  const nestedByTop = new Map<string, string[][]>();

  for (const path of paths) {
    const [top, ...rest] = path.split(".") as [string, ...string[]];
    if (rest.length === 0) {
      out[top] = full[top];
      nestedByTop.delete(top);
      continue;
    }
    if (top in out && !nestedByTop.has(top)) continue; // whole field already selected
    const list = nestedByTop.get(top) ?? [];
    list.push(rest);
    nestedByTop.set(top, list);
  }

  for (const [top, selections] of nestedByTop) {
    const source = full[top];
    if (!isPlainObject(source)) continue;
    const picked: Record<string, unknown> = {};
    for (const selection of selections) assignPath(picked, source, selection);
    if (Object.keys(picked).length > 0) out[top] = picked;
  }

  return out;
}

function assignPath(
  target: Record<string, unknown>,
  source: Record<string, unknown>,
  segments: string[]
): void {
  const [head, ...rest] = segments as [string, ...string[]];
  if (!(head in source)) return;
  if (rest.length === 0) {
    target[head] = source[head];
    return;
  }
  const child = source[head];
  if (!isPlainObject(child)) return;
  const nested = isPlainObject(target[head]) ? (target[head] as Record<string, unknown>) : {};
  assignPath(nested, child, rest);
  if (Object.keys(nested).length > 0) target[head] = nested;
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
