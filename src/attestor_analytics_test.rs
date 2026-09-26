//! Tests for attestor analytics module (Issues #1607, #1609, #1610, #1611)

#[cfg(test)]
mod tests {
    use soroban_sdk::testutils::Address as AddressTestUtils;
    use soroban_sdk::{Address, Env, String, Vec};
    use crate::attestor_analytics::*;
    use crate::types::DataKey;
    use crate::ContractError;

    fn create_test_env() -> Env {
        Env::default()
    }

    fn create_test_address(id: u32) -> Address {
        Address::generate(&Env::default())
    }

    // ── Tests for Issue #1607: Attestor Collusion Detection ─────────────────────

    #[test]
    fn test_detect_attestor_collusion_no_collusion() {
        let env = create_test_env();
        let slice_id = 1u64;

        // Create two attestors with low concordance
        let attestor_a = create_test_address(1);
        let attestor_b = create_test_address(2);

        // Set up attestor slice membership
        let mut members: Vec<Address> = Vec::new(&env);
        members.push_back(attestor_a.clone());
        members.push_back(attestor_b.clone());
        env.storage()
            .persistent()
            .set(&DataKey::AttestorSliceMembers(slice_id), &members);

        // Set low concordance score
        let concordance = ConcordanceRecord {
            attestor_a: attestor_a.clone(),
            attestor_b: attestor_b.clone(),
            concordance_score: 0.5,
            observation_count: 10,
            last_updated: 0,
        };
        env.storage()
            .persistent()
            .set(
                &DataKey::AttestorConcordance(attestor_a.clone(), attestor_b.clone()),
                &concordance,
            );

        // Detect collusion - should return empty vec
        let result = detect_attestor_collusion(&env, slice_id).unwrap();
        assert_eq!(result.len(), 0, "Should detect no collusion at 50% concordance");
    }

    #[test]
    fn test_detect_attestor_collusion_with_collusion() {
        let env = create_test_env();
        let slice_id = 1u64;

        let attestor_a = create_test_address(1);
        let attestor_b = create_test_address(2);

        // Set up attestor slice membership
        let mut members: Vec<Address> = Vec::new(&env);
        members.push_back(attestor_a.clone());
        members.push_back(attestor_b.clone());
        env.storage()
            .persistent()
            .set(&DataKey::AttestorSliceMembers(slice_id), &members);

        // Set high concordance score (above threshold)
        let concordance = ConcordanceRecord {
            attestor_a: attestor_a.clone(),
            attestor_b: attestor_b.clone(),
            concordance_score: 0.9, // Above COLLUSION_THRESHOLD (0.85)
            observation_count: 10,
            last_updated: 0,
        };
        env.storage()
            .persistent()
            .set(
                &DataKey::AttestorConcordance(attestor_a.clone(), attestor_b.clone()),
                &concordance,
            );

        // Detect collusion - should return the suspicious pair
        let result = detect_attestor_collusion(&env, slice_id).unwrap();
        assert_eq!(result.len(), 1, "Should detect one collusive pair");
        assert_eq!(result.get(0).unwrap().concordance, 0.9);
    }

    #[test]
    fn test_detect_attestor_collusion_empty_slice() {
        let env = create_test_env();
        let slice_id = 99u64;

        // Empty slice (no members set)
        let result = detect_attestor_collusion(&env, slice_id).unwrap();
        assert_eq!(result.len(), 0, "Empty slice should have no collusion");
    }

    // ── Tests for Issue #1609: Attestor Availability Tracking ──────────────────

    #[test]
    fn test_track_attestor_availability_first_check() {
        let env = create_test_env();
        let attestor = create_test_address(1);

        // First check: available
        let record = track_attestor_availability(&env, &attestor, true).unwrap();

        assert_eq!(record.attestor, attestor);
        assert_eq!(record.total_checks, 1);
        assert_eq!(record.successful_checks, 1);
        assert_eq!(record.availability_percentage, 100);
    }

    #[test]
    fn test_track_attestor_availability_multiple_checks() {
        let env = create_test_env();
        let attestor = create_test_address(1);

        // First check: available
        track_attestor_availability(&env, &attestor, true).unwrap();

        // Second check: unavailable
        let record = track_attestor_availability(&env, &attestor, false).unwrap();

        assert_eq!(record.total_checks, 2);
        assert_eq!(record.successful_checks, 1);
        assert_eq!(record.availability_percentage, 50);
    }

