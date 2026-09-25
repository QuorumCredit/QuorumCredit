import type { CostReport } from "./costAllocator.js";

export interface CostTrend {
  month: string;
  totalCostCents: number;
  transactionCount: number;
  costPerTransactionCents: number;
}

export interface OptimizationRecommendation {
  category: "right-sizing" | "batching" | "caching" | "feature-optimization" | "infrastructure";
  priority: "critical" | "high" | "medium" | "low";
  title: string;
  description: string;
  estimatedSavingsCents?: number;
  implementationEffort: "low" | "medium" | "high";
}

export interface CostAnalysis {
  period: string;
  generatedAt: number;
  currentCostCents: number;
  averageCostPerTransactionCents: number;
  trends: CostTrend[];
  recommendations: OptimizationRecommendation[];
  resourceUtilization: {
    peakCostMonth: string;
    peakCostCents: number;
    lowestCostMonth: string;
    lowestCostCents: number;
    variancePercentage: number;
  };
  estimatedAnnualSavingsPotentialCents: number;
}

/**
 * Issue #1581: Cost Optimization module — analyzes resource utilization patterns
 * and generates actionable cost reduction recommendations. Tracks cost trends
 * over time and implements automated right-sizing suggestions.
 */
export class CostOptimizer {
  private costHistory: Map<string, CostReport> = new Map();

  addReport(report: CostReport): void {
    this.costHistory.set(report.period, report);
  }

  /**
   * Generate a comprehensive cost analysis with trends and optimization recommendations.
   */
  analyzeCosts(latestReport: CostReport): CostAnalysis {
    const trends = this.buildCostTrends();
    const recommendations = this.generateRecommendations(latestReport, trends);
    const utilization = this.analyzeResourceUtilization(trends);
    const savingsPotential = this.estimateSavingsPotential(recommendations);

    return {
      period: latestReport.period,
      generatedAt: latestReport.generatedAt,
      currentCostCents: latestReport.totalCostCents,
      averageCostPerTransactionCents:
        latestReport.totalTransactionCount > 0
          ? latestReport.totalCostCents / latestReport.totalTransactionCount
          : 0,
      trends,
      recommendations,
      resourceUtilization: utilization,
      estimatedAnnualSavingsPotentialCents: savingsPotential,
    };
  }

  private buildCostTrends(): CostTrend[] {
    return [...this.costHistory.entries()]
      .map(([month, report]) => ({
        month,
        totalCostCents: report.totalCostCents,
        transactionCount: report.totalTransactionCount,
        costPerTransactionCents:
          report.totalTransactionCount > 0
            ? report.totalCostCents / report.totalTransactionCount
            : 0,
      }))
      .sort((a, b) => a.month.localeCompare(b.month));
  }

  private analyzeResourceUtilization(trends: CostTrend[]): {
    peakCostMonth: string;
    peakCostCents: number;
    lowestCostMonth: string;
    lowestCostCents: number;
    variancePercentage: number;
  } {
    if (trends.length === 0) {
      return {
        peakCostMonth: "N/A",
        peakCostCents: 0,
        lowestCostMonth: "N/A",
        lowestCostCents: 0,
        variancePercentage: 0,
      };
    }

    const peak = trends.reduce((max, t) =>
      t.totalCostCents > max.totalCostCents ? t : max
    );
    const lowest = trends.reduce((min, t) =>
      t.totalCostCents < min.totalCostCents ? t : min
    );

    const avgCost = trends.reduce((a, t) => a + t.totalCostCents, 0) / trends.length;
    const variance =
      avgCost > 0
        ? ((peak.totalCostCents - lowest.totalCostCents) / avgCost) * 100
        : 0;

    return {
      peakCostMonth: peak.month,
      peakCostCents: peak.totalCostCents,
      lowestCostMonth: lowest.month,
      lowestCostCents: lowest.totalCostCents,
      variancePercentage: Math.round(variance * 10) / 10,
    };
  }

