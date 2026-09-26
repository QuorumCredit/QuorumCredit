export type CredentialTier = "bronze" | "silver" | "gold" | "platinum";

export interface CredentialCondition {
  minCreditScore?: number;
  allowedIssuers?: string[];
  notBefore?: number;
  expiresAfter?: number;
}

export interface CredentialIssueRequest {
  id: string;
  holder: string;
  issuer: string;
  creditScore: number;
  condition?: CredentialCondition;
  metadata?: Record<string, unknown>;
}

export interface IssuedCredential {
  id: string;
  holder: string;
  issuer: string;
  tier: CredentialTier;
  issuedAt: number;
  expiresAt?: number;
  metadata: Record<string, unknown>;
}

export interface CredentialBatchResult {
  accepted: boolean;
  credentials: IssuedCredential[];
  errors: string[];
}

export interface HolderLockoutPolicy {
  maxFailures: number;
  windowMillis: number;
  lockoutMillis: number;
}

export interface HolderLockoutState {
  holder: string;
  failures: number[];
  lockedUntil?: number;
}

const DEFAULT_LOCKOUT_POLICY: HolderLockoutPolicy = {
  maxFailures: 5,
  windowMillis: 10 * 60 * 1000,
  lockoutMillis: 30 * 60 * 1000,
};

export class CredentialPolicyStore {
  private readonly credentials = new Map<string, IssuedCredential>();
  private readonly lockouts = new Map<string, HolderLockoutState>();

  constructor(private readonly lockoutPolicy: HolderLockoutPolicy = DEFAULT_LOCKOUT_POLICY) {}

  getCredential(id: string): IssuedCredential | undefined {
    return this.credentials.get(id);
  }

  getHolderLockout(holder: string, now = Date.now()): HolderLockoutState {
    const state = this.normalizeLockout(holder, now);
    return { ...state, failures: [...state.failures] };
  }

  recordHolderFailure(holder: string, now = Date.now()): HolderLockoutState {
    const state = this.normalizeLockout(holder, now);
    state.failures.push(now);
    if (state.failures.length >= this.lockoutPolicy.maxFailures) {
      state.lockedUntil = now + this.lockoutPolicy.lockoutMillis;
    }
    this.lockouts.set(holder, state);
    return { ...state, failures: [...state.failures] };
  }

  clearHolderFailures(holder: string): void {
    this.lockouts.delete(holder);
  }

  issueCredential(request: CredentialIssueRequest, now = Date.now()): CredentialBatchResult {
    return this.issueCredentialBatch([request], now);
  }

  issueCredentialBatch(requests: CredentialIssueRequest[], now = Date.now()): CredentialBatchResult {
    const seenIds = new Set<string>();
    const errors = requests.flatMap((request, index) => {
      const requestErrors = this.validateRequest(request, index, now);
      if (request.id && seenIds.has(request.id)) {
        requestErrors.push(`credential[${index}].id is duplicated in this batch`);
      }
      if (request.id) seenIds.add(request.id);
      return requestErrors;
    });
    if (errors.length > 0) {
      return { accepted: false, credentials: [], errors };
    }

    const credentials = requests.map((request) => this.toCredential(request, now));
    for (const credential of credentials) {
      this.credentials.set(credential.id, credential);
      this.clearHolderFailures(credential.holder);
    }

    return { accepted: true, credentials, errors: [] };
  }

  private validateRequest(request: CredentialIssueRequest, index: number, now: number): string[] {
    const prefix = `credential[${index}]`;
    const errors: string[] = [];
    if (!request.id) errors.push(`${prefix}.id is required`);
    if (!request.holder) errors.push(`${prefix}.holder is required`);
    if (!request.issuer) errors.push(`${prefix}.issuer is required`);
    if (!Number.isFinite(request.creditScore)) errors.push(`${prefix}.creditScore must be finite`);
    if (this.credentials.has(request.id)) errors.push(`${prefix}.id already exists`);

    const lockout = this.normalizeLockout(request.holder, now);
    if (lockout.lockedUntil && lockout.lockedUntil > now) {
      errors.push(`${prefix}.holder is locked until ${new Date(lockout.lockedUntil).toISOString()}`);
    }

    const condition = request.condition;
    if (condition?.minCreditScore !== undefined && request.creditScore < condition.minCreditScore) {
      errors.push(`${prefix}.creditScore is below required minimum`);
    }
    if (condition?.allowedIssuers?.length && !condition.allowedIssuers.includes(request.issuer)) {
      errors.push(`${prefix}.issuer is not allowed for this credential`);
    }
    if (condition?.notBefore !== undefined && now < condition.notBefore) {
      errors.push(`${prefix}.condition is not active yet`);
    }
    if (condition?.expiresAfter !== undefined && condition.expiresAfter <= now) {
      errors.push(`${prefix}.condition has already expired`);
    }

    return errors;
  }

  private toCredential(request: CredentialIssueRequest, now: number): IssuedCredential {
    return {
      id: request.id,
      holder: request.holder,
      issuer: request.issuer,
      tier: tierForCreditScore(request.creditScore),
      issuedAt: now,
      expiresAt: request.condition?.expiresAfter,
      metadata: request.metadata ?? {},
    };
  }

  private normalizeLockout(holder: string, now: number): HolderLockoutState {
    const current = this.lockouts.get(holder) ?? { holder, failures: [] };
    const windowStart = now - this.lockoutPolicy.windowMillis;
    const failures = current.failures.filter((failureAt) => failureAt >= windowStart);
    const lockedUntil = current.lockedUntil && current.lockedUntil > now ? current.lockedUntil : undefined;
    const normalized = { holder, failures, lockedUntil };
    this.lockouts.set(holder, normalized);
    return normalized;
  }
}

export function tierForCreditScore(score: number): CredentialTier {
  if (score >= 850) return "platinum";
  if (score >= 700) return "gold";
  if (score >= 550) return "silver";
  return "bronze";
}

export const credentialPolicyStore = new CredentialPolicyStore();
