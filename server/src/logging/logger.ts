/**
 * Minimal structured logger: one JSON object per line, filtered by LOG_LEVEL
 * (debug | info | warn | error, default info).
 */

type Level = "debug" | "info" | "warn" | "error";

const ORDER: Record<Level, number> = { debug: 10, info: 20, warn: 30, error: 40 };

export type LogFields = Record<string, unknown>;

function activeLevel(): Level {
  const raw = (process.env.LOG_LEVEL ?? "info").toLowerCase();
  return raw in ORDER ? (raw as Level) : "info";
}

function write(level: Level, message: string, fields: LogFields = {}): void {
  if (ORDER[level] < ORDER[activeLevel()]) return;
  const line = JSON.stringify({ time: new Date().toISOString(), level, message, ...fields });
  if (level === "warn" || level === "error") {
    console.error(line);
  } else {
    console.log(line);
  }
}

export const logger = {
  debug: (message: string, fields?: LogFields) => write("debug", message, fields),
  info: (message: string, fields?: LogFields) => write("info", message, fields),
  warn: (message: string, fields?: LogFields) => write("warn", message, fields),
  error: (message: string, fields?: LogFields) => write("error", message, fields),
};
