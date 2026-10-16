/**
 * Issue #1777: Attestor conflict resolution.
 *
 * Attestors record verdicts on credentials. A conflict exists when two attestors on
 * the same credential reach opposite verdicts. A dispute can only be opened against
 * a detected conflict, and each dispute is resolved once by an arbiter who is not a
 * party to it. Every state change is appended to the dispute's history.
 *
 * Resolution records an outcome. It does not move funds or slash stakes, which stay
 * with the on-chain contract. In-memory, like the other stores in this server.
 */

import { randomUUID } from "node:crypto";

export const ATTESTATION_VERDICTS = ["verified", "rejected"] as const;
export type AttestationVerdict = (typeof ATTESTATION_VERDICTS)[number];

export const DISPUTE_OUTCOMES = ["upheld", "dismissed"] as const;
export type DisputeOutcome = (typeof DISPUTE_OUTCOMES)[number];

export interface Attestation {
  credentialId: string;
  attestor: string;
  verdict: AttestationVerdict;
  note?: string;
  recordedAt: number;
}

export interface AttestationConflict {
  credentialId: string;
  attestorA: string;
  verdictA: AttestationVerdict;
  attestorB: string;
  verdictB: AttestationVerdict;
}

export interface DisputeEvent {
  type: "opened" | "resolved";
  actor: string;
  at: number;
  note?: string;
}

export interface Dispute {
  id: string;
  credentialId: string;
  raisedBy: string;
  attestors: string[];
  reason: string;
  conflicts: AttestationConflict[];
  status: "open" | "resolved";
  outcome?: DisputeOutcome;
  resolvedBy?: string;
  createdAt: number;
  resolvedAt?: number;
  history: DisputeEvent[];
}

export type DisputeError = { error: string; status: 400 | 403 | 404 | 409 };

export const MIN_REASON_LENGTH = 10;
export const MAX_REASON_LENGTH = 1000;
export const MAX_NOTE_LENGTH = 1000;

const now = () => Math.floor(Date.now() / 1000);

/** Records one verdict per attestor per credential and finds conflicting pairs. */
export class AttestationLedger {
  private byCredential = new Map<string, Map<string, Attestation>>();

  record(
    credentialId: string,
    attestor: string,
    verdict: AttestationVerdict,
    note?: string
  ): Attestation | DisputeError {
    if (!ATTESTATION_VERDICTS.includes(verdict)) {
      return { error: "verdict must be verified or rejected", status: 400 };
    }
    let attestations = this.byCredential.get(credentialId);
    if (!attestations) {
      attestations = new Map();
      this.byCredential.set(credentialId, attestations);
    }
    if (attestations.has(attestor)) {
      return { error: "attestor has already recorded a verdict for this credential", status: 409 };
    }
    const attestation: Attestation = { credentialId, attestor, verdict, note, recordedAt: now() };
    attestations.set(attestor, attestation);
    return attestation;
  }

  attestationsFor(credentialId: string): Attestation[] {
    return Array.from(this.byCredential.get(credentialId)?.values() ?? []);
  }

  /**
   * Opposing-verdict pairs on a credential. When `attestors` is given, only pairs
   * drawn from that set are returned.
   */
  detectConflicts(credentialId: string, attestors?: string[]): AttestationConflict[] {
    const scope = new Set(attestors ?? []);
    const relevant = this.attestationsFor(credentialId).filter(
      (a) => !attestors || scope.has(a.attestor)
    );
    const conflicts: AttestationConflict[] = [];
    for (let i = 0; i < relevant.length; i++) {
      for (let j = i + 1; j < relevant.length; j++) {
        const a = relevant[i]!;
        const b = relevant[j]!;
        if (a.verdict !== b.verdict) {
          conflicts.push({
            credentialId,
            attestorA: a.attestor,
            verdictA: a.verdict,
            attestorB: b.attestor,
            verdictB: b.verdict,
          });
        }
      }
    }
    return conflicts;
  }
}

export class DisputeService {
  private disputes = new Map<string, Dispute>();

  constructor(private readonly ledger: AttestationLedger) {}

  open(input: {
    credentialId: string;
    raisedBy: string;
    attestors: string[];
    reason: string;
  }): Dispute | DisputeError {
    const reason = typeof input.reason === "string" ? input.reason.trim() : "";
    if (reason.length < MIN_REASON_LENGTH || reason.length > MAX_REASON_LENGTH) {
      return {
        error: `reason must be between ${MIN_REASON_LENGTH} and ${MAX_REASON_LENGTH} characters`,
        status: 400,
      };
    }
    const attestors = Array.from(new Set(input.attestors));
    if (attestors.length < 2) {
      return { error: "attestors must name at least two distinct attestors", status: 400 };
    }

    const conflicts = this.ledger.detectConflicts(input.credentialId, attestors);
    if (conflicts.length === 0) {
      return { error: "no attestor conflict detected for these attestors", status: 409 };
    }
    const alreadyOpen = this.list({ credentialId: input.credentialId, status: "open" });
    if (alreadyOpen.length > 0) {
      return { error: "an open dispute already exists for this credential", status: 409 };
    }

    const openedAt = now();
    const dispute: Dispute = {
      id: randomUUID(),
      credentialId: input.credentialId,
      raisedBy: input.raisedBy,
      attestors,
      reason,
      conflicts,
      status: "open",
      createdAt: openedAt,
      history: [{ type: "opened", actor: input.raisedBy, at: openedAt, note: reason }],
    };
    this.disputes.set(dispute.id, dispute);
    return dispute;
  }

  resolve(
    disputeId: string,
    input: { resolver: string; outcome: DisputeOutcome; note?: string }
  ): Dispute | DisputeError {
    const dispute = this.disputes.get(disputeId);
    if (!dispute) return { error: "dispute not found", status: 404 };
    if (dispute.status === "resolved") return { error: "dispute is already resolved", status: 409 };
    if (!DISPUTE_OUTCOMES.includes(input.outcome)) {
      return { error: "outcome must be upheld or dismissed", status: 400 };
    }
    if (input.note !== undefined && (typeof input.note !== "string" || input.note.length > MAX_NOTE_LENGTH)) {
      return { error: `note must be a string of at most ${MAX_NOTE_LENGTH} characters`, status: 400 };
    }
    const parties = new Set([dispute.raisedBy, ...dispute.attestors]);
    if (parties.has(input.resolver)) {
      return { error: "a party to the dispute cannot resolve it", status: 403 };
    }

    const resolvedAt = now();
    dispute.status = "resolved";
    dispute.outcome = input.outcome;
    dispute.resolvedBy = input.resolver;
    dispute.resolvedAt = resolvedAt;
    dispute.history.push({ type: "resolved", actor: input.resolver, at: resolvedAt, note: input.note });
    return dispute;
  }

  get(disputeId: string): Dispute | undefined {
    return this.disputes.get(disputeId);
  }

  list(filter: { credentialId?: string; status?: Dispute["status"] } = {}): Dispute[] {
    return Array.from(this.disputes.values()).filter(
      (d) =>
        (filter.credentialId === undefined || d.credentialId === filter.credentialId) &&
        (filter.status === undefined || d.status === filter.status)
    );
  }
}

export const attestationLedger = new AttestationLedger();
export const disputeService = new DisputeService(attestationLedger);

export function isDisputeError(value: unknown): value is DisputeError {
  return typeof value === "object" && value !== null && "error" in value && "status" in value;
}
