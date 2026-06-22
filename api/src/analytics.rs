use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// All monetary values are in stroops (1 XLM = 10,000,000 stroops).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProtocolMetrics {
    /// Total Value Locked: sum of all active loan amounts in stroops
    pub tvl: i128,
    pub active_loans: u32,
    pub total_loans: u32,
    pub defaulted_loans: u32,
    /// default_rate = defaulted_loans / total_loans (0.0–1.0); 0.0 when total_loans == 0
    pub default_rate: f64,
    /// Total yield distributed to vouchers in stroops
    pub total_yield_distributed: i128,
    /// Number of slash events
    pub slash_count: u32,
    /// Accumulated protocol fees in stroops
    pub fee_revenue: i128,
    /// Top borrowers by loan amount: (address, total_borrowed_stroops)
    pub top_borrowers: Vec<(String, i128)>,
    /// Top vouchers by total staked: (address, total_staked_stroops)
    pub top_vouchers: Vec<(String, i128)>,
    pub timestamp: i64,
}

impl ProtocolMetrics {
    pub fn new() -> Self {
        Self {
            tvl: 0,
            active_loans: 0,
            total_loans: 0,
            defaulted_loans: 0,
            default_rate: 0.0,
            total_yield_distributed: 0,
            slash_count: 0,
            fee_revenue: 0,
            top_borrowers: Vec::new(),
            top_vouchers: Vec::new(),
            timestamp: 0,
        }
    }
}

/// Input record describing a single loan snapshot for aggregation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoanSnapshot {
    pub borrower: String,
    pub amount: i128,
    pub status: LoanStatusInput,
    pub yield_distributed: i128,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LoanStatusInput {
    Active,
    Repaid,
    Defaulted,
}

/// Input record for a vouch snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VouchSnapshot {
    pub voucher: String,
    pub stake: i128,
}

/// Filter parameters for the metrics endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricsFilter {
    /// Unix timestamp lower bound (inclusive)
    pub from: Option<i64>,
    /// Unix timestamp upper bound (inclusive)
    pub to: Option<i64>,
    /// "small" (<1M stroops), "medium" (1M–100M), "large" (>100M)
    pub loan_size: Option<String>,
}

/// Configurable alert thresholds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// Alert when default_rate exceeds this (e.g. 0.05 = 5%)
    pub max_default_rate: f64,
    /// Alert when TVL drops by more than this fraction from peak (e.g. 0.10 = 10%)
    pub max_tvl_drop_fraction: f64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            max_default_rate: 0.05,
            max_tvl_drop_fraction: 0.10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Alert {
    pub kind: String,
    pub message: String,
}

/// Compute `ProtocolMetrics` from raw loan + vouch snapshots, applying optional filters.
pub fn aggregate_metrics(
    loans: &[LoanSnapshot],
    vouches: &[VouchSnapshot],
    slash_count: u32,
    fee_revenue: i128,
    filter: &MetricsFilter,
    now_ts: i64,
) -> ProtocolMetrics {
    // Apply filters
    let filtered: Vec<&LoanSnapshot> = loans
        .iter()
        .filter(|l| {
            if let Some(from) = filter.from {
                if l.created_at < from {
                    return false;
                }
            }
            if let Some(to) = filter.to {
                if l.created_at > to {
                    return false;
                }
            }
            if let Some(size) = &filter.loan_size {
                match size.as_str() {
                    "small" if l.amount >= 1_000_000 => return false,
                    "medium" if l.amount < 1_000_000 || l.amount > 100_000_000 => return false,
                    "large" if l.amount <= 100_000_000 => return false,
                    _ => {}
                }
            }
            true
        })
        .collect();

    let total_loans = filtered.len() as u32;
    let active_loans = filtered
        .iter()
        .filter(|l| l.status == LoanStatusInput::Active)
        .count() as u32;
    let defaulted_loans = filtered
        .iter()
        .filter(|l| l.status == LoanStatusInput::Defaulted)
        .count() as u32;

    let tvl: i128 = filtered
        .iter()
        .filter(|l| l.status == LoanStatusInput::Active)
        .map(|l| l.amount)
        .sum();

    let total_yield_distributed: i128 = filtered.iter().map(|l| l.yield_distributed).sum();

    let default_rate = if total_loans > 0 {
        defaulted_loans as f64 / total_loans as f64
    } else {
        0.0
    };

    // Top 5 borrowers by amount
    let mut borrower_totals: HashMap<String, i128> = HashMap::new();
    for l in &filtered {
        *borrower_totals.entry(l.borrower.clone()).or_insert(0) += l.amount;
    }
    let mut top_borrowers: Vec<(String, i128)> = borrower_totals.into_iter().collect();
    top_borrowers.sort_by(|a, b| b.1.cmp(&a.1));
    top_borrowers.truncate(5);

    // Top 5 vouchers by stake
    let mut voucher_totals: HashMap<String, i128> = HashMap::new();
    for v in vouches {
        *voucher_totals.entry(v.voucher.clone()).or_insert(0) += v.stake;
    }
    let mut top_vouchers: Vec<(String, i128)> = voucher_totals.into_iter().collect();
    top_vouchers.sort_by(|a, b| b.1.cmp(&a.1));
    top_vouchers.truncate(5);

    ProtocolMetrics {
        tvl,
        active_loans,
        total_loans,
        defaulted_loans,
        default_rate,
        total_yield_distributed,
        slash_count,
        fee_revenue,
        top_borrowers,
        top_vouchers,
        timestamp: now_ts,
    }
}

