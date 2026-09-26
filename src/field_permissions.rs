//! Issue #1631: Credential Field-Level Permissions.
//!
//! Implements fine-grained access control for credential fields, allowing different
//! permissions for different fields within a credential. Supports permission inheritance
//! from parent credentials.

extern crate alloc;

use soroban_sdk::{Address, Env, Vec, Bytes};
use crate::errors::ContractError;
use crate::types::DataKey;

/// Permission types for field-level access control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldPermission {
    /// Field is public; anyone can read.
    Public = 0,
    /// Field is private; only owner and authorized parties can read.
    Private = 1,
    /// Field is read-only for non-owners.
    ReadOnly = 2,
    /// Field is hidden from public view.
    Hidden = 3,
}

/// Get FieldPermission from u32 value.
pub fn permission_from_u32(value: u32) -> Option<FieldPermission> {
    match value {
        0 => Some(FieldPermission::Public),
        1 => Some(FieldPermission::Private),
        2 => Some(FieldPermission::ReadOnly),
        3 => Some(FieldPermission::Hidden),
        _ => None,
    }
}

/// Represents field-level permissions for a credential.
#[derive(Clone)]
pub struct FieldPermissionMatrix {
    /// Credential ID this matrix applies to.
    pub credential_id: Address,
    /// Timestamp when this matrix was created.
    pub created_at: u64,
    /// Default permission for fields not explicitly configured.
    pub default_permission: u32, // 0-3
    /// Number of fields with custom permissions.
    pub custom_field_count: u32,
    /// Parent credential ID for permission inheritance.
    pub parent_credential_id: Option<Address>,
}

/// Set permissions for a specific field in a credential.
pub fn set_field_permissions(
    env: &Env,
    credential_id: &Address,
    field: &Bytes,
    permission: u32,
) -> Result<(), ContractError> {
    // Validate permission value (0-3)
    if permission > 3 {
        return Err(ContractError::InvalidPermission);
    }

    // Store field permission
    let key = DataKey::FieldPermission(credential_id.clone(), field.clone());
    env.storage().persistent().set(&key, &permission);

    // Update the matrix's custom field count
    let matrix_key = DataKey::FieldPermissionMatrix(credential_id.clone());
    let mut matrix: FieldPermissionMatrix = env
        .storage()
        .persistent()
        .get(&matrix_key)
        .unwrap_or(FieldPermissionMatrix {
            credential_id: credential_id.clone(),
            created_at: env.ledger().timestamp(),
            default_permission: 0, // Default to Public
            custom_field_count: 0,
            parent_credential_id: None,
        });

    // Increment custom field count
    matrix.custom_field_count = matrix.custom_field_count.saturating_add(1);
    env.storage().persistent().set(&matrix_key, &matrix);

    // Log the permission change
    let history_key = DataKey::FieldPermissionHistory(credential_id.clone(), field.clone());
    let mut history: Vec<(u64, u32, u32)> = env
        .storage()
        .persistent()
        .get(&history_key)
        .unwrap_or(Vec::new(env));

    // Keep only the last 100 changes
    if history.len() >= 100 {
        history.remove(0);
    }

    let old_permission = get_field_permission(env, credential_id, field)?;
    history.push_back((env.ledger().timestamp(), old_permission, permission));
    env.storage().persistent().set(&history_key, &history);

    Ok(())
}

/// Get the permission for a specific field in a credential.
pub fn get_field_permission(
    env: &Env,
    credential_id: &Address,
    field: &Bytes,
) -> Result<u32, ContractError> {
    let key = DataKey::FieldPermission(credential_id.clone(), field.clone());
    
    let permission: Option<u32> = env.storage().persistent().get(&key);
    
    if let Some(perm) = permission {
        return Ok(perm);
    }

    // If not explicitly set, check parent credential for inheritance
    let matrix_key = DataKey::FieldPermissionMatrix(credential_id.clone());
    if let Some(matrix) = env.storage().persistent().get::<_, FieldPermissionMatrix>(&matrix_key) {
        if let Some(parent_id) = matrix.parent_credential_id {
            // Recursively check parent
            return get_field_permission(env, &parent_id, field);
        }

        // Return default permission from matrix
        return Ok(matrix.default_permission);
    }

    // Default to Public (0) if no matrix exists
    Ok(0)
}

/// Check if an address has permission to read a field.
pub fn check_field_permission(
    env: &Env,
    credential_id: &Address,
    field: &Bytes,
    caller: &Address,
    owner: &Address,
) -> Result<bool, ContractError> {
    let permission = get_field_permission(env, credential_id, field)?;

    match permission_from_u32(permission) {
        Some(FieldPermission::Public) => Ok(true),
        Some(FieldPermission::Private) => {
            // Only owner and explicitly authorized parties can read
            Ok(caller == owner || is_authorized_reader(env, credential_id, field, caller)?)
        }
        Some(FieldPermission::ReadOnly) => {
            // Owner can read/write, others can only read
            Ok(caller == owner || !is_write_operation(env, caller)?)
        }
        Some(FieldPermission::Hidden) => {
            // Only owner can access
            Ok(caller == owner)
        }
        None => Err(ContractError::InvalidPermission),
    }
}

