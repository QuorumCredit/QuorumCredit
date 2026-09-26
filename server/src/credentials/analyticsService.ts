/**
 * Issue #1592: Credential Holder Analytics Dashboard
 *
 * Provides analytics and statistics about credential usage, verification,
 * and trends for dashboard visualization.
 */

import { credentialStore } from "./credentialStore.js";
import { identityVerificationService } from "./identityVerificationService.js";

export interface CredentialStats {
  totalCredentials: number;
  activeCredentials: number;
  expiredCredentials: number;
  revokedCredentials: number;
  byType: Record<string, number>;
}

export interface VerificationStats {
  totalVerifications: number;
  verifiedCount: number;
  pendingCount: number;
  rejectedCount: number;
  verificationRate: number;
  reVerificationRequired: number;
}

export interface TrendData {
  timestamp: number;
  credentials: number;
  verified: number;
  pending: number;
  avgVerificationScore: number;
}

export interface AnalyticsDashboardData {
  holderId: string;
  generatedAt: number;
  credentialStats: CredentialStats;
  verificationStats: VerificationStats;
  topCredentialTypes: Array<{ type: string; count: number }>;
  verificationScoreTrend: TrendData[];
  reVerificationAlert: {
    needsReVerification: number;
    credentialsNeedingAction: string[];
  };
  healthScore: number;
}

class AnalyticsService {
  private trendHistory = new Map<string, TrendData[]>();
  private updateInterval = 3600000; // 1 hour

  /**
   * Get comprehensive credential statistics for a holder
   */
  getCredentialStats(holderId: string): CredentialStats {
    const credentials = credentialStore.getCredentialsForHolder(holderId);
    const now = Math.floor(Date.now() / 1000);

    const stats: CredentialStats = {
      totalCredentials: credentials.length,
      activeCredentials: credentials.filter((c) => c.status === "active" && c.expiresAt > now).length,
      expiredCredentials: credentials.filter((c) => c.expiresAt <= now).length,
      revokedCredentials: credentials.filter((c) => c.status === "revoked").length,
      byType: {},
    };

    // Count by type
    for (const credential of credentials) {
      stats.byType[credential.type] = (stats.byType[credential.type] || 0) + 1;
    }

    return stats;
  }

  /**
   * Get verification statistics for a holder
   */
  getVerificationStats(holderId: string): VerificationStats {
    return credentialStore.getVerificationStats(holderId);
  }

  /**
   * Get top credential types for a holder
   */
  getTopCredentialTypes(
    holderId: string,
    limit: number = 5
  ): Array<{ type: string; count: number }> {
    const stats = this.getCredentialStats(holderId);
    return Object.entries(stats.byType)
      .map(([type, count]) => ({ type, count }))
      .sort((a, b) => b.count - a.count)
      .slice(0, limit);
  }

  /**
   * Record a trend data point
   */
  recordTrend(holderId: string, data: Omit<TrendData, "timestamp">): void {
    const trend: TrendData = {
      timestamp: Math.floor(Date.now() / 1000),
      ...data,
    };

    if (!this.trendHistory.has(holderId)) {
      this.trendHistory.set(holderId, []);
    }

    const history = this.trendHistory.get(holderId)!;
    history.push(trend);

    // Keep only last 30 days of data (at 1-hour intervals)
    const thirtyDaysAgo = Math.floor(Date.now() / 1000) - 30 * 24 * 60 * 60;
    const filtered = history.filter((t) => t.timestamp > thirtyDaysAgo);
    this.trendHistory.set(holderId, filtered);
  }

  /**
   * Get trend data for a holder
   */
  getTrendData(
    holderId: string,
    days: number = 30
  ): TrendData[] {
    const history = this.trendHistory.get(holderId) || [];
    const cutoff = Math.floor(Date.now() / 1000) - days * 24 * 60 * 60;
    return history.filter((t) => t.timestamp > cutoff);
  }

  /**
   * Calculate overall health score for a holder
   */
  calculateHealthScore(holderId: string): number {
    let score = 100;

    // Deduct points based on credential status
    const credentialStats = this.getCredentialStats(holderId);
    if (credentialStats.activeCredentials === 0 && credentialStats.totalCredentials > 0) {
      score -= 50;
    } else if (credentialStats.expiredCredentials > 0) {
      score -= Math.min(20, credentialStats.expiredCredentials * 5);
    }

    // Deduct points based on verification status
    const verificationStats = this.getVerificationStats(holderId);
    if (verificationStats.verificationRate < 0.5) {
      score -= 20;
    } else if (verificationStats.verificationRate < 0.8) {
      score -= 10;
    }

    // Check for re-verification requirements
    const needsReVerification = credentialStore.getCredentialsNeedingReVerification(
      holderId
    );
    if (needsReVerification.length > 0) {
      score -= Math.min(15, needsReVerification.length * 5);
    }

    return Math.max(0, score);
  }

  /**
   * Get re-verification alerts
   */
  getReVerificationAlerts(holderId: string) {
    const needsReVerification = credentialStore.getCredentialsNeedingReVerification(
      holderId
    );
    return {
      needsReVerification: needsReVerification.length,
      credentialsNeedingAction: needsReVerification.map((c) => c.id),
    };
  }

  /**
   * Generate complete analytics dashboard data
   */
  generateDashboardData(holderId: string): AnalyticsDashboardData {
    return {
      holderId,
      generatedAt: Math.floor(Date.now() / 1000),
      credentialStats: this.getCredentialStats(holderId),
      verificationStats: this.getVerificationStats(holderId),
      topCredentialTypes: this.getTopCredentialTypes(holderId),
      verificationScoreTrend: this.getTrendData(holderId),
      reVerificationAlert: this.getReVerificationAlerts(holderId),
      healthScore: this.calculateHealthScore(holderId),
    };
  }

  /**
   * Get usage statistics (aggregated metrics)
   */
  getUsageMetrics() {
    return {
      timestamp: Math.floor(Date.now() / 1000),
      totalTrends: this.trendHistory.size,
      avgHealthScore: this.getAverageHealthScore(),
    };
  }

  /**
   * Calculate average health score across all tracked holders
   */
  private getAverageHealthScore(): number {
    if (this.trendHistory.size === 0) return 100;

    const scores: number[] = [];
    for (const holderId of this.trendHistory.keys()) {
      scores.push(this.calculateHealthScore(holderId));
    }

    const sum = scores.reduce((a, b) => a + b, 0);
    return Math.round(sum / scores.length);
  }
}

export const analyticsService = new AnalyticsService();
