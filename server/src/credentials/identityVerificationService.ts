/**
 * Issue #1591: Advanced Identity Verification Service
 *
 * Provides document verification, fraud detection, and re-verification scheduling.
 */

import { createHash } from "node:crypto";

export interface DocumentVerification {
  documentType: "passport" | "driver_license" | "national_id" | "utility_bill" | "bank_statement";
  documentHash: string;
  verifiedAt?: number;
  expiresAt: number;
  issuer?: string;
  metadata: Record<string, unknown>;
}

export interface VerificationChallenge {
  challengeId: string;
  credentialId: string;
  holderId: string;
  challengeType: "face_match" | "document_liveness" | "manual_review";
  status: "pending" | "passed" | "failed";
  createdAt: number;
  completedAt?: number;
  metadata?: Record<string, unknown>;
}

export interface ReVerificationSchedule {
  credentialId: string;
  lastVerifiedAt: number;
  nextScheduledAt: number;
  frequency: "annual" | "biennial" | "on_demand";
  reason?: string;
}

class IdentityVerificationService {
  private documentVerifications = new Map<string, DocumentVerification[]>();
  private verificationChallenges = new Map<string, VerificationChallenge>();
  private reVerificationSchedules = new Map<string, ReVerificationSchedule>();
  private challengeCounter = 0;

  /**
   * Compute SHA256 hash of document for comparison
   */
  hashDocument(documentData: string): string {
    return createHash("sha256").update(documentData).digest("hex");
  }

  /**
   * Verify a document for a credential
   */
  verifyDocument(
    credentialId: string,
    documentType: DocumentVerification["documentType"],
    documentHash: string,
    expiresAt: number,
    metadata?: Record<string, unknown>
  ): DocumentVerification {
    const verification: DocumentVerification = {
      documentType,
      documentHash,
      verifiedAt: Math.floor(Date.now() / 1000),
      expiresAt,
      metadata: metadata || {},
    };

    if (!this.documentVerifications.has(credentialId)) {
      this.documentVerifications.set(credentialId, []);
    }

    this.documentVerifications.get(credentialId)!.push(verification);

    // Create re-verification schedule
    this.scheduleReVerification(credentialId, "annual");

    return verification;
  }

  /**
   * Get all document verifications for a credential
   */
  getDocumentVerifications(credentialId: string): DocumentVerification[] {
    return this.documentVerifications.get(credentialId) || [];
  }

  /**
   * Check if document verification is still valid
   */
  isDocumentValid(credentialId: string): boolean {
    const verifications = this.getDocumentVerifications(credentialId);
    if (verifications.length === 0) return false;

    const now = Math.floor(Date.now() / 1000);
    // Check if any verification is not expired
    return verifications.some((v) => v.verifiedAt && now < v.expiresAt);
  }

  /**
   * Create a verification challenge for additional verification
   */
  createChallenge(
    credentialId: string,
    holderId: string,
    challengeType: VerificationChallenge["challengeType"],
    metadata?: Record<string, unknown>
  ): VerificationChallenge {
    const challengeId = `ch_${++this.challengeCounter}_${Date.now()}`;

    const challenge: VerificationChallenge = {
      challengeId,
      credentialId,
      holderId,
      challengeType,
      status: "pending",
      createdAt: Math.floor(Date.now() / 1000),
      metadata: metadata || {},
    };

    this.verificationChallenges.set(challengeId, challenge);
    return challenge;
  }

  /**
   * Complete a verification challenge
   */
  completeChallenge(
    challengeId: string,
    passed: boolean,
    metadata?: Record<string, unknown>
  ): VerificationChallenge | undefined {
    const challenge = this.verificationChallenges.get(challengeId);
    if (!challenge) return undefined;

    challenge.status = passed ? "passed" : "failed";
    challenge.completedAt = Math.floor(Date.now() / 1000);
    if (metadata) {
      challenge.metadata = { ...challenge.metadata, ...metadata };
    }

    return challenge;
  }

  /**
   * Get verification challenge by ID
   */
  getChallenge(challengeId: string): VerificationChallenge | undefined {
    return this.verificationChallenges.get(challengeId);
  }

  /**
   * Schedule re-verification for a credential
   */
  scheduleReVerification(
    credentialId: string,
    frequency: "annual" | "biennial" | "on_demand",
    reason?: string
  ): ReVerificationSchedule {
    const now = Math.floor(Date.now() / 1000);
    const frequencySeconds = frequency === "annual" ? 365 * 24 * 60 * 60 :
                             frequency === "biennial" ? 2 * 365 * 24 * 60 * 60 : 0;

    const schedule: ReVerificationSchedule = {
      credentialId,
      lastVerifiedAt: now,
      nextScheduledAt: frequency === "on_demand" ? 0 : now + frequencySeconds,
      frequency,
      reason,
    };

    this.reVerificationSchedules.set(credentialId, schedule);
    return schedule;
  }

  /**
   * Get re-verification schedule
   */
  getReVerificationSchedule(
    credentialId: string
  ): ReVerificationSchedule | undefined {
    return this.reVerificationSchedules.get(credentialId);
  }

  /**
   * Check if re-verification is due
   */
  isReVerificationDue(credentialId: string): boolean {
    const schedule = this.getReVerificationSchedule(credentialId);
    if (!schedule) return false;

    if (schedule.frequency === "on_demand") {
      return true;
    }

    const now = Math.floor(Date.now() / 1000);
    return now >= schedule.nextScheduledAt;
  }

  /**
   * Get all credentials needing re-verification
   */
  getCredentialsNeedingReVerification(): string[] {
    const needsReVerification: string[] = [];

    for (const [credentialId] of this.reVerificationSchedules) {
      if (this.isReVerificationDue(credentialId)) {
        needsReVerification.push(credentialId);
      }
    }

    return needsReVerification;
  }

  /**
   * Get verification health score (0-100) for a credential
   */
  getVerificationScore(credentialId: string): number {
    let score = 100;

    // Check document validity
    if (!this.isDocumentValid(credentialId)) {
      score -= 30;
    }

    // Check re-verification status
    if (this.isReVerificationDue(credentialId)) {
      score -= 20;
    }

    // Check for pending challenges
    const hasFailedChallenges = Array.from(this.verificationChallenges.values()).some(
      (c) => c.credentialId === credentialId && c.status === "failed"
    );
    if (hasFailedChallenges) {
      score -= 25;
    }

    return Math.max(0, score);
  }

  /**
   * Get detailed verification report
   */
  getVerificationReport(credentialId: string) {
    const documents = this.getDocumentVerifications(credentialId);
    const schedule = this.getReVerificationSchedule(credentialId);
    const challenges = Array.from(this.verificationChallenges.values()).filter(
      (c) => c.credentialId === credentialId
    );
    const score = this.getVerificationScore(credentialId);
    const isReVerificationDue = this.isReVerificationDue(credentialId);

    return {
      credentialId,
      documents,
      schedule,
      challenges,
      verificationScore: score,
      isReVerificationDue,
      status: score >= 80 ? "verified" : score >= 50 ? "at_risk" : "failed",
      lastUpdated: Math.floor(Date.now() / 1000),
    };
  }
}

export const identityVerificationService = new IdentityVerificationService();
