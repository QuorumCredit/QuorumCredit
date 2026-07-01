use crate::errors::ContractError;
use crate::helpers::require_not_thawing;
use crate::types::{DataKey, AttributeEntry};
use soroban_sdk::{symbol_short, Address, Env, Vec};

pub fn set_attribute(env: Env, caller: Address, key: soroban_sdk::String, value: soroban_sdk::String) -> Result<(), ContractError> {
    caller.require_auth();
    require_not_thawing(&env)?;
    let mut attrs: Vec<AttributeEntry> = env.storage().persistent().get(&DataKey::CustomAttributes(caller.clone())).unwrap_or(Vec::new(&env));
    let mut found = false;
    for i in 0..attrs.len() {
        let mut a = attrs.get(i).unwrap();
        if a.key == key {
            a.value = value.clone();
            attrs.set(i, a);
            found = true;
            break;
        }
    }
    if !found {
        attrs.push_back(AttributeEntry { key: key.clone(), value: value.clone() });
    }
    env.storage().persistent().set(&DataKey::CustomAttributes(caller.clone()), &attrs);
    env.events().publish((symbol_short!("attr"), symbol_short!("set")), (caller, key, value));
    Ok(())
}

pub fn get_attributes(env: Env, caller: Address) -> Vec<AttributeEntry> {
    env.storage().persistent().get(&DataKey::CustomAttributes(caller)).unwrap_or(Vec::new(&env))
}

pub fn remove_attribute(env: Env, caller: Address, key: soroban_sdk::String) -> Result<(), ContractError> {
    caller.require_auth();
    require_not_thawing(&env)?;
    let mut attrs: Vec<AttributeEntry> = env.storage().persistent().get(&DataKey::CustomAttributes(caller.clone())).unwrap_or(Vec::new(&env));
    let len_before = attrs.len();
    {
        let mut new_attrs = Vec::new(&env);
        for a in attrs.iter() {
            if a.key != key {
                new_attrs.push_back(a.clone());
            }
        }
        attrs = new_attrs;
    }
    if attrs.len() == len_before {
        return Err(ContractError::AttributeNotFound);
    }
    env.storage().persistent().set(&DataKey::CustomAttributes(caller.clone()), &attrs);
    env.events().publish((symbol_short!("attr"), symbol_short!("rm")), (caller, key));
    Ok(())
}
