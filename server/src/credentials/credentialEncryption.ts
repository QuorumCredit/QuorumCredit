/**
 * Credential Encryption at Rest
 *
 * #1760: Sensitive credential data may not be encrypted at rest.
 *
 * Provides:
 * - AES-256-GCM field-level encryption / decryption for credential metadata.
 * - Key management: named keys with version tracking.
 * - Key rotation: re-encrypt existing ciphertext under a new key.
 * - Encryption status reporting.
 *
 * In production, key material should be sourced from a KMS (e.g. AWS KMS,
 * HashiCorp Vault). This module models the interface; swap `KeyStore` for a
 * KMS-backed implementation without changing callers.
 */

import {
  createCipheriv,
  createDecipheriv,
  randomBytes,
  scryptSync,
} from "node:crypto";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const ALGORITHM = "aes-256-gcm" as const;
const KEY_BYTES = 32; // 256 bits
const IV_BYTES = 12;  // 96 bits — recommended for GCM
const AUTH_TAG_BYTES = 16;
const SALT_BYTES = 16;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface EncryptionKey {
  keyId: string;
  /** Version increments on every rotation. */
  version: number;
  createdAt: number;
  rotatedAt?: number;
  /** True when this key is the current default for new encryptions. */
  active: boolean;
}

/** Wire format stored alongside ciphertext so we know which key to use. */
export interface EncryptedField {
  /** Identifying which key was used. */
  keyId: string;
  /** Key version at time of encryption. */
  keyVersion: number;
  /** Hex-encoded IV. */
  iv: string;
  /** Hex-encoded GCM authentication tag. */
  authTag: string;
  /** Hex-encoded salt used for key derivation (when key is passphrase-derived). */
  salt: string;
  /** Hex-encoded ciphertext. */
  ciphertext: string;
}

export interface EncryptionStatus {
  totalFields: number;
  encryptedFields: number;
  unencryptedFields: number;
  activeKeyId?: string;
}

// ---------------------------------------------------------------------------
// CredentialEncryptionService
// ---------------------------------------------------------------------------

/**
 * AES-256-GCM field-level encryption service.
 *
 * Keys are derived from a passphrase using scrypt so that raw key material
 * does not need to be stored. In a production KMS integration, replace
 * `_deriveKey` with a call to the KMS `generateDataKey` API and store the
 * encrypted DEK alongside the `EncryptedField`.
 */
export class CredentialEncryptionService {
  /** keyId → { passphrase, metadata } */
  private readonly keys = new Map<string, { passphrase: string; meta: EncryptionKey }>();
  private activeKeyId: string | undefined;

  // -------------------------------------------------------------------------
  // Key management
  // -------------------------------------------------------------------------

  /**
   * Register a new encryption key.
   *
   * @param keyId - Unique key identifier (e.g. "key-2024-01").
   * @param passphrase - Secret passphrase from which the actual key is derived.
   *   In production, supply a KMS-generated random key instead.
   * @param setActive - Make this the active key for new encryptions.
   */
  registerKey(keyId: string, passphrase: string, setActive: boolean = true): EncryptionKey {
    if (!keyId.trim()) throw new Error("keyId must not be empty");
    if (!passphrase) throw new Error("passphrase must not be empty");

    const existing = this.keys.get(keyId);
    const meta: EncryptionKey = {
      keyId,
      version: existing ? existing.meta.version + 1 : 1,
      createdAt: existing?.meta.createdAt ?? Date.now(),
      rotatedAt: existing ? Date.now() : undefined,
      active: setActive,
    };

    // Deactivate previous active key when setting a new one
    if (setActive && this.activeKeyId && this.activeKeyId !== keyId) {
      const prev = this.keys.get(this.activeKeyId);
      if (prev) prev.meta.active = false;
    }

    this.keys.set(keyId, { passphrase, meta });
    if (setActive) this.activeKeyId = keyId;

    return { ...meta };
  }

  /**
   * List all registered keys (passphrases are never returned).
   */
  listKeys(): EncryptionKey[] {
    return Array.from(this.keys.values()).map(({ meta }) => ({ ...meta }));
  }

  /**
   * Get metadata for a specific key (passphrase is never returned).
   */
  getKeyMeta(keyId: string): EncryptionKey | undefined {
    const entry = this.keys.get(keyId);
    if (!entry) return undefined;
    return { ...entry.meta };
  }

  // -------------------------------------------------------------------------
  // Encrypt / Decrypt
  // -------------------------------------------------------------------------

