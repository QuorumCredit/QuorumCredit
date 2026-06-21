/**
 * Stellar stroop conversion utilities.
 * 1 XLM = 10,000,000 stroops (10^7).
 * All monetary amounts from the contract are denominated in stroops (i128).
 */

export const STROOPS_PER_XLM = 10_000_000;

/**
 * Convert stroops to XLM with 7 decimal precision.
 * Handles 0, negative values, and large integers safely.
 *
 * @param stroops - Amount in stroops (number or bigint)
 * @returns XLM value as a string with 7 decimal places
 */
export function stroopsToXlm(stroops: number | bigint): string {
  const n = typeof stroops === "bigint" ? Number(stroops) : stroops;
  return (n / STROOPS_PER_XLM).toFixed(7);
}

/**
 * Convert XLM to stroops (rounds to nearest integer).
 */
export function xlmToStroops(xlm: number): number {
  return Math.round(xlm * STROOPS_PER_XLM);
}
