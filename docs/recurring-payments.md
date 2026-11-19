# Recurring Payments — Keeper Trust Model

Recurring repayments are pulled by an off-chain keeper, which calls
`execute_recurring_payment` once a schedule is due. The keeper also reports
failed attempts through `record_recurring_payment_failure`.

## Who may do what

| Function | Caller |
| --- | --- |
| `setup_recurring_payment`, `terminate_recurring_payment` | the borrower (requires auth) |
| `execute_recurring_payment` | anyone (permissionless, so the keeper can trigger it) |
| `set_recurring_payment_keeper` | admin approval (`admin_signers` threshold) |
| `record_recurring_payment_failure` | the registered keeper, or any registered admin |

## Why failure recording is restricted

Failure recording increments `failure_count` and `retry_count`, which drive
the keeper's retry handling and the schedule's success-rate figures. Letting an
arbitrary account call it would let a third party corrupt a borrower's
automated repayment record and disrupt its retry behaviour. The function therefore requires
`caller.require_auth()` and then checks that the caller is either the
registered keeper or a registered admin. Any other caller receives
`UnauthorizedCaller` and no state changes.

## Trust in the keeper

The keeper is trusted to report only real failed attempts. It cannot move
funds through this path: fund movement happens in `execute_recurring_payment`,
which the borrower authorizes by granting a token allowance. Admins rotate the
keeper with `set_recurring_payment_keeper`, which requires the configured
admin approval threshold.