/// Set the parent credential for permission inheritance.
pub fn set_field_permission_inheritance(
    env: &Env,
    credential_id: &Address,
    parent_credential_id: &Address,
) -> Result<(), ContractError> {
    let key = DataKey::FieldPermissionMatrix(credential_id.clone());
    let mut matrix: FieldPermissionMatrix = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(FieldPermissionMatrix {
            credential_id: credential_id.clone(),
            created_at: env.ledger().timestamp(),
            default_permission: 0,
            custom_field_count: 0,
            parent_credential_id: None,
        });

    // Prevent circular inheritance
    if check_circular_inheritance(env, parent_credential_id, credential_id)? {
        return Err(ContractError::InvalidPermission);
    }

    matrix.parent_credential_id = Some(parent_credential_id.clone());
    env.storage().persistent().set(&key, &matrix);

    Ok(())
}

/// Check if setting parent would create a circular dependency.
fn check_circular_inheritance(
    env: &Env,
    parent_id: &Address,
    child_id: &Address,
) -> Result<bool, ContractError> {
    let mut current = parent_id.clone();
    let mut depth = 0;
    const MAX_INHERITANCE_DEPTH: u32 = 10;

    loop {
        if &current == child_id {
            return Ok(true); // Circular dependency detected
        }

        if depth >= MAX_INHERITANCE_DEPTH {
            return Ok(false); // Reached max depth, no circular dependency
        }

        let matrix_key = DataKey::FieldPermissionMatrix(current.clone());
        if let Some(matrix) = env.storage().persistent().get::<_, FieldPermissionMatrix>(&matrix_key) {
            if let Some(parent) = matrix.parent_credential_id {
                current = parent;
                depth += 1;
            } else {
                return Ok(false); // Reached end of chain
            }
        } else {
            return Ok(false); // No matrix found
        }
    }
}

/// Check if an address is authorized to read a private field.
fn is_authorized_reader(
    env: &Env,
    _credential_id: &Address,
    _field: &Bytes,
    _reader: &Address,
) -> Result<bool, ContractError> {
    // This is a placeholder; in a real implementation, this would check
    // an explicit authorization list stored in the contract.
    // For now, we return false to restrict private field access to owner only.
    Ok(false)
}

/// Determine if the current operation is a write operation.
fn is_write_operation(
    _env: &Env,
    _caller: &Address,
) -> Result<bool, ContractError> {
    // This is a placeholder. In a real implementation, this would track
    // the current operation type from the contract call stack.
    // For now, assume read-only.
    Ok(false)
}

/// Get all field permissions for a credential.
pub fn get_field_permission_matrix(
    env: &Env,
    credential_id: &Address,
) -> Option<FieldPermissionMatrix> {
    let key = DataKey::FieldPermissionMatrix(credential_id.clone());
    env.storage().persistent().get(&key)
}

/// Set default permission for all fields in a credential.
pub fn set_default_field_permission(
    env: &Env,
    credential_id: &Address,
    permission: u32,
) -> Result<(), ContractError> {
    if permission > 3 {
        return Err(ContractError::InvalidPermission);
    }

    let key = DataKey::FieldPermissionMatrix(credential_id.clone());
    let mut matrix: FieldPermissionMatrix = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(FieldPermissionMatrix {
            credential_id: credential_id.clone(),
            created_at: env.ledger().timestamp(),
            default_permission: permission,
            custom_field_count: 0,
            parent_credential_id: None,
        });

    matrix.default_permission = permission;
    env.storage().persistent().set(&key, &matrix);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::token::StellarAssetClient;

    fn setup_contract(env: &Env) -> Address {
        env.mock_all_auths();
        let deployer = Address::generate(env);
        let admin = Address::generate(env);
        let admins = Vec::from_array(env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);
        StellarAssetClient::new(env, &token_id.address()).mint(&contract_id, &10_000_000);
        let client = QuorumCreditContractClient::new(env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        contract_id
    }

    #[test]
    fn test_set_and_get_field_permission() {
        let env = Env::default();
        let contract_id = setup_contract(&env);

        env.as_contract(&contract_id, || {
            let credential_id = Address::generate(&env);
            let field = Bytes::from_slice(&env, b"field_name");

            set_field_permissions(&env, &credential_id, &field, 1).unwrap();
            let perm = get_field_permission(&env, &credential_id, &field).unwrap();
            assert_eq!(perm, 1);
        });
    }

    #[test]
    fn test_invalid_permission_value() {
        let env = Env::default();
        let contract_id = setup_contract(&env);

        env.as_contract(&contract_id, || {
            let credential_id = Address::generate(&env);
            let field = Bytes::from_slice(&env, b"field_name");

            let result = set_field_permissions(&env, &credential_id, &field, 5);
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_permission_inheritance() {
        let env = Env::default();
        let contract_id = setup_contract(&env);

        env.as_contract(&contract_id, || {
            let parent_id = Address::generate(&env);
            let child_id = Address::generate(&env);
            let field = Bytes::from_slice(&env, b"field_name");

            // Set permission on parent
            set_field_permissions(&env, &parent_id, &field, 2).unwrap();

            // Set inheritance on child
            set_field_permission_inheritance(&env, &child_id, &parent_id).unwrap();

            // Child should inherit parent's permission
            let perm = get_field_permission(&env, &child_id, &field).unwrap();
            assert_eq!(perm, 2);
        });
    }
}
