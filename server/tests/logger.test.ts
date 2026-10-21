import { describe, it, expect, afterEach, vi } from "vitest";
import { logger } from "../src/logging/logger.js";

describe("structured logger", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.restoreAllMocks();
  });

  it("suppresses info entries when LOG_LEVEL is error", () => {
    vi.stubEnv("LOG_LEVEL", "error");
    const logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
    logger.info("should be hidden");
    expect(logSpy).not.toHaveBeenCalled();
  });

  it("writes one JSON object per entry at the default level", () => {
    vi.stubEnv("LOG_LEVEL", "");
    const logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
    logger.info("listening", { port: 4000 });
    const entry = JSON.parse(String(logSpy.mock.calls[0]![0]));
    expect(entry).toMatchObject({ level: "info", message: "listening", port: 4000 });
  });
});
