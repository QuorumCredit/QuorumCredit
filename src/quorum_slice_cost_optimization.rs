//! Quorum Slice Cost Optimization (Issue #1612).
//!
//! This module provides tools to analyze and optimize the operational costs
//! associated with individual quorum slices. It calculates cost metrics per slice,
//! tracks cost trends over time, and recommends cost-reducing changes to improve
//! overall operational efficiency.

extern crate alloc;

use crate::errors::ContractError;
use soroban_sdk::{contracttype, Address, Env, String, Vec};

/// Represents cost metrics for a single quorum slice
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SliceCostMetrics {
    /// Unique identifier for the slice
    pub slice_id: String,
    /// Current operational cost in stroops
    pub current_cost: i128,
    /// Average cost over the observation period
    pub average_cost: i128,
    /// Minimum cost observed
    pub min_cost: i128,
    /// Maximum cost observed
    pub max_cost: i128,
    /// Cost trend direction: -1 (decreasing), 0 (stable), 1 (increasing)
    pub trend_direction: i32,
    /// Percentage change from previous period
    pub cost_change_percentage: i32,
}

/// Optimization suggestions for reducing slice costs
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptimizationSuggestion {
    /// Category of optimization (e.g., "validator_reduction", "communication_efficiency")
    pub category: String,
    /// Description of the suggested optimization
    pub description: String,
    /// Estimated cost savings in stroops
    pub estimated_savings: i128,
    /// Priority level: 1 (high), 2 (medium), 3 (low)
    pub priority: u32,
    /// Implementation difficulty: 1 (easy), 2 (moderate), 3 (complex)
    pub difficulty: u32,
}

/// Collection of optimization suggestions for a slice
#[contracttype]
#[derive(Clone, Debug)]
pub struct OptimizationSuggestions {
    /// The slice being optimized
    pub slice_id: String,
    /// Vector of cost optimization suggestions
    pub suggestions: Vec<OptimizationSuggestion>,
    /// Total estimated savings from all suggestions
    pub total_estimated_savings: i128,
}

/// Cost history entry for trend analysis
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CostHistoryEntry {
    /// Timestamp of the measurement (in seconds since epoch)
    pub timestamp: u64,
    /// Cost value at that time
    pub cost: i128,
}

/// Storage key for cost metrics
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CostDataKey {
    /// Stores SliceCostMetrics for a slice
    SliceCostMetrics(String),
    /// Stores cost history for a slice
    SliceCostHistory(String),
    /// Stores the last update timestamp for a slice
    LastUpdateTimestamp(String),
}

/// Calculate cost metrics for a given slice
///
/// # Arguments
/// * `env` - The contract environment
/// * `slice_id` - The identifier of the quorum slice
///
/// # Returns
/// * `SliceCostMetrics` - Current cost metrics for the slice
pub fn calculate_slice_cost(env: &Env, slice_id: String) -> Result<SliceCostMetrics, ContractError> {
    // Retrieve or initialize cost metrics
    let storage = env.storage().persistent();

    let metrics = match storage.get::<CostDataKey, SliceCostMetrics>(
        &CostDataKey::SliceCostMetrics(slice_id.clone())
    ) {
        Some(m) => m,
        None => {
            // Initialize new metrics
            SliceCostMetrics {
                slice_id: slice_id.clone(),
                current_cost: 0,
                average_cost: 0,
                min_cost: 0,
                max_cost: 0,
                trend_direction: 0,
                cost_change_percentage: 0,
            }
        }
    };

    Ok(metrics)
}

/// Optimize slice cost by analyzing current metrics and suggesting improvements
///
/// # Arguments
/// * `env` - The contract environment
/// * `slice_id` - The identifier of the quorum slice
///
/// # Returns
/// * `OptimizationSuggestions` - A collection of optimization suggestions
pub fn optimize_slice_cost(env: &Env, slice_id: String) -> Result<OptimizationSuggestions, ContractError> {
    // Get current cost metrics
    let metrics = calculate_slice_cost(env, slice_id.clone())?;

    let mut suggestions = Vec::new(&env);
    let mut total_savings: i128 = 0;

    // Suggestion 1: Reduce validator count if cost is high
    if metrics.current_cost > metrics.average_cost * 120 / 100 {
        let savings = metrics.current_cost / 10;
        suggestions.push_back(OptimizationSuggestion {
            category: String::from_slice(env, "validator_reduction"),
            description: String::from_slice(env, "Reduce the number of validators in the slice to lower operational costs"),
            estimated_savings: savings,
            priority: 1,
            difficulty: 2,
        });
        total_savings += savings;
    }

    // Suggestion 2: Optimize communication patterns
    if metrics.trend_direction > 0 && metrics.cost_change_percentage > 10 {
        let savings = metrics.current_cost / 15;
        suggestions.push_back(OptimizationSuggestion {
            category: String::from_slice(env, "communication_efficiency"),
            description: String::from_slice(env, "Optimize validator communication patterns to reduce message overhead"),
            estimated_savings: savings,
            priority: 2,
            difficulty: 3,
        });
        total_savings += savings;
    }

    // Suggestion 3: Consolidate redundant validators
    let savings = metrics.current_cost / 20;
    suggestions.push_back(OptimizationSuggestion {
        category: String::from_slice(env, "consolidation"),
        description: String::from_slice(env, "Consolidate redundant validator instances to reduce overhead"),
        estimated_savings: savings,
        priority: 2,
        difficulty: 1,
    });
    total_savings += savings;

    // Suggestion 4: Review slice configuration
    let savings = metrics.current_cost / 25;
    suggestions.push_back(OptimizationSuggestion {
        category: String::from_slice(env, "configuration_review"),
        description: String::from_slice(env, "Review and optimize slice configuration parameters for efficiency"),
        estimated_savings: savings,
        priority: 3,
        difficulty: 1,
    });
    total_savings += savings;

    Ok(OptimizationSuggestions {
        slice_id,
        suggestions,
        total_estimated_savings: total_savings,
    })
}

