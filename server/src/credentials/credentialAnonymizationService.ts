/**
 * Issue #1764: Credential Anonymization Service
 *
 * Anonymizes credential data for safe export and data sharing.
 * Supports reversible anonymization via a token map, and tracks
 * anonymization operations for audit purposes.
 */

import type { Credential } from "./credentialStore.js";

export interface AnonymizationOptions {
  /** Include reversible token so anonymization can be undone. Default: false. */
  reversible?: boolean;
  /** Fields within metadata to suppress (replace with null). */
  suppressMetadataFields?: string[];
}

export interface AnonymizedCredential {
  /** Opaque identifier — not the real credential ID. */
  anonymizedId: string;
  /** Opaque holder reference. */
  anonymizedHolderId: string;
  type: Credential["type"];
  issuedAt: number;
  expiresAt: number;
  /** Issuer is replaced with a hash-like opaque string. */
  anonymizedIssuer: string;
  status: Credential["status"];
  metadata: Record<string, unknown>;
  /** Present only when `reversible: true` was requested. */
  reversalToken?: string;
}

export interface AnonymizationRecord {
  id: string;
  originalCredentialId: string;
  anonymizedId: string;
  anonymizedAt: number;
  reversible: boolean;
  reversalToken?: string;
}

/** Simple deterministic obfuscation — not cryptographic, sufficient for in-memory demo. */
function obfuscate(value: string, salt: string): string {
  let hash = 0;
  const input = `${salt}:${value}`;
  for (let i = 0; i < input.length; i++) {
    hash = ((hash << 5) - hash + input.charCodeAt(i)) | 0;
  }
  return Math.abs(hash).toString(16).padStart(8, "0");
}

export class CredentialAnonymizationService {
  private readonly records = new Map<string, AnonymizationRecord>();
  /** Maps reversalToken → original credential ID for reversal. */
  private readonly reversalMap = new Map<string, string>();
  private recordCounter = 0;
  private readonly salt: string;

  constructor(salt = "qc-anon-v1") {
    this.salt = salt;
  }

  /**
   * Anonymize a credential for safe export.
   * Returns the anonymized representation; optionally stores a reversal token.
   */
  anonymize(
    credential: Credential,
    options: AnonymizationOptions = {}
  ): AnonymizedCredential {
    const anonymizedId = `anon_${obfuscate(credential.id, this.salt)}`;
    const anonymizedHolderId = `holder_${obfuscate(credential.holderId, this.salt)}`;
    const anonymizedIssuer = `issuer_${obfuscate(credential.issuer, this.salt)}`;

    // Suppress specified metadata fields
    const metadata: Record<string, unknown> = { ...credential.metadata };
    for (const field of options.suppressMetadataFields ?? []) {
      if (field in metadata) metadata[field] = null;
    }

    const reversalToken = options.reversible
      ? `rev_${obfuscate(credential.id, `${this.salt}-rev-${Date.now()}`)}`
      : undefined;

    const record: AnonymizationRecord = {
      id: `anonrec_${++this.recordCounter}`,
      originalCredentialId: credential.id,
      anonymizedId,
      anonymizedAt: Date.now(),
      reversible: options.reversible ?? false,
      reversalToken,
    };

    this.records.set(record.id, record);
    if (reversalToken) {
      this.reversalMap.set(reversalToken, credential.id);
    }

    const result: AnonymizedCredential = {
      anonymizedId,
      anonymizedHolderId,
      type: credential.type,
      issuedAt: credential.issuedAt,
      expiresAt: credential.expiresAt,
      anonymizedIssuer,
      status: credential.status,
      metadata,
    };

    if (reversalToken) result.reversalToken = reversalToken;
    return result;
  }

  /**
   * Bulk anonymize a list of credentials.
   */
  anonymizeBatch(
    credentials: Credential[],
    options: AnonymizationOptions = {}
  ): AnonymizedCredential[] {
    return credentials.map((c) => this.anonymize(c, options));
  }

  /**
   * Reverse an anonymization using a reversal token.
   * Returns the original credential ID, or undefined if token is unknown/invalid.
   */
  reverse(reversalToken: string): string | undefined {
    return this.reversalMap.get(reversalToken);
  }

  /** Get all anonymization records. */
  getRecords(): AnonymizationRecord[] {
    return Array.from(this.records.values());
  }

  /** Anonymization statistics. */
  getStats(): {
    totalAnonymized: number;
    reversibleRecords: number;
    irreversibleRecords: number;
  } {
    const all = Array.from(this.records.values());
    return {
      totalAnonymized: all.length,
      reversibleRecords: all.filter((r) => r.reversible).length,
      irreversibleRecords: all.filter((r) => !r.reversible).length,
    };
  }
}

export const credentialAnonymizationService = new CredentialAnonymizationService();
