// Mock the entire stellar-sdk to avoid ESM/CJS incompatibility in jest
jest.mock('@stellar/stellar-sdk', () => {
  const mockKeypair = (pub: string) => ({
    publicKey: () => pub,
    sign: jest.fn(),
  });
  return {
    Keypair: {
      random: () => mockKeypair('GRANDOM000000000000000000000000000000000000000000000000'),
      fromSecret: (s: string) => mockKeypair('GPUBLIC' + s.slice(1, 10)),
    },
    Account: jest.fn().mockImplementation(() => ({})),
    Networks: { TESTNET: 'Test SDF Network ; September 2015' },
    TransactionBuilder: jest.fn().mockImplementation(() => ({
      addOperation: jest.fn().mockReturnThis(),
      setTimeout: jest.fn().mockReturnThis(),
      build: jest.fn().mockReturnValue({ sign: jest.fn() }),
    })),
    BASE_FEE: '100',
    rpc: {
      Server: jest.fn().mockImplementation(() => ({
        getAccount: jest.fn().mockResolvedValue({}),
        sendTransaction: jest.fn().mockResolvedValue({ hash: 'testhash' }),
        simulateTransaction: jest.fn().mockResolvedValue({ result: undefined }),
      })),
    },
    Contract: jest.fn().mockImplementation(() => ({
      call: jest.fn().mockReturnValue({}),
    })),
    nativeToScVal: jest.fn().mockReturnValue({}),
    scValToNative: jest.fn().mockReturnValue(null),
  };
});

import {
  QuorumCreditClient,
  xlmToStroops,
  stroopsToXlm,
  ClientConfig,
} from './client';

const TX_HASH = 'testhash';
const CONTRACT = 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4';
const BORROWER = 'GBBORROWER11111111111111111111111111111111111111111111111';
const VOUCHER = 'GBVOUCHER111111111111111111111111111111111111111111111111';
const ADMIN = 'GBADMIN1111111111111111111111111111111111111111111111111';
const TOKEN = CONTRACT;
const NETWORK = 'Test SDF Network ; September 2015';

