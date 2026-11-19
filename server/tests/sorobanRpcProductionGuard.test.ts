import { describe, it, expect } from "vitest";
import {
  assertSorobanRpcConfigured,
  buildSorobanRpcClient,
  NoOpRpcClient,
  NOOP_RPC_CLIENT_METRIC,
} from "../src/soroban/rpcClient.js";
import { metrics } from "../src/http/metricsRegistry.js";

const URL_ = "https://soroban-testnet.stellar.org";
const CONTRACT = "CDUMMYCONTRACTIDFORTESTSONLYXXXXXXXXXXXXXXXXXXXXXXXXXXXX";
const KEY = "SDUMMYKEYFORTESTSONLYXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX";

describe("Soroban RPC production guard (issue #2007)", () => {
  it("refuses to start in production when SOROBAN_RPC_URL is missing", () => {
    expect(() => assertSorobanRpcConfigured(undefined, CONTRACT, KEY, "production")).toThrow(
      /SOROBAN_RPC_URL/
    );
  });

  it("refuses to start in production when the keeper key is missing", () => {
    expect(() => assertSorobanRpcConfigured(URL_, CONTRACT, undefined, "production")).toThrow(
      /SOROBAN_KEEPER_SECRET_KEY/
    );
  });

  it("lists every missing setting in one error", () => {
    expect(() => buildSorobanRpcClient(undefined, undefined, undefined, "production")).toThrow(
      /SOROBAN_RPC_URL.*QUORUM_CREDIT_CONTRACT_ID.*SOROBAN_KEEPER_SECRET_KEY/
    );
  });

  it("does not throw in production when all settings are present", () => {
    expect(() => assertSorobanRpcConfigured(URL_, CONTRACT, KEY, "production")).not.toThrow();
  });

  it("allows the no-op client outside production", () => {
    expect(() => assertSorobanRpcConfigured(undefined, undefined, undefined, "development")).not.toThrow();
    expect(buildSorobanRpcClient(undefined, undefined, undefined, "development")).toBeInstanceOf(
      NoOpRpcClient
    );
  });
});

describe("Soroban RPC no-op metric (issue #2007)", () => {
  it("emits a counter when the no-op client is built and used", async () => {
    const before = countOf(metrics.toPrometheusText(), NOOP_RPC_CLIENT_METRIC);
    const client = buildSorobanRpcClient(undefined, undefined, undefined, "development");
    const result = await client.executeRecurringPayment("GABC");
    expect(result.ok).toBe(false);
    const after = countOf(metrics.toPrometheusText(), NOOP_RPC_CLIENT_METRIC);
    expect(after).toBe(before + 2);
  });
});

function countOf(text: string, name: string): number {
  const line = text.split("\n").find((l) => l.startsWith(`${name} `));
  return line ? Number(line.split(" ")[1]) : 0;
}
