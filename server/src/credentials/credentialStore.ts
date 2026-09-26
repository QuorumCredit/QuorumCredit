/**
 * Issue #1590, #1591: Credential Storage and Verification Status
 *
 * In-memory store for managing credentials and their verification status.
 * In production, this would be backed by a database like PostgreSQL.
 */

export interface Credential {
  id: string;
  holderId: string;
  type: "identity" | "education" | "professional" | "financial";
  issuedAt: number;
  expiresAt: number;
  issuer: string;
  status: "active" | "revoked" | "expired" | "suspended";
  metadata: Record<string, unknown>;
}

export interface VerificationRecord {
  credentialId: string;
  holderId: string;
  documentType: string;
  verificationStatus: "pending" | "verified" | "rejected" | "re_verification_required";
  verifiedAt?: number;
  verificationExpiresAt: number;
  verifier?: string;
  rejectionReason?: string;
  nextVerificationRequired: number; // timestamp when re-verification is required
}

export class CredentialStore {
  private credentials = new Map<string, Credential>();
  private verifications = new Map<string, VerificationRecord>();
  private credentialCounter = 0;
  private verificationCounter = 0;

  /**
   * Issue a new credential
   */
  issueCredential(
    holderId: string,
    type: Credential["type"],
    issuer: string,
    expiresAt: number,
    metadata?: Record<string, unknown>
  ): Credential {
    const credentialId = `cred_${++this.credentialCounter}_${Date.now()}`;
    const credential: Credential = {
      id: credentialId,
      holderId,
      type,
      issuedAt: Math.floor(Date.now() / 1000),
      expiresAt,
      issuer,
      status: "active",
      metadata: metadata || {},
    };

    this.credentials.set(credentialId, credential);
    return credential;
  }

  /**
   * Get credential by ID
   */
  getCredential(credentialId: string): Credential | undefined {
    return this.credentials.get(credentialId);
  }

  /**
   * Get all credentials for a holder
   */
  getCredentialsForHolder(holderId: string): Credential[] {
    return Array.from(this.credentials.values()).filter(
      (c) => c.holderId === holderId
    );
  }

  getAllCredentials(): Credential[] {
    return Array.from(this.credentials.values());
  }

  /**
   * Revoke a credential
   */
  revokeCredential(credentialId: string): boolean {
    const credential = this.credentials.get(credentialId);
    if (!credential) return false;
    credential.status = "revoked";
    return true;
  }

  /**
   * Record a verification attempt
   */
  recordVerification(
    credentialId: string,
    holderId: string,
    documentType: string,
    expiresAt: number,
    verificationStatus: VerificationRecord["verificationStatus"] = "pending"
  ): VerificationRecord {
    const verificationId = `ver_${++this.verificationCounter}_${Date.now()}`;
    const now = Math.floor(Date.now() / 1000);

    // Re-verification required after 1 year (365 days)
    const reVerificationRequired = now + 365 * 24 * 60 * 60;

    const verification: VerificationRecord = {
      credentialId,
      holderId,
      documentType,
      verificationStatus,
      verificationExpiresAt: expiresAt,
      nextVerificationRequired: reVerificationRequired,
    };

    this.verifications.set(verificationId, verification);
    return verification;
  }

  /**
   * Get verification status for a credential
   */
  getVerification(
    credentialId: string
  ): VerificationRecord | undefined {
    return Array.from(this.verifications.values()).find(
      (v) => v.credentialId === credentialId
    );
  }

  /**
   * Update verification status
   */
  updateVerificationStatus(
    credentialId: string,
    status: VerificationRecord["verificationStatus"],
    verifier?: string,
    rejectionReason?: string
  ): boolean {
    const verification = this.getVerification(credentialId);
    if (!verification) return false;

    verification.verificationStatus = status;
    if (status === "verified") {
      verification.verifiedAt = Math.floor(Date.now() / 1000);
    }
    if (verifier) {
      verification.verifier = verifier;
    }
    if (rejectionReason) {
      verification.rejectionReason = rejectionReason;
    }

    return true;
  }

  /**
   * Check if credential needs re-verification
   */
  needsReVerification(credentialId: string): boolean {
    const verification = this.getVerification(credentialId);
    if (!verification) return false;

    const now = Math.floor(Date.now() / 1000);
    return now > verification.nextVerificationRequired;
  }

  /**
   * Get all credentials requiring re-verification for a holder
   */
  getCredentialsNeedingReVerification(holderId: string): Credential[] {
    return this.getCredentialsForHolder(holderId).filter(
      (c) => this.needsReVerification(c.id)
    );
  }

  /**
   * Get verification statistics for a holder
   */
  getVerificationStats(holderId: string) {
    const credentials = this.getCredentialsForHolder(holderId);
    const verified = credentials.filter((c) => {
      const ver = this.getVerification(c.id);
      return ver?.verificationStatus === "verified";
    }).length;

    const pending = credentials.filter((c) => {
      const ver = this.getVerification(c.id);
      return ver?.verificationStatus === "pending";
    }).length;

    const needsReVerification = credentials.filter((c) =>
      this.needsReVerification(c.id)
    ).length;

    return {
      total: credentials.length,
      verified,
      pending,
      needsReVerification,
      verificationRate: credentials.length > 0 ? verified / credentials.length : 0,
    };
  }
}

export const credentialStore = new CredentialStore();