    #[test]
    fn test_track_attestor_availability_low_threshold_alert() {
        let env = create_test_env();
        let attestor = create_test_address(1);

        // Simulate many failed checks
        for _ in 0..10 {
            track_attestor_availability(&env, &attestor, false).unwrap();
        }
        for _ in 0..2 {
            track_attestor_availability(&env, &attestor, true).unwrap();
        }

        let record = track_attestor_availability(&env, &attestor, false).unwrap();

        assert_eq!(record.total_checks, 13);
        assert_eq!(record.successful_checks, 2);
        assert!(record.availability_percentage < AVAILABILITY_THRESHOLD);
    }

    // ── Tests for Issue #1610: Quorum Slice Performance Prediction ──────────────

    #[test]
    fn test_predict_slice_performance_insufficient_data() {
        let env = create_test_env();
        let slice_id = 1u64;

        // Add just 2 performance records (below MIN_PERFORMANCE_SAMPLES of 5)
        let mut history: Vec<SlicePerformanceRecord> = Vec::new(&env);
        history.push_back(SlicePerformanceRecord {
            slice_id,
            consensus_time_secs: 30,
            consensus_success: true,
            timestamp: 1000,
        });
        history.push_back(SlicePerformanceRecord {
            slice_id,
            consensus_time_secs: 35,
            consensus_success: false,
            timestamp: 2000,
        });
        env.storage()
            .persistent()
            .set(&DataKey::SlicePerformanceHistory(slice_id), &history);

        let prediction = predict_slice_performance(&env, slice_id).unwrap();

        assert_eq!(prediction.sample_size, 2);
        assert_eq!(prediction.confidence_level, 20, "Low confidence with insufficient data");
        assert_eq!(
            prediction.predicted_consensus_time_secs, 30,
            "Conservative estimate with low data"
        );
    }

    #[test]
    fn test_predict_slice_performance_sufficient_data() {
        let env = create_test_env();
        let slice_id = 1u64;

        // Add 10 performance records (above MIN_PERFORMANCE_SAMPLES)
        let mut history: Vec<SlicePerformanceRecord> = Vec::new(&env);
        for i in 0..10 {
            history.push_back(SlicePerformanceRecord {
                slice_id,
                consensus_time_secs: 30 + (i as u32),
                consensus_success: true,
                timestamp: 1000 + (i as u64 * 100),
            });
        }
        env.storage()
            .persistent()
            .set(&DataKey::SlicePerformanceHistory(slice_id), &history);

        let prediction = predict_slice_performance(&env, slice_id).unwrap();

        assert_eq!(prediction.sample_size, 10);
        assert!(prediction.confidence_level > 20, "Higher confidence with more data");
        assert_eq!(prediction.predicted_reliability, 100, "All records successful");
    }

    #[test]
    fn test_record_slice_performance() {
        let env = create_test_env();
        let slice_id = 1u64;

        record_slice_performance(&env, slice_id, 25, true).unwrap();
        record_slice_performance(&env, slice_id, 30, true).unwrap();
        record_slice_performance(&env, slice_id, 35, false).unwrap();

        let history: Vec<SlicePerformanceRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::SlicePerformanceHistory(slice_id))
            .unwrap_or_else(|| Vec::new(&env));

