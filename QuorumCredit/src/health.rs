use crate::errors::ContractError;
use crate::types::{Config, DataKey};
use soroban_sdk::{contracttype, Address, Env, String, Vec};
use crate::helpers::is_zero_address;

#[contracttype]
#[derive(Clone, Debug)]
pub struct HealthStatus {
    pub is_healthy: bool,
    pub initialized: bool,
    pub paused: bool,
    pub yield_reserve_solvent: bool,
    pub issues: Vec<String>,
}

pub fn health_check(env: &Env) -> HealthStatus {
    let mut issues = Vec::new(env);
    let mut is_healthy = true;

    // Check if initialized
    let initialized = env.storage().instance().has(&DataKey::Config);
    if !initialized {
        issues.push_back(String::from_str(env, "Contract not initialized"));
        is_healthy = false;
    }

    // Check if paused
    let paused: bool = env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false);

    // Check yield reserve solvency (contract must hold enough XLM for yield payouts)
    let yield_reserve_solvent = if initialized {
        let config: Config = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .unwrap_or_else(|| {
                panic!("Config not found despite initialized check");
            });

        let token_client = soroban_sdk::token::Client::new(env, &config.token);
        let contract_balance = token_client.balance(&env.current_contract_address());
        
        // Reserve must be at least 1 XLM (10_000_000 stroops) to be considered solvent
        contract_balance >= 10_000_000
    } else {
        false
    };

    if !yield_reserve_solvent && initialized {
        issues.push_back(String::from_str(env, "Yield reserve below minimum threshold"));
        is_healthy = false;
    }

    HealthStatus {
        is_healthy,
        initialized,
        paused,
        yield_reserve_solvent,
        issues,
    }
}
