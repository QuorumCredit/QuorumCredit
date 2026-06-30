import {
  Keypair,
  Account,
  Networks,
  TransactionBuilder,
  BASE_FEE,
  rpc,
  Contract,
  nativeToScVal,
  scValToNative,
} from '@stellar/stellar-sdk';

export interface ClientConfig {
  contractId: string;
  rpcUrl: string;
  networkPassphrase: string;
  keypair: Keypair;
}

export interface VouchParams {
  voucher: string;
  borrower: string;
  stake: string;
  token: string;
}

export interface BatchVouchParams {
  voucher: string;
  borrowers: string[];
  stakes: string[];
  token: string;
}

export interface IncreaseStakeParams {
  voucher: string;
  borrower: string;
  additionalStake: string;
  token: string;
}

export interface DecreaseStakeParams {
  voucher: string;
  borrower: string;
  reducedStake: string;
  token: string;
}

export interface WithdrawVouchParams {
  voucher: string;
  borrower: string;
  token: string;
}

export interface RequestLoanParams {
  borrower: string;
  amount: string;
  threshold: string;
  loanPurpose: string;
  token: string;
}

export interface RepayParams {
  borrower: string;
  payment: string;
}

export interface SlashParams {
  adminSigners: string[];
  borrower: string;
}

export interface AdminParams {
  adminSigners: string[];
}

export interface UpdateConfigParams {
  adminSigners: string[];
  yieldBps?: string | null;
  slashBps?: string | null;
}

export interface VoteSlashParams {
  voucher: string;
  borrower: string;
  approve: boolean;
}

export interface LoanRecord {
  id: string;
  borrower: string;
  amount: string;
  amountRepaid: string;
  totalYield: string;
  status: 'Active' | 'Repaid' | 'Defaulted';
  createdAt: number;
  deadline: number;
  loanPurpose: string;
}

export interface VouchRecord {
  voucher: string;
  stake: string;
  vouchTimestamp: number;
  token: string;
}

export interface Config {
  admins: string[];
  adminThreshold: number;
  token: string;
  allowedTokens: string[];
  yieldBps: number;
  slashBps: number;
  minLoanAmount: string;
  maxLoanAmount: string;
  loanDuration: number;
}

export type LoanStatus = 'None' | 'Active' | 'Repaid' | 'Defaulted';

/** Convert XLM to stroops (bigint). */
export const xlmToStroops = (xlm: number): bigint =>
  BigInt(Math.round(xlm * 10_000_000));

/** Convert stroops to XLM. */
export const stroopsToXlm = (stroops: string | bigint): number =>
  Number(stroops) / 10_000_000;

export class QuorumCreditClient {
  private config: ClientConfig;
  private sorobanRpc: rpc.Server;
  private contract: Contract;

  constructor(config: ClientConfig) {
    this.config = config;
    this.sorobanRpc = new rpc.Server(config.rpcUrl);
    this.contract = new Contract(config.contractId);
  }

  // ── Initialization ─────────────────────────────────────────────────────────

  async initialize(
    deployer: string,
    admins: string[],
    adminThreshold: number,
    token: string
  ): Promise<string> {
    return this.submitCall(
      'initialize',
      nativeToScVal(deployer, { type: 'address' }),
      nativeToScVal(admins),
      nativeToScVal(adminThreshold, { type: 'u32' }),
      nativeToScVal(token, { type: 'address' })
    );
  }

  // ── Vouching ───────────────────────────────────────────────────────────────

  async vouch(params: VouchParams): Promise<string> {
    return this.submitCall(
      'vouch',
      nativeToScVal(params.voucher, { type: 'address' }),
      nativeToScVal(params.borrower, { type: 'address' }),
      nativeToScVal(params.stake, { type: 'i128' }),
      nativeToScVal(params.token, { type: 'address' })
    );
  }

  async batchVouch(params: BatchVouchParams): Promise<string> {
    return this.submitCall(
      'batch_vouch',
      nativeToScVal(params.voucher, { type: 'address' }),
      nativeToScVal(params.borrowers),
      nativeToScVal(params.stakes),
      nativeToScVal(params.token, { type: 'address' })
    );
  }

