# Adding a New Locale to the QuorumCredit Dashboard

The dashboard uses [i18next](https://www.i18next.com/) with
[react-i18next](https://react.i18next.com/). All translations live as plain
JSON files under `dashboard/src/locales/`. Number and date formatting is
handled entirely by the browser's built-in `Intl` APIs, so you only need to
provide translated strings — the numeric/date rendering adapts automatically.

---

## Step-by-step

### 1. Copy the English baseline

```bash
cp dashboard/src/locales/en.json dashboard/src/locales/<lang>.json
```

Replace `<lang>` with the [BCP 47 language tag](https://www.ietf.org/rfc/bcp/bcp47.txt)
for your language (e.g. `fr` for French, `es` for Spanish, `pt-BR` for
Brazilian Portuguese).

### 2. Translate the strings

Open `dashboard/src/locales/<lang>.json` and replace every English value with
the equivalent in the target language. **Keep all JSON keys unchanged** — only
the values should change.

```json
// en.json (do NOT edit this)
{
  "emptyState": {
    "loans": {
      "title": "No loans yet",
      "subtitle": "Request your first loan to get started. ..."
    }
  }
}

// fr.json (new file)
{
  "emptyState": {
    "loans": {
      "title": "Aucun prêt pour l'instant",
      "subtitle": "Demandez votre premier prêt pour commencer. ..."
    }
  }
}
```

Interpolation placeholders like `{{time}}` must be preserved exactly as-is —
i18next will substitute them at runtime.

### 3. Register the new locale in `i18n.ts`

Open `dashboard/src/i18n.ts` and add an import + resource entry:

```ts
import en from "./locales/en.json";
import fr from "./locales/fr.json"; // ← add this

i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
    fr: { translation: fr }, // ← add this
  },
  // ...
});
```

### 4. Verify

Run the test suite to make sure no keys are missing:

```bash
cd dashboard
npm test
```

Then start the dev server and test in your browser by setting the browser
language to the new locale (or temporarily hard-code `lng: "fr"` in `i18n.ts`).

---

## How locale detection works

`i18n.ts` initialises i18next with:

```ts
lng: navigator.language.split("-")[0]   // e.g. "fr" from "fr-FR"
fallbackLng: "en"
```

If the browser reports a language that has no matching resource, i18next falls
back to English automatically.

---

## Number and date formatting

Numbers and dates are formatted using `Intl.NumberFormat` and
`Intl.DateTimeFormat` respectively, seeded with `i18n.language`. You do **not**
need to provide any numeric or date translations — they adapt automatically
once the locale is registered.

| Old (hardcoded) | New (Intl-aware) |
|---|---|
| `(stroops / XLM).toFixed(2)` | `new Intl.NumberFormat(locale, { minimumFractionDigits: 2 }).format(stroops / XLM)` |
| `new Date(ts * 1000).toLocaleDateString()` | `new Intl.DateTimeFormat(locale, { year: "numeric", month: "short", day: "numeric" }).format(...)` |

---

## File layout reference

```
dashboard/src/
├── i18n.ts                  ← initialisation; register new locales here
├── locales/
│   ├── en.json              ← English (source of truth / baseline)
│   └── <lang>.json          ← your new locale file goes here
└── ...
```

---

## Checklist

- [ ] `dashboard/src/locales/<lang>.json` created with all keys translated
- [ ] Import added in `dashboard/src/i18n.ts`
- [ ] `resources` object in `i18n.ts` updated
- [ ] `npm test` passes with no missing-key warnings
- [ ] Manual smoke-test with browser locale set to new language
