/**
 * Issue #1763: Credential Deduplication Service
 *
 * Identifies and merges duplicate credentials based on similarity matching
 * across holder, issuer, type, and metadata fields. Tracks deduplication
 * results for auditability.
 */

import type { Credential } from "./credentialStore.js";

export interface DuplicateGroup {
  id: string;
  primaryCredentialId: string;
  duplicateCredentialIds: string[];
  similarityScore: number;
  detectedAt: number;
  status: "pending" | "merged" | "dismissed";
  mergedAt?: number;
}

export interface DeduplicationResult {
  totalScanned: number;
  duplicateGroupsFound: number;
  groups: DuplicateGroup[];
}

export interface MergeResult {
  groupId: string;
  primaryCredentialId: string;
  mergedCount: number;
  mergedAt: number;
}

/**
 * Computes a similarity score [0, 1] between two credentials.
 * Exact match on (holderId, issuer, type) scores high; metadata overlap adds more.
 */
function computeSimilarity(a: Credential, b: Credential): number {
  let score = 0;

  if (a.holderId === b.holderId) score += 0.4;
  if (a.issuer === b.issuer) score += 0.3;
  if (a.type === b.type) score += 0.2;

  // Metadata key overlap contribution
  const aKeys = Object.keys(a.metadata ?? {});
  const bKeys = new Set(Object.keys(b.metadata ?? {}));
  if (aKeys.length > 0 && bKeys.size > 0) {
    const overlap = aKeys.filter((k) => bKeys.has(k)).length;
    const union = new Set([...aKeys, ...bKeys]).size;
    score += 0.1 * (overlap / union);
  }

  return score;
}

export class CredentialDeduplicationService {
  /** Minimum similarity score to consider two credentials duplicates. */
  private readonly similarityThreshold: number;
  private readonly groups = new Map<string, DuplicateGroup>();
  private groupCounter = 0;

  /** Stats */
  private totalScanned = 0;
  private totalMerged = 0;

  constructor(similarityThreshold = 0.9) {
    this.similarityThreshold = similarityThreshold;
  }

  /**
   * Scan a list of credentials and detect duplicates.
   * Returns groups of credentials that appear to be duplicates of each other.
   */
  detectDuplicates(credentials: Credential[]): DeduplicationResult {
    this.totalScanned += credentials.length;
    const newGroups: DuplicateGroup[] = [];
    const alreadyGrouped = new Set<string>();

    for (let i = 0; i < credentials.length; i++) {
      const a = credentials[i]!;
      if (alreadyGrouped.has(a.id)) continue;

      const duplicates: string[] = [];
      let maxScore = 0;

      for (let j = i + 1; j < credentials.length; j++) {
        const b = credentials[j]!;
        if (alreadyGrouped.has(b.id)) continue;

        const score = computeSimilarity(a, b);
        if (score >= this.similarityThreshold) {
          duplicates.push(b.id);
          alreadyGrouped.add(b.id);
          if (score > maxScore) maxScore = score;
        }
      }

      if (duplicates.length > 0) {
        alreadyGrouped.add(a.id);
        const group: DuplicateGroup = {
          id: `dedup_${++this.groupCounter}`,
          primaryCredentialId: a.id,
          duplicateCredentialIds: duplicates,
          similarityScore: maxScore,
          detectedAt: Date.now(),
          status: "pending",
        };
        this.groups.set(group.id, group);
        newGroups.push(group);
      }
    }

    return {
      totalScanned: credentials.length,
      duplicateGroupsFound: newGroups.length,
      groups: newGroups,
    };
  }

  /**
   * Merge duplicates in a group: marks the group as merged and returns the result.
   * The caller is responsible for actually removing the duplicate credentials from storage.
   */
  mergeGroup(groupId: string): MergeResult | undefined {
    const group = this.groups.get(groupId);
    if (!group || group.status !== "pending") return undefined;

    group.status = "merged";
    group.mergedAt = Date.now();
    this.totalMerged += group.duplicateCredentialIds.length;

    return {
      groupId,
      primaryCredentialId: group.primaryCredentialId,
      mergedCount: group.duplicateCredentialIds.length,
      mergedAt: group.mergedAt,
    };
  }

  /** Dismiss a duplicate group without merging. */
  dismissGroup(groupId: string): boolean {
    const group = this.groups.get(groupId);
    if (!group || group.status !== "pending") return false;
    group.status = "dismissed";
    return true;
  }

  /** Get a duplicate group by ID. */
  getGroup(groupId: string): DuplicateGroup | undefined {
    return this.groups.get(groupId);
  }

  /** List all pending duplicate groups. */
  getPendingGroups(): DuplicateGroup[] {
    return Array.from(this.groups.values()).filter((g) => g.status === "pending");
  }

  /** Deduplication statistics. */
  getStats(): {
    totalScanned: number;
    totalGroupsDetected: number;
    pendingGroups: number;
    mergedGroups: number;
    dismissedGroups: number;
    totalMerged: number;
  } {
    const all = Array.from(this.groups.values());
    return {
      totalScanned: this.totalScanned,
      totalGroupsDetected: all.length,
      pendingGroups: all.filter((g) => g.status === "pending").length,
      mergedGroups: all.filter((g) => g.status === "merged").length,
      dismissedGroups: all.filter((g) => g.status === "dismissed").length,
      totalMerged: this.totalMerged,
    };
  }
}

export const credentialDeduplicationService = new CredentialDeduplicationService();
