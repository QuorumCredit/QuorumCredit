#!/usr/bin/env node
/**
 * scripts/check-locale-keys.mjs
 *
 * CI guard: verifies that every key present in en.json also exists in every
 * other locale file under dashboard/src/locales/.
 *
 * Exit codes:
 *   0 — all locales are complete
 *   1 — one or more locales have missing keys (details printed to stderr)
 *
 * Usage:
 *   node scripts/check-locale-keys.mjs
 *   # or via npm script:
 *   npm run check:locales
 */

import { readFileSync, readdirSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const LOCALES_DIR = join(__dirname, "../src/locales");
const BASELINE = "en.json";

// ---------------------------------------------------------------------------
// Flatten a nested object into dot-separated key paths.
// e.g. { a: { b: "x" } } → ["a.b"]
// ---------------------------------------------------------------------------
function flatKeys(obj, prefix = "") {
  return Object.entries(obj).flatMap(([k, v]) => {
    const path = prefix ? `${prefix}.${k}` : k;
    return v !== null && typeof v === "object" && !Array.isArray(v)
      ? flatKeys(v, path)
      : [path];
  });
}

// ---------------------------------------------------------------------------
// Load all locale files
// ---------------------------------------------------------------------------
const files = readdirSync(LOCALES_DIR).filter((f) => f.endsWith(".json"));
const baseline = JSON.parse(
  readFileSync(join(LOCALES_DIR, BASELINE), "utf8"),
);
const baselineKeys = new Set(flatKeys(baseline));

let hasErrors = false;

for (const file of files) {
  if (file === BASELINE) continue;

  const locale = JSON.parse(readFileSync(join(LOCALES_DIR, file), "utf8"));
  const localeKeys = new Set(flatKeys(locale));

  const missing = [...baselineKeys].filter((k) => !localeKeys.has(k));
  const extra = [...localeKeys].filter((k) => !baselineKeys.has(k));

  if (missing.length > 0) {
    hasErrors = true;
    console.error(`\n❌  ${file}: ${missing.length} key(s) missing from en.json baseline:`);
    for (const k of missing) {
      console.error(`     - ${k}`);
    }
  }

  if (extra.length > 0) {
    // Extra keys are warned but do not fail the build — they may be locale-
    // specific additions that haven't been back-ported to en.json yet.
    console.warn(`\n⚠️   ${file}: ${extra.length} key(s) not present in en.json (extra):`);
    for (const k of extra) {
      console.warn(`     + ${k}`);
    }
  }

  if (missing.length === 0 && extra.length === 0) {
    console.log(`✅  ${file}: all keys present`);
  }
}

if (hasErrors) {
  console.error(
    "\nLocale check failed. Add the missing keys to the locale file(s) listed above.",
  );
  process.exit(1);
} else {
  console.log("\nLocale check passed.");
}
