import React from "react";
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import CredentialAnalyticsDashboard from "../CredentialAnalyticsDashboard";

const zeroData = {
  holderId: "holder-1",
  generatedAt: 0,
  credentialStats: {
    totalCredentials: 0,
    activeCredentials: 0,
    expiredCredentials: 0,
    revokedCredentials: 0,
    byType: {},
  },
  verificationStats: {
    totalVerifications: 0,
    verifiedCount: 0,
    pendingCount: 0,
    rejectedCount: 0,
    verificationRate: 0,
    reVerificationRequired: 0,
  },
  topCredentialTypes: [],
  verificationScoreTrend: [
    { timestamp: 0, credentials: 0, verified: 0, pending: 0, avgVerificationScore: 0 },
  ],
  reVerificationAlert: { needsReVerification: 0, credentialsNeedingAction: [] },
  healthScore: 0,
};

describe("CredentialAnalyticsDashboard", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("does not show an error while the data is loading", () => {
    vi.stubGlobal("fetch", vi.fn(() => new Promise(() => {})));
    render(<CredentialAnalyticsDashboard holderId="holder-1" apiBaseUrl="http://api.test" />);
    expect(screen.queryByText(/Error:/)).toBeNull();
  });

  it("shows the error when the analytics request fails", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => ({ ok: false, json: async () => ({}) })));
    render(<CredentialAnalyticsDashboard holderId="holder-1" apiBaseUrl="http://api.test" />);
    expect(await screen.findByText(/Failed to fetch analytics data/)).toBeTruthy();
  });

  it("renders a zero-value series without crashing", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => ({ ok: true, json: async () => zeroData })));
    render(<CredentialAnalyticsDashboard holderId="holder-1" apiBaseUrl="http://api.test" />);
    expect(await screen.findByText(/Last updated/)).toBeTruthy();
  });
});
