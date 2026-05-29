# API Error Code Reference

All QuorumCredit contract errors are returned with numeric codes. This document provides a complete reference for understanding and handling each error.

## Error Codes

| Code | Name | Meaning | Resolution |
|------|------|---------|-----------|
| 1 | `InsufficientFunds` | Stake or amount ≤ 0; or contract balance insufficient for loan/yield disbursement | Ensure positive amounts; verify contract has sufficient liquidity |
| 2 | `ActiveLoanExists` | Borrower already has an active loan; cannot vouch for them | Wait for existing loan to be repaid or defaulted |
| 3 | `StakeOverflow` | Total vouched stake would overflow i128 | Reduce number or size of vouches |
| 4 | `ZeroAddress` | Admin or token address is the zero address | Provide a valid, non-zero address |
| 5 | `DuplicateVouch` | Same voucher attempting to vouch for same borrower twice | Use `increase_stake()` to add more stake instead |
| 6 | `NoActiveLoan` | `repay()`, `slash()`, or `withdraw_vouch()` called with no active loan | Verify borrower address and that loan was disbursed |
| 7 | `ContractPaused` | Any state-mutating function called while contract is paused | Wait for admin to call `unpause()` |
| 8 | `LoanPastDeadline` | Repayment attempted after loan deadline | Use `slash()` to mark default instead |
| 13 | `MinStakeNotMet` | Vouch stake below admin-configured minimum | Increase stake to at least `get_min_stake()` stroops |
| 14 | `LoanExceedsMaxAmount` | Requested loan exceeds admin-configured maximum | Request smaller amount or ask admin to raise cap |
| 15 | `InsufficientVouchers` | Number of vouchers below admin-configured minimum | Recruit more vouchers before requesting loan |
| 16 | `UnauthorizedCaller` | `repay()` called by non-borrower; or `withdraw_vouch()` called by non-voucher | Ensure transaction is signed by correct address |
| 17 | `InvalidAmount` | Numeric parameter fails validity check (e.g., negative fee BPS) | Pass value within documented valid range |
| 18 | `InvalidStateTransition` | Operation not valid for current loan status | Check `loan_status()` before calling function |
| 19 | `AlreadyInitialized` | `initialize()` called on already-initialized contract | `initialize()` is one-time only; no action needed |
| 20 | `VouchTooRecent` | Vouch added too recently (within `MIN_VOUCH_AGE` seconds) | Wait for vouch age requirement to pass |
| 24 | `Blacklisted` | Borrower address is blacklisted | Contact protocol admin |
| 25 | `TimelockNotFound` | Governance timelock operation references non-existent ID | Verify timelock ID from queue operation |
| 26 | `TimelockNotReady` | Timelocked operation executed before delay elapsed | Wait until timelock delay has passed |
| 27 | `TimelockExpired` | Timelocked operation executed after expiry window | Re-queue operation and execute within window |
| 28 | `NoVouchesForBorrower` | Governance slash vote initiated for borrower with no vouches | Verify borrower address |
| 29 | `VoucherNotFound` | Governance slash vote references voucher not in borrower's list | Verify voucher address |
| 30 | `InvalidToken` | Token address not allowed or doesn't implement SEP-41 | Use `get_config()` to retrieve allowed tokens |
| 31 | `AlreadyVoted` | Voucher attempting to cast second slash vote for same borrower | Each voucher votes once per slash proposal |
| 32 | `SlashVoteNotFound` | `execute_slash_vote()` called with no open slash proposal | Initiate slash vote first via `initiate_slash_vote()` |
| 33 | `SlashAlreadyExecuted` | Slash vote executed more than once for same borrower | No action needed; slash already applied |
| 34 | `LoanBelowMinAmount` | Requested loan below admin-configured minimum | Request larger amount or ask admin to lower minimum |
| 35 | `QuorumNotMet` | Slash vote quorum not reached | Recruit more voucher votes |
| 36 | `MaxVouchersPerBorrowerExceeded` | Borrower has reached maximum voucher limit | Wait for existing vouches to be withdrawn |
| 37 | `InsufficientVoucherBalance` | Voucher has insufficient token balance to stake | Ensure voucher has sufficient balance |
| 38 | `SelfVouchNotAllowed` | Voucher and borrower are the same address | Use different addresses for voucher and borrower |
| 39 | `DuplicateToken` | Token already in allowed tokens list | No action needed; token already allowed |
| 40 | `InvalidAdminThreshold` | Admin threshold is 0 or exceeds number of admins | Set threshold between 1 and number of admins |
| 41 | `InsufficientYieldReserve` | Yield reserve insufficient to cover promised yield | Admin must pre-fund yield reserve |
| 42 | `ReminderAlreadySent` | Repayment reminder already sent for this loan | No action needed; reminder already sent |

