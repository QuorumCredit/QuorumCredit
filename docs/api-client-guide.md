# API Client Library Documentation

Guide for integrating with QuorumCredit contract through Stellar Soroban RPC.

## Authentication

All contract invocations require the caller to sign the transaction with their private key.

### Stellar Account Setup

```typescript
import { Keypair, Networks, TransactionBuilder, BASE_FEE } from '@stellar/js-sdk';

// Generate or import keypair
const keypair = Keypair.fromSecret('S...');
const publicKey = keypair.publicKey();

// Create account on testnet (if needed)
// https://friendbot.stellar.org/?addr=GXXXXXX
```

```python
from stellar_sdk import Keypair, Network, TransactionBuilder, BASE_FEE

keypair = Keypair.from_secret('S...')
public_key = keypair.public_key()
```

```rust
use stellar_sdk::{Keypair, Network};

let keypair = Keypair::from_secret("S...")?;
let public_key = keypair.public_key();
```

## Error Handling

All contract functions return `Result<T, ContractError>`. Handle errors gracefully:

### TypeScript

```typescript
interface ContractError {
  code: number;
  message: string;
}

async function handleContractCall(fn: () => Promise<any>) {
  try {
    return await fn();
  } catch (error) {
    if (error.code === 1) {
      console.error('InsufficientFunds:', error.message);
    } else if (error.code === 2) {
      console.error('ActiveLoanExists:', error.message);
    } else if (error.code === 7) {
      console.error('ContractPaused:', error.message);
    }
    throw error;
  }
}
```

### Python

```python
class ContractError(Exception):
    def __init__(self, code: int, message: str):
        self.code = code
        self.message = message
        super().__init__(f"Error {code}: {message}")

def handle_contract_error(error):
    if error.code == 1:
        print("InsufficientFunds")
    elif error.code == 2:
        print("ActiveLoanExists")
    elif error.code == 7:
        print("ContractPaused")
    raise error
```

### Rust

```rust
use quorum_credit::ContractError;

fn handle_contract_error(error: ContractError) {
    match error {
        ContractError::InsufficientFunds => println!("Insufficient funds"),
        ContractError::ActiveLoanExists => println!("Active loan exists"),
        ContractError::ContractPaused => println!("Contract paused"),
        _ => println!("Unknown error"),
    }
}
```

## Retry Logic

Implement exponential backoff for transient failures:

### TypeScript

```typescript
async function retryWithBackoff<T>(
  fn: () => Promise<T>,
  maxRetries: number = 3,
  baseDelay: number = 1000
): Promise<T> {
  for (let attempt = 0; attempt < maxRetries; attempt++) {
    try {
      return await fn();
    } catch (error) {
      if (attempt === maxRetries - 1) throw error;
      
      // Don't retry on contract errors
      if (error.code && error.code >= 1 && error.code <= 34) {
        throw error;
      }
      
      const delay = baseDelay * Math.pow(2, attempt);
      await new Promise(resolve => setTimeout(resolve, delay));
    }
  }
  throw new Error('Max retries exceeded');
}
```

### Python

```python
import time
from typing import TypeVar, Callable

T = TypeVar('T')

def retry_with_backoff(
    fn: Callable[[], T],
    max_retries: int = 3,
    base_delay: float = 1.0
) -> T:
    for attempt in range(max_retries):
        try:
            return fn()
        except ContractError as e:
            # Don't retry on contract errors
            raise
        except Exception as e:
            if attempt == max_retries - 1:
                raise
            delay = base_delay * (2 ** attempt)
            time.sleep(delay)
    raise Exception("Max retries exceeded")
```

## Common Use Cases

### 1. Vouch for a Borrower

**TypeScript**

```typescript
import { SorobanRpc, TransactionBuilder, Networks, BASE_FEE } from '@stellar/js-sdk';

async function vouch(
  contractId: string,
  voucher: Keypair,
  borrower: string,
  stakeStroops: bigint,
  tokenAddress: string
) {
  const server = new SorobanRpc.Server('https://soroban-testnet.stellar.org');
  
  const account = await server.getAccount(voucher.publicKey());
  
  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: Networks.TESTNET_NETWORK_PASSPHRASE
  })
    .addOperation(
      Operation.invokeContractFunction({
        contract: contractId,
        method: 'vouch',
        args: [
          nativeToScVal(voucher.publicKey(), { type: 'address' }),
          nativeToScVal(borrower, { type: 'address' }),
          nativeToScVal(stakeStroops, { type: 'i128' }),
          nativeToScVal(tokenAddress, { type: 'address' })
        ]
      })
    )
    .setTimeout(30)
    .build();
  
  const signed = voucher.sign(tx);
  const result = await server.sendTransaction(signed);
  
  return result;
}

// Usage
const stakeXlm = 100n; // 100 XLM
const stakeStroops = stakeXlm * 10_000_000n;
await vouch(CONTRACT_ID, voucherKeypair, borrowerAddress, stakeStroops, TOKEN_ADDRESS);
```

