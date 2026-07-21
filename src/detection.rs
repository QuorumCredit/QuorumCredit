#![allow(unused_variables)]
use crate::errors::ContractError;
use crate::types::{FraudScoreConfig, VoucherFraudScore};
use soroban_sdk::{Address, Env, Vec};

pub fn update_fraud_score(env: Env, voucher: Address) -> Result<(), ContractError> {
    Ok(())
}
pub fn get_fraud_score(env: Env, voucher: Address) -> Option<VoucherFraudScore> {
    None
}
pub fn set_fraud_score_config(env: Env, admin_signers: Vec<Address>, config: FraudScoreConfig) -> Result<(), ContractError> {
    Ok(())
}
pub fn get_fraud_score_config_view(env: Env) -> FraudScoreConfig {
    FraudScoreConfig::default_config()
}