## Common Error Scenarios

### Vouching Errors

**Error 5 (DuplicateVouch)**
```
Scenario: Voucher A tries to vouch for Borrower B twice
Solution: Call increase_stake(voucher_a, borrower_b, additional_stake) instead
```

**Error 13 (MinStakeNotMet)**
```
Scenario: Attempting to vouch with 25 stroops when minimum is 50
Solution: Increase stake to at least 50 stroops (0.000005 XLM)
```

**Error 37 (InsufficientVoucherBalance)**
```
Scenario: Voucher has 100 XLM but tries to stake 150 XLM
Solution: Ensure voucher has sufficient balance before calling vouch()
```

### Loan Errors

**Error 2 (ActiveLoanExists)**
```
Scenario: Trying to vouch for borrower who already has active loan
Solution: Wait for existing loan to be repaid or defaulted
```

**Error 6 (NoActiveLoan)**
```
Scenario: Calling repay() for borrower with no active loan
Solution: Verify borrower address and that loan was disbursed
```

**Error 8 (LoanPastDeadline)**
```
Scenario: Attempting repayment 31 days after 30-day loan disbursement
Solution: Use slash() to mark default; loan cannot be repaid after deadline
```

**Error 14 (LoanExceedsMaxAmount)**
```
Scenario: Requesting 1000 XLM when max is 500 XLM
Solution: Request smaller amount or ask admin to raise cap via set_max_loan_amount()
```

### Authorization Errors

**Error 16 (UnauthorizedCaller)**
```
Scenario: Non-borrower attempts to call repay()
Solution: Ensure transaction is signed by the borrower address
```

**Error 38 (SelfVouchNotAllowed)**
```
Scenario: Address A tries to vouch for themselves
Solution: Use different addresses for voucher and borrower
```

## Error Response Format

All errors are returned as Soroban contract errors with the numeric code:

```json
{
  "error": {
    "code": 1,
    "message": "InsufficientFunds"
  }
}
```

## Debugging Tips

1. **Check contract state**: Use `get_config()` to verify current settings
2. **Verify addresses**: Ensure all addresses are valid and non-zero
3. **Check balances**: Confirm voucher has sufficient token balance
4. **Review loan status**: Use `loan_status(borrower)` before state-changing calls
5. **Inspect vouches**: Use `get_vouches(borrower)` to see all active vouches

## Integration Guide

When integrating with QuorumCredit, handle errors by matching on the numeric code:

```javascript
try {
  await contract.vouch(voucher, borrower, stake, token);
} catch (error) {
  if (error.code === 5) {
    // DuplicateVouch - use increase_stake instead
    await contract.increase_stake(voucher, borrower, additional_stake, token);
  } else if (error.code === 13) {
    // MinStakeNotMet - increase stake amount
    console.error("Stake below minimum");
  } else if (error.code === 37) {
    // InsufficientVoucherBalance - check balance
    console.error("Voucher has insufficient balance");
  }
}
```

---

For more information, see the [API Reference](../README.md#api-reference) and [Error Reference](../README.md#error-reference).
