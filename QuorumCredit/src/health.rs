use crate::types::{
    Config, DataKey, ExposureReport, HealthAlertThresholds, LoanHealthScore, LoanRecord,
    LoanStatus, ProtocolHealthReport, RiskLevel, VouchRecord,
};
use soroban_sdk::{contracttype, symbol_short, Address, Env, String, Vec};

// ── Protocol-level health check (existing) ────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug)]
pub struct HealthStatus {
    pub is_healthy: bool,
    pub initialized: bool,
    pub paused: bool,
    pub yield_reserve_solvent: bool,
    pub issues: Vec<String>,
}

pub fn health_check(env: &Env) -> HealthStatus {
    let mut issues = Vec::new(env);
    let mut is_healthy = true;

    let initialized = env.storage().instance().has(&DataKey::Config);
    if !initialized {
        issues.push_back(String::from_str(env, "Contract not initialized"));
        is_healthy = false;
    }

    let paused: bool = env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false);

    let yield_reserve_solvent = if initialized {
        let config: Config = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .unwrap_or_else(|| panic!("Config not found despite initialized check"));
        let token_client = soroban_sdk::token::Client::new(env, &config.token);
        let contract_balance = token_client.balance(&env.current_contract_address());
        contract_balance >= 10_000_000
    } else {
        false
    };

    if !yield_reserve_solvent && initialized {
        issues.push_back(String::from_str(env, "Yield reserve below minimum threshold"));
        is_healthy = false;
    }

    HealthStatus {
        is_healthy,
        initialized,
        paused,
        yield_reserve_solvent,
        issues,
    }
}

// ── Alert threshold helpers ───────────────────────────────────────────────────

const HEALTH_THRESHOLDS_KEY: &str = "hlth_thr";

pub fn get_alert_thresholds(env: &Env) -> HealthAlertThresholds {
    env.storage()
        .instance()
        .get(&soroban_sdk::Symbol::new(env, HEALTH_THRESHOLDS_KEY))
        .unwrap_or_else(HealthAlertThresholds::default)
}

pub fn set_alert_thresholds(env: &Env, thresholds: HealthAlertThresholds) {
    env.storage()
        .instance()
        .set(&soroban_sdk::Symbol::new(env, HEALTH_THRESHOLDS_KEY), &thresholds);
}

// ── Individual loan health ────────────────────────────────────────────────────

/// Compute a health score for a single active loan.
///
/// Score components (each 0–100, averaged):
/// 1. Time component: full marks if > at_risk_deadline, zero if past deadline.
/// 2. Repayment component: proportional to repayment progress.
/// 3. History component: inverted borrower risk score.
/// 4. Concentration component: penalises single-voucher dominance.
pub fn get_loan_health(env: Env, borrower: Address) -> Option<LoanHealthScore> {
    let loan_record: LoanRecord = env
        .storage()
        .persistent()
        .get(&DataKey::ActiveLoan(borrower.clone()))
        .and_then(|loan_id: u64| {
            env.storage()
                .persistent()
                .get(&DataKey::Loan(loan_id))
        })?;

    if loan_record.status != LoanStatus::Active {
        return None;
    }

    let thresholds = get_alert_thresholds(&env);
    let now = env.ledger().timestamp();

    // 1. Time component
    let seconds_until_deadline = if now >= loan_record.deadline {
        0u64
    } else {
        loan_record.deadline - now
    };
    let time_score: u32 = if loan_record.deadline <= now {
        0
    } else {
        let duration = loan_record.deadline.saturating_sub(loan_record.disbursement_timestamp);
        if duration == 0 {
            100
        } else {
            let remaining_ratio = seconds_until_deadline * 100 / duration;
            remaining_ratio.min(100) as u32
        }
    };

    // 2. Repayment component
    let total_owed = loan_record.amount + loan_record.total_yield;
    let repayment_progress_bps: u32 = if total_owed > 0 {
        (loan_record.amount_repaid * 10_000 / total_owed).min(10_000) as u32
    } else {
        10_000
    };
    let repayment_score: u32 = repayment_progress_bps / 100;

    // 3. History component (inverted risk score)
    let default_count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::DefaultCount(borrower.clone()))
        .unwrap_or(0);
    let loan_count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::LoanCount(borrower.clone()))
        .unwrap_or(0);
    let repayment_count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::RepaymentCount(borrower.clone()))
        .unwrap_or(0);
    let numerator = (default_count as i128) * 10_000;
    let denominator = (loan_count as i128) + (repayment_count as i128) + 1;
    let borrower_risk_score = (numerator / denominator).min(10_000);
    let history_score: u32 = (100 - (borrower_risk_score / 100).min(100)) as u32;

    // 4. Concentration component
    let vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .unwrap_or_else(|| Vec::new(&env));
    let total_stake: i128 = vouches
        .iter()
        .filter(|v| v.token == loan_record.token_address)
        .map(|v| v.amount)
        .sum();
    let max_stake: i128 = vouches
        .iter()
        .filter(|v| v.token == loan_record.token_address)
        .map(|v| v.amount)
        .fold(0i128, |acc, s| if s > acc { s } else { acc });
    let top_voucher_concentration_bps: u32 = if total_stake > 0 {
        (max_stake * 10_000 / total_stake).min(10_000) as u32
    } else {
        10_000
    };
    let concentration_score: u32 = if top_voucher_concentration_bps >= thresholds.concentration_risk_bps {
        0
    } else {
        100 - (top_voucher_concentration_bps / 100).min(100)
    };

    let score = (time_score + repayment_score + history_score + concentration_score) / 4;

    let risk_level = if seconds_until_deadline <= thresholds.critical_deadline_secs
        || repayment_progress_bps < thresholds.at_risk_repayment_bps / 4
    {
        RiskLevel::Critical
    } else if seconds_until_deadline <= thresholds.at_risk_deadline_secs
        || repayment_progress_bps < thresholds.at_risk_repayment_bps
    {
        RiskLevel::AtRisk
    } else {
        RiskLevel::Healthy
    };

    // Emit warning event when at-risk or critical
    if risk_level != RiskLevel::Healthy {
        env.events().publish(
            (symbol_short!("health"), symbol_short!("at_risk")),
            (borrower.clone(), loan_record.id, score),
        );
    }

    Some(LoanHealthScore {
        borrower,
        loan_id: loan_record.id,
        score,
        risk_level,
        seconds_until_deadline,
        repayment_progress_bps,
        borrower_risk_score,
        top_voucher_concentration_bps,
    })
}

