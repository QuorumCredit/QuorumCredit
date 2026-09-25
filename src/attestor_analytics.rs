//! Attestor Analytics and Quorum Slice Monitoring (Issues #1607, #1609, #1610, #1611)
//!
//! This module provides comprehensive monitoring and analysis of attestors and quorum slices:
//! - Issue #1607: Attestor Collusion Detection — identifies suspicious attestor pairs
//! - Issue #1609: Attestor Availability Tracking — monitors attestor uptime
//! - Issue #1610: Quorum Slice Performance Prediction — predicts slice performance
//! - Issue #1611: Geographic Diversity Validation — ensures geographically distributed slices

extern crate alloc;

use crate::errors::ContractError;
use crate::types::DataKey;
use soroban_sdk::{contracttype, Address, Env, String, Vec};

// ── Issue #1607: Attestor Collusion Detection ─────────────────────────────────────

/// Represents a pair of attestors with a concordance score.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttestorPair {
    pub attestor_a: Address,
    pub attestor_b: Address,
    pub concordance: f32,
}

/// High concordance threshold indicating suspicious collusion (0.85 = 85% agreement).
pub const COLLUSION_THRESHOLD: f32 = 0.85;

/// Records consensus patterns between attestor pairs.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConcordanceRecord {
    pub attestor_a: Address,
    pub attestor_b: Address,
    /// Agreement percentage (0.0 to 1.0).
    pub concordance_score: f32,
    /// Number of observations contributing to this score.
    pub observation_count: u32,
    /// Timestamp of last update in seconds.
    pub last_updated: u64,
}

/// Detects collusion among attestors in a quorum slice by measuring concordance.
///
/// Analyzes voting/attestation patterns and returns pairs of attestors with suspiciously
/// high agreement rates. High-concordance pairs indicate potential collusion and require review.
///
/// # Arguments
/// * `env` – the environment
/// * `slice_id` – the quorum slice identifier
///
/// # Returns
/// Vec of (Attestor A, Attestor B, Concordance Score) tuples for suspicious pairs
pub fn detect_attestor_collusion(
    env: &Env,
    slice_id: u64,
) -> Result<Vec<AttestorPair>, ContractError> {
    let mut collusion_pairs: Vec<AttestorPair> = Vec::new(env);

    // Retrieve all attestors in this slice
    let slice_attestors: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::AttestorSliceMembers(slice_id))
        .unwrap_or_else(|| Vec::new(env));

    if slice_attestors.len() < 2 {
        return Ok(collusion_pairs);
    }

    // Compute concordance for all pairs
    for i in 0..slice_attestors.len() {
        for j in (i + 1)..slice_attestors.len() {
            let attestor_a = slice_attestors.get(i).unwrap();
            let attestor_b = slice_attestors.get(j).unwrap();

            // Retrieve concordance record for this pair
            let concordance_score = get_attestor_concordance(env, &attestor_a, &attestor_b)?;

            // Flag pairs exceeding collusion threshold
            if concordance_score >= COLLUSION_THRESHOLD {
                collusion_pairs.push_back(AttestorPair {
                    attestor_a: attestor_a.clone(),
                    attestor_b: attestor_b.clone(),
                    concordance: concordance_score,
                });

                // Record alert event
                record_collusion_alert(env, &attestor_a, &attestor_b, concordance_score)?;
            }
        }
    }

    Ok(collusion_pairs)
}

/// Retrieves the concordance score between two attestors.
fn get_attestor_concordance(
    env: &Env,
    attestor_a: &Address,
    attestor_b: &Address,
) -> Result<f32, ContractError> {
    let record: Option<ConcordanceRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::AttestorConcordance(attestor_a.clone(), attestor_b.clone()));

    match record {
        Some(r) => Ok(r.concordance_score),
        None => Ok(0.0),
    }
}

