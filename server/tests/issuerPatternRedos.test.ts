import { describe, expect, it } from "vitest";
import {
  MAX_PATTERN_LENGTH,
  searchCredentialsByIssuerPattern,
  validateIssuerPattern,
} from "../src/credentials/issuerPatternSearch.js";

// Known catastrophic-backtracking shapes. Each must be rejected before compilation.
const REDOS_PATTERNS = [
  "(a+)+$",
  "(a*)*$",
  "(a|aa)+$",
  "(a|a)+$",
  "(a|a?)+$",
  "(a|a)*b",
  "(x|xy)*z",
  "(\\w|\\d)+$",
  "(.*a){10}",
  "((a+)b)+c",
  "(?:a|ab)+$",
  "(?<n>a|aa)+$",
  "((a)|a)+$",
];

// Ordinary issuer patterns that must keep working.
const SAFE_PATTERNS = [
  "^acme",
  "bank\\.(com|org)$",
  "(acme|initech)",
  "(\\d+)-(\\d+)",
  "[(|]+x",
  "[a|b]+",
  "(?:ab)+c",
  "^(?<org>acme)\\.io$",
  "a{2,3}b",
];

describe("issuer pattern ReDoS guard", () => {
  it.each(REDOS_PATTERNS)("rejects repeated ambiguous group %s", (pattern) => {
    const result = validateIssuerPattern(pattern);
    expect(result.valid).toBe(false);
  });

  it.each(SAFE_PATTERNS)("accepts ordinary pattern %s", (pattern) => {
    const result = validateIssuerPattern(pattern);
    expect(result.valid).toBe(true);
  });

  it("rejects unbalanced groups instead of compiling them", () => {
    expect(validateIssuerPattern("(a+").valid).toBe(false);
    expect(validateIssuerPattern("a+)").valid).toBe(false);
  });

  it("still rejects backreferences and over-long patterns", () => {
    expect(validateIssuerPattern("(a)\\1").valid).toBe(false);
    expect(validateIssuerPattern("a".repeat(MAX_PATTERN_LENGTH + 1)).valid).toBe(false);
  });

  it("refuses a known ReDoS pattern at the search entry point", () => {
    const result = searchCredentialsByIssuerPattern("(a|aa)+$");
    expect(result).toHaveProperty("error");
  });
});
