import "@testing-library/jest-dom";

// Recharts' ResponsiveContainer uses ResizeObserver, which jsdom doesn't implement.
globalThis.ResizeObserver = class ResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
};
