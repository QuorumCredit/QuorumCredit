/**
 * Issue #2086: bounded integer query-parameter parsing.
 *
 * `parseInt` silently yields NaN for non-numeric input and accepts any size,
 * so unbounded `limit`/`days` values flow straight into the stores. This helper
 * returns `fallback` for missing or non-numeric input and clamps everything
 * else into [min, max].
 */
export function parseBoundedInt(
  value: string | null | undefined,
  min: number,
  max: number,
  fallback: number
): number {
  if (value === null || value === undefined) return fallback;
  const trimmed = value.trim();
  if (trimmed === "") return fallback;

  const parsed = Number(trimmed);
  if (!Number.isFinite(parsed)) return fallback;

  const truncated = Math.trunc(parsed);
  return Math.min(max, Math.max(min, truncated));
}