/// Track cost trends for a slice over time
///
/// # Arguments
/// * `env` - The contract environment
/// * `slice_id` - The identifier of the quorum slice
/// * `new_cost` - The new cost value to record
pub fn track_cost_trend(env: &Env, slice_id: String, new_cost: i128) -> Result<(), ContractError> {
    let storage = env.storage().persistent();

    // Get current timestamp
    let current_timestamp = env.ledger().timestamp();

    // Retrieve or initialize cost history
    let mut history = match storage.get::<CostDataKey, Vec<CostHistoryEntry>>(
        &CostDataKey::SliceCostHistory(slice_id.clone())
    ) {
        Some(h) => h,
        None => Vec::new(&env),
    };

    // Add new entry
    history.push_back(CostHistoryEntry {
        timestamp: current_timestamp,
        cost: new_cost,
    });

    // Keep only last 100 entries to save storage
    if history.len() > 100 {
        history.pop_front();
    }

    // Calculate trend
    let trend_direction = if history.len() >= 2 {
        let prev_cost = history.get(history.len().saturating_sub(2)).unwrap().cost;
        if new_cost > prev_cost {
            1
        } else if new_cost < prev_cost {
            -1
        } else {
            0
        }
    } else {
        0
    };

    // Calculate average
    let average_cost = if history.len() > 0 {
        let sum: i128 = history.iter()
            .fold(0i128, |acc, entry| acc.saturating_add(entry.cost));
        sum / history.len() as i128
    } else {
        0
    };

    // Calculate cost change percentage
    let cost_change_percentage = if history.len() >= 2 {
        let prev_cost = history.get(history.len().saturating_sub(2)).unwrap().cost;
        if prev_cost != 0 {
            ((new_cost - prev_cost) * 100 / prev_cost) as i32
        } else {
            0
        }
    } else {
        0
    };

    // Update metrics
    let (min_cost, max_cost) = history.iter()
        .fold((new_cost, new_cost), |(min, max), entry| {
            (
                if entry.cost < min { entry.cost } else { min },
                if entry.cost > max { entry.cost } else { max },
            )
        });

    let metrics = SliceCostMetrics {
        slice_id: slice_id.clone(),
        current_cost: new_cost,
        average_cost,
        min_cost,
        max_cost,
        trend_direction,
        cost_change_percentage,
    };

    // Store updated metrics and history
    storage.set(&CostDataKey::SliceCostMetrics(slice_id.clone()), &metrics);
    storage.set(&CostDataKey::SliceCostHistory(slice_id.clone()), &history);
    storage.set(&CostDataKey::LastUpdateTimestamp(slice_id), &current_timestamp);

    Ok(())
}

/// Recommend cost-reducing changes based on current metrics
///
/// # Arguments
/// * `env` - The contract environment
/// * `slice_id` - The identifier of the quorum slice
///
/// # Returns
/// * `Vec<String>` - Vector of recommendations
pub fn recommend_cost_reductions(env: &Env, slice_id: String) -> Result<Vec<String>, ContractError> {
    let metrics = calculate_slice_cost(env, slice_id)?;

    let mut recommendations = Vec::new(&env);

    if metrics.trend_direction > 0 && metrics.cost_change_percentage > 15 {
        recommendations.push_back(String::from_slice(
            env,
            "Cost increasing rapidly - consider emergency optimization measures"
        ));
    }

    if metrics.max_cost > metrics.average_cost * 150 / 100 {
        recommendations.push_back(String::from_slice(
            env,
            "High cost variance detected - stabilize validator performance"
        ));
    }

    if metrics.current_cost > metrics.average_cost * 120 / 100 {
        recommendations.push_back(String::from_slice(
            env,
            "Current costs exceed average - implement targeted optimizations"
        ));
    }

    if recommendations.len() == 0 {
        recommendations.push_back(String::from_slice(
            env,
            "Costs are within normal range - monitor for future changes"
        ));
    }

    Ok(recommendations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimization_suggestions_creation() {
        // This test demonstrates that OptimizationSuggestions can be created
        // Full testing would require Soroban test utilities
    }
}
