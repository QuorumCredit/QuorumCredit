//! Attestor Bankruptcy Detection (Issue #1613).
//!
//! This module provides tools to monitor the financial health of attestors
//! and detect signs of financial distress or bankruptcy risk. It maintains
//! financial health metrics, performs health assessments, and flags
//! financially unstable attestors to prevent trust in failed entities.

extern crate alloc;

use crate::errors::ContractError;
use soroban_sdk::{contracttype, Address, Env, String, Vec};

/// Health status levels for attestors
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HealthStatus {
    Healthy,
    AtRisk,
    Unstable,
    Bankrupt,
}

/// Financial metrics for an attestor
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinancialMetrics {
    /// Address of the attestor
    pub attestor_address: Address,
    /// Total assets in stroops
    pub total_assets: i128,
    /// Total liabilities in stroops
    pub total_liabilities: i128,
    /// Available liquidity in stroops
    pub available_liquidity: i128,
    /// Number of active attestations
    pub active_attestations: u32,
    /// Number of failed attestations
    pub failed_attestations: u32,
    /// Failure rate as a percentage (0-100)
    pub failure_rate_percentage: u32,
    /// Last update timestamp
    pub last_update: u64,
}

/// Health assessment result for an attestor
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthAssessment {
    /// Address of the attestor
    pub attestor_address: Address,
    /// Current health status
    pub status: HealthStatus,
    /// Risk score from 0 (no risk) to 100 (extreme risk)
    pub risk_score: u32,
    /// Key health indicators
    pub solvency_ratio: i32,      // assets / liabilities * 100
    pub liquidity_ratio: i32,      // available_liquidity / liabilities * 100
    pub failure_rate: u32,         // percentage of failed attestations
    /// Timestamp of assessment
    pub assessment_timestamp: u64,
    /// Detailed reasons for the assessment
    pub reasons: Vec<String>,
}

/// Risk scoring result
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RiskScore {
    /// Address of the attestor
    pub attestor_address: Address,
    /// Overall risk score
    pub overall_score: u32,
    /// Score based on financial metrics
    pub financial_risk: u32,
    /// Score based on operational metrics
    pub operational_risk: u32,
    /// Score based on historical performance
    pub historical_risk: u32,
}

/// Storage key for financial data
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BankruptcyDataKey {
    /// Stores FinancialMetrics for an attestor
    FinancialMetrics(Address),
    /// Stores HealthAssessment for an attestor
    HealthAssessment(Address),
    /// Stores flagged attestors
    FlaggedAttestors,
    /// Stores bankruptcy history
    BankruptcyHistory(Address),
}

/// Check the financial health of an attestor
///
/// # Arguments
/// * `env` - The contract environment
/// * `attestor` - The address of the attestor to check
///
/// # Returns
/// * `HealthStatus` - The health status of the attestor
pub fn check_attestor_financial_health(env: &Env, attestor: Address) -> Result<HealthStatus, ContractError> {
    let storage = env.storage().persistent();

    let metrics = match storage.get::<BankruptcyDataKey, FinancialMetrics>(
        &BankruptcyDataKey::FinancialMetrics(attestor.clone())
    ) {
        Some(m) => m,
        None => {
            // If no metrics exist, assume healthy (new attestor)
            return Ok(HealthStatus::Healthy);
        }
    };

    // Determine health status based on metrics
    let status = determine_health_status(&metrics);

    Ok(status)
}

