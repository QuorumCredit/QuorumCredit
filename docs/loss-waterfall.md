# Loss Waterfall: How a Slash Shortfall Is Absorbed

> **Issue #1435.** This document is the single source of truth for the order in
> which the protocol's loss-absorption layers are consulted when a borrower
> defaults and the resulting slash cannot make every voucher whole. It exists
> because both `src/insurance.rs` (`claim_insurance_for_shortfall`) and
> `src/bond_protection.rs` (`apply_bond_coverage`) can pay against the same
> shortfall, and the sequencing between them was previously undocumented.

## Summary

When a loan defaults, `governance.rs` executes the slash. Each voucher's stake is
reduced by the effective slash percentage; the slashed tokens are pooled to
reimburse the losses that vouchers collectively bear. If the slashed/recovered
amount is **less than** the loss that needs to be covered, the difference is a
**shortfall**, and the protocol walks the following waterfall. Each layer is
consulted **in order**, each pays **at most the remaining shortfall**, and a
layer is only reached once every layer above it is exhausted:

| # | Layer | Source module | Scope | Funded by |
|---|-------|---------------|-------|-----------|
| 1 | **Voucher protection bond** | `bond_protection.rs` · `apply_bond_coverage` | Per (voucher, loan) | The voucher's own bond stake (≤ 50% of the vouch) |
| 2 | **Bond insurance rider** | `bond_protection.rs` · `apply_bond_coverage` (insurance branch) | Per (voucher, loan), only if `bond.has_insurance` | The 3% bond-insurance premium the voucher paid (`purchase_bond_insurance`) |
| 3 | **Per-vouch insurance opt-in** | `insurance.rs` / `DataKey::VoucherInsurance`, `DataKey::InsuranceVoucherClaim` | Per (voucher, borrower) that opted in | The per-vouch insurance premium (`insurance_premium_bps`) |
| 4 | **Protocol insurance fund** | `insurance.rs` · `claim_insurance_for_shortfall` | Protocol-wide | `insurance_fund_premium_bps` of every disbursement + admin top-ups (`contribute_to_insurance_fund`) |
| 5 | **Residual loss** | — | Per voucher | Absorbed by the voucher — this is the uninsured tail risk the model deliberately keeps |

### The rule in one sentence

> **Bond → bond-insurance rider → per-vouch insurance → protocol insurance fund → voucher eats the rest.**

## Why this order

1. **Most specific / most pre-funded first.** A protection bond is capital the
   voucher already locked *for this exact loan*. Spending it first is free to the
   rest of the system and imposes no socialised cost.
2. **The bond-insurance rider is scoped to the bond.** It only exists to top up a
   bond the voucher already paid a 3% premium to protect, so it is consulted
   immediately after the bond it rides on, before any protocol-level pool.
3. **Per-vouch insurance is opt-in and pre-paid** by the individual voucher, so it
   is consulted before the shared fund — a voucher who paid for coverage should
   exhaust it before drawing on the mutualised pool.
4. **The protocol insurance fund is the mutualised backstop.** It is drawn last
   among the funded layers, and its payout is itself capped at
   `insurance_max_payout_bps` of the shortfall, so it never fully socialises a
   single large default.
5. **The voucher absorbs the residual.** Keeping an uninsured tail is intentional:
   it preserves voucher skin-in-the-game and bounds the protocol's liability. See
   [economic-security-model.md](economic-security-model.md).

## No double-counting

Each layer receives the **remaining** shortfall after the layers above it, never
the original gross shortfall:

```
remaining = gross_shortfall
for layer in [bond, bond_insurance_rider, per_vouch_insurance, protocol_fund]:
    paid       = layer.pay(min(remaining, layer.available, layer.cap))
    remaining -= paid
    if remaining == 0: break
residual_loss = remaining   # absorbed by the voucher
```

* `apply_bond_coverage` already enforces this internally: the insurance-rider
  branch is entered only `if bond.has_insurance && bond_used < slash_amount`, and
  it pays `shortfall = slash_amount - bond_used` (not `slash_amount`).
* `claim_insurance_for_shortfall` must be called by `governance.rs` slash
  execution with the shortfall that is **still outstanding after layers 1–3**,
  not the gross voucher loss. It pays `min(fund_balance, shortfall * insurance_max_payout_bps / 10_000)`.
* The same `(voucher, loan)` bond cannot be applied twice — its status moves to
  `PartiallyUsed` / `Exhausted` and re-entry with a `Released`/`Exhausted` bond
  returns `ContractError::InvalidAmount`.

## Governance slash-execution sequencing (expected)

`governance.rs` slash execution is expected to, per voucher on the defaulted loan:

1. Compute `slash_amount` for the voucher from `effective_slash_bps`.
2. Call `apply_bond_coverage(env, loan_id, voucher, slash_amount)` — this returns
   the amount covered by **layers 1 + 2** combined.
3. Apply per-vouch insurance (layer 3) to `slash_amount - covered_so_far` for
   vouchers that opted in.
4. Accumulate the still-outstanding amount into a loan-level `shortfall`.

Then once, at the loan level:

5. Call `claim_insurance_for_shortfall(env, shortfall, &config)` (layer 4). This
   also emits the `("insurance", "low_bal")` event (Issue #1436) if the
   post-claim balance crosses `insurance_fund_low_bal_thresh`.
6. Whatever remains is the residual loss (layer 5), already reflected in the
   vouchers' reduced stake.

## Related events

| Event topic | Emitted by | Meaning |
|-------------|-----------|---------|
| `insurance_fund` / `contrib` | `contribute_to_insurance_fund` (#1437) | Admin top-up: `(amount, new_balance, admin_signer_count)` |
| `insurance_fund` / `low_bal` | `claim_insurance_for_shortfall` (#1436) | Fund crossed the low-balance threshold: `(remaining_balance, threshold)` |

## See also

* [economic-security-model.md](economic-security-model.md) — why the residual
  tail is left uninsured.
* [monitoring-guide.md](monitoring-guide.md) — `InsuranceFundLowBalance` alert.
* `src/circuit_breaker_insurance_integration_test.rs` — integration coverage for
  the fund interacting with the circuit breaker; the full-waterfall integration
  test tracked by #1435 lives alongside it.