/// Check thresholds and return any triggered alerts.
pub fn check_alerts(
    metrics: &ProtocolMetrics,
    peak_tvl: i128,
    thresholds: &AlertThresholds,
) -> Vec<Alert> {
    let mut alerts = Vec::new();

    if metrics.default_rate > thresholds.max_default_rate {
        alerts.push(Alert {
            kind: "high_default_rate".to_string(),
            message: format!(
                "Default rate {:.1}% exceeds threshold {:.1}%",
                metrics.default_rate * 100.0,
                thresholds.max_default_rate * 100.0
            ),
        });
    }

    if peak_tvl > 0 {
        let drop = (peak_tvl - metrics.tvl) as f64 / peak_tvl as f64;
        if drop > thresholds.max_tvl_drop_fraction {
            alerts.push(Alert {
                kind: "tvl_drop".to_string(),
                message: format!(
                    "TVL dropped {:.1}% from peak, exceeds threshold {:.1}%",
                    drop * 100.0,
                    thresholds.max_tvl_drop_fraction * 100.0
                ),
            });
        }
    }

    alerts
}

/// Serialize metrics to CSV string.
/// Columns: timestamp,tvl,active_loans,total_loans,defaulted_loans,default_rate,
///          total_yield_distributed,slash_count,fee_revenue
pub fn metrics_to_csv(rows: &[ProtocolMetrics]) -> String {
    let mut out = String::from(
        "timestamp,tvl,active_loans,total_loans,defaulted_loans,\
         default_rate,total_yield_distributed,slash_count,fee_revenue\n",
    );
    for r in rows {
        out.push_str(&format!(
            "{},{},{},{},{},{:.6},{},{},{}\n",
            r.timestamp,
            r.tvl,
            r.active_loans,
            r.total_loans,
            r.defaulted_loans,
            r.default_rate,
            r.total_yield_distributed,
            r.slash_count,
            r.fee_revenue,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_loans() -> Vec<LoanSnapshot> {
        vec![
            LoanSnapshot {
                borrower: "addr_a".into(),
                amount: 5_000_000_000,
                status: LoanStatusInput::Active,
                yield_distributed: 0,
                created_at: 1000,
            },
            LoanSnapshot {
                borrower: "addr_b".into(),
                amount: 3_000_000_000,
                status: LoanStatusInput::Active,
                yield_distributed: 0,
                created_at: 2000,
            },
            LoanSnapshot {
                borrower: "addr_c".into(),
                amount: 1_000_000_000,
                status: LoanStatusInput::Defaulted,
                yield_distributed: 0,
                created_at: 3000,
            },
            LoanSnapshot {
                borrower: "addr_d".into(),
                amount: 2_000_000_000,
                status: LoanStatusInput::Repaid,
                yield_distributed: 40_000_000,
                created_at: 4000,
            },
        ]
    }

    fn sample_vouches() -> Vec<VouchSnapshot> {
        vec![
            VouchSnapshot { voucher: "v1".into(), stake: 1_000_000_000 },
            VouchSnapshot { voucher: "v2".into(), stake: 500_000_000 },
            VouchSnapshot { voucher: "v1".into(), stake: 200_000_000 },
        ]
    }

    // Test 1: TVL = sum of active loan amounts only
    #[test]
    fn test_tvl_equals_sum_of_active_loans() {
        let metrics = aggregate_metrics(
            &sample_loans(), &[], 0, 0, &MetricsFilter::default(), 0,
        );
        // active: addr_a (5B) + addr_b (3B) = 8B stroops
        assert_eq!(metrics.tvl, 8_000_000_000);
    }

    // Test 2: Default rate = defaulted / total
    #[test]
    fn test_default_rate_calculation() {
        let loans: Vec<LoanSnapshot> = (0..10)
            .map(|i| LoanSnapshot {
                borrower: format!("addr_{}", i),
                amount: 1_000_000_000,
                status: if i < 2 {
                    LoanStatusInput::Defaulted
                } else {
                    LoanStatusInput::Repaid
                },
                yield_distributed: 0,
                created_at: i as i64,
            })
            .collect();
        let m = aggregate_metrics(&loans, &[], 0, 0, &MetricsFilter::default(), 0);
        assert_eq!(m.total_loans, 10);
        assert_eq!(m.defaulted_loans, 2);
        assert!((m.default_rate - 0.2).abs() < 1e-9);
    }

    // Test 3: Zero loans → default_rate = 0.0, no panic
    #[test]
    fn test_default_rate_no_loans() {
        let m = aggregate_metrics(&[], &[], 0, 0, &MetricsFilter::default(), 0);
        assert_eq!(m.default_rate, 0.0);
        assert_eq!(m.tvl, 0);
    }

    // Test 4: active_loans count is correct
    #[test]
    fn test_active_loans_count() {
        let m = aggregate_metrics(
            &sample_loans(), &[], 0, 0, &MetricsFilter::default(), 0,
        );
        assert_eq!(m.active_loans, 2);
    }

    // Test 5: Yield distributed is summed across all filtered loans
    #[test]
    fn test_yield_distribution_sum() {
        let loans = vec![
            LoanSnapshot {
                borrower: "a".into(), amount: 100, status: LoanStatusInput::Repaid,
                yield_distributed: 20_000_000, created_at: 0,
            },
            LoanSnapshot {
                borrower: "b".into(), amount: 100, status: LoanStatusInput::Repaid,
                yield_distributed: 10_000_000, created_at: 0,
            },
        ];
        let m = aggregate_metrics(&loans, &[], 0, 0, &MetricsFilter::default(), 0);
        assert_eq!(m.total_yield_distributed, 30_000_000);
    }

    // Test 6: Date range filter excludes out-of-range loans
    #[test]
    fn test_date_range_filter() {
        let filter = MetricsFilter { from: Some(1500), to: Some(3500), loan_size: None };
        let m = aggregate_metrics(
            &sample_loans(), &[], 0, 0, &filter, 0,
        );
        // Only loans with created_at in [1500, 3500]: addr_b (2000), addr_c (3000)
        assert_eq!(m.total_loans, 2);
    }

    // Test 7: Loan size filter "small" keeps only < 1M stroops
    #[test]
    fn test_loan_size_filter_small() {
        let loans = vec![
            LoanSnapshot {
                borrower: "a".into(), amount: 500_000,
                status: LoanStatusInput::Active, yield_distributed: 0, created_at: 0,
            },
            LoanSnapshot {
                borrower: "b".into(), amount: 2_000_000,
                status: LoanStatusInput::Active, yield_distributed: 0, created_at: 0,
            },
        ];
        let filter = MetricsFilter { loan_size: Some("small".into()), ..Default::default() };
        let m = aggregate_metrics(&loans, &[], 0, 0, &filter, 0);
        assert_eq!(m.total_loans, 1);
        assert_eq!(m.tvl, 500_000);
    }

    // Test 8: Top borrowers sorted by descending total amount
    #[test]
    fn test_top_borrowers_sorted() {
        let m = aggregate_metrics(
            &sample_loans(), &[], 0, 0, &MetricsFilter::default(), 0,
        );
        // addr_a=5B, addr_b=3B, addr_d=2B, addr_c=1B
        assert_eq!(m.top_borrowers[0].0, "addr_a");
        assert_eq!(m.top_borrowers[0].1, 5_000_000_000);
    }

    // Test 9: Top vouchers aggregates by voucher address
    #[test]
    fn test_top_vouchers_aggregated() {
        let m = aggregate_metrics(
            &[], &sample_vouches(), 0, 0, &MetricsFilter::default(), 0,
        );
        // v1 = 1.2B, v2 = 0.5B
        assert_eq!(m.top_vouchers[0].0, "v1");
        assert_eq!(m.top_vouchers[0].1, 1_200_000_000);
    }

    // Test 10: Top lists capped at 5 entries
    #[test]
    fn test_top_borrowers_capped_at_5() {
        let loans: Vec<LoanSnapshot> = (0..10)
            .map(|i| LoanSnapshot {
                borrower: format!("addr_{}", i),
                amount: (i as i128 + 1) * 1_000_000_000,
                status: LoanStatusInput::Active,
                yield_distributed: 0,
                created_at: 0,
            })
            .collect();
        let m = aggregate_metrics(&loans, &[], 0, 0, &MetricsFilter::default(), 0);
        assert_eq!(m.top_borrowers.len(), 5);
    }

    // Test 11: Alert fires when default rate exceeds threshold
    #[test]
    fn test_alert_high_default_rate() {
        let m = ProtocolMetrics {
            default_rate: 0.06,
            ..ProtocolMetrics::new()
        };
        let alerts = check_alerts(&m, 0, &AlertThresholds::default());
        assert!(alerts.iter().any(|a| a.kind == "high_default_rate"));
    }

    // Test 12: No alert when default rate is below threshold
    #[test]
    fn test_no_alert_default_rate_below_threshold() {
        let m = ProtocolMetrics {
            default_rate: 0.03,
            ..ProtocolMetrics::new()
        };
        let alerts = check_alerts(&m, 0, &AlertThresholds::default());
        assert!(!alerts.iter().any(|a| a.kind == "high_default_rate"));
    }

    // Test 13: Alert fires when TVL drops > 10% from peak
    #[test]
    fn test_alert_tvl_drop() {
        let m = ProtocolMetrics {
            tvl: 8_000_000_000,
            ..ProtocolMetrics::new()
        };
        let peak = 10_000_000_000i128;
        let alerts = check_alerts(&m, peak, &AlertThresholds::default());
        assert!(alerts.iter().any(|a| a.kind == "tvl_drop"));
    }

    // Test 14: No TVL alert when drop is within threshold
    #[test]
    fn test_no_alert_tvl_small_drop() {
        let m = ProtocolMetrics {
            tvl: 9_500_000_000,
            ..ProtocolMetrics::new()
        };
        let peak = 10_000_000_000i128;
        let alerts = check_alerts(&m, peak, &AlertThresholds::default());
        assert!(!alerts.iter().any(|a| a.kind == "tvl_drop"));
    }

    // Test 15: Custom alert threshold respected
    #[test]
    fn test_custom_alert_threshold() {
        let m = ProtocolMetrics {
            default_rate: 0.03,
            ..ProtocolMetrics::new()
        };
        let thresholds = AlertThresholds {
            max_default_rate: 0.02,
            max_tvl_drop_fraction: 0.10,
        };
        let alerts = check_alerts(&m, 0, &thresholds);
        assert!(alerts.iter().any(|a| a.kind == "high_default_rate"));
    }

    // Test 16: CSV has correct headers
    #[test]
    fn test_csv_headers() {
        let csv = metrics_to_csv(&[]);
        assert!(csv.starts_with(
            "timestamp,tvl,active_loans,total_loans,defaulted_loans,\
             default_rate,total_yield_distributed,slash_count,fee_revenue"
        ));
    }

    // Test 17: CSV data rows contain correct values
    #[test]
    fn test_csv_data_rows() {
        let row = ProtocolMetrics {
            tvl: 5_000_000_000,
            active_loans: 2,
            total_loans: 4,
            defaulted_loans: 1,
            default_rate: 0.25,
            total_yield_distributed: 100_000_000,
            slash_count: 1,
            fee_revenue: 50_000,
            top_borrowers: vec![],
            top_vouchers: vec![],
            timestamp: 9999,
        };
        let csv = metrics_to_csv(&[row]);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 2); // header + 1 data row
        assert!(lines[1].starts_with("9999,5000000000,2,4,1,"));
    }

    // Test 18: slash_count and fee_revenue pass through unchanged
    #[test]
    fn test_slash_count_and_fee_revenue_passthrough() {
        let m = aggregate_metrics(&[], &[], 7, 1_234_567, &MetricsFilter::default(), 42);
        assert_eq!(m.slash_count, 7);
        assert_eq!(m.fee_revenue, 1_234_567);
        assert_eq!(m.timestamp, 42);
    }

    // Test 19: Defaulted loans are excluded from TVL
    #[test]
    fn test_defaulted_loans_excluded_from_tvl() {
        let loans = vec![
            LoanSnapshot {
                borrower: "a".into(), amount: 1_000_000_000,
                status: LoanStatusInput::Defaulted, yield_distributed: 0, created_at: 0,
            },
        ];
        let m = aggregate_metrics(&loans, &[], 0, 0, &MetricsFilter::default(), 0);
        assert_eq!(m.tvl, 0);
        assert_eq!(m.defaulted_loans, 1);
    }

    // Test 20: Repaid loans are excluded from TVL and active count
    #[test]
    fn test_repaid_loans_excluded_from_tvl_and_active_count() {
        let loans = vec![
            LoanSnapshot {
                borrower: "a".into(), amount: 1_000_000_000,
                status: LoanStatusInput::Repaid, yield_distributed: 20_000_000, created_at: 0,
            },
        ];
        let m = aggregate_metrics(&loans, &[], 0, 0, &MetricsFilter::default(), 0);
        assert_eq!(m.tvl, 0);
        assert_eq!(m.active_loans, 0);
        assert_eq!(m.total_yield_distributed, 20_000_000);
    }
}