/// Records a collusion alert in the audit log.
fn record_collusion_alert(
    env: &Env,
    attestor_a: &Address,
    attestor_b: &Address,
    concordance: f32,
) -> Result<(), ContractError> {
    let ledger = env.ledger().sequence();
    let timestamp = env.ledger().timestamp();

    let alert_key = DataKey::CollusionAlert(
        attestor_a.clone(),
        attestor_b.clone(),
        timestamp,
    );

    env.storage()
        .persistent()
        .set(&alert_key, &(concordance, ledger));

    env.storage()
        .instance()
        .set(&DataKey::LastCollusionCheckLedger, &ledger);

    Ok(())
}

// ── Issue #1609: Attestor Availability Tracking ────────────────────────────────────

/// Tracks availability metrics for an attestor.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttestorAvailability {
    pub attestor: Address,
    /// Total checks performed.
    pub total_checks: u32,
    /// Successful responses.
    pub successful_checks: u32,
    /// Availability percentage (0-100).
    pub availability_percentage: u32,
    /// Timestamp of last check in seconds.
    pub last_check_timestamp: u64,
}

/// Threshold for low availability alerts (50% = requires attention).
pub const AVAILABILITY_THRESHOLD: u32 = 50;

/// Tracks availability of a given attestor.
///
/// Records periodic availability checks and computes uptime percentage.
/// Triggers notifications when availability falls below thresholds.
///
/// # Arguments
/// * `env` – the environment
/// * `attestor` – the attestor address to track
/// * `is_available` – whether the attestor responded to health check
///
/// # Returns
/// Updated `AttestorAvailability` record
pub fn track_attestor_availability(
    env: &Env,
    attestor: &Address,
    is_available: bool,
) -> Result<AttestorAvailability, ContractError> {
    let mut availability: AttestorAvailability = env
        .storage()
        .persistent()
        .get(&DataKey::AttestorAvailability(attestor.clone()))
        .unwrap_or(AttestorAvailability {
            attestor: attestor.clone(),
            total_checks: 0,
            successful_checks: 0,
            availability_percentage: 100,
            last_check_timestamp: 0,
        });

    availability.total_checks += 1;
    if is_available {
        availability.successful_checks += 1;
    }

    availability.availability_percentage = if availability.total_checks == 0 {
        100
    } else {
        ((availability.successful_checks as u128 * 100) / availability.total_checks as u128) as u32
    };

    let timestamp = env.ledger().timestamp();
    availability.last_check_timestamp = timestamp;

    // Persist updated availability
    env.storage()
        .persistent()
        .set(&DataKey::AttestorAvailability(attestor.clone()), &availability);

    // Check if availability is low and trigger notification
    if availability.availability_percentage < AVAILABILITY_THRESHOLD {
        record_availability_alert(env, attestor, availability.availability_percentage)?;
    }

    Ok(availability)
}

/// Records a low-availability alert.
fn record_availability_alert(
    env: &Env,
    attestor: &Address,
    availability: u32,
) -> Result<(), ContractError> {
    let timestamp = env.ledger().timestamp();
    let alert_key = DataKey::AvailabilityAlert(attestor.clone(), timestamp);

    env.storage()
        .persistent()
        .set(&alert_key, &availability);

    Ok(())
}

// ── Issue #1610: Quorum Slice Performance Prediction ─────────────────────────────

/// Represents predicted performance metrics for a quorum slice.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PerformancePrediction {
    pub slice_id: u64,
    /// Predicted consensus time in seconds.
    pub predicted_consensus_time_secs: u32,
    /// Predicted reliability score (0-100).
    pub predicted_reliability: u32,
    /// Confidence level in prediction (0-100).
    pub confidence_level: u32,
    /// Number of historical data points used.
    pub sample_size: u32,
    /// Timestamp of prediction in seconds.
    pub prediction_timestamp: u64,
}

