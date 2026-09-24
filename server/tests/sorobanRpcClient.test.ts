import { describe, it, expect, vi, beforeEach } from "vitest";
import { SorobanRpcClientImpl, NoOpRpcClient, buildSorobanRpcClient } from "../../src/soroban/rpcClient.js";

// Mock the stellar-sdk to avoid network calls in unit tests.
vi.mock("@stellar/stellar-sdk", async () => {
  const actual = await vi.importActual("@stellar/stellar-sdk");
  return {
    ...actual,
    Server: vi.fn().mockImplementation(() => ({
      getAccount: vi.fn().mockResolvedValue({ sequence: "123456789" }),
      simulateTransaction: vi.fn().mockResolvedValue({}),
      submitTransaction: vi.fn().mockResolvedValue({ status: "success", hash: "abc123" }),
      getTransaction: vi.fn().mockResolvedValue({ status: "success" }),
      close: vi.fn().mockResolvedValue(undefined),
    })),
    Keypair: {
      fromSecret: vi.fn().mockReturnValue({ publicKey: () => "G..." }),
    },
    Network: {
      parseNetworkPassphraseFromUrl: vi.fn().mockReturnValue("Test SDF Network ; September 2015"),
      PUBLIC: "Public Global Stellar Network ; September 2015",
    },
    TransactionBuilder: vi.fn().mockImplementation(() => ({
      addOperation: vi.fn().mockReturnThis(),
      setBaseFee: vi.fn().mockReturnThis(),
      setTimeout: vi.fn().mockReturnThis(),
      build: vi.fn().mockReturnValue({}),
    })),
    Operation: {
      invokeHostFunction: vi.fn().mockReturnValue({}),
    },
  };
});

describe("SorobanRpcClientImpl", () => {
  let client: SorobanRpcClientImpl;

  beforeEach(() => {
    client = new SorobanRpcClientImpl(
      "https://soroban-testnet.stellar.org",
      "CContractId",
      "SSecretKey"
    );
  });

  it("returns ok:true on successful execution", async () => {
    const result = await client.executeRecurringPayment("GBorrowerAddress");
    expect(result.ok).toBe(true);
    expect(result.txHash).toBe("abc123");
  });

  it("returns ok:false for invalid borrower address", async () => {
    const result = await client.executeRecurringPayment("not-a-stellar-address");
    expect(result.ok).toBe(false);
    expect(result.error).toContain("invalid loanId");
  });

  it("returns ok:false when RPC URL is missing", async () => {
    const stub = buildSorobanRpcClient(undefined, undefined, undefined);
    const result = await stub.executeRecurringPayment("GBorrowerAddress");
    expect(result.ok).toBe(false);
    expect(result.error).toBe("RPC not configured");
  });

  it("closes the server connection", async () => {
    await expect(client.close()).resolves.toBeUndefined();
  });
});
