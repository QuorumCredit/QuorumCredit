# Vouch Cooldown Bypass for Emergency Cases

**Issue**: #1056  
**Priority**: Medium  
**Estimated Time**: 1.5 hours

## Overview

The `vouch_cooldown` mechanism enforces a minimum interval between vouch calls to prevent spam and abuse. However, in emergency cases (e.g., an imminent loan default where a voucher needs to increase stake or re-vouch quickly), the cooldown can be an obstacle. This feature introduces a governance-based cooldown bypass mechanism that allows vouchers to request and receive a temporary waiver of the cooldown via admin voting.

## Architecture

### Storage

- **`DataKey::CooldownBypass(Address, Address)`** — maps `(borrower, voucher)` to a `CooldownBypassRequest` record. The first `Address` is the borrower, the second is the voucher.

### Data Structures

**`CooldownBypassRequest`** (defined in `src/types.rs`)
| Field | Type | Description |
|---|---|---|
| `voucher` | `Address` | The voucher requesting the bypass |
| `borrower` | `Address` | The borrower the voucher is vouching for |
| `reason` | `String` | Human-readable reason for the emergency bypass |
| `requested_at` | `u64` | Ledger timestamp when the request was created |
| `approvers` | `Vec<Address>` | List of admin addresses that have voted to approve |
| `approved` | `bool` | Whether the 2/3 threshold has been met |

### Errors

| Error Code | Value | Description |
|---|---|---|
| `CooldownBypassAlreadyRequested` | 143 | Duplicate request for the same (borrower, voucher) pair |
| `CooldownBypassNotFound` | 144 | No bypass request exists for the given pair |
| `CooldownBypassAlreadyApproved` | 145 | Bypass has already been granted |
| `CooldownBypassInsufficientApprovals` | 146 | Not enough admin approvals (unused; threshold checked internally) |

### Flow

1. **Request**: A voucher calls `request_cooldown_bypass(voucher, borrower, reason)`.
   - Requires auth from `voucher`
   - Verifies the voucher has an active vouch for the borrower
   - Rejects duplicate requests for the same `(borrower, voucher)` pair
   - Creates a `CooldownBypassRequest` with `approved = false` and empty `approvers`

2. **Voting**: Admins call `vote_bypass(approver, voucher, borrower, approve)`.
   - Requires auth from `approver`
   - Verifies `approver` is a registered admin
   - Rejects if no request exists, or if the admin already voted
   - Rejects if the bypass is already approved
   - Records the vote; if `approve` count reaches ceil(2/3) of total admins, sets `approved = true`

3. **Consumption**: When `validate_vouch()` encounters an active cooldown, it calls `has_cooldown_bypass(env, voucher, borrower)` before rejecting with `VouchCooldownActive`. If a bypass is approved, the cooldown is skipped.

4. **Cleanup**: Admins can call `clear_cooldown_bypass(admin_signers, borrower, voucher)` to remove a bypass record.

### Files Changed

| File | Change |
|---|---|
| `src/cooldown_bypass.rs` | **NEW** — Core module with request, vote, check, and clear logic |
| `src/cooldown_bypass_test.rs` | **NEW** — 12 tests covering all bypass scenarios |
| `src/types.rs` | Added `DataKey::CooldownBypass` variant and `CooldownBypassRequest` struct |
| `src/errors.rs` | Added 4 new error variants (143-146) |
| `src/lib.rs` | Added `pub mod cooldown_bypass`, 5 entry point functions, test module |
| `src/vouch.rs` | Modified `validate_vouch` to check `has_cooldown_bypass` before rejecting |

## Testing

All tests are in `src/cooldown_bypass_test.rs` and include:

- `test_request_cooldown_bypass_success` — basic request creation
- `test_request_cooldown_bypass_not_voucher_fails` — non-voucher rejected
- `test_request_cooldown_bypass_duplicate_fails` — duplicate request rejected
- `test_vote_bypass_non_admin_fails` — non-admin cannot vote
- `test_vote_bypass_no_request_fails` — voting without request rejected
- `test_vote_bypass_approval_threshold_2_of_3` — 2/3 threshold verification
- `test_vote_bypass_double_vote_fails` — admin cannot vote twice
- `test_vote_bypass_after_approved_fails` — no voting after approval
- `test_vote_bypass_rejection_recorded` — rejection is tracked
- `test_cooldown_bypass_allows_vouch_during_cooldown` — end-to-end bypass flow
- `test_clear_cooldown_bypass` — admin cleanup
- `test_has_cooldown_bypass` — state query utility

### Test Setup

- 3 admins, admin_threshold = 3
- Mocked auth via `env.mock_all_auths()`
- Standard vouch + cooldown scenario to trigger `VouchCooldownActive`
- Bypass granted after 2 of 3 admins approve (ceil(2/3 * 3) = 2)

closes #1056
