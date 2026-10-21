import { describe, it, expect } from "vitest";
import { NotificationStore } from "../src/notifications/notificationStore.js";
import { NotificationDelivery } from "../src/notifications/notificationDelivery.js";

describe("notification store keeps failed deliveries", () => {
  it("keeps a notification after a failed delivery, with the reason", () => {
    const store = new NotificationStore();
    const n = store.createNotification("cred-1", "credential_issued", "email", "Issued", "Ready");
    store.markAsFailed(n.id, "smtp unavailable");

    const history = store.getNotificationHistory("cred-1");
    expect(history).toHaveLength(1);
    expect(history[0]).toMatchObject({ id: n.id, failureReason: "smtp unavailable", retryCount: 1 });
  });

  it("keeps retrying a failed notification until its retries run out", () => {
    const store = new NotificationStore();
    const n = store.createNotification("cred-2", "credential_expiring", "email", "Expiring", "Soon");

    store.markAsFailed(n.id, "timeout");
    expect(store.getPendingNotifications().map((p) => p.id)).toContain(n.id);

    for (let i = 1; i < n.maxRetries; i++) store.markAsFailed(n.id, "timeout");
    expect(store.getPendingNotifications().map((p) => p.id)).not.toContain(n.id);
    expect(store.getNotificationHistory("cred-2")[0]!.status).toBe("failed");
  });
});

describe("notification delivery reports failure", () => {
  it("returns false instead of dropping the notification when no recipient is set", async () => {
    const store = new NotificationStore();
    const delivery = new NotificationDelivery(
      { enabled: true, senderEmail: "noreply@example.com", senderName: "Quorum" },
      { enabled: false }
    );
    const n = store.createNotification("cred-3", "credential_issued", "email", "Issued", "Ready");

    expect(await delivery.send(n)).toBe(false);
  });
});