**Python**

```python
from stellar_sdk import SorobanServer, TransactionBuilder, Network, Keypair, BASE_FEE
from stellar_sdk.operation import InvokeContractFunction
from stellar_sdk.soroban_types import Address, Int128

async def vouch(
    contract_id: str,
    voucher: Keypair,
    borrower: str,
    stake_stroops: int,
    token_address: str
):
    server = SorobanServer("https://soroban-testnet.stellar.org")
    
    account = await server.get_account(voucher.public_key)
    
    tx = (
        TransactionBuilder(account, Network.TESTNET_NETWORK_PASSPHRASE, BASE_FEE)
        .add_invoke_contract_function_op(
            contract_id=contract_id,
            method="vouch",
            parameters=[
                Address(voucher.public_key),
                Address(borrower),
                Int128(stake_stroops),
                Address(token_address)
            ]
        )
        .set_timeout(30)
        .build()
    )
    
    signed = voucher.sign_transaction(tx)
    result = await server.send_transaction(signed)
    
    return result

# Usage
stake_xlm = 100
stake_stroops = stake_xlm * 10_000_000
await vouch(CONTRACT_ID, voucher_keypair, borrower_address, stake_stroops, TOKEN_ADDRESS)
```

**Rust**

```rust
use stellar_sdk::{SorobanRpc, TransactionBuilder, Networks, BASE_FEE, Keypair};

async fn vouch(
    contract_id: &str,
    voucher: &Keypair,
    borrower: &str,
    stake_stroops: i128,
    token_address: &str
) -> Result<String, Box<dyn std::error::Error>> {
    let server = SorobanRpc::new("https://soroban-testnet.stellar.org")?;
    
    let account = server.get_account(voucher.public_key()).await?;
    
    let tx = TransactionBuilder::new(&account, BASE_FEE, Networks::TESTNET)
        .add_operation(
            InvokeContractFunction {
                contract_id: contract_id.to_string(),
                method: "vouch".to_string(),
                parameters: vec![
                    Address::new(voucher.public_key()),
                    Address::new(borrower),
                    Int128::new(stake_stroops),
                    Address::new(token_address)
                ]
            }
        )
        .set_timeout(30)
        .build()?;
    
    let signed = voucher.sign_transaction(&tx)?;
    let result = server.send_transaction(&signed).await?;
    
    Ok(result)
}

// Usage
let stake_xlm = 100i128;
let stake_stroops = stake_xlm * 10_000_000;
vouch(CONTRACT_ID, &voucher_keypair, borrower_address, stake_stroops, TOKEN_ADDRESS).await?;
```

### 2. Request a Loan

**TypeScript**

```typescript
async function requestLoan(
  contractId: string,
  borrower: Keypair,
  amountStroops: bigint,
  thresholdStroops: bigint,
  purpose: string,
  tokenAddress: string
) {
  const server = new SorobanRpc.Server('https://soroban-testnet.stellar.org');
  const account = await server.getAccount(borrower.publicKey());
  
  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: Networks.TESTNET_NETWORK_PASSPHRASE
  })
    .addOperation(
      Operation.invokeContractFunction({
        contract: contractId,
        method: 'request_loan',
        args: [
          nativeToScVal(borrower.publicKey(), { type: 'address' }),
          nativeToScVal(amountStroops, { type: 'i128' }),
          nativeToScVal(thresholdStroops, { type: 'i128' }),
          nativeToScVal(purpose, { type: 'string' }),
          nativeToScVal(tokenAddress, { type: 'address' })
        ]
      })
    )
    .setTimeout(30)
    .build();
  
  const signed = borrower.sign(tx);
  return await server.sendTransaction(signed);
}

// Usage
const loanAmount = 50n; // 50 XLM
const threshold = 100n; // 100 XLM required stake
await requestLoan(
  CONTRACT_ID,
  borrowerKeypair,
  loanAmount * 10_000_000n,
  threshold * 10_000_000n,
  "Business expansion",
  TOKEN_ADDRESS
);
```

