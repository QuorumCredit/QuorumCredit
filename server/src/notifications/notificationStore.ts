/**
 * Issue #1583: Notification store for managing credential holder notifications
 * and preferences. Provides infrastructure for email and push notifications.
 */

export type NotificationType =
  | "credential_issued"
  | "credential_expiring"
  | "credential_expired"
  | "verification_requested"
  | "verification_completed"
  | "account_activity"
  | "security_alert";

export type NotificationChannel = "email" | "push" | "sms";

export interface NotificationPreferences {
  credentialId: string;
  email?: string;
  pushToken?: string;
  phoneNumber?: string;
  enabledChannels: NotificationChannel[];
  enabledTypes: Set<NotificationType>;
  unsubscribedAt?: number;
}

export interface Notification {
  id: string;
  credentialId: string;
  type: NotificationType;
  channel: NotificationChannel;
  status: "pending" | "sent" | "failed" | "bounced";
  title: string;
  message: string;
  metadata?: Record<string, unknown>;
  createdAt: number;
  sentAt?: number;
  failureReason?: string;
  retryCount: number;
  maxRetries: number;
}

export class NotificationStore {
  private preferences = new Map<string, NotificationPreferences>();
  private notifications: Notification[] = [];
  private notificationId = 0;

  /**
   * Get or create notification preferences for a credential holder.
   */
  getPreferences(credentialId: string): NotificationPreferences {
    if (!this.preferences.has(credentialId)) {
      this.preferences.set(credentialId, {
        credentialId,
        enabledChannels: ["email"],
        enabledTypes: new Set([
          "credential_issued",
          "credential_expiring",
          "verification_completed",
        ]),
      });
    }
    return this.preferences.get(credentialId)!;
  }

  /**
   * Update notification preferences for a credential holder.
   */
  updatePreferences(
    credentialId: string,
    updates: Partial<NotificationPreferences>
  ): NotificationPreferences {
    const prefs = this.getPreferences(credentialId);
    Object.assign(prefs, updates);
    return prefs;
  }

  /**
   * Enable notifications for specific types and channels.
   */
  enableNotifications(
    credentialId: string,
    types: NotificationType[],
    channels: NotificationChannel[]
  ): void {
    const prefs = this.getPreferences(credentialId);
    types.forEach((t) => prefs.enabledTypes.add(t));
    channels.forEach((c) => {
      if (!prefs.enabledChannels.includes(c)) {
        prefs.enabledChannels.push(c);
      }
    });
    delete prefs.unsubscribedAt;
  }

  /**
   * Disable notifications entirely for a credential holder.
   */
  unsubscribe(credentialId: string): void {
    const prefs = this.getPreferences(credentialId);
    prefs.unsubscribedAt = Date.now();
    prefs.enabledChannels = [];
    prefs.enabledTypes.clear();
  }

  /**
   * Check if a notification type is enabled for a credential holder.
   */
  isNotificationEnabled(
    credentialId: string,
    type: NotificationType
  ): boolean {
    const prefs = this.getPreferences(credentialId);
    if (prefs.unsubscribedAt) return false;
    return prefs.enabledTypes.has(type);
  }

  /**
   * Create a new notification.
   */
  createNotification(
    credentialId: string,
    type: NotificationType,
    channel: NotificationChannel,
    title: string,
    message: string,
    metadata?: Record<string, unknown>
  ): Notification {
    const notification: Notification = {
      id: `notif_${++this.notificationId}`,
      credentialId,
      type,
      channel,
      status: "pending",
      title,
      message,
      metadata,
      createdAt: Date.now(),
      retryCount: 0,
      maxRetries: 3,
    };
    this.notifications.push(notification);
    return notification;
  }

  /**
   * Get pending notifications for delivery.
   */
  getPendingNotifications(
    limit: number = 100
  ): Notification[] {
    return this.notifications
      .filter((n) => n.status === "pending" && n.retryCount < n.maxRetries)
      .slice(0, limit);
  }

  /**
   * Mark a notification as sent.
   */
  markAsSent(notificationId: string): void {
    const notif = this.notifications.find((n) => n.id === notificationId);
    if (notif) {
      notif.status = "sent";
      notif.sentAt = Date.now();
    }
  }

  /**
   * Mark a notification as failed and increment retry count.
   */
  markAsFailed(
    notificationId: string,
    reason: string
  ): void {
    const notif = this.notifications.find((n) => n.id === notificationId);
    if (notif) {
      notif.retryCount++;
      notif.failureReason = reason;
      if (notif.retryCount >= notif.maxRetries) {
        notif.status = "failed";
      }
    }
  }

  /**
   * Get notification history for a credential holder.
   */
  getNotificationHistory(
    credentialId: string,
    limit: number = 50
  ): Notification[] {
    return this.notifications
      .filter((n) => n.credentialId === credentialId)
      .sort((a, b) => b.createdAt - a.createdAt)
      .slice(0, limit);
  }

  /**
   * Get statistics about notifications.
   */
  getStatistics(): {
    totalNotifications: number;
    pendingCount: number;
    sentCount: number;
    failedCount: number;
    bouncedCount: number;
    credentialsWithPreferences: number;
  } {
    return {
      totalNotifications: this.notifications.length,
      pendingCount: this.notifications.filter((n) => n.status === "pending")
        .length,
      sentCount: this.notifications.filter((n) => n.status === "sent").length,
      failedCount: this.notifications.filter((n) => n.status === "failed")
        .length,
      bouncedCount: this.notifications.filter((n) => n.status === "bounced")
        .length,
      credentialsWithPreferences: this.preferences.size,
    };
  }
}

// Singleton instance
export const notificationStore = new NotificationStore();
