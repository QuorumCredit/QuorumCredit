import { createHash } from 'node:crypto';

export interface HolderCommitment {
  commitment: string;
  salt: string;
}

export interface CredentialChainLink {
  credentialId: string;
  previousCredentialId?: string;
  chainRoot: string;
}

export interface AnonymityPoolSummary {
  poolId: string;
  memberCount: number;
  minimumViable: boolean;
}

export function commitHolderName(holderName: string, salt: string): HolderCommitment {
  const normalized = holderName.trim().toLowerCase();
  const commitment = createHash('sha256')
    .update(`${normalized}:${salt}`)
    .digest('hex');
  return { commitment, salt };
}

export function linkCredentialChain(credentialId: string, previousCredentialId?: string): CredentialChainLink {
  const chainRoot = createHash('sha256')
    .update(`${previousCredentialId ?? 'root'}:${credentialId}`)
    .digest('hex');
  return { credentialId, previousCredentialId, chainRoot };
}

export function summarizeAnonymityPool(poolId: string, members: string[], minimumSize = 5): AnonymityPoolSummary {
  const uniqueMembers = new Set(members.filter(Boolean));
  return {
    poolId,
    memberCount: uniqueMembers.size,
    minimumViable: uniqueMembers.size >= minimumSize,
  };
}

export function applyExpirationGracePeriod(expiresAt: number, graceSeconds: number, now = Date.now()): {
  expiresAt: number;
  graceEndsAt: number;
  isExpired: boolean;
  isInGracePeriod: boolean;
} {
  const graceEndsAt = expiresAt + Math.max(0, graceSeconds * 1000);
  return {
    expiresAt,
    graceEndsAt,
    isExpired: now > graceEndsAt,
    isInGracePeriod: now > expiresAt && now <= graceEndsAt,
  };
}