/// Determine health status based on financial metrics
fn determine_health_status(metrics: &FinancialMetrics) -> HealthStatus {
    // Bankruptcy condition: liabilities exceed assets
    if metrics.total_liabilities > 0 && metrics.total_assets <= metrics.total_liabilities {
        return HealthStatus::Bankrupt;
    }

    // Calculate key ratios
    let solvency_ratio = if metrics.total_liabilities > 0 {
        metrics.total_assets * 100 / metrics.total_liabilities
    } else {
        100
    };

    let liquidity_ratio = if metrics.total_liabilities > 0 {
        metrics.available_liquidity * 100 / metrics.total_liabilities
    } else {
        100
    };

    // Unstable condition: low solvency or liquidity
    if solvency_ratio < 120 || liquidity_ratio < 30 || metrics.failure_rate_percentage > 50 {
        return HealthStatus::Unstable;
    }

    // At risk condition: moderate concerns
    if solvency_ratio < 150 || liquidity_ratio < 50 || metrics.failure_rate_percentage > 25 {
        return HealthStatus::AtRisk;
    }

    HealthStatus::Healthy
}

/// Assess the financial health with detailed information
///
/// # Arguments
/// * `env` - The contract environment
/// * `attestor` - The address of the attestor to assess
///
/// # Returns
/// * `HealthAssessment` - Detailed health assessment
pub fn assess_financial_health(env: &Env, attestor: Address) -> Result<HealthAssessment, ContractError> {
    let storage = env.storage().persistent();

    let metrics = match storage.get::<BankruptcyDataKey, FinancialMetrics>(
        &BankruptcyDataKey::FinancialMetrics(attestor.clone())
    ) {
        Some(m) => m,
        None => {
            // Return default healthy assessment for new attestor
            return Ok(HealthAssessment {
                attestor_address: attestor,
                status: HealthStatus::Healthy,
                risk_score: 0,
                solvency_ratio: 100,
                liquidity_ratio: 100,
                failure_rate: 0,
                assessment_timestamp: env.ledger().timestamp(),
                reasons: Vec::new(&env),
            });
        }
    };

    // Calculate ratios
    let solvency_ratio = if metrics.total_liabilities > 0 {
        (metrics.total_assets * 100 / metrics.total_liabilities) as i32
    } else {
        100
    };

    let liquidity_ratio = if metrics.total_liabilities > 0 {
        (metrics.available_liquidity * 100 / metrics.total_liabilities) as i32
    } else {
        100
    };

    let status = determine_health_status(&metrics);
    let risk_score = calculate_risk_score(&metrics);

    let mut reasons = Vec::new(&env);

    // Add reasons based on metrics
    if metrics.total_liabilities > 0 && metrics.total_assets <= metrics.total_liabilities {
        reasons.push_back(String::from_slice(env, "Assets do not exceed liabilities - bankruptcy risk"));
    }

    if solvency_ratio < 120 {
        reasons.push_back(String::from_slice(env, "Low solvency ratio - insufficient assets relative to liabilities"));
    }

    if liquidity_ratio < 30 {
        reasons.push_back(String::from_slice(env, "Critical liquidity shortage - inability to meet short-term obligations"));
    }

    if metrics.failure_rate_percentage > 50 {
        reasons.push_back(String::from_slice(env, "High attestation failure rate - poor operational performance"));
    }

    if metrics.available_liquidity < metrics.total_liabilities / 3 {
        reasons.push_back(String::from_slice(env, "Insufficient available liquidity for current obligations"));
    }

    Ok(HealthAssessment {
        attestor_address: attestor,
        status,
        risk_score,
        solvency_ratio,
        liquidity_ratio,
        failure_rate: metrics.failure_rate_percentage,
        assessment_timestamp: env.ledger().timestamp(),
        reasons,
    })
}

/// Calculate risk score based on financial metrics
fn calculate_risk_score(metrics: &FinancialMetrics) -> u32 {
    let mut score: u32 = 0;

    // Bankruptcy risk (50 points)
    if metrics.total_liabilities > 0 && metrics.total_assets <= metrics.total_liabilities {
        score += 50;
    } else if metrics.total_assets * 100 / metrics.total_liabilities < 120 {
        score += 40;
    } else if metrics.total_assets * 100 / metrics.total_liabilities < 150 {
        score += 30;
    }

    // Liquidity risk (30 points)
    if metrics.total_liabilities > 0 {
        let liquidity_ratio = metrics.available_liquidity * 100 / metrics.total_liabilities;
        if liquidity_ratio < 30 {
            score += 30;
        } else if liquidity_ratio < 50 {
            score += 20;
        } else if liquidity_ratio < 100 {
            score += 10;
        }
    }

    // Operational risk based on failure rate (20 points)
    if metrics.failure_rate_percentage > 50 {
        score += 20;
    } else if metrics.failure_rate_percentage > 25 {
        score += 10;
    }

    // Cap score at 100
    if score > 100 { 100 } else { score }
}

