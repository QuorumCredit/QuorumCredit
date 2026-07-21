import { defineConfig } from "vitest/config";

// Separate config: multi-instance tests spawn real child processes and a test-only
// relay server, so they run in a single worker with a longer timeout and are kept
// out of the default `npm test` run (which should stay fast and hermetic).
export default defineConfig({
  test: {
    environment: "node",
    include: ["test/multi-instance/**/*.test.ts"],
    testTimeout: 30000,
    hookTimeout: 30000,
    fileParallelism: false,
  },
});