  async increaseStake(params: IncreaseStakeParams): Promise<string> {
    return this.submitCall(
      'increase_stake',
      nativeToScVal(params.voucher, { type: 'address' }),
      nativeToScVal(params.borrower, { type: 'address' }),
      nativeToScVal(params.additionalStake, { type: 'i128' }),
      nativeToScVal(params.token, { type: 'address' })
    );
  }

  async decreaseStake(params: DecreaseStakeParams): Promise<string> {
    return this.submitCall(
      'decrease_stake',
      nativeToScVal(params.voucher, { type: 'address' }),
      nativeToScVal(params.borrower, { type: 'address' }),
      nativeToScVal(params.reducedStake, { type: 'i128' }),
      nativeToScVal(params.token, { type: 'address' })
    );
  }

  async withdrawVouch(params: WithdrawVouchParams): Promise<string> {
    return this.submitCall(
      'withdraw_vouch',
      nativeToScVal(params.voucher, { type: 'address' }),
      nativeToScVal(params.borrower, { type: 'address' }),
      nativeToScVal(params.token, { type: 'address' })
    );
  }

  // ── Loans ──────────────────────────────────────────────────────────────────

  async requestLoan(params: RequestLoanParams): Promise<string> {
    return this.submitCall(
      'request_loan',
      nativeToScVal(params.borrower, { type: 'address' }),
      nativeToScVal(params.amount, { type: 'i128' }),
      nativeToScVal(params.threshold, { type: 'i128' }),
      nativeToScVal(params.loanPurpose, { type: 'string' }),
      nativeToScVal(params.token, { type: 'address' })
    );
  }

  async repay(params: RepayParams): Promise<string> {
    return this.submitCall(
      'repay',
      nativeToScVal(params.borrower, { type: 'address' }),
      nativeToScVal(params.payment, { type: 'i128' })
    );
  }

  // ── Governance ─────────────────────────────────────────────────────────────

  async slash(params: SlashParams): Promise<string> {
    return this.submitCall(
      'slash',
      nativeToScVal(params.adminSigners),
      nativeToScVal(params.borrower, { type: 'address' })
    );
  }

  async voteSlash(params: VoteSlashParams): Promise<string> {
    return this.submitCall(
      'vote_slash',
      nativeToScVal(params.voucher, { type: 'address' }),
      nativeToScVal(params.borrower, { type: 'address' }),
      nativeToScVal(params.approve)
    );
  }

  async executeSlashVote(borrower: string): Promise<string> {
    return this.submitCall(
      'execute_slash_vote',
      nativeToScVal(borrower, { type: 'address' })
    );
  }

  // ── Admin ──────────────────────────────────────────────────────────────────

  async pause(params: AdminParams): Promise<string> {
    return this.submitCall('pause', nativeToScVal(params.adminSigners));
  }

  async unpause(params: AdminParams): Promise<string> {
    return this.submitCall('unpause', nativeToScVal(params.adminSigners));
  }

  async updateConfig(params: UpdateConfigParams): Promise<string> {
    return this.submitCall(
      'update_config',
      nativeToScVal(params.adminSigners),
      params.yieldBps != null
        ? nativeToScVal(params.yieldBps, { type: 'i128' })
        : nativeToScVal(null),
      params.slashBps != null
        ? nativeToScVal(params.slashBps, { type: 'i128' })
        : nativeToScVal(null)
    );
  }

  // ── Queries ────────────────────────────────────────────────────────────────

  async getLoan(borrower: string): Promise<LoanRecord | null> {
    const val = await this.readCall(
      'get_loan',
      nativeToScVal(borrower, { type: 'address' })
    );
    return val ? this.parseLoanRecord(val) : null;
  }

  async getVouches(borrower: string): Promise<VouchRecord[]> {
    const val = await this.readCall(
      'get_vouches',
      nativeToScVal(borrower, { type: 'address' })
    );
    return val ? this.parseVouchRecords(val) : [];
  }