/// Flag financially unstable attestors
///
/// # Arguments
/// * `env` - The contract environment
/// * `attestor` - The address of the attestor
/// * `is_flagged` - True to flag, false to unflag
pub fn flag_unstable_attestor(env: &Env, attestor: Address, is_flagged: bool) -> Result<(), ContractError> {
    let storage = env.storage().persistent();

    let mut flagged = match storage.get::<BankruptcyDataKey, Vec<Address>>(
        &BankruptcyDataKey::FlaggedAttestors
    ) {
        Some(f) => f,
        None => Vec::new(&env),
    };

    if is_flagged {
        // Check if already flagged
        let is_already_flagged = flagged.iter().any(|a| a == &attestor);
        if !is_already_flagged {
            flagged.push_back(attestor);
        }
    } else {
        // Remove from flagged list
        let mut new_flagged = Vec::new(&env);
        for a in flagged.iter() {
            if a != &attestor {
                new_flagged.push_back(a.clone());
            }
        }
        flagged = new_flagged;
    }

    storage.set(&BankruptcyDataKey::FlaggedAttestors, &flagged);

    Ok(())
}

/// Update financial metrics for an attestor
///
/// # Arguments
/// * `env` - The contract environment
/// * `attestor` - The address of the attestor
/// * `total_assets` - Total assets in stroops
/// * `total_liabilities` - Total liabilities in stroops
/// * `available_liquidity` - Available liquidity in stroops
/// * `active_attestations` - Number of active attestations
/// * `failed_attestations` - Number of failed attestations
pub fn update_financial_metrics(
    env: &Env,
    attestor: Address,
    total_assets: i128,
    total_liabilities: i128,
    available_liquidity: i128,
    active_attestations: u32,
    failed_attestations: u32,
) -> Result<(), ContractError> {
    let storage = env.storage().persistent();

    let failure_rate_percentage = if active_attestations > 0 {
        ((failed_attestations as i128 * 100) / (active_attestations as i128)) as u32
    } else {
        0
    };

    let metrics = FinancialMetrics {
        attestor_address: attestor.clone(),
        total_assets,
        total_liabilities,
        available_liquidity,
        active_attestations,
        failed_attestations,
        failure_rate_percentage,
        last_update: env.ledger().timestamp(),
    };

    storage.set(&BankruptcyDataKey::FinancialMetrics(attestor.clone()), &metrics);

    // Auto-flag if health status is unstable or bankrupt
    let status = determine_health_status(&metrics);
    if status == HealthStatus::Unstable || status == HealthStatus::Bankrupt {
        flag_unstable_attestor(env, attestor, true)?;
    } else {
        flag_unstable_attestor(env, attestor, false)?;
    }

    Ok(())
}

/// Check if an attestor is flagged as unstable
///
/// # Arguments
/// * `env` - The contract environment
/// * `attestor` - The address of the attestor
///
/// # Returns
/// * `bool` - True if the attestor is flagged, false otherwise
pub fn is_attestor_flagged(env: &Env, attestor: Address) -> Result<bool, ContractError> {
    let storage = env.storage().persistent();

    let flagged = match storage.get::<BankruptcyDataKey, Vec<Address>>(
        &BankruptcyDataKey::FlaggedAttestors
    ) {
        Some(f) => f,
        None => return Ok(false),
    };

    Ok(flagged.iter().any(|a| a == &attestor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_bankrupt() {
        // Bankruptcy test would verify that liabilities > assets results in Bankrupt status
    }
}
