/**
 * i18n.ts — i18next initialisation for the QuorumCredit dashboard.
 *
 * Import this module once at the app entry point (main.tsx / index.tsx) before
 * rendering. Every component can then call `useTranslation()` without any
 * additional setup.
 *
 * Adding a new locale
 * --------------------
 * See docs/adding-a-locale.md for the full walkthrough.
 */

import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en.json";
import fr from "./locales/fr.json";

i18n
  .use(initReactI18next)
  .init({
    // Default language
    lng: typeof navigator !== "undefined" ? navigator.language.split("-")[0] : "en",
    fallbackLng: "en",

    // All translations are bundled; no HTTP back-end needed.
    resources: {
      en: { translation: en },
      fr: { translation: fr },
    },

    interpolation: {
      // React already escapes output — no need for i18next escaping.
      escapeValue: false,
    },

    // Suppress the "no translations found" warning during tests.
    missingKeyHandler: (lngs, ns, key) => {
      if (process.env.NODE_ENV !== "test") {
        console.warn(`[i18n] missing key: ${key}`);
      }
    },
  });

export default i18n;