/// Historical performance data point for a slice.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlicePerformanceRecord {
    pub slice_id: u64,
    /// Consensus time for this observation in seconds.
    pub consensus_time_secs: u32,
    /// Whether consensus was successful (true) or failed (false).
    pub consensus_success: bool,
    /// Timestamp of observation in seconds.
    pub timestamp: u64,
}

/// Minimum observations needed for reliable prediction.
pub const MIN_PERFORMANCE_SAMPLES: u32 = 5;

/// Predicts future performance of a quorum slice based on historical data.
///
/// Analyzes past consensus times and success rates to forecast reliability.
/// Provides confidence metrics to indicate prediction reliability.
///
/// # Arguments
/// * `env` – the environment
/// * `slice_id` – the quorum slice identifier
///
/// # Returns
/// `PerformancePrediction` with consensus time and reliability estimates
pub fn predict_slice_performance(
    env: &Env,
    slice_id: u64,
) -> Result<PerformancePrediction, ContractError> {
    // Retrieve historical performance data
    let history: Vec<SlicePerformanceRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::SlicePerformanceHistory(slice_id))
        .unwrap_or_else(|| Vec::new(env));

    let sample_size = history.len() as u32;

    if sample_size < MIN_PERFORMANCE_SAMPLES {
        // Insufficient data: return conservative estimate
        return Ok(PerformancePrediction {
            slice_id,
            predicted_consensus_time_secs: 30,
            predicted_reliability: 70,
            confidence_level: 20,
            sample_size,
            prediction_timestamp: env.ledger().timestamp(),
        });
    }

    // Calculate metrics from historical data
    let (avg_consensus_time, success_rate) = compute_slice_metrics(&history)?;

    // Confidence increases with more samples, capped at 95
    let confidence = core::cmp::min(20 + (sample_size / 2), 95);

    Ok(PerformancePrediction {
        slice_id,
        predicted_consensus_time_secs: avg_consensus_time,
        predicted_reliability: success_rate,
        confidence_level: confidence,
        sample_size,
        prediction_timestamp: env.ledger().timestamp(),
    })
}

/// Computes average consensus time and success rate from performance records.
fn compute_slice_metrics(
    history: &Vec<SlicePerformanceRecord>,
) -> Result<(u32, u32), ContractError> {
    if history.is_empty() {
        return Ok((30, 80));
    }

    let mut total_time: u64 = 0;
    let mut successful_count: u32 = 0;

    for record in history.iter() {
        total_time += record.consensus_time_secs as u64;
        if record.consensus_success {
            successful_count += 1;
        }
    }

    let avg_time = (total_time / history.len() as u64) as u32;
    let success_rate = ((successful_count as u64 * 100) / history.len() as u64) as u32;

    Ok((avg_time, success_rate))
}

/// Records a performance observation for a slice.
pub fn record_slice_performance(
    env: &Env,
    slice_id: u64,
    consensus_time_secs: u32,
    consensus_success: bool,
) -> Result<(), ContractError> {
    let mut history: Vec<SlicePerformanceRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::SlicePerformanceHistory(slice_id))
        .unwrap_or_else(|| Vec::new(env));

    history.push_back(SlicePerformanceRecord {
        slice_id,
        consensus_time_secs,
        consensus_success,
        timestamp: env.ledger().timestamp(),
    });

    // Keep only recent history (last 100 observations)
    if history.len() > 100 {
        let start_idx = history.len() - 100;
        let trimmed: Vec<SlicePerformanceRecord> = history.iter().skip(start_idx).collect();
        history = trimmed;
    }

    env.storage()
        .persistent()
        .set(&DataKey::SlicePerformanceHistory(slice_id), &history);

    Ok(())
}

// ── Issue #1611: Attestor Geographic Diversity ────────────────────────────────────

/// Geographic metadata for an attestor.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttestorLocation {
    pub attestor: Address,
    /// ISO 3166-1 alpha-2 country code (e.g., "US", "CN", "JP").
    pub country_code: String,
    /// Geographical region (e.g., "North America", "Asia", "Europe").
    pub region: String,
}

