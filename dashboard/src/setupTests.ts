import "@testing-library/jest-dom";
// Initialise i18next before any component renders in tests.
// This ensures useTranslation() resolves immediately instead of suspending.
import "./i18n";

// Recharts' ResponsiveContainer uses ResizeObserver, which jsdom doesn't implement.
globalThis.ResizeObserver = class ResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
};
