/**
 * Stellar stroop conversion utilities.
 * 1 XLM = 10,000,000 stroops (10^7).
 * All monetary amounts from the contract are denominated in stroops (i128).
 */

export const STROOPS_PER_XLM = 10_000_000;

/** BigInt constant used for the integer-split path in stroopsToXlm. */
const STROOPS_PER_XLM_BN = 10_000_000n;

/**
 * Convert stroops to XLM with exactly 7 decimal places.
 *
 * Precision guarantee: when a `bigint` is supplied the conversion is done
 * entirely with BigInt arithmetic (integer division + modulo) and only a
 * final, bounded string is produced — `Number()` is never called on the
 * input value.  This means balances beyond `Number.MAX_SAFE_INTEGER`
 * (≈ 9 × 10¹⁵ stroops, ≈ 900 million XLM) are rendered exactly.
 *
 * When a `number` is supplied the caller is responsible for ensuring the
 * value is within the safe-integer range; values beyond that range cannot
 * be represented exactly in IEEE-754 and will have already lost precision
 * before this function is called.
 *
 * Handles 0, negative values, and arbitrarily large bigint inputs safely.
 *
 * @param stroops - Amount in stroops (number or bigint)
 * @returns XLM value as a string with exactly 7 decimal places
 */
export function stroopsToXlm(stroops: number | bigint): string {
  // Always work in bigint to avoid Number() precision loss on large inputs.
  const raw: bigint =
    typeof stroops === "bigint" ? stroops : BigInt(Math.trunc(stroops as number));

  const negative = raw < 0n;
  const abs = negative ? -raw : raw;

  const whole = abs / STROOPS_PER_XLM_BN;           // integer XLM part
  const frac  = abs % STROOPS_PER_XLM_BN;            // remainder in stroops

  // Pad fractional part to exactly 7 digits.
  const fracStr = frac.toString().padStart(7, "0");

  return `${negative ? "-" : ""}${whole}.${fracStr}`;
}

/**
 * Convert XLM to stroops (rounds to nearest integer).
 */
export function xlmToStroops(xlm: number): number {
  return Math.round(xlm * STROOPS_PER_XLM);
}
