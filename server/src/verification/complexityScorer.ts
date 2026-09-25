/**
 * Issue #1585: Credential verification complexity scoring system.
 * Calculates verification complexity, tracks trends, and recommends optimizations.
 */

export interface VerificationRequest {
  credentialId: string;
  method: string; // "document", "biometric", "manual", "automated"
  dataFields: number; // Number of data fields to verify
  requiresBiometric: boolean;
  requiresManualReview: boolean;
  documentCount: number;
  createdAt: number;
}

export interface ComplexityScore {
  credentialId: string;
  totalScore: number;
  methodComplexity: number; // 1-10
  dataComplexity: number; // 1-10
  processComplexity: number; // 1-10
  estimatedCostCents: number;
  estimatedDurationSeconds: number;
  optimizationOpportunities: string[];
  scoredAt: number;
}

export interface ComplexityTrend {
  month: string;
  averageComplexity: number;
  verificationCount: number;
  averageCostCents: number;
  averageDurationSeconds: number;
}

export interface ComplexityOptimization {
  area: "method" | "data" | "process";
  title: string;
  description: string;
  potentialReduction: number; // Percentage reduction in complexity
  estimatedSavingsCents: number;
  implementationEffort: "low" | "medium" | "high";
}

export class ComplexityScorer {
  private scores: Map<string, ComplexityScore> = new Map();
  private trends: Map<string, ComplexityTrend> = new Map();
  private requests: VerificationRequest[] = [];

  /**
   * Calculate complexity score for a verification request.
   */
  scoreVerification(request: VerificationRequest): ComplexityScore {
    const methodComplexity = this.calculateMethodComplexity(request.method);
    const dataComplexity = this.calculateDataComplexity(request);
    const processComplexity = this.calculateProcessComplexity(request);

    // Weighted average: method (30%), data (40%), process (30%)
    const totalScore =
      methodComplexity * 0.3 + dataComplexity * 0.4 + processComplexity * 0.3;

    const estimatedCostCents = this.estimateCost(totalScore, request);
    const estimatedDurationSeconds = this.estimateDuration(totalScore, request);
    const optimizations = this.findOptimizations(request, totalScore);

    const score: ComplexityScore = {
      credentialId: request.credentialId,
      totalScore: Math.round(totalScore * 10) / 10,
      methodComplexity,
      dataComplexity,
      processComplexity,
      estimatedCostCents,
      estimatedDurationSeconds,
      optimizationOpportunities: optimizations,
      scoredAt: Date.now(),
    };

    this.scores.set(request.credentialId, score);
    this.requests.push(request);
    this.updateTrends(request, score);

    return score;
  }

  /**
   * Calculate method complexity score (1-10).
   */
  private calculateMethodComplexity(method: string): number {
    const scores: Record<string, number> = {
      automated: 2,
      document: 5,
      manual: 7,
      biometric: 8,
    };
    return scores[method] || 5;
  }

  /**
   * Calculate data complexity score based on number of fields (1-10).
   */
  private calculateDataComplexity(request: VerificationRequest): number {
    let score = Math.min(1 + request.dataFields * 0.5, 8);
    if (request.documentCount > 3) score = Math.min(score + 1.5, 10);
    return score;
  }

  /**
   * Calculate process complexity based on review requirements (1-10).
   */
  private calculateProcessComplexity(request: VerificationRequest): number {
    let score = 1;
    if (request.requiresManualReview) score += 3;
    if (request.requiresBiometric) score += 2;
    return Math.min(score, 10);
  }

  /**
   * Estimate cost in cents based on complexity.
   */
  private estimateCost(complexity: number, request: VerificationRequest): number {
    // Base cost: 50 cents
    let cost = 50;

    // Method-based cost multiplier
    const methodCosts: Record<string, number> = {
      automated: 1,
      document: 1.5,
      manual: 2.5,
      biometric: 2,
    };
    cost *= methodCosts[request.method] || 1;

    // Complexity multiplier
    cost *= 1 + complexity * 0.1;

    // Additional fees
    if (request.requiresManualReview) cost += 100;
    if (request.requiresBiometric) cost += 50;
    if (request.documentCount > 1) cost += 25 * (request.documentCount - 1);

    return Math.round(cost);
  }

  /**
   * Estimate duration in seconds based on complexity.
   */
  private estimateDuration(complexity: number, request: VerificationRequest): number {
    // Base duration: 30 seconds (automated)
    let duration = 30;

    const methodDurations: Record<string, number> = {
      automated: 30,
      document: 120,
      manual: 300,
      biometric: 180,
    };
    duration = methodDurations[request.method] || 120;

    // Add time for additional fields
    duration += request.dataFields * 20;

    // Add time for manual review
    if (request.requiresManualReview) duration += 180;

    // Add time for biometric
    if (request.requiresBiometric) duration += 60;

    return Math.round(duration);
  }

