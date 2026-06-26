use crate::errors::ContractError;
use crate::helpers::{get_active_loan_record, require_not_thawing};
use crate::types::{DataKey, PeriodicPaymentConfig, PeriodicPaymentStatus, ScheduleType};
use soroban_sdk::{symbol_short, Address, Env};

fn get_period_secs(schedule_type: &ScheduleType) -> u64 {
    match schedule_type {
        ScheduleType::Weekly => 7 * 24 * 60 * 60,
        ScheduleType::BiWeekly => 14 * 24 * 60 * 60,
        ScheduleType::Monthly => 30 * 24 * 60 * 60,
        ScheduleType::Quarterly => 90 * 24 * 60 * 60,
    }
}

pub fn set_periodic_payment(
    env: Env,
    caller: Address,
    loan_id: u64,
    schedule_type: ScheduleType,
    period_count: u32,
    period_interest_bps: u32,
) -> Result<(), ContractError> {
    caller.require_auth();
    require_not_thawing(&env)?;
    let loan = get_active_loan_record(&env, &caller)?;
    if loan.id != loan_id {
        return Err(ContractError::NoActiveLoan);
    }
    let config = PeriodicPaymentConfig {
        schedule_type,
        period_count,
        period_interest_bps,
        periods_completed: 0,
        enabled: true,
    };
    let now = env.ledger().timestamp();
    let period_secs = get_period_secs(&config.schedule_type);
    let status = PeriodicPaymentStatus {
        loan_id,
        config: config.clone(),
        next_period_due: now + period_secs,
        last_payment_timestamp: now,
        total_period_interest_paid: 0,
    };
    env.storage().persistent().set(&DataKey::PeriodicPaymentConfig(loan_id), &config);
    env.storage().persistent().set(&DataKey::PeriodicPaymentStatus(loan_id), &status);
    env.events().publish(
        (symbol_short!("ppay"), symbol_short!("set")),
        (caller, loan_id, period_count, period_interest_bps),
    );
    Ok(())
}

pub fn make_periodic_payment(
    env: Env,
    borrower: Address,
    loan_id: u64,
    payment: i128,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_thawing(&env)?;
    let mut config: PeriodicPaymentConfig = env.storage().persistent().get(&DataKey::PeriodicPaymentConfig(loan_id)).ok_or(ContractError::NoActiveLoan)?;
    if !config.enabled {
        return Err(ContractError::InvalidStateTransition);
    }
    if config.periods_completed >= config.period_count {
        return Err(ContractError::InvalidStateTransition);
    }
    let mut status: PeriodicPaymentStatus = env.storage().persistent().get(&DataKey::PeriodicPaymentStatus(loan_id)).ok_or(ContractError::NoActiveLoan)?;
    let now = env.ledger().timestamp();
    if now < status.next_period_due {
        return Err(ContractError::InvalidStateTransition);
    }
    let period_interest = payment * config.period_interest_bps as i128 / 10_000;
    status.total_period_interest_paid = status.total_period_interest_paid.checked_add(period_interest).ok_or(ContractError::ArithmeticError)?;
    status.last_payment_timestamp = now;
    let period_secs = get_period_secs(&config.schedule_type);
    status.next_period_due = now + period_secs;
    config.periods_completed += 1;
    if config.periods_completed >= config.period_count {
        config.enabled = false;
    }
    env.storage().persistent().set(&DataKey::PeriodicPaymentConfig(loan_id), &config);
    env.storage().persistent().set(&DataKey::PeriodicPaymentStatus(loan_id), &status);
    env.events().publish(
        (symbol_short!("ppay"), symbol_short!("pay")),
        (borrower, loan_id, payment, period_interest),
    );
    Ok(())
}

pub fn get_periodic_payment_config(env: Env, loan_id: u64) -> Option<PeriodicPaymentConfig> {
    env.storage().persistent().get(&DataKey::PeriodicPaymentConfig(loan_id))
}

pub fn get_periodic_payment_status(env: Env, loan_id: u64) -> Option<PeriodicPaymentStatus> {
    env.storage().persistent().get(&DataKey::PeriodicPaymentStatus(loan_id))
}
