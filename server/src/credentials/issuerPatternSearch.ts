/**
 * Issue #1742: Credential search by issuer pattern.
 *
 * Patterns are user-supplied regular expressions, so they are validated before
 * compilation to keep catastrophic-backtracking (ReDoS) shapes out, and compiled
 * regexes are cached so repeated searches don't recompile. Matching runs against the
 * store's issuer index — once per distinct issuer rather than once per credential —
 * and per-issuer match results are memoised per pattern.
 */

import { credentialStore, type Credential } from "./credentialStore.js";

export const MAX_PATTERN_LENGTH = 256;
const MAX_CACHED_PATTERNS = 500;
const MAX_ISSUER_LENGTH_TESTED = 1024;

export type PatternValidation =
  | { valid: true; regex: RegExp }
  | { valid: false; error: string };

// Backreferences make matching non-linear.
const BACKREFERENCE = /\\[1-9]|\\k</;
// A quantifier token at the start of the string: *, +, ?, {n}, {n,}, {n,m}.
const QUANTIFIER_AT = /^(?:[+*?]|\{\d+(?:,\d*)?\})/;

/**
 * Structural check for ReDoS-prone shapes: a repeated group whose body contains
 * another quantifier or an alternation, e.g. (a+)+, (a*)*, (a|aa)+, (a|a)*, (.*a){10}.
 * Such groups can be matched in exponentially many ways.
 *
 * Walks the pattern rather than matching a regex over it, so escaped characters
 * and character classes (e.g. [(|]) are not mistaken for group syntax. Malformed
 * nesting is reported as risky; RegExp construction will reject it separately.
 */
function hasRiskyRepeatedGroup(pattern: string): boolean {
  interface Frame {
    hasQuantifier: boolean;
    hasAlternation: boolean;
  }
  const stack: Frame[] = [{ hasQuantifier: false, hasAlternation: false }];
  const top = (): Frame => stack[stack.length - 1];

  let i = 0;
  while (i < pattern.length) {
    const ch = pattern[i];

    if (ch === "\\") {
      i += 2;
      continue;
    }

    if (ch === "[") {
      // Skip the character class, honouring escapes inside it.
      i++;
      if (pattern[i] === "^") i++;
      while (i < pattern.length && pattern[i] !== "]") {
        i += pattern[i] === "\\" ? 2 : 1;
      }
      i++;
      continue;
    }

    if (ch === "(") {
      stack.push({ hasQuantifier: false, hasAlternation: false });
      i++;
      if (pattern[i] === "?") {
        i++;
        if (pattern[i] === "<" && pattern[i + 1] !== "=" && pattern[i + 1] !== "!") {
          // Named group: (?<name>...)
          const close = pattern.indexOf(">", i);
          if (close === -1) return true;
          i = close + 1;
        } else if (pattern[i] === "<") {
          // Lookbehind: (?<= or (?<!
          i += 2;
        } else {
          // (?: (?= (?!
          i++;
        }
      }
      continue;
    }

    if (ch === ")") {
      if (stack.length === 1) return true; // unbalanced
      const frame = stack.pop() as Frame;
      i++;
      const quantified = QUANTIFIER_AT.test(pattern.slice(i));
      if (quantified && (frame.hasQuantifier || frame.hasAlternation)) return true;
      const parent = top();
      parent.hasQuantifier = parent.hasQuantifier || frame.hasQuantifier || quantified;
      parent.hasAlternation = parent.hasAlternation || frame.hasAlternation;
      continue;
    }

    if (ch === "|") {
      top().hasAlternation = true;
      i++;
      continue;
    }

    const quantifier = pattern.slice(i).match(QUANTIFIER_AT);
    if (quantifier) {
      top().hasQuantifier = true;
      i += quantifier[0].length;
      continue;
    }

    i++;
  }

  return stack.length !== 1;
}

interface CacheEntry {
  regex: RegExp;
  /** issuer -> matched? */
  issuerResults: Map<string, boolean>;
}

const patternCache = new Map<string, CacheEntry>();

export function validateIssuerPattern(pattern: string, flags = "i"): PatternValidation {
  if (typeof pattern !== "string" || pattern.length === 0) {
    return { valid: false, error: "issuer_pattern must be a non-empty string" };
  }
  if (pattern.length > MAX_PATTERN_LENGTH) {
    return { valid: false, error: `issuer_pattern exceeds ${MAX_PATTERN_LENGTH} characters` };
  }
  if (hasRiskyRepeatedGroup(pattern)) {
    return {
      valid: false,
      error: "issuer_pattern repeats a group that contains a quantifier or alternation, which is not allowed",
    };
  }
  if (BACKREFERENCE.test(pattern)) {
    return { valid: false, error: "issuer_pattern contains backreferences, which are not allowed" };
  }
  try {
    return { valid: true, regex: new RegExp(pattern, flags) };
  } catch (err) {
    return { valid: false, error: `invalid issuer_pattern: ${(err as Error).message}` };
  }
}

function getCompiled(pattern: string, caseSensitive: boolean): CacheEntry | { error: string } {
  const key = `${caseSensitive ? "s" : "i"}:${pattern}`;
  const cached = patternCache.get(key);
  if (cached) {
    // Refresh LRU position.
    patternCache.delete(key);
    patternCache.set(key, cached);
    return cached;
  }
  const validation = validateIssuerPattern(pattern, caseSensitive ? "" : "i");
  if (!validation.valid) return { error: validation.error };

  const entry: CacheEntry = { regex: validation.regex, issuerResults: new Map() };
  patternCache.set(key, entry);
  if (patternCache.size > MAX_CACHED_PATTERNS) {
    const oldest = patternCache.keys().next().value;
    if (oldest !== undefined) patternCache.delete(oldest);
  }
  return entry;
}

export interface IssuerPatternSearchOptions {
  caseSensitive?: boolean;
  status?: Credential["status"];
  type?: Credential["type"];
  limit?: number;
  offset?: number;
}

export interface IssuerPatternSearchResult {
  pattern: string;
  matchedIssuers: string[];
  total: number;
  limit: number;
  offset: number;
  credentials: Credential[];
}

export function searchCredentialsByIssuerPattern(
  pattern: string,
  options: IssuerPatternSearchOptions = {}
): IssuerPatternSearchResult | { error: string } {
  const compiled = getCompiled(pattern, options.caseSensitive ?? false);
  if ("error" in compiled) return compiled;

  const matchedIssuers: string[] = [];
  for (const issuer of credentialStore.getIssuers()) {
    let matched = compiled.issuerResults.get(issuer);
    if (matched === undefined) {
      matched =
        issuer.length <= MAX_ISSUER_LENGTH_TESTED && compiled.regex.test(issuer);
      compiled.issuerResults.set(issuer, matched);
    }
    if (matched) matchedIssuers.push(issuer);
  }

  let credentials = credentialStore.getCredentialsByIssuers(matchedIssuers);
  if (options.status) credentials = credentials.filter((c) => c.status === options.status);
  if (options.type) credentials = credentials.filter((c) => c.type === options.type);
  credentials.sort((a, b) => b.issuedAt - a.issuedAt);

  const limit = Math.min(Math.max(options.limit ?? 50, 1), 500);
  const offset = Math.max(options.offset ?? 0, 0);

  return {
    pattern,
    matchedIssuers,
    total: credentials.length,
    limit,
    offset,
    credentials: credentials.slice(offset, offset + limit),
  };
}
