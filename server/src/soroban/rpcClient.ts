/**
 * Soroban RPC client for invoking QuorumCreditContract functions.
 * Issue #1362: Wire up on-chain recurring payment execution.
 *
 * This client wraps @stellar/stellar-sdk's SorobanRpc for a simplified API.
 * In production, SOROBAN_RPC_URL must point to the network's RPC endpoint.
 * For local dev/test, the URL can be omitted to return a stub (no-op).
 */

// TODO: SorobanRpc will be used when chain client wiring is implemented (issue #1322/#1356)
// eslint-disable-next-line @typescript-eslint/no-unused-vars
import { SorobanRpc } from "@stellar/stellar-sdk";

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

// eslint-disable-next-line @typescript-eslint/no-unused-vars
export class SorobanRpcClientImpl implements SorobanRpcClient {
  constructor(rpcUrl: string, contractId: string) {
    // TODO: Use rpcUrl and contractId when chain client wiring is implemented.
    void rpcUrl;
    void contractId;
  }

  /**
   * Invoke execute_recurring_payment on the QuorumCreditContract.
   * Returns success/failure with transaction hash on success.
   * Note: loanId is currently just a string for logging; the actual borrower
   * address derivation and on-chain execution requires full chain client wiring.
   */
  async executeRecurringPayment(loanId: string): Promise<ExecuteRecurringPaymentResult> {
    try {
      // TODO: Once chain client wiring is added (issue #1322 / #1356), this would:
      // 1. Derive the borrower's Stellar address from loanId
      // 2. Build a Soroban invoke operation for execute_recurring_payment(borrower)
      // 3. Build a transaction with the invoke operation
      // 4. Sign it with the keeper's keypair
      // 5. Submit to the network via submitTransaction
      // 6. Poll for confirmation
      // 7. Return the transaction hash and on-chain result

      // For now, stub behavior: the real implementation requires the full chain
      // client stack (keypair, network selection, address derivation) that is out
      // of scope for this immediate fix.
      console.debug(`[quorum-credit] would execute_recurring_payment for loan=${loanId}`);
      return {
        ok: false,
        error: "chain client wiring not yet implemented (see issue #1322/#1356)",
      };
    } catch (err) {
      const error = err instanceof Error ? err.message : String(err);
      console.error(`[quorum-credit] execute_recurring_payment failed: ${error}`);
      return { ok: false, error };
    }
  }

  async close(): Promise<void> {
    // No explicit close needed for SorobanRpc.Server, but included for interface compliance
  }
}

// ── Factory ────────────────────────────────────────────────────────────────

export function buildSorobanRpcClient(
  rpcUrl: string | undefined,
  contractId: string | undefined
): SorobanRpcClient {
  if (!rpcUrl || !contractId) {
    return new NoOpRpcClient();
  }
  return new SorobanRpcClientImpl(rpcUrl, contractId);
}
