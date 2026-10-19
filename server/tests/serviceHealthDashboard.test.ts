import { describe, it, expect } from "vitest";
import { ServiceHealthDashboard } from "../src/health/serviceHealthDashboard.js";

describe("service health dashboard", () => {
  it("reports a healthy service as healthy", () => {
    const dashboard = new ServiceHealthDashboard();
    dashboard.register({ name: "db" });
    dashboard.recordResult({ service: "db", status: "healthy", checkedAt: Date.now() });
    expect(dashboard.getServiceHealth("db")?.status).toBe("healthy");
  });

  it("reports a degraded service as degraded", () => {
    const dashboard = new ServiceHealthDashboard();
    dashboard.register({ name: "rpc" });
    dashboard.recordResult({ service: "rpc", status: "degraded", checkedAt: Date.now() });
    expect(dashboard.getServiceHealth("rpc")?.status).toBe("degraded");
  });

  it("reports a down service as unhealthy after the failure threshold", () => {
    const dashboard = new ServiceHealthDashboard();
    dashboard.register({ name: "indexer", unhealthyThreshold: 2 });
    dashboard.recordResult({ service: "indexer", status: "unhealthy", checkedAt: Date.now() });
    dashboard.recordResult({ service: "indexer", status: "unhealthy", checkedAt: Date.now() });
    expect(dashboard.getServiceHealth("indexer")?.status).toBe("unhealthy");
  });

  it("keeps the dashboard working when a result arrives for an unregistered service", () => {
    const dashboard = new ServiceHealthDashboard();
    expect(() =>
      dashboard.recordResult({ service: "unknown-service", status: "unhealthy", message: "timeout", checkedAt: Date.now() })
    ).not.toThrow();
    expect(() => dashboard.getSnapshot()).not.toThrow();
  });
});
