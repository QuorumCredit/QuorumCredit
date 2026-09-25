/**
 * Issue #1592: Credential Holder Analytics Dashboard
 *
 * React component for displaying credential analytics and verification statistics.
 */

import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

interface AnalyticsDashboardData {
  holderId: string;
  generatedAt: number;
  credentialStats: {
    totalCredentials: number;
    activeCredentials: number;
    expiredCredentials: number;
    revokedCredentials: number;
    byType: Record<string, number>;
  };
  verificationStats: {
    totalVerifications: number;
    verifiedCount: number;
    pendingCount: number;
    rejectedCount: number;
    verificationRate: number;
    reVerificationRequired: number;
  };
  topCredentialTypes: Array<{ type: string; count: number }>;
  verificationScoreTrend: Array<{
    timestamp: number;
    credentials: number;
    verified: number;
    pending: number;
    avgVerificationScore: number;
  }>;
  reVerificationAlert: {
    needsReVerification: number;
    credentialsNeedingAction: string[];
  };
  healthScore: number;
}

interface CredentialAnalyticsDashboardProps {
  holderId: string;
  apiBaseUrl: string;
}

const CredentialAnalyticsDashboard: React.FC<CredentialAnalyticsDashboardProps> = ({
  holderId,
  apiBaseUrl,
}) => {
  const { t } = useTranslation();
  const [dashboardData, setDashboardData] = useState<AnalyticsDashboardData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fetchAnalytics = async () => {
      try {
        setLoading(true);
        const response = await fetch(
          `${apiBaseUrl}/analytics/credentials/${encodeURIComponent(holderId)}`
        );

        if (!response.ok) {
          throw new Error("Failed to fetch analytics data");
        }

        const data = await response.json();
        setDashboardData(data);
        setError(null);
      } catch (err) {
        setError(err instanceof Error ? err.message : "Unknown error");
        setDashboardData(null);
      } finally {
        setLoading(false);
      }
    };

    fetchAnalytics();
    // Refresh every 5 minutes
    const interval = setInterval(fetchAnalytics, 5 * 60 * 1000);

    return () => clearInterval(interval);
  }, [holderId, apiBaseUrl]);

  const getHealthScoreColor = (score: number): string => {
    if (score >= 80) return "text-green-600";
    if (score >= 60) return "text-yellow-600";
    return "text-red-600";
  };

  const getHealthScoreLabel = (score: number): string => {
    if (score >= 80) return "Healthy";
    if (score >= 60) return "At Risk";
    return "Critical";
  };

  if (loading) {
    return (
      <div className="flex justify-center items-center h-full">
        <div className="text-gray-500">Loading analytics...</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="bg-red-50 border border-red-200 rounded-lg p-4">
        <p className="text-red-800">Error: {error}</p>
      </div>
    );
  }

  if (!dashboardData) {
    return (
      <div className="bg-gray-50 border border-gray-200 rounded-lg p-4">
        <p className="text-gray-600">No data available</p>
      </div>
    );
  }

  const { credentialStats, verificationStats, healthScore, reVerificationAlert } =
    dashboardData;

  return (
    <div className="w-full bg-white rounded-lg shadow-lg p-6 space-y-6">
      {/* Header */}
      <div className="flex justify-between items-center">
        <h1 className="text-2xl font-bold text-gray-900">Credential Analytics</h1>
        <div
          className={`text-right ${getHealthScoreColor(healthScore)}`}
        >
          <div className="text-3xl font-bold">{healthScore}</div>
          <div className="text-sm font-medium">
            {getHealthScoreLabel(healthScore)}
          </div>
        </div>
      </div>

      {/* Alert for Re-verification */}
      {reVerificationAlert.needsReVerification > 0 && (
        <div className="bg-yellow-50 border border-yellow-200 rounded-lg p-4">
          <p className="text-yellow-800 font-medium">
            ⚠️ {reVerificationAlert.needsReVerification} credential(s) need re-verification
          </p>
        </div>
      )}

      {/* Stats Grid */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div className="bg-blue-50 rounded-lg p-4">
          <div className="text-gray-600 text-sm font-medium">Total Credentials</div>
          <div className="text-2xl font-bold text-blue-600">
            {credentialStats.totalCredentials}
          </div>
        </div>

        <div className="bg-green-50 rounded-lg p-4">
          <div className="text-gray-600 text-sm font-medium">Active</div>
          <div className="text-2xl font-bold text-green-600">
            {credentialStats.activeCredentials}
          </div>
        </div>

        <div className="bg-yellow-50 rounded-lg p-4">
          <div className="text-gray-600 text-sm font-medium">Expired</div>
          <div className="text-2xl font-bold text-yellow-600">
            {credentialStats.expiredCredentials}
          </div>
        </div>

        <div className="bg-red-50 rounded-lg p-4">
          <div className="text-gray-600 text-sm font-medium">Revoked</div>
          <div className="text-2xl font-bold text-red-600">
            {credentialStats.revokedCredentials}
          </div>
        </div>
      </div>

      {/* Verification Stats */}
      <div className="bg-gray-50 rounded-lg p-4">
        <h2 className="text-lg font-semibold text-gray-900 mb-4">
          Verification Status
        </h2>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <div>
            <div className="text-gray-600 text-sm">Verified</div>
            <div className="text-lg font-bold text-gray-900">
              {verificationStats.verifiedCount}
            </div>
          </div>
          <div>
            <div className="text-gray-600 text-sm">Pending</div>
            <div className="text-lg font-bold text-gray-900">
              {verificationStats.pendingCount}
            </div>
          </div>
          <div>
            <div className="text-gray-600 text-sm">Rejected</div>
            <div className="text-lg font-bold text-gray-900">
              {verificationStats.rejectedCount}
            </div>
          </div>
          <div>
            <div className="text-gray-600 text-sm">Verification Rate</div>
            <div className="text-lg font-bold text-gray-900">
              {Math.round(verificationStats.verificationRate * 100)}%
            </div>
          </div>
        </div>
      </div>

      {/* Credential Types */}
      {dashboardData.topCredentialTypes.length > 0 && (
        <div className="bg-gray-50 rounded-lg p-4">
          <h2 className="text-lg font-semibold text-gray-900 mb-4">
            Top Credential Types
          </h2>
          <div className="space-y-2">
            {dashboardData.topCredentialTypes.map((type) => (
              <div
                key={type.type}
                className="flex justify-between items-center py-2 border-b border-gray-200"
              >
                <span className="text-gray-700 capitalize">{type.type}</span>
                <span className="font-semibold text-gray-900">{type.count}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Generated At */}
      <div className="text-xs text-gray-500 text-right">
        Last updated: {new Date(dashboardData.generatedAt * 1000).toLocaleString()}
      </div>
    </div>
  );
};

export default CredentialAnalyticsDashboard;