  async isEligible(borrower: string, threshold: string, token: string): Promise<boolean> {
    const val = await this.readCall(
      'is_eligible',
      nativeToScVal(borrower, { type: 'address' }),
      nativeToScVal(threshold, { type: 'i128' }),
      nativeToScVal(token, { type: 'address' })
    );
    return val ? (scValToNative(val) as boolean) : false;
  }

  async getConfig(): Promise<Config> {
    const val = await this.readCall('get_config');
    if (!val) throw new Error('Failed to fetch config');
    return this.parseConfig(val);
  }

  async loanStatus(borrower: string): Promise<LoanStatus> {
    const val = await this.readCall(
      'loan_status',
      nativeToScVal(borrower, { type: 'address' })
    );
    return val ? (scValToNative(val) as LoanStatus) : 'None';
  }

  async getAdmins(): Promise<string[]> {
    const val = await this.readCall('get_admins');
    return val ? (scValToNative(val) as string[]) : [];
  }

  async totalVouched(borrower: string): Promise<string> {
    const val = await this.readCall(
      'total_vouched',
      nativeToScVal(borrower, { type: 'address' })
    );
    return val ? String(scValToNative(val)) : '0';
  }

  async getFeeTreasury(): Promise<string> {
    const val = await this.readCall('get_fee_treasury');
    return val ? String(scValToNative(val)) : '0';
  }

  // ── Internals ──────────────────────────────────────────────────────────────

  private async submitCall(...args: Parameters<Contract['call']>): Promise<string> {
    const account = await this.sorobanRpc.getAccount(this.config.keypair.publicKey());
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: this.config.networkPassphrase,
    })
      .addOperation(this.contract.call(...args))
      .setTimeout(30)
      .build();

    tx.sign(this.config.keypair);
    const result = await this.sorobanRpc.sendTransaction(tx);
    if (result.errorResult) {
      throw new Error(`Transaction failed: ${JSON.stringify(result.errorResult)}`);
    }
    return result.hash;
  }

  private async readCall(...args: Parameters<Contract['call']>): Promise<ReturnType<typeof scValToNative> | null> {
    const account = new Account(this.config.keypair.publicKey(), '0');
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: this.config.networkPassphrase,
    })
      .addOperation(this.contract.call(...args))
      .setTimeout(30)
      .build();

    const result = await this.sorobanRpc.simulateTransaction(tx);
    if ('error' in result && result.error) return null;
    const simResult = result as rpc.Api.SimulateTransactionSuccessResponse;
    return simResult.result?.retval ?? null;
  }

  private parseLoanRecord(scVal: ReturnType<typeof scValToNative>): LoanRecord {
    const native = scValToNative(scVal) as Record<string, unknown>;
    return {
      id: String(native['id']),
      borrower: String(native['borrower']),
      amount: String(native['amount']),
      amountRepaid: String(native['amount_repaid']),
      totalYield: String(native['total_yield']),
      status: native['status'] as LoanRecord['status'],
      createdAt: Number(native['created_at']),
      deadline: Number(native['deadline']),
      loanPurpose: String(native['loan_purpose']),
    };
  }

  private parseVouchRecords(scVal: ReturnType<typeof scValToNative>): VouchRecord[] {
    const native = scValToNative(scVal);
    if (!Array.isArray(native)) return [];
    return (native as Record<string, unknown>[]).map((v) => ({
      voucher: String(v['voucher']),
      stake: String(v['stake']),
      vouchTimestamp: Number(v['vouch_timestamp']),
      token: String(v['token']),
    }));
  }

  private parseConfig(scVal: ReturnType<typeof scValToNative>): Config {
    const native = scValToNative(scVal) as Record<string, unknown>;
    return {
      admins: native['admins'] as string[],
      adminThreshold: Number(native['admin_threshold']),
      token: String(native['token']),
      allowedTokens: (native['allowed_tokens'] as string[]) ?? [],
      yieldBps: Number(native['yield_bps']),
      slashBps: Number(native['slash_bps']),
      minLoanAmount: String(native['min_loan_amount']),
      maxLoanAmount: String(native['max_loan_amount']),
      loanDuration: Number(native['loan_duration']),
    };
  }
}

export default QuorumCreditClient;
