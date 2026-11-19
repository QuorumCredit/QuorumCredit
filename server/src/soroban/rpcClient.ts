/**
 * Soroban RPC client for invoking QuorumCreditContract functions.
 * Issue #1362: Wire up on-chain recurring payment execution.
 *
 * This client wraps @stellar/stellar-sdk's SorobanRpc for a simplified API.
 * In production, SOROBAN_RPC_URL must point to the network's RPC endpoint.
 * For local dev/test, the URL can be omitted to return a stub (no-op).
 */

import {
  Address,
  Keypair,
  Network,
  Operation,
  Server,
  TransactionBuilder,
  type InvokeHostFunctionArgs,
} from "@stellar/stellar-sdk";
import { metrics } from "../http/metricsRegistry.js";

/** Incremented every time the no-op RPC client is used (issue #2007). */
export const NOOP_RPC_CLIENT_METRIC = "qc_soroban_noop_rpc_client_total";

export interface ExecuteRecurringPaymentResult {
  ok: boolean;
  amount?: number;
  error?: string;
  txHash?: string;
}

export interface SorobanRpcClient {
  executeRecurringPayment(loanId: string): Promise<ExecuteRecurringPaymentResult>;
  close(): Promise<void>;
}

// ── Stub (no-op) implementation for dev/test without RPC ──────────────────

export class NoOpRpcClient implements SorobanRpcClient {
  async executeRecurringPayment(): Promise<ExecuteRecurringPaymentResult> {
    metrics.incCounter(NOOP_RPC_CLIENT_METRIC);
    console.warn(
      "[quorum-credit] SOROBAN_RPC_URL not configured — recurring payment execution is a no-op"
    );
    return { ok: false, error: "RPC not configured" };
  }

  async close(): Promise<void> {
    // No-op
  }
}

// ── Real Soroban RPC implementation ────────────────────────────────────────

export class SorobanRpcClientImpl implements SorobanRpcClient {
  private readonly server: Server;
  private readonly contractId: string;
  private readonly keeperKeypair: Keypair;
  private readonly networkPassphrase: string;

  constructor(rpcUrl: string, contractId: string, keeperSecretKey: string) {
    this.server = new Server(rpcUrl);
    this.contractId = contractId;
    this.keeperKeypair = Keypair.fromSecret(keeperSecretKey);
    this.networkPassphrase = Network.parseNetworkPassphraseFromUrl(rpcUrl) ?? Network.PUBLIC;
  }