        assert_eq!(history.len(), 3, "Should have 3 performance records");
        assert_eq!(history.get(0).unwrap().consensus_time_secs, 25);
        assert_eq!(history.get(2).unwrap().consensus_success, false);
    }

    // ── Tests for Issue #1611: Attestor Geographic Diversity ──────────────────

    #[test]
    fn test_validate_geographic_diversity_insufficient_countries() {
        let env = create_test_env();
        let slice_id = 1u64;

        let attestor_a = create_test_address(1);
        let attestor_b = create_test_address(2);

        // Set up slice members
        let mut members: Vec<Address> = Vec::new(&env);
        members.push_back(attestor_a.clone());
        members.push_back(attestor_b.clone());
        env.storage()
            .persistent()
            .set(&DataKey::AttestorSliceMembers(slice_id), &members);

        // Both attestors in the same country
        let location_a = AttestorLocation {
            attestor: attestor_a.clone(),
            country_code: String::from_slice(&env, "US"),
            region: String::from_slice(&env, "North America"),
        };
        let location_b = AttestorLocation {
            attestor: attestor_b.clone(),
            country_code: String::from_slice(&env, "US"),
            region: String::from_slice(&env, "North America"),
        };

        env.storage()
            .persistent()
            .set(&DataKey::AttestorLocation(attestor_a.clone()), &location_a);
        env.storage()
            .persistent()
            .set(&DataKey::AttestorLocation(attestor_b.clone()), &location_b);

        let is_diverse = validate_geographic_diversity(&env, slice_id).unwrap();
        assert!(!is_diverse, "Should fail diversity check with single country");
    }

    #[test]
    fn test_validate_geographic_diversity_sufficient() {
        let env = create_test_env();
        let slice_id = 1u64;

        let attestor_a = create_test_address(1);
        let attestor_b = create_test_address(2);
        let attestor_c = create_test_address(3);

        // Set up slice members
        let mut members: Vec<Address> = Vec::new(&env);
        members.push_back(attestor_a.clone());
        members.push_back(attestor_b.clone());
        members.push_back(attestor_c.clone());
        env.storage()
            .persistent()
            .set(&DataKey::AttestorSliceMembers(slice_id), &members);

        // Attestors in different countries and regions
        let location_a = AttestorLocation {
            attestor: attestor_a.clone(),
            country_code: String::from_slice(&env, "US"),
            region: String::from_slice(&env, "North America"),
        };
        let location_b = AttestorLocation {
            attestor: attestor_b.clone(),
            country_code: String::from_slice(&env, "JP"),
            region: String::from_slice(&env, "Asia"),
        };
        let location_c = AttestorLocation {
            attestor: attestor_c.clone(),
            country_code: String::from_slice(&env, "DE"),
            region: String::from_slice(&env, "Europe"),
        };

        env.storage()
            .persistent()
            .set(&DataKey::AttestorLocation(attestor_a.clone()), &location_a);
        env.storage()
            .persistent()
            .set(&DataKey::AttestorLocation(attestor_b.clone()), &location_b);
        env.storage()
            .persistent()
            .set(&DataKey::AttestorLocation(attestor_c.clone()), &location_c);

        let is_diverse = validate_geographic_diversity(&env, slice_id).unwrap();
        assert!(is_diverse, "Should pass diversity check with 3 countries and 3 regions");
    }

    #[test]
    fn test_set_and_get_attestor_location() {
        let env = create_test_env();
        let attestor = create_test_address(1);

        set_attestor_location(&env, &attestor, String::from_slice(&env, "US"), String::from_slice(&env, "North America")).unwrap();

        let location = get_attestor_location(&env, &attestor).expect("Location should exist");
        assert_eq!(
            location.country_code,
            String::from_slice(&env, "US")
        );
        assert_eq!(
            location.region,
            String::from_slice(&env, "North America")
        );
    }

    #[test]
    fn test_calculate_geographic_diversity_metrics() {
        let env = create_test_env();
        let slice_id = 1u64;

        let attestor_a = create_test_address(1);
        let attestor_b = create_test_address(2);

        // Set up slice members
        let mut members: Vec<Address> = Vec::new(&env);
        members.push_back(attestor_a.clone());
        members.push_back(attestor_b.clone());
        env.storage()
            .persistent()
            .set(&DataKey::AttestorSliceMembers(slice_id), &members);

        // Set locations
        set_attestor_location(
            &env,
            &attestor_a,
            String::from_slice(&env, "US"),
            String::from_slice(&env, "North America"),
        ).unwrap();
        set_attestor_location(
            &env,
            &attestor_b,
            String::from_slice(&env, "CA"),
            String::from_slice(&env, "North America"),
        ).unwrap();

        let diversity = calculate_geographic_diversity(&env, slice_id).unwrap();

        assert_eq!(diversity.unique_countries, 2);
        assert_eq!(diversity.unique_regions, 1); // Both in North America
        assert_eq!(diversity.meets_requirements, false); // Need 2 regions minimum
    }
}