// ── Dashboard query functions ─────────────────────────────────────────────────

/// Return health scores for all active loans that are at-risk or critical.
pub fn get_at_risk_loans(env: Env) -> Vec<LoanHealthScore> {
    let borrower_list: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::BorrowerList)
        .unwrap_or_else(|| Vec::new(&env));

    let mut results: Vec<LoanHealthScore> = Vec::new(&env);
    for borrower in borrower_list.iter() {
        if let Some(health) = get_loan_health(env.clone(), borrower) {
            if health.risk_level != RiskLevel::Healthy {
                results.push_back(health);
            }
        }
    }
    results
}

/// Return the total active stake exposure for a voucher across all active loans.
pub fn get_voucher_exposure(env: Env, voucher: Address) -> ExposureReport {
    let backed_borrowers: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::VoucherHistory(voucher.clone()))
        .unwrap_or_else(|| Vec::new(&env));

    let thresholds = get_alert_thresholds(&env);
    let now = env.ledger().timestamp();
    let mut total_active_stake: i128 = 0;
    let mut active_loan_count: u32 = 0;
    let mut at_risk_count: u32 = 0;

    for borrower in backed_borrowers.iter() {
        let loan_id_opt: Option<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveLoan(borrower.clone()));
        let loan_id = match loan_id_opt {
            Some(id) => id,
            None => continue,
        };
        let loan_record: LoanRecord = match env
            .storage()
            .persistent()
            .get(&DataKey::Loan(loan_id))
        {
            Some(r) => r,
            None => continue,
        };
        if loan_record.status != LoanStatus::Active {
            continue;
        }

        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or_else(|| Vec::new(&env));

        for v in vouches.iter() {
            if v.voucher == voucher {
                total_active_stake += v.amount;
                active_loan_count += 1;

                let seconds_until_deadline = if now >= loan_record.deadline {
                    0u64
                } else {
                    loan_record.deadline - now
                };
                let total_owed = loan_record.amount + loan_record.total_yield;
                let repayment_bps: u32 = if total_owed > 0 {
                    (loan_record.amount_repaid * 10_000 / total_owed).min(10_000) as u32
                } else {
                    10_000
                };
                if seconds_until_deadline <= thresholds.at_risk_deadline_secs
                    || repayment_bps < thresholds.at_risk_repayment_bps
                {
                    at_risk_count += 1;
                }
                break;
            }
        }
    }

    ExposureReport {
        voucher,
        total_active_stake,
        active_loan_count,
        at_risk_count,
    }
}

/// Return a protocol-wide health summary.
pub fn get_protocol_health(env: Env) -> ProtocolHealthReport {
    let borrower_list: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::BorrowerList)
        .unwrap_or_else(|| Vec::new(&env));

    let thresholds = get_alert_thresholds(&env);
    let now = env.ledger().timestamp();
    let mut active_loan_count: u32 = 0;
    let mut at_risk_loan_count: u32 = 0;
    let mut total_outstanding: i128 = 0;
    let mut total_locked_stake: i128 = 0;

    for borrower in borrower_list.iter() {
        let loan_id_opt: Option<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveLoan(borrower.clone()));
        let loan_id = match loan_id_opt {
            Some(id) => id,
            None => continue,
        };
        let loan_record: LoanRecord = match env
            .storage()
            .persistent()
            .get(&DataKey::Loan(loan_id))
        {
            Some(r) => r,
            None => continue,
        };
        if loan_record.status != LoanStatus::Active {
            continue;
        }

        active_loan_count += 1;
        total_outstanding += loan_record.amount - loan_record.amount_repaid.min(loan_record.amount);

        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        let stake: i128 = vouches
            .iter()
            .filter(|v| v.token == loan_record.token_address)
            .map(|v| v.amount)
            .sum();
        total_locked_stake += stake;

        let seconds_until_deadline = if now >= loan_record.deadline {
            0u64
        } else {
            loan_record.deadline - now
        };
        let total_owed = loan_record.amount + loan_record.total_yield;
        let repayment_bps: u32 = if total_owed > 0 {
            (loan_record.amount_repaid * 10_000 / total_owed).min(10_000) as u32
        } else {
            10_000
        };
        if seconds_until_deadline <= thresholds.at_risk_deadline_secs
            || repayment_bps < thresholds.at_risk_repayment_bps
        {
            at_risk_loan_count += 1;
        }
    }

    let config: Config = env
        .storage()
        .instance()
        .get(&DataKey::Config)
        .unwrap_or_else(|| panic!("not initialized"));
    let token_client = soroban_sdk::token::Client::new(&env, &config.token);
    let contract_balance = token_client.balance(&env.current_contract_address());

    ProtocolHealthReport {
        active_loan_count,
        at_risk_loan_count,
        total_outstanding,
        total_locked_stake,
        contract_balance,
    }
}
