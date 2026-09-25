/**
 * Issue #1593: API Authentication Token Expiration Monitoring
 *
 * Monitors token expiration, sends warnings, and tracks authentication failures.
 */

export interface TokenExpirationEvent {
  jti: string;
  expiresAt: number;
  subject: string;
  issuedAt: number;
  expiresIn: number; // seconds
  warningsSent: { level: string; timestamp: number }[];
}

export interface AuthenticationFailure {
  timestamp: number;
  reason: "invalid_key" | "invalid_signature" | "expired_token" | "revoked_token" | "rate_limited";
  sourceIp: string;
  subject?: string;
}

export interface ExpirationWarning {
  jti: string;
  level: "info" | "warning" | "critical";
  message: string;
  expiresIn: number; // seconds remaining
  timestamp: number;
}

class TokenExpirationMonitor {
  private tokenRegistry = new Map<string, TokenExpirationEvent>();
  private authFailures: AuthenticationFailure[] = [];
  private expirationWarnings: ExpirationWarning[] = [];
  private maxFailuresPerIp = 10;
  private failureWindow = 3600000; // 1 hour

  /**
   * Register a new token for monitoring
   */
  registerToken(
    jti: string,
    expiresAt: number,
    subject: string,
    issuedAt: number
  ): TokenExpirationEvent {
    const event: TokenExpirationEvent = {
      jti,
      expiresAt,
      subject,
      issuedAt,
      expiresIn: Math.floor((expiresAt - Date.now()) / 1000),
      warningsSent: [],
    };

    this.tokenRegistry.set(jti, event);
    return event;
  }

  /**
   * Get token expiration info
   */
  getTokenInfo(jti: string): TokenExpirationEvent | undefined {
    return this.tokenRegistry.get(jti);
  }

  /**
   * Check if token is expiring soon (within 5 minutes)
   */
  isExpiringsoon(jti: string): boolean {
    const token = this.getTokenInfo(jti);
    if (!token) return false;

    const expiresIn = Math.floor((token.expiresAt - Date.now()) / 1000);
    return expiresIn < 300 && expiresIn > 0; // 5 minutes
  }

  /**
   * Check if token has expired
   */
  hasExpired(jti: string): boolean {
    const token = this.getTokenInfo(jti);
    if (!token) return false;

    return Date.now() >= token.expiresAt;
  }

  /**
   * Generate expiration warning
   */
  generateWarning(jti: string): ExpirationWarning | null {
    const token = this.getTokenInfo(jti);
    if (!token) return null;

    const now = Date.now();
    const expiresIn = Math.floor((token.expiresAt - now) / 1000);

    if (expiresIn <= 0) {
      return null; // Already expired
    }

    let level: "info" | "warning" | "critical";
    let message: string;

    if (expiresIn < 60) {
      level = "critical";
      message = `Token expires in ${expiresIn} seconds`;
    } else if (expiresIn < 300) {
      level = "warning";
      message = `Token expires in ${Math.floor(expiresIn / 60)} minutes`;
    } else if (expiresIn < 3600) {
      level = "info";
      message = `Token expires in ${Math.floor(expiresIn / 60)} minutes`;
    } else {
      return null; // Not expiring soon
    }

    const warning: ExpirationWarning = {
      jti,
      level,
      message,
      expiresIn,
      timestamp: now,
    };

    // Track that we sent this warning
    token.warningsSent.push({ level, timestamp: now });

    // Keep warnings list bounded (keep last 100)
    this.expirationWarnings.push(warning);
    if (this.expirationWarnings.length > 100) {
      this.expirationWarnings = this.expirationWarnings.slice(-100);
    }

    return warning;
  }

  /**
   * Record an authentication failure
   */
  recordFailure(
    reason: AuthenticationFailure["reason"],
    sourceIp: string,
    subject?: string
  ): void {
    const failure: AuthenticationFailure = {
      timestamp: Date.now(),
      reason,
      sourceIp,
      subject,
    };

    this.authFailures.push(failure);

    // Keep failures list bounded (keep last 1000)
    if (this.authFailures.length > 1000) {
      this.authFailures = this.authFailures.slice(-1000);
    }
  }

  /**
   * Get authentication failures for a specific IP within the failure window
   */
  getRecentFailures(sourceIp: string): AuthenticationFailure[] {
    const cutoff = Date.now() - this.failureWindow;
    return this.authFailures.filter(
      (f) => f.sourceIp === sourceIp && f.timestamp > cutoff
    );
  }

  /**
   * Check if an IP is rate-limited based on failures
   */
  isRateLimited(sourceIp: string): boolean {
    return this.getRecentFailures(sourceIp).length >= this.maxFailuresPerIp;
  }

  /**
   * Get expiration status for all tokens
   */
  getExpirationStatus() {
    const now = Date.now();
    const tokens = Array.from(this.tokenRegistry.values());

    const expired = tokens.filter((t) => t.expiresAt <= now);
    const expiringSoon = tokens.filter(
      (t) => t.expiresAt > now && t.expiresAt - now < 300000 // 5 minutes
    );
    const active = tokens.filter(
      (t) => t.expiresAt > now && t.expiresAt - now >= 300000
    );

    return {
      total: tokens.length,
      expired: expired.length,
      expiringSoon: expiringSoon.length,
      active: active.length,
      expiringTokens: expiringSoon.map((t) => ({
        jti: t.jti,
        subject: t.subject,
        expiresIn: Math.floor((t.expiresAt - now) / 1000),
      })),
    };
  }

  /**
   * Get recent warnings
   */
  getRecentWarnings(limit: number = 20): ExpirationWarning[] {
    return this.expirationWarnings.slice(-limit).reverse();
  }

  /**
   * Get authentication failure statistics
   */
  getFailureStatistics() {
    const failures = this.authFailures;
    const byReason: Record<string, number> = {};
    const bySourceIp: Record<string, number> = {};

    for (const failure of failures) {
      byReason[failure.reason] = (byReason[failure.reason] || 0) + 1;
      bySourceIp[failure.sourceIp] = (bySourceIp[failure.sourceIp] || 0) + 1;
    }

    return {
      totalFailures: failures.length,
      byReason,
      bySourceIp,
      topFailingSources: Object.entries(bySourceIp)
        .sort(([, a], [, b]) => b - a)
        .slice(0, 5)
        .map(([ip, count]) => ({ ip, count })),
    };
  }

  /**
   * Clean up expired tokens from registry (call periodically)
   */
  cleanup(): number {
    const before = this.tokenRegistry.size;
    const now = Date.now();

    for (const [jti, token] of this.tokenRegistry) {
      // Remove tokens that expired more than 1 day ago
      if (token.expiresAt < now - 86400000) {
        this.tokenRegistry.delete(jti);
      }
    }

    return before - this.tokenRegistry.size;
  }

  /**
   * Get health metrics
   */
  getHealthMetrics() {
    const status = this.getExpirationStatus();
    const failures = this.getFailureStatistics();
    const now = Date.now();
    const oldestToken = Math.min(
      ...Array.from(this.tokenRegistry.values()).map((t) => t.issuedAt)
    );

    return {
      tokenCount: status.total,
      expiredTokens: status.expired,
      expiringTokens: status.expiringSoon,
      recentFailures: failures.totalFailures,
      monitoringDuration: oldestToken ? now - oldestToken : 0,
      registrySize: this.tokenRegistry.size,
      warningsGenerated: this.expirationWarnings.length,
    };
  }
}

export const tokenExpirationMonitor = new TokenExpirationMonitor();
