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

/** Issue #1743: emitted whenever a credential's status (or verification status) changes. */
export interface CredentialStatusChange {
  credentialId: string;
  holderId: string;
  status: Credential["status"];
  verificationStatus?: VerificationRecord["verificationStatus"];
  previousStatus?: Credential["status"];
  changedAt: number;
}

export type CredentialStatusListener = (change: CredentialStatusChange) => void;

class CredentialStore {
  private credentials = new Map<string, Credential>();
  /** Issue #1742: issuer -> credential ids, so pattern search runs the regex once
   * per distinct issuer instead of once per credential. */
  private issuerIndex = new Map<string, Set<string>>();
  private statusListeners = new Set<CredentialStatusListener>();
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
    let ids = this.issuerIndex.get(issuer);
    if (!ids) {
      ids = new Set();
      this.issuerIndex.set(issuer, ids);
    }
    ids.add(credentialId);
    this.emitStatusChange(credential);
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

  /**
   * Revoke a credential
   */
  revokeCredential(credentialId: string): boolean {
    const credential = this.credentials.get(credentialId);
    if (!credential) return false;
    return this.setCredentialStatus(credentialId, "revoked");
  }

  /**
   * Issue #1743: change a credential's status and notify status subscribers.
   */
  setCredentialStatus(credentialId: string, status: Credential["status"]): boolean {
    const credential = this.credentials.get(credentialId);
    if (!credential) return false;
    const previousStatus = credential.status;
    if (previousStatus === status) return true;
    credential.status = status;
    this.emitStatusChange(credential, previousStatus);
    return true;
  }

  /** Issue #1742: distinct issuers currently holding at least one credential. */
  getIssuers(): string[] {
    return Array.from(this.issuerIndex.keys());
  }

  /** Issue #1742: credentials issued by any of the given issuers. */
  getCredentialsByIssuers(issuers: Iterable<string>): Credential[] {
    const out: Credential[] = [];
    for (const issuer of issuers) {
      const ids = this.issuerIndex.get(issuer);
      if (!ids) continue;
      for (const id of ids) {
        const credential = this.credentials.get(id);
        if (credential) out.push(credential);
      }
    }
    return out;
  }

  /** Issue #1743: subscribe to credential status changes. Returns an unsubscribe fn. */
  onStatusChange(listener: CredentialStatusListener): () => void {
    this.statusListeners.add(listener);
    return () => this.statusListeners.delete(listener);
  }

  private emitStatusChange(
    credential: Credential,
    previousStatus?: Credential["status"]
  ): void {
    const change: CredentialStatusChange = {
      credentialId: credential.id,
      holderId: credential.holderId,
      status: credential.status,
      verificationStatus: this.getVerification(credential.id)?.verificationStatus,
      previousStatus,
      changedAt: Math.floor(Date.now() / 1000),
    };
    for (const listener of this.statusListeners) {
      try {
        listener(change);
      } catch (err) {
        console.error("[credentialStore] status listener failed", err);
      }
    }
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

    const credential = this.credentials.get(credentialId);
    if (credential) this.emitStatusChange(credential, credential.status);

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
