use crate::errors::ContractError;
use crate::helpers::require_not_thawing;
use crate::types::{DataKey, VouchGroup};
use soroban_sdk::{symbol_short, Address, Env, Vec};

pub fn create_vouch_group(env: Env, caller: Address, name: soroban_sdk::String) -> Result<u64, ContractError> {
    caller.require_auth();
    require_not_thawing(&env)?;
    let group_id: u64 = env.storage().instance().get(&DataKey::VouchGroupCounter).unwrap_or(0u64).checked_add(1).expect("group ID overflow");
    let now = env.ledger().timestamp();
    let group = VouchGroup {
        group_id,
        name,
        vouchers: Vec::new(&env),
        created_at: now,
    };
    env.storage().persistent().set(&DataKey::VouchGroup(group_id), &group);
    env.storage().instance().set(&DataKey::VouchGroupCounter, &group_id);
    env.events().publish((symbol_short!("vgrp"), symbol_short!("create")), (caller, group_id));
    Ok(group_id)
}

pub fn add_voucher_to_group(env: Env, caller: Address, group_id: u64, voucher: Address) -> Result<(), ContractError> {
    caller.require_auth();
    require_not_thawing(&env)?;
    let mut group: VouchGroup = env.storage().persistent().get(&DataKey::VouchGroup(group_id)).ok_or(ContractError::NoActiveLoan)?;
    if group.vouchers.iter().any(|v| v == voucher) {
        return Ok(());
    }
    group.vouchers.push_back(voucher.clone());
    env.storage().persistent().set(&DataKey::VouchGroup(group_id), &group);
    let mut group_ids: Vec<u64> = env.storage().persistent().get(&DataKey::VoucherGroupIds(voucher.clone())).unwrap_or(Vec::new(&env));
    group_ids.push_back(group_id);
    env.storage().persistent().set(&DataKey::VoucherGroupIds(voucher.clone()), &group_ids);
    env.events().publish((symbol_short!("vgrp"), symbol_short!("add")), (caller, group_id, voucher));
    Ok(())
}

pub fn remove_voucher_from_group(env: Env, caller: Address, group_id: u64, voucher: Address) -> Result<(), ContractError> {
    caller.require_auth();
    require_not_thawing(&env)?;
    let mut group: VouchGroup = env.storage().persistent().get(&DataKey::VouchGroup(group_id)).ok_or(ContractError::NoActiveLoan)?;
    let len_before = group.vouchers.len();
    {
        let mut new_vouchers = Vec::new(&env);
        for v in group.vouchers.iter() {
            if v != voucher {
                new_vouchers.push_back(v.clone());
            }
        }
        group.vouchers = new_vouchers;
    }
    if group.vouchers.len() == len_before {
        return Err(ContractError::VoucherNotFound);
    }
    env.storage().persistent().set(&DataKey::VouchGroup(group_id), &group);
    let mut group_ids: Vec<u64> = env.storage().persistent().get(&DataKey::VoucherGroupIds(voucher.clone())).unwrap_or(Vec::new(&env));
    {
        let mut new_ids = Vec::new(&env);
        for g in group_ids.iter() {
            if g != group_id {
                new_ids.push_back(g);
            }
        }
        group_ids = new_ids;
    }
    env.storage().persistent().set(&DataKey::VoucherGroupIds(voucher.clone()), &group_ids);
    env.events().publish((symbol_short!("vgrp"), symbol_short!("rm")), (caller, group_id, voucher));
    Ok(())
}

pub fn get_vouch_group(env: Env, group_id: u64) -> Option<VouchGroup> {
    env.storage().persistent().get(&DataKey::VouchGroup(group_id))
}

pub fn get_voucher_group_ids(env: Env, voucher: Address) -> Vec<u64> {
    env.storage().persistent().get(&DataKey::VoucherGroupIds(voucher)).unwrap_or(Vec::new(&env))
}
