import type { Notification } from "./notificationStore.js";

/**
 * Issue #1583: Notification delivery service for sending credential holder
 * notifications via email and push notification channels.
 */

export interface EmailConfig {
  enabled: boolean;
  senderEmail: string;
  senderName: string;
  smtpUrl?: string;
}

export interface PushConfig {
  enabled: boolean;
  providerApiKey?: string;
  providerUrl?: string;
}

export class NotificationDelivery {
  private emailConfig: EmailConfig;
  private pushConfig: PushConfig;

  constructor(emailConfig: EmailConfig, pushConfig: PushConfig) {
    this.emailConfig = emailConfig;
    this.pushConfig = pushConfig;
  }

  /**
   * Send a notification via the specified channel.
   */
  async send(
    notification: Notification,
    recipientEmail?: string,
    recipientPushToken?: string
  ): Promise<boolean> {
    try {
      switch (notification.channel) {
        case "email":
          if (!this.emailConfig.enabled || !recipientEmail) {
            throw new Error("Email delivery is not configured");
          }
          await this.sendEmail(notification, recipientEmail);
          return true;

        case "push":
          if (!this.pushConfig.enabled || !recipientPushToken) {
            throw new Error("Push notification delivery is not configured");
          }
          await this.sendPush(notification, recipientPushToken);
          return true;

        case "sms":
          // SMS support placeholder for future implementation
          console.warn(
            `[notifications] SMS delivery not yet implemented for ${notification.id}`
          );
          return false;

        default:
          throw new Error(`Unknown notification channel: ${notification.channel}`);
      }
    } catch (error) {
      const errorMsg =
        error instanceof Error ? error.message : String(error);
      console.error(
        `[notifications] Delivery failed for ${notification.id}: ${errorMsg}`
      );
      throw error;
    }
  }

  /**
   * Send email notification.
   */
  private async sendEmail(
    notification: Notification,
    recipientEmail: string
  ): Promise<void> {
    // In production, this would integrate with a real email service (SendGrid, AWS SES, etc.)
    // For now, we'll simulate the send
    console.log(
      `[notifications:email] Sending email to ${recipientEmail}`,
      `Subject: ${notification.title}`,
      `Message: ${notification.message}`
    );

    // Simulate email delivery
    await new Promise((resolve) => setTimeout(resolve, 100));

    // In a real implementation, you would:
    // 1. Connect to SMTP server or email service API
    // 2. Format the email with HTML template
    // 3. Add unsubscribe link based on credentials
    // 4. Send and wait for confirmation
    // 5. Handle bounces and complaints

    console.log(
      `[notifications:email] Successfully queued email for ${recipientEmail}`
    );
  }

  /**
   * Send push notification.
   */
  private async sendPush(
    notification: Notification,
    recipientPushToken: string
  ): Promise<void> {
    // In production, this would integrate with FCM, APNs, or another push service
    console.log(
      `[notifications:push] Sending push notification to token ${recipientPushToken.substring(0, 20)}...`,
      `Title: ${notification.title}`,
      `Message: ${notification.message}`
    );

    // Simulate push delivery
    await new Promise((resolve) => setTimeout(resolve, 100));

    // In a real implementation, you would:
    // 1. Connect to Firebase Cloud Messaging, APNs, or WebPush
    // 2. Format notification payload with title and body
    // 3. Include metadata for deep linking
    // 4. Send and handle delivery receipts
    // 5. Track opens and clicks

    console.log(
      `[notifications:push] Successfully queued push notification for token ${recipientPushToken.substring(0, 20)}...`
    );
  }

  /**
   * Validate email address format.
   */
  isValidEmail(email: string): boolean {
    const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
    return emailRegex.test(email);
  }

  /**
   * Validate push token format (basic check).
   */
  isValidPushToken(token: string): boolean {
    return !!token && token.length > 10;
  }

  /**
   * Get notification template for a type.
   */
  getTemplate(
    type: string
  ): { subject: string; template: (_data?: Record<string, unknown>) => string } {
    const templates: Record<
      string,
      { subject: string; template: (data?: Record<string, unknown>) => string }
    > = {
      credential_issued: {
        subject: "Your credential has been issued",
        template: (data) =>
          `Your new credential has been successfully issued. Credential ID: ${data?.credentialId || ""}`,
      },
      credential_expiring: {
        subject: "Your credential is expiring soon",
        template: (data) =>
          `Your credential will expire on ${data?.expiryDate || ""}. Please renew it to avoid service interruption.`,
      },
      credential_expired: {
        subject: "Your credential has expired",
        template: () => "Your credential has expired. Please renew it immediately.",
      },
      verification_requested: {
        subject: "Verification requested",
        template: (data) =>
          `A verification has been requested for your account. Please respond to complete the process.`,
      },
      verification_completed: {
        subject: "Verification completed",
        template: () => "Your verification has been completed successfully.",
      },
      account_activity: {
        subject: "Account activity alert",
        template: (data) =>
          `Unusual activity detected: ${data?.activity || "unknown"}. If this wasn't you, please secure your account.`,
      },
      security_alert: {
        subject: "Security alert",
        template: (data) =>
          `Security alert: ${data?.alert || "unknown"}. Please review your account security settings.`,
      },
    };

    return (
      templates[type] || {
        subject: "Notification",
        template: () => "You have a new notification.",
      }
    );
  }
}
