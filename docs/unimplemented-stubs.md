# Unimplemented Stub Functions (issue #1394)

`src/loan.rs` contains seven public functions that are non-functional stubs —
they either always return a hardcoded error or silently do nothing. This
document is the audit trail for that finding: which functions, whether they
are reachable from outside the contract today, and what they actually do.

## Audit result: none are exposed as contract entry points

All seven are `pub fn`s in `loan.rs`, but **none of them are wired up as
`#[contractimpl]` entry points in `lib.rs`**, and nothing else in this crate
calls them either. From an external caller's perspective (the API server,
the dashboard, a client SDK) these functions do not exist — there is no way
to invoke them through the deployed contract. They cannot silently hand back
non-functional behavior to an integrator today, because there is no entry
point to integrate against.

They remain in the codebase as scaffolding for features that haven't been
built yet. Each is marked with a `STUB` doc comment at its definition in
`loan.rs` pointing back to this document.

**If any of these is ever wired into `lib.rs`, implement it first.** Exposing
one of these as a live entry point without implementing it is exactly the
silent-no-op risk this audit exists to prevent.

## Current behavior, function by function

| Function | Current behavior |
|---|---|
| `deposit_collateral(env, borrower, amount, token)` | Always returns `Err(ContractError::InvalidStateTransition)`. No collateral is ever recorded, regardless of arguments. |
| `get_borrower_collateral(env, borrower)` | Always returns `0`, independent of any prior (always-failing) `deposit_collateral` call. |
| `emit_repayment_reminders(env)` | No-op. Sends no reminders, touches no storage. |
| `mint_reputation_nft(env, borrower)` | No-op that always returns `Ok(())`. Mints no NFT. A caller checking only the `Result` would incorrectly conclude an NFT was minted. |
| `send_repayment_reminder(env, loan_id)` | No-op that always returns `Ok(())`. Sends no reminder — same caveat as `mint_reputation_nft`. |
| `defer_payment(env, borrower)` | Checks `borrower.require_auth()` and `require_not_thawing()` (so a real auth/thaw error is returned first if either applies), then always returns `Err(ContractError::InvalidStateTransition)`. No payment is ever deferred. |
| `check_acceleration(env, borrower)` | Always returns `Err(ContractError::InvalidStateTransition)`. No acceleration check ever runs. |

`mint_reputation_nft` and `send_repayment_reminder` are the two most worth
flagging: they report success (`Ok(())`) while doing nothing, which is the
"silent no-op" shape the issue was concerned about. The other five at least
fail loudly or return an obviously-placeholder value (`0`).

## Tests

`src/unimplemented_stubs_test.rs` asserts each function's behavior above
explicitly, so a future partial implementation (one that starts doing
*something* but isn't finished, or a copy-paste that accidentally changes
one but not another) is caught by CI rather than silently changing behavior
no test was watching.
