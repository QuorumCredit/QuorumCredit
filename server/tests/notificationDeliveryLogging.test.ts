import { describe, it, expect, afterEach, vi } from "vitest";
import { NotificationDelivery } from "../src/notifications/notificationDelivery.js";

describe("notification delivery logging", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("includes the notification ID in every delivery log entry", async () => {
    const logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
    const delivery = new NotificationDelivery(
      { enabled: true, senderEmail: "noreply@example.com", senderName: "Quorum" },
      { enabled: false }
    );

    await delivery.send(
      {
        id: "notif_test_1",
        credentialId: "cred_1",
        type: "credential_issued",
        channel: "email",
        status: "pending",
        title: "Credential issued",
        message: "Your credential is ready",
      } as never,
      "holder@example.com"
    );

    const entries = logSpy.mock.calls.map((call) => String(call[0]));
    expect(entries.length).toBeGreaterThan(0);
    for (const entry of entries) {
      expect(entry).toContain("notif_test_1");
    }
  });
});