  /**
   * Invoke execute_recurring_payment on the QuorumCreditContract.
   *
   * Steps:
   * 1. Derive the borrower's Stellar address from loanId.
   *    In the current system, loanId is the borrower's Stellar address string
   *    extracted from the URL path. If the format does not match a valid
   *    Stellar address, the call returns an error immediately.
   * 2. Build a Soroban invoke transaction for execute_recurring_payment(borrower).
   * 3. Sign the transaction with the keeper's keypair.
   * 4. Submit to the network via submitTransaction.
   * 5. Poll for confirmation.
   * 6. Return the transaction hash and on-chain result.
   */
  async executeRecurringPayment(loanId: string): Promise<ExecuteRecurringPaymentResult> {
    try {
      // 1. Derive borrower address from loanId.
      // loanId is the borrower's Stellar address string in the URL path.
      let borrowerAddress: Address;
      try {
        borrowerAddress = new Address(loanId);
      } catch {
        return {
          ok: false,
          error: `invalid loanId / borrower address format: ${loanId}`,
        };
      }

      // 2. Load the keeper account to fetch the current sequence number.
      const keeperAccount = await this.server.getAccount(this.keeperKeypair.publicKey());
      const sequence = parseInt(keeperAccount.sequence, 10);

      // 3. Build the Soroban invoke operation.
      const invokeArgs: InvokeHostFunctionArgs = {
        type: "invokeHostFunction",
        invokeHostFunction: {
          type: 1,
          contractId: this.contractId,
          functionName: "execute_recurring_payment",
          args: [borrowerAddress.toScVal()],
        },
      };

      const operation = Operation.invokeHostFunction(invokeArgs);

      // 4. Build, sign, and simulate the transaction.
      const transaction = new TransactionBuilder(keeperAccount, this.networkPassphrase)
        .addOperation(operation)
        .setBaseFee(100000)
        .setTimeout(30)
        .build();

      transaction.sign(this.keeperKeypair);

      const simulateResponse = await this.server.simulateTransaction(transaction);

      if (simulateResponse.result) {
        return {
          ok: false,
          error: `simulation failed: ${JSON.stringify(simulateResponse.result)}`,
        };
      }

      if (simulateResponse.error) {
        return {
          ok: false,
          error: `simulation error: ${JSON.stringify(simulateResponse.error)}`,
        };
      }

      // 5. Submit the transaction.
      const submitResponse = await this.server.submitTransaction(transaction);

      if (submitResponse.status === "ERROR") {
        const errorResult = submitResponse.result as any;
        return {
          ok: false,
          error: `submission failed: ${errorResult?.error ?? "unknown"}`,
        };
      }

      const txHash = submitResponse.hash ?? "";

      // 6. Poll for confirmation (up to ~30 seconds).
      const startTime = Date.now();
      const timeoutMs = 30_000;
      const pollIntervalMs = 1_000;

      while (Date.now() - startTime < timeoutMs) {
        try {
          const txResult = await this.server.getTransaction(txHash);
          if (txResult.status === "success") {
            return {
              ok: true,
              txHash,
            };
          }
          if (txResult.status === "failed" || txResult.status === "timeout") {
            return {
              ok: false,
              error: `transaction ${txResult.status}: ${txHash}`,
              txHash,
            };
          }
        } catch {
          // Transaction might not be indexed yet; retry.
        }
        await new Promise((resolve) => setTimeout(resolve, pollIntervalMs));
      }

      return {
        ok: false,
        error: "transaction confirmation timeout",
        txHash,
      };
    } catch (err) {
      const error = err instanceof Error ? err.message : String(err);
      console.error(`[quorum-credit] execute_recurring_payment failed: ${error}`);
      return { ok: false, error };
    }
  }

  async close(): Promise<void> {
    await this.server.close();
  }
}

// ── Factory ────────────────────────────────────────────────────────────────

/**
 * Fail-fast guard (issue #2007): in production, recurring payments must never
 * silently no-op. Throws when any required Soroban setting is missing.
 * Outside production this is a no-op so local dev and tests keep working.
 */
export function assertSorobanRpcConfigured(
  rpcUrl: string | undefined,
  contractId: string | undefined,
  keeperSecretKey: string | undefined,
  nodeEnv: string | undefined = process.env.NODE_ENV
): void {
  if (nodeEnv !== "production") return;

  const missing: string[] = [];
  if (!rpcUrl) missing.push("SOROBAN_RPC_URL");
  if (!contractId) missing.push("QUORUM_CREDIT_CONTRACT_ID");
  if (!keeperSecretKey) missing.push("SOROBAN_KEEPER_SECRET_KEY");

  if (missing.length > 0) {
    throw new Error(
      "[quorum-credit] FATAL — NODE_ENV=production requires Soroban RPC configuration; missing: " +
        missing.join(", ")
    );
  }
}

export function buildSorobanRpcClient(
  rpcUrl: string | undefined,
  contractId: string | undefined,
  keeperSecretKey: string | undefined,
  nodeEnv: string | undefined = process.env.NODE_ENV
): SorobanRpcClient {
  assertSorobanRpcConfigured(rpcUrl, contractId, keeperSecretKey, nodeEnv);
  if (!rpcUrl || !contractId || !keeperSecretKey) {
    metrics.incCounter(NOOP_RPC_CLIENT_METRIC);
    return new NoOpRpcClient();
  }
  return new SorobanRpcClientImpl(rpcUrl, contractId, keeperSecretKey);
}
