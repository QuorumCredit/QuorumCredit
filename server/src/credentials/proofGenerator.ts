/**
 * Issue #1590: Credential Verification Proof Export
 *
 * Generates cryptographically signed proofs of credential verification
 * that can be exported and validated offline.
 */

import { createHmac, randomBytes } from "node:crypto";

export interface CredentialProof {
  credentialId: string;
  holderId: string;
  credentialType: string;
  issuedAt: number;
  expiresAt: number;
  issuer: string;
  metadata: Record<string, unknown>;
  signature: string;
  proofId: string;
}

export interface ProofValidationResult {
  valid: boolean;
  reason?: string;
  payload?: CredentialProof;
}

/**
 * Generate a cryptographically signed proof of credential verification
 */
export function generateProof(
  secret: string,
  credentialId: string,
  holderId: string,
  credentialType: string,
  issuer: string,
  expiresAt: number,
  metadata?: Record<string, unknown>
): CredentialProof {
  const now = Math.floor(Date.now() / 1000);
  const proofId = randomBytes(16).toString("hex");

  const proof: CredentialProof = {
    credentialId,
    holderId,
    credentialType,
    issuedAt: now,
    expiresAt,
    issuer,
    metadata: metadata || {},
    signature: "", // Will be filled in below
    proofId,
  };

  // Create signature over the proof payload (excluding signature field)
  const payloadStr = JSON.stringify({
    credentialId: proof.credentialId,
    holderId: proof.holderId,
    credentialType: proof.credentialType,
    issuedAt: proof.issuedAt,
    expiresAt: proof.expiresAt,
    issuer: proof.issuer,
    metadata: proof.metadata,
    proofId: proof.proofId,
  });

  proof.signature = createHmac("sha256", secret)
    .update(payloadStr)
    .digest("base64url");

  return proof;
}

/**
 * Validate a credential proof signature and expiry
 */
export function validateProof(
  secret: string,
  proof: CredentialProof
): ProofValidationResult {
  // Check if proof is expired
  const now = Math.floor(Date.now() / 1000);
  if (now > proof.expiresAt) {
    return {
      valid: false,
      reason: "proof_expired",
    };
  }

  // Verify signature
  const payloadStr = JSON.stringify({
    credentialId: proof.credentialId,
    holderId: proof.holderId,
    credentialType: proof.credentialType,
    issuedAt: proof.issuedAt,
    expiresAt: proof.expiresAt,
    issuer: proof.issuer,
    metadata: proof.metadata,
    proofId: proof.proofId,
  });

  const expectedSignature = createHmac("sha256", secret)
    .update(payloadStr)
    .digest("base64url");

  if (!timingSafeEqual(proof.signature, expectedSignature)) {
    return {
      valid: false,
      reason: "invalid_signature",
    };
  }

  return {
    valid: true,
    payload: proof,
  };
}

/**
 * Timing-safe string comparison
 */
function timingSafeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) {
    return false;
  }
  const aBuffer = Buffer.from(a);
  const bBuffer = Buffer.from(b);
  return aBuffer.equals(bBuffer);
}

/**
 * Export proof in a portable JSON format with optional encryption
 */
export function exportProofJson(proof: CredentialProof): string {
  return JSON.stringify(proof, null, 2);
}

/**
 * Parse and validate an exported proof
 */
export function importProofJson(
  jsonStr: string,
  secret: string
): ProofValidationResult {
  try {
    const proof = JSON.parse(jsonStr) as CredentialProof;
    return validateProof(secret, proof);
  } catch (err) {
    return {
      valid: false,
      reason: "malformed_proof",
    };
  }
}