  /**
   * Encrypt a plaintext string using the active key.
   *
   * @param plaintext - The sensitive field value to encrypt.
   * @param keyId - Override the active key (optional).
   */
  encrypt(plaintext: string, keyId?: string): EncryptedField {
    const resolvedKeyId = keyId ?? this.activeKeyId;
    if (!resolvedKeyId) throw new Error("no active encryption key registered");

    const entry = this.keys.get(resolvedKeyId);
    if (!entry) throw new Error(`encryption key not found: ${resolvedKeyId}`);

    const salt = randomBytes(SALT_BYTES);
    const iv = randomBytes(IV_BYTES);
    const key = this._deriveKey(entry.passphrase, salt);

    const cipher = createCipheriv(ALGORITHM, key, iv);
    const encrypted = Buffer.concat([
      cipher.update(plaintext, "utf8"),
      cipher.final(),
    ]);
    const authTag = cipher.getAuthTag();

    return {
      keyId: resolvedKeyId,
      keyVersion: entry.meta.version,
      iv: iv.toString("hex"),
      authTag: authTag.toString("hex"),
      salt: salt.toString("hex"),
      ciphertext: encrypted.toString("hex"),
    };
  }

  /**
   * Decrypt an `EncryptedField` back to plaintext.
   */
  decrypt(field: EncryptedField): string {
    const entry = this.keys.get(field.keyId);
    if (!entry) throw new Error(`decryption key not found: ${field.keyId}`);

    const salt = Buffer.from(field.salt, "hex");
    const iv = Buffer.from(field.iv, "hex");
    const authTag = Buffer.from(field.authTag, "hex");
    const ciphertext = Buffer.from(field.ciphertext, "hex");

    const key = this._deriveKey(entry.passphrase, salt);
    const decipher = createDecipheriv(ALGORITHM, key, iv);
    decipher.setAuthTag(authTag);

    return Buffer.concat([decipher.update(ciphertext), decipher.final()]).toString("utf8");
  }

  // -------------------------------------------------------------------------
  // Key rotation
  // -------------------------------------------------------------------------

  /**
   * Re-encrypt a ciphertext field under a new key.
   *
   * Decrypts with the key referenced in `field`, then re-encrypts with
   * `newKeyId` (defaults to the current active key).
   */
  rotate(field: EncryptedField, newKeyId?: string): EncryptedField {
    const plaintext = this.decrypt(field);
    return this.encrypt(plaintext, newKeyId ?? this.activeKeyId);
  }

  /**
   * Bulk-rotate an array of encrypted fields to the current active key.
   * Fields already encrypted under the active key version are skipped.
   *
   * @returns Number of fields actually re-encrypted.
   */
  rotateAll(fields: EncryptedField[]): { rotated: number; skipped: number } {
    if (!this.activeKeyId) throw new Error("no active encryption key registered");

    const activeEntry = this.keys.get(this.activeKeyId)!;
    let rotated = 0;
    let skipped = 0;

    for (let i = 0; i < fields.length; i++) {
      const f = fields[i];
      if (f.keyId === this.activeKeyId && f.keyVersion === activeEntry.meta.version) {
        skipped += 1;
        continue;
      }
      fields[i] = this.rotate(f);
      rotated += 1;
    }

    return { rotated, skipped };
  }

  // -------------------------------------------------------------------------
  // Status
  // -------------------------------------------------------------------------

  /**
   * Report encryption coverage for a set of fields.
   *
   * A field is considered "encrypted" when it is a valid `EncryptedField`
   * object with a non-empty `ciphertext` property.
   */
  getEncryptionStatus(fields: Array<EncryptedField | null | undefined | string>): EncryptionStatus {
    let encryptedFields = 0;
    let unencryptedFields = 0;

    for (const f of fields) {
      if (f && typeof f === "object" && "ciphertext" in f && f.ciphertext) {
        encryptedFields += 1;
      } else {
        unencryptedFields += 1;
      }
    }

    return {
      totalFields: fields.length,
      encryptedFields,
      unencryptedFields,
      activeKeyId: this.activeKeyId,
    };
  }

  // -------------------------------------------------------------------------
  // Internal helpers
  // -------------------------------------------------------------------------

  /**
   * Derive a 256-bit key from a passphrase + salt using scrypt.
   *
   * Parameters follow OWASP recommendations for scrypt (N=16384, r=8, p=1).
   */
  private _deriveKey(passphrase: string, salt: Buffer): Buffer {
    return scryptSync(passphrase, salt, KEY_BYTES, { N: 16384, r: 8, p: 1 }) as Buffer;
  }
}

export const credentialEncryptionService = new CredentialEncryptionService();
