# Contract Event Indexing Guide

Guide for indexing and querying QuorumCredit contract events off-chain.

---

## Overview

QuorumCredit emits Soroban contract events for every state-changing operation. Off-chain systems (dashboards, notification services, analytics) can subscribe to these events via the Stellar RPC or Horizon API to track protocol state without querying contract storage directly.

---

## Event Structure

Every Soroban contract event has:

| Field | Description |
|-------|-------------|
| `type` | Always `"contract"` for contract events |
| `contractId` | The deployed QuorumCredit contract address |
| `topics` | Array of XDR-encoded values identifying the event |
| `value` | XDR-encoded event payload |
| `ledger` | Ledger sequence number when the event was emitted |
| `ledgerClosedAt` | ISO 8601 timestamp of ledger close |
| `txHash` | Transaction hash that triggered the event |

Topics are always a two-element array: `[category, action]`, both encoded as `Symbol`.

---

## All Contract Events

### `contract/init`

Emitted once when the contract is initialized.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"contract"` |
| topics[1] | Symbol | `"init"` |
| value | `(Address, Vec<Address>, u32, Address)` | `(deployer, admins, admin_threshold, token)` |

---

### `vouch/create`

Emitted when a new vouch is created.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"vouch"` |
| topics[1] | Symbol | `"create"` |
| value | `(Address, Address, i128, Address)` | `(voucher, borrower, stake_stroops, token)` |

---

### `vouch/increase`

Emitted when a voucher increases their stake.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"vouch"` |
| topics[1] | Symbol | `"increase"` |
| value | `(Address, Address, i128, Address)` | `(voucher, borrower, additional_stake_stroops, token)` |

---

### `vouch/decrease`

Emitted when a voucher decreases their stake.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"vouch"` |
| topics[1] | Symbol | `"decrease"` |
| value | `(Address, Address, i128, Address)` | `(voucher, borrower, new_stake_stroops, token)` |

---

### `vouch/withdraw`

Emitted when a voucher fully withdraws their vouch.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"vouch"` |
| topics[1] | Symbol | `"withdraw"` |
| value | `(Address, Address, i128, Address)` | `(voucher, borrower, returned_stake_stroops, token)` |

---

### `loan/request`

Emitted when a loan is disbursed to a borrower.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"loan"` |
| topics[1] | Symbol | `"request"` |
| value | `(Address, i128, i128, String, Address)` | `(borrower, amount_stroops, threshold_stroops, loan_purpose, token)` |

---

### `loan/repay`

Emitted when a borrower makes a repayment.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"loan"` |
| topics[1] | Symbol | `"repay"` |
| value | `(Address, i128)` | `(borrower, payment_stroops)` |

---

### `loan/slash`

Emitted when a borrower's loan is slashed.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"loan"` |
| topics[1] | Symbol | `"slash"` |
| value | `(Address, i128)` | `(borrower, total_slashed_stroops)` |

---

### `admin/config`

Emitted when the protocol configuration is updated.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"admin"` |
| topics[1] | Symbol | `"config"` |
| value | `(Address, Config)` | `(admin, new_config)` |

---

### `admin/pause` / `admin/unpause`

Emitted when the contract is paused or unpaused.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"admin"` |
| topics[1] | Symbol | `"pause"` or `"unpause"` |
| value | `Address` | Admin address that triggered the action |

---

## Querying Events via Stellar RPC

### Using `getEvents` RPC Method

```typescript
import { SorobanRpc } from '@stellar/stellar-sdk';

const server = new SorobanRpc.Server('https://soroban-testnet.stellar.org');

const CONTRACT_ID = 'C...'; // your deployed contract address

// Fetch all vouch/create events from ledger 1000000 onwards
const response = await server.getEvents({
  startLedger: 1000000,
  filters: [
    {
      type: 'contract',
      contractIds: [CONTRACT_ID],
      topics: [
        ['AAAADwAAAAV2b3VjaA==', 'AAAADwAAAAZjcmVhdGU='], // ["vouch", "create"] base64-encoded
      ],
    },
  ],
  limit: 100,
});

for (const event of response.events) {
  console.log('txHash:', event.txHash);
  console.log('ledger:', event.ledger);
  // Decode value with XDR
  const value = event.value.value();
  console.log('payload:', value);
}
```

### Using Horizon API

Horizon does not expose Soroban contract events directly. Use the Soroban RPC `getEvents` endpoint instead.

---

## Example Indexer Implementation

The following TypeScript indexer polls for new events and stores them in a local database.

