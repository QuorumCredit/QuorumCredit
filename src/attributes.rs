#![allow(unused_variables)]
use crate::errors::ContractError;
use crate::types::{DataKey};
use soroban_sdk::{Address, Env, String, Vec};

#[crate::types::contracttype_alias]
pub struct AttributeEntry {
    pub key: String,
    pub value: String,
}

pub fn set_attribute(env: Env, caller: Address, key: String, value: String) -> Result<(), ContractError> {
    Ok(())
}

pub fn get_attributes(env: Env, caller: Address) -> Vec<AttributeEntry> {
    Vec::new(&env)
}

pub fn remove_attribute(env: Env, caller: Address, key: String) -> Result<(), ContractError> {
    Ok(())
}
