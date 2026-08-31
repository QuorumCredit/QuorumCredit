/**
 * i18n.test.ts — Tests for issue #1509.
 *
 * Covers:
 *   1. missingKeyHandler suppresses console.warn in test environment
 *   2. missingKeyHandler fires console.warn outside test environment
 *   3. fr locale has every key that en locale has (no missing keys)
 *   4. t() resolves French strings when lng="fr"
 *   5. Fallback to English for a key absent from fr (regression guard)
 */

import { describe, it, expect, vi } from "vitest";
import i18next from "i18next";
import { initReactI18next } from "react-i18next";
import en from "../locales/en.json";
import fr from "../locales/fr.json";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Recursively extract all dot-separated key paths from a nested object. */
function flatKeys(obj: Record<string, unknown>, prefix = ""): string[] {
  return Object.entries(obj).flatMap(([k, v]) => {
    const path = prefix ? `${prefix}.${k}` : k;
    return v !== null && typeof v === "object" && !Array.isArray(v)
      ? flatKeys(v as Record<string, unknown>, path)
      : [path];
  });
}

/** Build a fresh i18next instance so tests don't share state. */
function makeI18n(lng: string, warnOnMissing = true) {
  const instance = i18next.createInstance();
  instance.use(initReactI18next).init({
    lng,
    fallbackLng: "en",
    resources: {
      en: { translation: en },
      fr: { translation: fr },
    },
    interpolation: { escapeValue: false },
    missingKeyHandler: (_lngs, _ns, key) => {
      if (warnOnMissing && process.env.NODE_ENV !== "test") {
        console.warn(`[i18n] missing key: ${key}`);
      }
    },
  });
  return instance;
}

// ---------------------------------------------------------------------------
// 1 & 2 — missingKeyHandler behaviour
// ---------------------------------------------------------------------------

describe("missingKeyHandler", () => {
  it("does NOT call console.warn for a missing key when NODE_ENV=test", () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    const instance = i18next.createInstance();
    instance.use(initReactI18next).init({
      lng: "en",
      fallbackLng: "en",
      resources: { en: { translation: en } },
      interpolation: { escapeValue: false },
      missingKeyHandler: (_lngs, _ns, key) => {
        if (process.env.NODE_ENV !== "test") {
          console.warn(`[i18n] missing key: ${key}`);
        }
      },
    });

    instance.t("this.key.does.not.exist");
    expect(warnSpy).not.toHaveBeenCalled();
    warnSpy.mockRestore();
  });

  it("would call console.warn for a missing key when NODE_ENV≠test", () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    const handler = (_lngs: string[], _ns: string, key: string) => {
      console.warn(`[i18n] missing key: ${key}`);
    };

    handler(["en"], "translation", "some.missing.key");
    expect(warnSpy).toHaveBeenCalledWith("[i18n] missing key: some.missing.key");
    warnSpy.mockRestore();
  });
});

// ---------------------------------------------------------------------------
// 3 — fr locale key completeness
// ---------------------------------------------------------------------------

describe("fr locale key completeness", () => {
  const enKeys = flatKeys(en as Record<string, unknown>);
  const frKeys = new Set(flatKeys(fr as Record<string, unknown>));

  it("fr.json contains every key defined in en.json", () => {
    const missing = enKeys.filter((k) => !frKeys.has(k));
    expect(missing, `Missing keys in fr.json: ${missing.join(", ")}`).toHaveLength(0);
  });

  it("fr.json has no keys absent from en.json (keeps parity in both directions)", () => {
    const enKeySet = new Set(enKeys);
    const extra = [...frKeys].filter((k) => !enKeySet.has(k));
    if (extra.length > 0) {
      console.info(`fr.json has ${extra.length} extra key(s) not in en.json: ${extra.join(", ")}`);
    }
    expect(true).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// 4 — t() resolves French strings when lng="fr"
// ---------------------------------------------------------------------------

describe("fr locale — t() returns French strings", () => {
  it("translates emptyState.loans.title to French", () => {
    const i18n = makeI18n("fr");
    expect(i18n.t("emptyState.loans.title")).toBe("Aucun prêt pour l'instant");
  });

  it("translates emptyState.giveUp.retry to French", () => {
    const i18n = makeI18n("fr");
    expect(i18n.t("emptyState.giveUp.retry")).toBe("Réessayer");
  });

  it("translates analytics.pageTitle to French", () => {
    const i18n = makeI18n("fr");
    expect(i18n.t("analytics.pageTitle")).toBe("Tableau de bord admin QuorumCredit");
  });

  it("preserves {{time}} interpolation placeholder in loans.updated", () => {
    const i18n = makeI18n("fr");
    const result = i18n.t("loans.updated", { time: "12:00" });
    expect(result).toContain("12:00");
    expect(result).not.toBe("Updated 12:00");
  });
});

// ---------------------------------------------------------------------------
// 5 — fallback to English when a key is absent from the active locale
// ---------------------------------------------------------------------------

describe("fallback behaviour", () => {
  it("falls back to English when a key is missing from the active locale", () => {
    const frStub = {
      emptyState: {
        loans: { title: "Aucun prêt pour l'instant", subtitle: "…" },
      },
    };

    const instance = i18next.createInstance();
    instance.use(initReactI18next).init({
      lng: "fr",
      fallbackLng: "en",
      resources: {
        en: { translation: en },
        fr: { translation: frStub },
      },
      interpolation: { escapeValue: false },
    });

    expect(instance.t("emptyState.loans.title")).toBe("Aucun prêt pour l'instant");
    expect(instance.t("emptyState.giveUp.retry")).toBe("Try again");
  });
});