**Python**

```python
async def request_loan(
    contract_id: str,
    borrower: Keypair,
    amount_stroops: int,
    threshold_stroops: int,
    purpose: str,
    token_address: str
):
    server = SorobanServer("https://soroban-testnet.stellar.org")
    account = await server.get_account(borrower.public_key)
    
    tx = (
        TransactionBuilder(account, Network.TESTNET_NETWORK_PASSPHRASE, BASE_FEE)
        .add_invoke_contract_function_op(
            contract_id=contract_id,
            method="request_loan",
            parameters=[
                Address(borrower.public_key),
                Int128(amount_stroops),
                Int128(threshold_stroops),
                String(purpose),
                Address(token_address)
            ]
        )
        .set_timeout(30)
        .build()
    )
    
    signed = borrower.sign_transaction(tx)
    return await server.send_transaction(signed)

# Usage
loan_amount = 50
threshold = 100
await request_loan(
    CONTRACT_ID,
    borrower_keypair,
    loan_amount * 10_000_000,
    threshold * 10_000_000,
    "Business expansion",
    TOKEN_ADDRESS
)
```

### 3. Repay a Loan

**TypeScript**

```typescript
async function repay(
  contractId: string,
  borrower: Keypair,
  paymentStroops: bigint
) {
  const server = new SorobanRpc.Server('https://soroban-testnet.stellar.org');
  const account = await server.getAccount(borrower.publicKey());
  
  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: Networks.TESTNET_NETWORK_PASSPHRASE
  })
    .addOperation(
      Operation.invokeContractFunction({
        contract: contractId,
        method: 'repay',
        args: [
          nativeToScVal(borrower.publicKey(), { type: 'address' }),
          nativeToScVal(paymentStroops, { type: 'i128' })
        ]
      })
    )
    .setTimeout(30)
    .build();
  
  const signed = borrower.sign(tx);
  return await server.sendTransaction(signed);
}

// Usage: Repay 50 XLM + 2% yield
const principal = 50n;
const yield2Percent = (principal * 200n) / 10_000n;
const totalRepayment = (principal + yield2Percent) * 10_000_000n;
await repay(CONTRACT_ID, borrowerKeypair, totalRepayment);
```

**Python**

```python
async def repay(
    contract_id: str,
    borrower: Keypair,
    payment_stroops: int
):
    server = SorobanServer("https://soroban-testnet.stellar.org")
    account = await server.get_account(borrower.public_key)
    
    tx = (
        TransactionBuilder(account, Network.TESTNET_NETWORK_PASSPHRASE, BASE_FEE)
        .add_invoke_contract_function_op(
            contract_id=contract_id,
            method="repay",
            parameters=[
                Address(borrower.public_key),
                Int128(payment_stroops)
            ]
        )
        .set_timeout(30)
        .build()
    )
    
    signed = borrower.sign_transaction(tx)
    return await server.send_transaction(signed)

# Usage
principal = 50
yield_2_percent = (principal * 200) // 10_000
total_repayment = (principal + yield_2_percent) * 10_000_000
await repay(CONTRACT_ID, borrower_keypair, total_repayment)
```

## Unit Conversion

Always convert between XLM and stroops:

```typescript
const XLM_TO_STROOPS = 10_000_000n;
const xlmToStroops = (xlm: bigint) => xlm * XLM_TO_STROOPS;
const stroopsToXlm = (stroops: bigint) => stroops / XLM_TO_STROOPS;
```

```python
XLM_TO_STROOPS = 10_000_000
def xlm_to_stroops(xlm: int) -> int:
    return xlm * XLM_TO_STROOPS
def stroops_to_xlm(stroops: int) -> int:
    return stroops // XLM_TO_STROOPS
```

```rust
const XLM_TO_STROOPS: i128 = 10_000_000;
fn xlm_to_stroops(xlm: i128) -> i128 {
    xlm * XLM_TO_STROOPS
}
fn stroops_to_xlm(stroops: i128) -> i128 {
    stroops / XLM_TO_STROOPS
}
```

## Testing

Always test on testnet before mainnet:

```bash
# Testnet RPC
https://soroban-testnet.stellar.org

# Testnet network passphrase
"Test SDF Network ; September 2015"

# Fund testnet account
https://friendbot.stellar.org/?addr=GXXXXXX
```