function makeClient() {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const { Keypair } = require('@stellar/stellar-sdk');
  const keypair = Keypair.random();
  const config: ClientConfig = {
    contractId: CONTRACT,
    rpcUrl: 'https://soroban-testnet.stellar.org:443',
    networkPassphrase: NETWORK,
    keypair,
  };
  return new QuorumCreditClient(config);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

describe('xlmToStroops', () => {
  it('converts whole XLM', () => expect(xlmToStroops(1)).toBe(10_000_000n));
  it('converts 100 XLM', () => expect(xlmToStroops(100)).toBe(1_000_000_000n));
  it('converts fractional', () => expect(xlmToStroops(0.1)).toBe(1_000_000n));
  it('minimum stroop', () => expect(xlmToStroops(0.0000001)).toBe(1n));
});

describe('stroopsToXlm', () => {
  it('string input', () => expect(stroopsToXlm('10000000')).toBe(1));
  it('bigint input', () => expect(stroopsToXlm(1_000_000_000n)).toBe(100));
  it('minimum', () => expect(stroopsToXlm('1')).toBeCloseTo(1e-7));
});

// ── Constructor ───────────────────────────────────────────────────────────────

describe('QuorumCreditClient constructor', () => {
  it('instantiates without throwing', () => {
    expect(() => makeClient()).not.toThrow();
  });
});

// ── Write methods ─────────────────────────────────────────────────────────────

describe('write methods', () => {
  let client: QuorumCreditClient;
  let sendTransaction: jest.Mock;

  beforeEach(() => {
    client = makeClient();
    sendTransaction = jest.fn().mockResolvedValue({ hash: TX_HASH });
    // Patch the private rpc server
    (client as unknown as Record<string, { sendTransaction: jest.Mock; getAccount: jest.Mock }>)
      ['sorobanRpc'].sendTransaction = sendTransaction;
    (client as unknown as Record<string, { getAccount: jest.Mock }>)
      ['sorobanRpc'].getAccount = jest.fn().mockResolvedValue({});
  });

  it('vouch', async () => {
    expect(await client.vouch({ voucher: VOUCHER, borrower: BORROWER, stake: '1000000000', token: TOKEN })).toBe(TX_HASH);
    expect(sendTransaction).toHaveBeenCalledTimes(1);
  });

  it('batchVouch', async () => {
    expect(await client.batchVouch({ voucher: VOUCHER, borrowers: [BORROWER], stakes: ['500000000'], token: TOKEN })).toBe(TX_HASH);
  });

  it('increaseStake', async () => {
    expect(await client.increaseStake({ voucher: VOUCHER, borrower: BORROWER, additionalStake: '100000000', token: TOKEN })).toBe(TX_HASH);
  });

  it('decreaseStake', async () => {
    expect(await client.decreaseStake({ voucher: VOUCHER, borrower: BORROWER, reducedStake: '500000000', token: TOKEN })).toBe(TX_HASH);
  });

  it('withdrawVouch', async () => {
    expect(await client.withdrawVouch({ voucher: VOUCHER, borrower: BORROWER, token: TOKEN })).toBe(TX_HASH);
  });

  it('requestLoan', async () => {
    expect(await client.requestLoan({ borrower: BORROWER, amount: '500000000', threshold: '1000000000', loanPurpose: 'test', token: TOKEN })).toBe(TX_HASH);
  });

  it('repay', async () => {
    expect(await client.repay({ borrower: BORROWER, payment: '510000000' })).toBe(TX_HASH);
  });

  it('slash', async () => {
    expect(await client.slash({ adminSigners: [ADMIN], borrower: BORROWER })).toBe(TX_HASH);
  });

  it('voteSlash', async () => {
    expect(await client.voteSlash({ voucher: VOUCHER, borrower: BORROWER, approve: true })).toBe(TX_HASH);
  });

  it('executeSlashVote', async () => {
    expect(await client.executeSlashVote(BORROWER)).toBe(TX_HASH);
  });

  it('pause', async () => {
    expect(await client.pause({ adminSigners: [ADMIN] })).toBe(TX_HASH);
  });

  it('unpause', async () => {
    expect(await client.unpause({ adminSigners: [ADMIN] })).toBe(TX_HASH);
  });

  it('updateConfig yieldBps only', async () => {
    expect(await client.updateConfig({ adminSigners: [ADMIN], yieldBps: '300' })).toBe(TX_HASH);
  });

  it('updateConfig both params', async () => {
    expect(await client.updateConfig({ adminSigners: [ADMIN], yieldBps: '300', slashBps: '5000' })).toBe(TX_HASH);
  });

  it('throws on errorResult', async () => {
    sendTransaction.mockResolvedValueOnce({ errorResult: { code: 1 }, hash: '' });
    await expect(client.vouch({ voucher: VOUCHER, borrower: BORROWER, stake: '100', token: TOKEN })).rejects.toThrow('Transaction failed');
  });
});

// ── Read methods ──────────────────────────────────────────────────────────────

describe('read methods', () => {
  let client: QuorumCreditClient;
  let simulateTransaction: jest.Mock;

  beforeEach(() => {
    client = makeClient();
    simulateTransaction = jest.fn().mockResolvedValue({ result: undefined });
    (client as unknown as Record<string, { simulateTransaction: jest.Mock }>)
      ['sorobanRpc'].simulateTransaction = simulateTransaction;
  });

  it('getLoan returns null on error', async () => {
    simulateTransaction.mockResolvedValue({ error: 'not found' });
    expect(await client.getLoan(BORROWER)).toBeNull();
  });

  it('getVouches returns [] on error', async () => {
    simulateTransaction.mockResolvedValue({ error: 'not found' });
    expect(await client.getVouches(BORROWER)).toEqual([]);
  });

  it('isEligible returns false on error', async () => {
    simulateTransaction.mockResolvedValue({ error: 'not found' });
    expect(await client.isEligible(BORROWER, '1000000000', TOKEN)).toBe(false);
  });

  it('loanStatus returns None when no result', async () => {
    expect(await client.loanStatus(BORROWER)).toBe('None');
  });

  it('getAdmins returns [] when no result', async () => {
    expect(await client.getAdmins()).toEqual([]);
  });

  it('totalVouched returns "0" when no result', async () => {
    expect(await client.totalVouched(BORROWER)).toBe('0');
  });

  it('getFeeTreasury returns "0" when no result', async () => {
    expect(await client.getFeeTreasury()).toBe('0');
  });

  it('getConfig throws when simulate fails', async () => {
    simulateTransaction.mockResolvedValue({ error: 'rpc error' });
    await expect(client.getConfig()).rejects.toThrow('Failed to fetch config');
  });
});