  /**
   * Find optimization opportunities.
   */
  private findOptimizations(
    request: VerificationRequest,
    complexity: number
  ): string[] {
    const opportunities: string[] = [];

    if (request.requiresManualReview && complexity > 6) {
      opportunities.push("Consider implementing automated document verification");
    }

    if (request.dataFields > 10) {
      opportunities.push("Reduce number of required data fields");
    }

    if (request.documentCount > 3) {
      opportunities.push("Streamline document requirements");
    }

    if (request.method === "manual" && complexity < 7) {
      opportunities.push("Consider upgrading to semi-automated verification");
    }

    if (request.requiresBiometric && request.documentCount > 2) {
      opportunities.push("Combine biometric and document verification");
    }

    return opportunities;
  }

  /**
   * Get complexity score for a credential.
   */
  getScore(credentialId: string): ComplexityScore | undefined {
    return this.scores.get(credentialId);
  }

  /**
   * Get all complexity scores.
   */
  getAllScores(): ComplexityScore[] {
    return [...this.scores.values()].sort((a, b) => b.totalScore - a.totalScore);
  }

  /**
   * Get complexity scores above a threshold.
   */
  getHighComplexityScores(threshold: number = 7): ComplexityScore[] {
    return this.getAllScores().filter((s) => s.totalScore >= threshold);
  }

  /**
   * Update trends based on new score.
   */
  private updateTrends(request: VerificationRequest, score: ComplexityScore): void {
    const month = new Date(request.createdAt).toISOString().substring(0, 7);

    if (!this.trends.has(month)) {
      this.trends.set(month, {
        month,
        averageComplexity: 0,
        verificationCount: 0,
        averageCostCents: 0,
        averageDurationSeconds: 0,
      });
    }

    const trend = this.trends.get(month)!;
    const count = trend.verificationCount + 1;

    trend.averageComplexity =
      (trend.averageComplexity * trend.verificationCount +
        score.totalScore) /
      count;
    trend.averageCostCents =
      (trend.averageCostCents * trend.verificationCount +
        score.estimatedCostCents) /
      count;
    trend.averageDurationSeconds =
      (trend.averageDurationSeconds * trend.verificationCount +
        score.estimatedDurationSeconds) /
      count;
    trend.verificationCount = count;
  }

  /**
   * Get complexity trends.
   */
  getTrends(): ComplexityTrend[] {
    return [...this.trends.values()].sort((a, b) =>
      a.month.localeCompare(b.month)
    );
  }

  /**
   * Get optimizations for high-complexity verifications.
   */
  getOptimizations(threshold: number = 7): ComplexityOptimization[] {
    const optimizations: ComplexityOptimization[] = [];
    const highComplexity = this.getHighComplexityScores(threshold);

    if (highComplexity.length > 0) {
      const manualCount = highComplexity.filter(
        (s) => s.processComplexity > 5
      ).length;
      if (manualCount > highComplexity.length * 0.5) {
        optimizations.push({
          area: "process",
          title: "Automate manual review processes",
          description: `${manualCount} high-complexity verifications require manual review. Implementing automated review could reduce complexity by 30%.`,
          potentialReduction: 30,
          estimatedSavingsCents: Math.round(
            highComplexity.reduce((a, s) => a + s.estimatedCostCents, 0) *
              0.3
          ),
          implementationEffort: "high",
        });
      }

      const dataIntensiveCount = highComplexity.filter(
        (s) => s.dataComplexity > 6
      ).length;
      if (dataIntensiveCount > 0) {
        optimizations.push({
          area: "data",
          title: "Simplify data collection",
          description: `${dataIntensiveCount} verifications are data-intensive. Streamlining data fields could reduce complexity by 20%.`,
          potentialReduction: 20,
          estimatedSavingsCents: Math.round(
            highComplexity.reduce((a, s) => a + s.estimatedCostCents, 0) *
              0.2
          ),
          implementationEffort: "medium",
        });
      }
    }

    return optimizations;
  }

  /**
   * Get statistics about verification complexity.
   */
  getStatistics(): {
    totalVerifications: number;
    averageComplexity: number;
    highComplexityCount: number;
    totalEstimatedCostCents: number;
    averageEstimatedCostCents: number;
  } {
    const scores = this.getAllScores();
    const highComplexity = this.getHighComplexityScores();

    return {
      totalVerifications: scores.length,
      averageComplexity:
        scores.length > 0
          ? Math.round(
              (scores.reduce((a, s) => a + s.totalScore, 0) / scores.length) *
                10
            ) / 10
          : 0,
      highComplexityCount: highComplexity.length,
      totalEstimatedCostCents: scores.reduce(
        (a, s) => a + s.estimatedCostCents,
        0
      ),
      averageEstimatedCostCents:
        scores.length > 0
          ? Math.round(
              scores.reduce((a, s) => a + s.estimatedCostCents, 0) /
                scores.length
            )
          : 0,
    };
  }
}