```typescript
import { SorobanRpc, xdr, scValToNative } from '@stellar/stellar-sdk';

const RPC_URL = 'https://soroban-testnet.stellar.org';
const CONTRACT_ID = 'C...';
const POLL_INTERVAL_MS = 5000;

const server = new SorobanRpc.Server(RPC_URL);

interface VouchEvent {
  action: string;
  voucher: string;
  borrower: string;
  stakeStroops: bigint;
  token: string;
  ledger: number;
  txHash: string;
}

interface LoanEvent {
  action: string;
  borrower: string;
  amountStroops: bigint;
  ledger: number;
  txHash: string;
}

let lastLedger = 0; // persist this across restarts

async function fetchEvents(startLedger: number): Promise<void> {
  const response = await server.getEvents({
    startLedger,
    filters: [
      {
        type: 'contract',
        contractIds: [CONTRACT_ID],
      },
    ],
    limit: 200,
  });

  for (const event of response.events) {
    const [categoryVal, actionVal] = event.topic;
    const category = scValToNative(categoryVal) as string;
    const action = scValToNative(actionVal) as string;
    const payload = scValToNative(event.value);

    if (category === 'vouch') {
      const [voucher, borrower, stake, token] = payload as [string, string, bigint, string];
      const vouchEvent: VouchEvent = {
        action,
        voucher,
        borrower,
        stakeStroops: stake,
        token,
        ledger: event.ledger,
        txHash: event.txHash,
      };
      await storeVouchEvent(vouchEvent);
    } else if (category === 'loan') {
      const [borrower, amount] = payload as [string, bigint];
      const loanEvent: LoanEvent = {
        action,
        borrower,
        amountStroops: amount,
        ledger: event.ledger,
        txHash: event.txHash,
      };
      await storeLoanEvent(loanEvent);
    }

    lastLedger = Math.max(lastLedger, event.ledger);
  }
}

async function storeVouchEvent(event: VouchEvent): Promise<void> {
  // Insert into your database here
  console.log('[vouch]', event);
}

async function storeLoanEvent(event: LoanEvent): Promise<void> {
  // Insert into your database here
  console.log('[loan]', event);
}

async function runIndexer(): Promise<void> {
  const { sequence } = await server.getLatestLedger();
  if (lastLedger === 0) lastLedger = sequence - 1000; // start from ~1000 ledgers back

  setInterval(async () => {
    try {
      await fetchEvents(lastLedger + 1);
    } catch (err) {
      console.error('Indexer error:', err);
    }
  }, POLL_INTERVAL_MS);
}

runIndexer();
```

---

## Querying Indexed Events

Once events are stored in a database, you can query them to reconstruct protocol state.

### Example: Get all active vouches for a borrower

```sql
SELECT voucher, SUM(stake_stroops) AS total_stake
FROM vouch_events
WHERE borrower = $1
  AND action IN ('create', 'increase')
GROUP BY voucher
HAVING SUM(CASE WHEN action = 'withdraw' THEN -stake_stroops ELSE stake_stroops END) > 0;
```

### Example: Get loan history for a borrower

```sql
SELECT action, amount_stroops, ledger, tx_hash
FROM loan_events
WHERE borrower = $1
ORDER BY ledger ASC;
```

### Example: Get all vouchers who backed a specific borrower

```typescript
async function getVouchersForBorrower(borrower: string): Promise<string[]> {
  const response = await server.getEvents({
    startLedger: DEPLOY_LEDGER,
    filters: [
      {
        type: 'contract',
        contractIds: [CONTRACT_ID],
        topics: [['vouch'], ['create']],
      },
    ],
    limit: 500,
  });

  const vouchers = new Set<string>();
  for (const event of response.events) {
    const [voucher, eventBorrower] = scValToNative(event.value) as [string, string, bigint, string];
    if (eventBorrower === borrower) vouchers.add(voucher);
  }
  return [...vouchers];
}
```

---

## Amount Conversion

All monetary values in events are in **stroops** (1 XLM = 10,000,000 stroops).

```typescript
const XLM_TO_STROOPS = 10_000_000n;
const stroopsToXlm = (stroops: bigint): number => Number(stroops) / 10_000_000;
const xlmToStroops = (xlm: number): bigint => BigInt(Math.round(xlm * 10_000_000));
```

---

## Notes

- Events are only available for a limited number of ledgers via the RPC `getEvents` endpoint (typically ~17,280 ledgers / ~24 hours on testnet). For long-term storage, run a persistent indexer.
- The `startLedger` parameter in `getEvents` must be within the node's event retention window. Store `lastLedger` persistently to avoid gaps.
- On mainnet, use `https://rpc.mainnet.stellar.org` as the RPC URL.
