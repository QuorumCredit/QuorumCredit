import { describe, it, expect } from "vitest";
import { CredentialEncryptionService } from "../src/credentials/credentialEncryption.js";
import { generateProof, validateProof } from "../src/credentials/proofGenerator.js";

describe("credential encryption", () => {
  it("round-trips plaintext through encrypt and decrypt", () => {
    const svc = new CredentialEncryptionService();
    svc.registerKey("key-roundtrip", "a-long-test-passphrase");
    const field = svc.encrypt("holder-national-id-123");
    expect(svc.decrypt(field)).toBe("holder-national-id-123");
  });

  it("rejects ciphertext that was changed after encryption", () => {
    const svc = new CredentialEncryptionService();
    svc.registerKey("key-tamper", "a-long-test-passphrase");
    const field = svc.encrypt("holder-national-id-123");
    const lastByte = field.ciphertext.slice(-2);
    const tampered = {
      ...field,
      ciphertext: field.ciphertext.slice(0, -2) + (lastByte === "00" ? "ff" : "00"),
    };
    expect(() => svc.decrypt(tampered)).toThrow();
  });
});

describe("credential proofs", () => {
  const secret = "proof-test-secret-with-enough-length";
  const expiresAt = Math.floor(Date.now() / 1000) + 3600;

  it("validates an untouched proof", () => {
    const proof = generateProof(secret, "cred-1", "holder-1", "license", "issuer-x", expiresAt, {});
    expect(validateProof(secret, proof).valid).toBe(true);
  });

  it("fails validation when a proof field is changed", () => {
    const proof = generateProof(secret, "cred-1", "holder-1", "license", "issuer-x", expiresAt, {});
    const changed = { ...proof, holderId: "holder-2" };
    expect(validateProof(secret, changed).valid).toBe(false);
  });
});