  private generateRecommendations(
    report: CostReport,
    trends: CostTrend[]
  ): OptimizationRecommendation[] {
    const recommendations: OptimizationRecommendation[] = [];

    // Detect underutilized features
    for (const feature of report.features) {
      if (feature.transactionCount === 0 && feature.totalCostCents > 0) {
        recommendations.push({
          category: "right-sizing",
          priority: "high",
          title: `Review ${feature.feature} feature capacity`,
          description: `The ${feature.feature} feature is allocated $${(feature.totalCostCents / 100).toFixed(
            2
          )} of infrastructure cost but has zero transactions this period. Consider scaling down capacity.`,
          estimatedSavingsCents: feature.totalCostCents,
          implementationEffort: "low",
        });
      }
    }

    // Detect cost outliers
    const avgCostPerTx = this.calculateAverageCostPerTransaction(report);
    for (const feature of report.features) {
      if (
        feature.transactionCount > 0 &&
        feature.costPerTransactionCents > avgCostPerTx * 1.5
      ) {
        const excessCost = Math.round(
          (feature.costPerTransactionCents - avgCostPerTx) *
            feature.transactionCount
        );
        recommendations.push({
          category: "feature-optimization",
          priority: "medium",
          title: `Optimize ${feature.feature} feature efficiency`,
          description: `Cost per transaction for ${feature.feature} is ${(
            feature.costPerTransactionCents / avgCostPerTx
          ).toFixed(1)}x the platform average. Investigate batching or caching opportunities.`,
          estimatedSavingsCents: Math.round(excessCost * 0.2),
          implementationEffort: "medium",
        });
      }
    }

    // Detect transaction volume volatility
    if (trends.length > 1) {
      const volatility = this.calculateVolatility(trends);
      if (volatility > 30) {
        recommendations.push({
          category: "batching",
          priority: "medium",
          title: "Implement transaction batching",
          description:
            "Transaction volume varies significantly across months (${volatility.toFixed(1)}%). " +
            "Implementing batching could smooth costs and reduce infrastructure overhead.",
          estimatedSavingsCents: Math.round(
            report.totalCostCents * 0.15
          ),
          implementationEffort: "high",
        });
      }
    }

    // Detect caching opportunities
    if (report.totalTransactionCount > 1000) {
      recommendations.push({
        category: "caching",
        priority: "medium",
        title: "Implement response caching",
        description:
          "High transaction volume detected. Implementing caching for frequently accessed data could reduce " +
          "database and API server load by 10-20%.",
        estimatedSavingsCents: Math.round(
          (report.totalCostCents * 0.15) / Math.max(1, trends.length)
        ),
        implementationEffort: "medium",
      });
    }

    // Recommend infrastructure consolidation
    if (trends.length > 3) {
      const avgCost =
        trends.reduce((a, t) => a + t.totalCostCents, 0) / trends.length;
      const underutilizedMonths = trends.filter(
        (t) => t.totalCostCents < avgCost * 0.7
      ).length;

      if (underutilizedMonths > 0) {
        recommendations.push({
          category: "infrastructure",
          priority: "low",
          title: "Consider auto-scaling infrastructure",
          description:
            `${underutilizedMonths} months show underutilization (below 70% of average). ` +
            "Auto-scaling infrastructure could reduce idle capacity costs.",
          estimatedSavingsCents: Math.round(avgCost * 0.1),
          implementationEffort: "high",
        });
      }
    }

    return recommendations.sort((a, b) => {
      const priorityOrder = { critical: 0, high: 1, medium: 2, low: 3 };
      return priorityOrder[a.priority] - priorityOrder[b.priority];
    });
  }

  private calculateAverageCostPerTransaction(report: CostReport): number {
    const activeFeatures = report.features.filter((f) => f.transactionCount > 0);
    if (activeFeatures.length === 0) return 0;
    return (
      activeFeatures.reduce((a, f) => a + f.costPerTransactionCents, 0) /
      activeFeatures.length
    );
  }

  private calculateVolatility(trends: CostTrend[]): number {
    if (trends.length < 2) return 0;
    const avg =
      trends.reduce((a, t) => a + t.totalCostCents, 0) / trends.length;
    const variance =
      trends.reduce((a, t) => a + Math.pow(t.totalCostCents - avg, 2), 0) /
      trends.length;
    const stdDev = Math.sqrt(variance);
    return avg > 0 ? (stdDev / avg) * 100 : 0;
  }

  private estimateSavingsPotential(
    recommendations: OptimizationRecommendation[]
  ): number {
    return recommendations.reduce(
      (total, r) => total + (r.estimatedSavingsCents ?? 0),
      0
    );
  }
}
