use soroban_sdk::{Address, Env, Vec};
use crate::errors::ContractError;
use crate::types::{FraudScoreConfig, VoucherFraudScore};

pub fn update_fraud_score(
    _env: Env,
    _voucher: Address,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn get_fraud_score(
    _env: Env,
    _voucher: Address,
) -> Option<VoucherFraudScore> {
    None
}

pub fn set_fraud_score_config(
    _env: Env,
    _admin_signers: Vec<Address>,
    _config: FraudScoreConfig,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn get_fraud_score_config_view(
    _env: Env,
) -> FraudScoreConfig {
    FraudScoreConfig {
        threshold: 0,
        enabled: false,
    }
}
