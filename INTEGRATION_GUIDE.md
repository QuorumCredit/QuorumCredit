# QuorumCredit Integration Guide

This guide covers everything needed to integrate off-chain systems — frontends, indexers, bots — with the QuorumCredit smart contract on Stellar Soroban.

---

## Table of Contents

- [Prerequisites](#prerequisites)
- [Querying Contract State](#querying-contract-state)
- [Constructing Transactions](#constructing-transactions)
- [Event Topics and Data Structures](#event-topics-and-data-structures)
- [Error Handling](#error-handling)
- [Common Integration Patterns](#common-integration-patterns)

---

## Prerequisites

You need:
- A Soroban RPC endpoint (testnet: `https://soroban-testnet.stellar.org`, mainnet: `https://rpc.mainnet.stellar.org`)
- The deployed contract ID
- The Stellar JS SDK: `npm install @stellar/stellar-sdk`

```js
import { Contract, SorobanRpc, TransactionBuilder, Networks, BASE_FEE } from '@stellar/stellar-sdk';

const server = new SorobanRpc.Server('https://soroban-testnet.stellar.org');
const CONTRACT_ID = 'C...'; // your deployed contract ID
```

---

## Querying Contract State

All read-only queries use `simulateTransaction` — no fee, no signing required.

### Check if a borrower is eligible

```js
async function isEligible(borrower, thresholdStroops, tokenAddress) {
  const contract = new Contract(CONTRACT_ID);
  const tx = new TransactionBuilder(sourceAccount, { fee: BASE_FEE, networkPassphrase: Networks.TESTNET })
    .addOperation(contract.call('is_eligible',
      nativeToScVal(borrower, { type: 'address' }),
      nativeToScVal(thresholdStroops, { type: 'i128' }),
      nativeToScVal(tokenAddress, { type: 'address' }),
    ))
    .setTimeout(30)
    .build();

  const result = await server.simulateTransaction(tx);
  return scValToNative(result.result.retval); // boolean
}
```

### Get all vouches for a borrower

```js
async function getVouches(borrower) {
  // Returns Option<Vec<VouchRecord>> — null if no vouches exist
  const result = await simulateCall('get_vouches', [
    nativeToScVal(borrower, { type: 'address' }),
  ]);
  if (!result) return [];
  // Each VouchRecord: { voucher: Address, stake: i128, vouch_timestamp: u64, token: Address }
  return scValToNative(result);
}
```

### Get loan status

```js
async function getLoanStatus(borrower) {
  // Returns LoanStatus: 'None' | 'Active' | 'Repaid' | 'Defaulted'
  return simulateCall('loan_status', [nativeToScVal(borrower, { type: 'address' })]);
}
```

### Get protocol config

```js
async function getConfig() {
  // Returns Config struct — see Data Structures section
  return simulateCall('get_config', []);
}
```

---

## Constructing Transactions

State-mutating calls require a signed transaction submitted via `sendTransaction`.

### Vouch for a borrower

```js
async function vouch(voucherKeypair, borrower, stakeStroops, tokenAddress) {
  const contract = new Contract(CONTRACT_ID);
  const account = await server.getAccount(voucherKeypair.publicKey());

  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: Networks.TESTNET })
    .addOperation(contract.call('vouch',
      nativeToScVal(voucherKeypair.publicKey(), { type: 'address' }),
      nativeToScVal(borrower, { type: 'address' }),
      nativeToScVal(stakeStroops, { type: 'i128' }),
      nativeToScVal(tokenAddress, { type: 'address' }),
    ))
    .setTimeout(30)
    .build();

  const prepared = await server.prepareTransaction(tx);
  prepared.sign(voucherKeypair);
  return server.sendTransaction(prepared);
}
```

### Batch vouch (atomic — all or nothing)

`batch_vouch` validates all entries before committing any. If one fails, the entire batch is rejected.

```js
async function batchVouch(voucherKeypair, borrowers, stakesStroops, tokenAddress) {
  const contract = new Contract(CONTRACT_ID);
  const account = await server.getAccount(voucherKeypair.publicKey());

  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: Networks.TESTNET })
    .addOperation(contract.call('batch_vouch',
      nativeToScVal(voucherKeypair.publicKey(), { type: 'address' }),
      nativeToScVal(borrowers, { type: 'array', values: borrowers.map(b => nativeToScVal(b, { type: 'address' })) }),
      nativeToScVal(stakesStroops, { type: 'array', values: stakesStroops.map(s => nativeToScVal(s, { type: 'i128' })) }),
      nativeToScVal(tokenAddress, { type: 'address' }),
    ))
    .setTimeout(30)
    .build();

  const prepared = await server.prepareTransaction(tx);
  prepared.sign(voucherKeypair);
  return server.sendTransaction(prepared);
}
```

### Request a loan

```js
async function requestLoan(borrowerKeypair, amountStroops, thresholdStroops, purpose, tokenAddress) {
  const contract = new Contract(CONTRACT_ID);
  const account = await server.getAccount(borrowerKeypair.publicKey());

  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: Networks.TESTNET })
    .addOperation(contract.call('request_loan',
      nativeToScVal(borrowerKeypair.publicKey(), { type: 'address' }),
      nativeToScVal(amountStroops, { type: 'i128' }),
      nativeToScVal(thresholdStroops, { type: 'i128' }),
      nativeToScVal(purpose, { type: 'string' }),
      nativeToScVal(tokenAddress, { type: 'address' }),
    ))
    .setTimeout(30)
    .build();

  const prepared = await server.prepareTransaction(tx);
  prepared.sign(borrowerKeypair);
  return server.sendTransaction(prepared);
}
```

### Repay a loan

```js
async function repay(borrowerKeypair, paymentStroops) {
  const contract = new Contract(CONTRACT_ID);
  const account = await server.getAccount(borrowerKeypair.publicKey());

  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: Networks.TESTNET })
    .addOperation(contract.call('repay',
      nativeToScVal(borrowerKeypair.publicKey(), { type: 'address' }),
      nativeToScVal(paymentStroops, { type: 'i128' }),
    ))
    .setTimeout(30)
    .build();

  const prepared = await server.prepareTransaction(tx);
  prepared.sign(borrowerKeypair);
  return server.sendTransaction(prepared);
}
```

---

## Event Topics and Data Structures

The contract emits Soroban events on every state change. Subscribe via the Soroban RPC `getEvents` method or Horizon's event stream.

### Fetching events

```js
const events = await server.getEvents({
  startLedger: fromLedger,
  filters: [{
    type: 'contract',
    contractIds: [CONTRACT_ID],
  }],
});
```

### Event reference

| Topic (symbol pair) | Data payload | Trigger |
|---|---|---|
| `["contract", "init"]` | `(deployer, admins, admin_threshold, token)` | `initialize()` |
| `["vouch", "added"]` | `(voucher, borrower, stake, token)` | `vouch()` or `batch_vouch()` per entry |
| `["vouch", "increased"]` | `(voucher, borrower, additional_stake)` | `increase_stake()` |
| `["vouch", "decreased"]` | `(voucher, borrower, reduced_amount)` | `decrease_stake()` |
| `["vouch", "withdrawn"]` | `(voucher, borrower, returned_stake)` | `withdraw_vouch()` |
| `["loan", "request"]` | `(borrower, amount, threshold, loan_purpose, token)` | `request_loan()` |
| `["loan", "repay"]` | `(borrower, payment)` | `repay()` |
| `["loan", "slash"]` | `(borrower, total_slashed)` | `vote_slash()` auto-execute or admin slash |
| `["admin", "pause"]` | `(admin)` | `pause()` |
| `["admin", "unpause"]` | `(admin)` | `unpause()` |
| `["admin", "config"]` | `(admin, config)` | `set_config()` / `update_config()` |

### Parsing a vouch event

```js
function parseVouchEvent(event) {
  const [voucher, borrower, stake, token] = event.value.map(scValToNative);
  return {
    voucher,   // string (Stellar address)
    borrower,  // string
    stake,     // bigint (stroops)
    token,     // string (contract address)
    xlm: Number(stake) / 10_000_000,
  };
}
```

---

## Data Structures

All amounts are in stroops. 1 XLM = 10,000,000 stroops.

### VouchRecord

```ts
interface VouchRecord {
  voucher: string;         // Stellar address
  stake: bigint;           // stroops
  vouch_timestamp: bigint; // Unix timestamp (seconds)
  token: string;           // token contract address
}
```

### LoanRecord

```ts
interface LoanRecord {
  id: bigint;
  borrower: string;
  amount: bigint;           // principal in stroops
  amount_repaid: bigint;    // cumulative repayment in stroops
  total_yield: bigint;      // yield locked at disbursement in stroops
  status: 'None' | 'Active' | 'Repaid' | 'Defaulted';
  created_at: bigint;       // Unix timestamp
  deadline: bigint;         // Unix timestamp
  loan_purpose: string;
  token_address: string;
}
```

### Config

```ts
interface Config {
  admins: string[];
  admin_threshold: number;
  token: string;
  allowed_tokens: string[];
  yield_bps: bigint;             // e.g. 200 = 2%
  slash_bps: bigint;             // e.g. 5000 = 50%
  max_vouchers: number;
  min_loan_amount: bigint;       // stroops
  loan_duration: bigint;         // seconds
  max_loan_to_stake_ratio: number; // e.g. 150 = 150%
  grace_period: bigint;          // seconds
}
```

---

## Error Handling

Contract errors are returned as typed Soroban errors. Match on the numeric code:

```js
async function safeVouch(voucherKeypair, borrower, stake, token) {
  try {
    const result = await vouch(voucherKeypair, borrower, stake, token);
    if (result.status === 'ERROR') {
      const errorCode = parseContractError(result.errorResult);
      switch (errorCode) {
        case 1:  throw new Error('InsufficientFunds: stake must be > 0');
        case 5:  throw new Error('DuplicateVouch: already vouched for this borrower');
        case 13: throw new Error('MinStakeNotMet: increase stake to meet minimum');
        case 21: throw new Error('VouchCooldownActive: wait before vouching again');
        case 36: throw new Error('InsufficientVoucherBalance: top up your token balance');
        case 37: throw new Error('SelfVouchNotAllowed: cannot vouch for yourself');
        default: throw new Error(`ContractError code ${errorCode}`);
      }
    }
    return result;
  } catch (e) {
    console.error('vouch failed:', e.message);
    throw e;
  }
}
```

Full error code reference: see [README.md — Error Reference](README.md#error-reference).

---

## Common Integration Patterns

### Indexer: track all vouches

```js
async function indexVouches(fromLedger) {
  const events = await server.getEvents({
    startLedger: fromLedger,
    filters: [{ type: 'contract', contractIds: [CONTRACT_ID] }],
  });

  for (const event of events.events) {
    const [topic1, topic2] = event.topic.map(scValToNative);
    if (topic1 === 'vouch' && topic2 === 'added') {
      const [voucher, borrower, stake, token] = event.value.map(scValToNative);
      await db.upsertVouch({ voucher, borrower, stake: stake.toString(), token, ledger: event.ledger });
    }
  }
}
```

### Frontend: display borrower trust score

```js
async function getBorrowerProfile(borrower) {
  const [vouches, loanStatus, eligible] = await Promise.all([
    getVouches(borrower),
    getLoanStatus(borrower),
    isEligible(borrower, MIN_THRESHOLD, PRIMARY_TOKEN),
  ]);

  const totalStake = vouches.reduce((sum, v) => sum + v.stake, 0n);
  return {
    borrower,
    voucherCount: vouches.length,
    totalStakeXlm: Number(totalStake) / 10_000_000,
    loanStatus,
    eligible,
  };
}
```

### Stroop conversion helpers

```js
const XLM_TO_STROOPS = 10_000_000n;
const xlmToStroops = (xlm) => BigInt(Math.round(Number(xlm) * 10_000_000));
const stroopsToXlm = (stroops) => Number(stroops) / 10_000_000;
```