/// Diversity score for a quorum slice.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiversityScore {
    pub slice_id: u64,
    /// Number of unique countries.
    pub unique_countries: u32,
    /// Number of unique regions.
    pub unique_regions: u32,
    /// Diversity score (0-100).
    pub diversity_score: u32,
    /// Whether the slice meets minimum diversity requirements.
    pub meets_requirements: bool,
}

/// Minimum unique countries required in a slice.
pub const MIN_COUNTRIES: u32 = 2;

/// Minimum unique regions required in a slice.
pub const MIN_REGIONS: u32 = 2;

/// Validates that a quorum slice has geographically diverse attestors.
///
/// Ensures that attestors are distributed across multiple countries and regions
/// to reduce correlated failure risk. Returns false if diversity requirements not met.
///
/// # Arguments
/// * `env` – the environment
/// * `slice_id` – the quorum slice identifier
///
/// # Returns
/// Boolean indicating whether slice meets geographic diversity requirements
pub fn validate_geographic_diversity(
    env: &Env,
    slice_id: u64,
) -> Result<bool, ContractError> {
    let diversity = calculate_geographic_diversity(env, slice_id)?;
    Ok(diversity.meets_requirements)
}

/// Calculates detailed geographic diversity metrics for a slice.
pub fn calculate_geographic_diversity(
    env: &Env,
    slice_id: u64,
) -> Result<DiversityScore, ContractError> {
    // Get attestors in this slice
    let attestors: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::AttestorSliceMembers(slice_id))
        .unwrap_or_else(|| Vec::new(env));

    let mut countries: Vec<String> = Vec::new(env);
    let mut regions: Vec<String> = Vec::new(env);

    // Collect unique countries and regions
    for attestor in attestors.iter() {
        let location: Option<AttestorLocation> = env
            .storage()
            .persistent()
            .get(&DataKey::AttestorLocation(attestor.clone()));

        if let Some(loc) = location {
            // Add country if not already present
            let mut country_found = false;
            for c in countries.iter() {
                if c == &loc.country_code {
                    country_found = true;
                    break;
                }
            }
            if !country_found {
                countries.push_back(loc.country_code.clone());
            }

            // Add region if not already present
            let mut region_found = false;
            for r in regions.iter() {
                if r == &loc.region {
                    region_found = true;
                    break;
                }
            }
            if !region_found {
                regions.push_back(loc.region);
            }
        }
    }

    let unique_countries = countries.len() as u32;
    let unique_regions = regions.len() as u32;

    // Calculate diversity score: 50 points for countries + 50 for regions
    let country_score = if unique_countries >= MIN_COUNTRIES {
        50
    } else {
        (unique_countries as u32 * 25) / MIN_COUNTRIES
    };

    let region_score = if unique_regions >= MIN_REGIONS {
        50
    } else {
        (unique_regions as u32 * 25) / MIN_REGIONS
    };

    let diversity_score = country_score + region_score;

    let meets_requirements = unique_countries >= MIN_COUNTRIES && unique_regions >= MIN_REGIONS;

    Ok(DiversityScore {
        slice_id,
        unique_countries,
        unique_regions,
        diversity_score,
        meets_requirements,
    })
}

/// Sets geographic metadata for an attestor.
pub fn set_attestor_location(
    env: &Env,
    attestor: &Address,
    country_code: String,
    region: String,
) -> Result<(), ContractError> {
    let location = AttestorLocation {
        attestor: attestor.clone(),
        country_code,
        region,
    };

    env.storage()
        .persistent()
        .set(&DataKey::AttestorLocation(attestor.clone()), &location);

    Ok(())
}

/// Retrieves geographic metadata for an attestor.
pub fn get_attestor_location(
    env: &Env,
    attestor: &Address,
) -> Option<AttestorLocation> {
    env.storage()
        .persistent()
        .get(&DataKey::AttestorLocation(attestor.clone()))
}
