use crate::errors::ContractError;
use crate::helpers::{require_not_thawing, require_allowed_token};
use crate::types::{DataKey, LoanRecord, LoanStatus, VouchRecord, YieldStreamState, VoucherYieldClaim, YIELD_STREAM_PERIOD_SECS};
use soroban_sdk::{symbol_short, Address, Env, Vec};

pub fn claim_streamed_yield(env: Env, voucher: Address, loan_id: u64) -> Result<i128, ContractError> {
    voucher.require_auth();
    require_not_thawing(&env)?;

    let loan: LoanRecord = env.storage().persistent().get(&DataKey::Loan(loan_id)).ok_or(ContractError::NoActiveLoan)?;
    if loan.status != LoanStatus::Active {
        return Err(ContractError::InvalidStateTransition);
    }

    let now = env.ledger().timestamp();
    let disbursed = loan.disbursement_timestamp;

    let mut stream_state: YieldStreamState = env.storage().persistent().get(&DataKey::YieldStreamState(loan_id)).unwrap_or(YieldStreamState {
        loan_id,
        last_claim_timestamp: disbursed,
        total_yield_claimed: 0,
    });

    let mut voucher_claim: VoucherYieldClaim = env.storage().persistent().get(&DataKey::VoucherYieldClaim(loan_id, voucher.clone())).unwrap_or(VoucherYieldClaim {
        voucher: voucher.clone(),
        loan_id,
        last_claim_timestamp: disbursed,
        yield_claimed: 0,
    });

    let last_claim = stream_state.last_claim_timestamp.max(voucher_claim.last_claim_timestamp);
    let elapsed = now.saturating_sub(last_claim);
    if elapsed == 0 {
        return Ok(0);
    }

    let total_duration = loan.deadline.saturating_sub(disbursed);
    if total_duration == 0 {
        return Ok(0);
    }

    let vouches: Vec<VouchRecord> = env.storage().persistent().get(&DataKey::Vouches(loan.borrower.clone())).unwrap_or(Vec::new(&env));
    let total_stake: i128 = vouches.iter().filter(|v| v.token == loan.token_address).map(|v| v.stake).sum();
    if total_stake == 0 {
        return Ok(0);
    }

    let voucher_stake: i128 = vouches.iter().find(|v| v.voucher == voucher && v.token == loan.token_address).map(|v| v.stake).unwrap_or(0);
    if voucher_stake == 0 {
        return Err(ContractError::VoucherNotFound);
    }

    let elapsed_periods = elapsed / YIELD_STREAM_PERIOD_SECS;
    let total_periods = total_duration / YIELD_STREAM_PERIOD_SECS;
    if total_periods == 0 {
        return Ok(0);
    }

    let total_yield_for_voucher = loan.total_yield * voucher_stake / total_stake;
    let proportional_claim = total_yield_for_voucher * elapsed_periods as i128 / total_periods as i128;
    let claim_amount = proportional_claim.saturating_sub(voucher_claim.yield_claimed);

    if claim_amount <= 0 {
        return Ok(0);
    }

    stream_state.total_yield_claimed = stream_state.total_yield_claimed.checked_add(claim_amount).ok_or(ContractError::ArithmeticError)?;
    stream_state.last_claim_timestamp = now;
    voucher_claim.yield_claimed = voucher_claim.yield_claimed.checked_add(claim_amount).ok_or(ContractError::ArithmeticError)?;
    voucher_claim.last_claim_timestamp = now;

    let token_client = require_allowed_token(&env, &loan.token_address)?;
    token_client.transfer(&env.current_contract_address(), &voucher, &claim_amount);

    env.storage().persistent().set(&DataKey::YieldStreamState(loan_id), &stream_state);
    env.storage().persistent().set(&DataKey::VoucherYieldClaim(loan_id, voucher.clone()), &voucher_claim);

    env.events().publish(
        (symbol_short!("yield"), symbol_short!("stream")),
        (voucher, loan_id, claim_amount),
    );

    Ok(claim_amount)
}

pub fn get_yield_stream_state(env: Env, loan_id: u64) -> Option<YieldStreamState> {
    env.storage().persistent().get(&DataKey::YieldStreamState(loan_id))
}

pub fn get_voucher_yield_claim(env: Env, loan_id: u64, voucher: Address) -> Option<VoucherYieldClaim> {
    env.storage().persistent().get(&DataKey::VoucherYieldClaim(loan_id, voucher))
}
