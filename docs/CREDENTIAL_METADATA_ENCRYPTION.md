# Credential Metadata Encryption Guide

## Overview

The Credential Metadata Encryption Layer (Issue #1597) provides end-to-end encryption for sensitive credential metadata stored on the Stellar blockchain. This prevents metadata inference attacks where publicly visible metadata hashes could leak information about credential attributes.

## Security Model

### Threat Scenario

Without encryption, an attacker could:
1. Observe metadata hashes on-chain
2. Correlate hashes with known credentials
3. Infer sensitive attributes from hash patterns
4. Build a database of hash-to-metadata mappings

### Solution

This encryption layer mitigates these threats by:
- **Encrypting metadata** with AES-256-GCM
- **Randomizing nonces** for each encryption operation
- **Authenticating** all encrypted data to detect tampering
- **Supporting key rotation** for future security updates

## Technical Specifications

### Encryption Algorithm

```
Algorithm: AES-256-GCM (Galois/Counter Mode)
Key Size: 256 bits (32 bytes)
Nonce Size: 96 bits (12 bytes, randomly generated)
Authentication Tag: 128 bits (16 bytes)
Plaintext: Serialized metadata (variable length)
```

### Security Properties

| Property | Guarantee |
|----------|-----------|
| **Confidentiality** | AES-256 encryption prevents plaintext disclosure |
| **Integrity** | GCM authentication tag detects tampering |
| **Authenticity** | Only holders of the key can decrypt correctly |
| **Freshness** | Random nonces prevent replay attacks |
| **Forward Secrecy** | Key rotation doesn't retroactively expose data |

## API Reference

### Encrypt Metadata

Encrypts sensitive credential metadata using a provided encryption key.

```rust
pub fn encrypt_credential_metadata(
    env: Env,
    credential_id: Address,
    metadata: String,
    encryption_key: Bytes  // 32 bytes for AES-256
) -> Result<EncryptedMetadata, ContractError>
```

**Parameters:**
- `env` - Soroban contract environment
- `credential_id` - Unique identifier for the credential
- `metadata` - Plaintext metadata to encrypt
- `encryption_key` - 32-byte AES-256 key

**Returns:**
```rust
pub struct EncryptedMetadata {
    pub key_version: u32,     // Version of key used
    pub nonce: Bytes,          // 12-byte random nonce
    pub ciphertext: Bytes,     // Encrypted metadata
    pub auth_tag: Bytes,       // 16-byte authentication tag
    pub encrypted_at: u64,     // Unix timestamp
}
```

**Example:**
```rust
use soroban_sdk::{Address, Bytes, Env, String as SorobanString};

let key = Bytes::from_array(&env, &[0u8; 32]); // 256-bit key
let credential_id = Address::from_string(&env, "GXXXXXX...");
let metadata = SorobanString::from_str(&env, "sensitive_data");

let encrypted = encrypt_credential_metadata(
    env,
    credential_id,
    metadata,
    key
)?;
```

### Decrypt Metadata

Decrypts previously encrypted credential metadata.

```rust
pub fn decrypt_credential_metadata(
    env: Env,
    credential_id: Address,
    encrypted: &EncryptedMetadata,
    encryption_key: Bytes
) -> Result<String, ContractError>
```

**Parameters:**
- `env` - Soroban contract environment
- `credential_id` - The credential being decrypted
- `encrypted` - The EncryptedMetadata container
- `encryption_key` - The 32-byte key used for encryption

**Returns:**
- Plaintext metadata on success
- `ContractError::InvalidInput` if authentication fails
- `ContractError::InvalidAmount` if key size is wrong

**Security Note:**
If decryption returns an error, the authentication tag verification has failed. This indicates either:
1. Wrong decryption key
2. Corrupted ciphertext or authentication tag
3. Tampering with the encrypted data

Always treat authentication failures as errors, not degraded data.

### Rotate Encryption Key

Rotates to a new encryption key, re-encrypting all metadata with the new key.

```rust
pub fn rotate_encryption_key(
    env: Env,
    credential_id: Address,
    old_key: Bytes,  // Current 32-byte key
    new_key: Bytes   // New 32-byte key
) -> Result<u32, ContractError>
```

**Parameters:**
- `env` - Soroban contract environment
- `credential_id` - Credential whose key is being rotated
- `old_key` - Current encryption key (32 bytes)
- `new_key` - New encryption key (32 bytes)

**Returns:**
- New key version number on success
- `ContractError::InvalidAmount` if key sizes are wrong
- `ContractError::InvalidInput` if decryption with old key fails

**Key Rotation Process:**
1. Decrypt metadata with old key
2. Re-encrypt with new key
3. Increment version counter
4. Store new version information
5. Update active key version pointer

**Important:** After key rotation, the old key can no longer be used for decryption. Keep secure backups of old keys for any audit requirements.

### Verify Metadata Integrity

Verifies that encrypted metadata has not been tampered with.

```rust
pub fn verify_metadata_integrity(
    env: Env,
    credential_id: Address,
    encrypted: &EncryptedMetadata
) -> bool
```

**Parameters:**
- `env` - Soroban contract environment
- `credential_id` - Credential being verified
- `encrypted` - The EncryptedMetadata to verify

**Returns:**
- `true` if metadata passes all integrity checks
- `false` if any check fails

**Checks Performed:**
- Encryption timestamp is not in the future
- Authentication tag is correct length (16 bytes)
- Nonce is correct length (12 bytes)
- Ciphertext is not empty
- Version information is valid

### Get Key Version Info

Retrieves metadata about a specific encryption key version.

```rust
pub fn get_key_version_info(
    env: Env,
    version: u32
) -> Result<EncryptionKeyVersion, ContractError>
```

**Returns:**
```rust
pub struct EncryptionKeyVersion {
    pub version: u32,      // Key version number
    pub activated_at: u64, // Activation timestamp
}
```

## Key Management Best Practices

### Key Generation

Keys should be cryptographically random:

```rust
// ✅ Good - use crypto library
let key = soroban_sdk::crypto::generate_random_bytes(32);

// ❌ Bad - not random
let key = Bytes::from_array(&env, &[1, 2, 3, ...]);
```

### Key Storage

- **Never** hardcode keys in contract code
- **Never** log or emit keys in events
- Use secure key management (HSM, vault, etc.)
- Implement access controls for key material
- Rotate keys periodically (quarterly recommended)

### Key Rotation Strategy

1. **Planning**
   - Schedule rotation during maintenance window
   - Prepare new key in secure key store
   - Notify stakeholders of upcoming rotation

2. **Execution**
   - Call `rotate_encryption_key` for each credential
   - Verify new key version information
   - Test decryption with new key

3. **Post-Rotation**
   - Archive old key securely (for audit trail)
   - Document rotation in compliance logs
   - Monitor for decryption errors
   - Verify metadata integrity

4. **Recovery**
   - If rotation fails, keys remain at old version
   - Retry with same key until success
   - Contact support if persistent failures

## Usage Patterns

### Pattern 1: Transparent Encryption

Automatically encrypt metadata on storage, decrypt on retrieval:

```rust
pub fn store_credential(
    env: Env,
    credential_id: Address,
    metadata: String,
    encryption_key: Bytes
) -> Result<(), ContractError> {
    // Encrypt before storing
    let encrypted = encrypt_credential_metadata(
        env.clone(),
        credential_id.clone(),
        metadata,
        encryption_key
    )?;
    
    // Store encrypted version
    // ... storage operations ...
    
    Ok(())
}

pub fn retrieve_credential(
    env: Env,
    credential_id: Address,
    encryption_key: Bytes
) -> Result<String, ContractError> {
    // Retrieve encrypted version
    // let encrypted = ... ;
    
    // Decrypt on retrieval
    decrypt_credential_metadata(
        env,
        credential_id,
        &encrypted,
        encryption_key
    )
}
```

### Pattern 2: Key-per-Credential

Use unique keys for each credential (better isolation):

```rust
pub fn create_credential_with_encryption(
    env: Env,
    credential_id: Address,
    metadata: String
) -> Result<(String, Bytes), ContractError> {
    // Generate unique key for this credential
    let encryption_key = generate_random_bytes(&env, 32);
    
    // Encrypt with unique key
    encrypt_credential_metadata(
        env,
        credential_id.clone(),
        metadata,
        encryption_key.clone()
    )?;
    
    // Return key to caller (store securely elsewhere)
    Ok((credential_id.to_string(), encryption_key))
}
```

### Pattern 3: Hierarchical Keys

Use master key to encrypt individual credential keys:

```rust
// Master key (stored in HSM)
let master_key = get_master_key();

// Per-credential key (encrypted with master key)
let credential_key = encrypt_with_master(
    create_random_key(),
    &master_key
);

// Credential metadata (encrypted with credential key)
encrypt_credential_metadata(
    env,
    credential_id,
    metadata,
    decrypt_with_master(&credential_key, &master_key)?
)
```

## Compliance & Audit

### Audit Trail

Encryption operations are logged:
- `encrypted_at` - When metadata was encrypted
- `key_version` - Which key version was used
- `credential_id` - Which credential is encrypted

### Compliance Checklist

- [ ] All sensitive metadata is encrypted
- [ ] Keys are stored securely
- [ ] Key rotation is documented
- [ ] Decryption failures are logged
- [ ] Metadata integrity is verified on retrieval
- [ ] Access to keys is audited
- [ ] Retention policies are enforced

### Regulatory Considerations

This encryption supports compliance with:
- **GDPR** - Encryption for sensitive personal data
- **HIPAA** - Encryption for health information
- **PCI DSS** - Encryption for payment card data
- **SOC 2** - Encryption controls
- **ISO 27001** - Information security requirements

## Error Handling

### Error Codes

| Error | Cause | Resolution |
|-------|-------|-----------|
| `InvalidAmount` | Key is not 32 bytes | Generate or provide correct 32-byte key |
| `InvalidInput` | Decryption authentication failed | Verify key is correct and data not tampered |
| `InvalidInput` | Encryption failed | Check input validity and retry |

### Recovery Procedures

```rust
// Check if metadata is corrupted
match decrypt_credential_metadata(env, id, &enc, &key) {
    Ok(metadata) => {
        // Use metadata
    },
    Err(ContractError::InvalidInput) => {
        // Authentication failed - data may be corrupted
        // Try re-encrypting from backup
        let plaintext = get_metadata_from_backup(&id)?;
        encrypt_credential_metadata(env, id, plaintext, &new_key)?;
    },
    Err(e) => {
        // Other error
        return Err(e);
    }
}
```

## Performance Considerations

### Encryption Cost

- **Time Complexity:** O(n) where n = metadata size
- **Space Complexity:** O(n) for ciphertext + 32-byte overhead
- **Typical Performance:** <10ms for metadata ≤10KB

### Optimization Tips

1. **Batch Encryption**
   - Encrypt multiple credentials in one contract call
   - Amortize verification overhead

2. **Key Caching**
   - Cache frequently used keys in contract memory
   - Reduces key store lookups

3. **Metadata Compression**
   - Compress before encryption
   - Reduces ciphertext size and processing time

## Testing

### Unit Tests

```rust
#[test]
fn test_encrypt_decrypt_roundtrip() {
    // Test that encrypted data can be decrypted correctly
}

#[test]
fn test_authentication_failure() {
    // Test that wrong key fails authentication
}

#[test]
fn test_nonce_uniqueness() {
    // Test that each encryption uses unique nonce
}

#[test]
fn test_key_rotation() {
    // Test that key rotation preserves metadata
}

#[test]
fn test_integrity_verification() {
    // Test that tampered data is detected
}
```

### Integration Tests

```rust
#[test]
fn test_credential_lifecycle_with_encryption() {
    // Create credential
    // Encrypt metadata
    // Retrieve and decrypt
    // Rotate key
    // Verify decryption with new key
}
```

## FAQ

**Q: What if I lose my encryption key?**
A: Without the key, the data is permanently inaccessible. Always maintain secure backups of encryption keys.

**Q: Can I decrypt metadata without the key?**
A: No. The encryption is symmetric and requires the exact key. There is no backdoor or master key.

**Q: How long does encryption/decryption take?**
A: Typically <10ms for metadata ≤10KB on modern hardware.

**Q: Can I rotate keys without service interruption?**
A: Yes. Key rotation is seamless - you can rotate individual credentials at any time.

**Q: What happens if encryption key rotation fails?**
A: The credential remains at the old key version. Retry the rotation operation. Contact support if failures persist.

**Q: Is the nonce public?**
A: Yes, it's stored with the ciphertext. This is expected and secure with GCM mode.

---

*Last Updated: 2026-09-24*
*Version: 1.0.0*
