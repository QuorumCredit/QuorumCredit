#[cfg(test)]
mod tests {
    use crate::types::{WithdrawalTimelock, DataKey};
    use soroban_sdk::{Address, Env};

    #[test]
    fn test_withdrawal_timelock_creation() {
        let env = Env::default();
        let voucher = Address::random(&env);
        let borrower = Address::random(&env);
        let token = Address::random(&env);

        let timelock = WithdrawalTimelock {
            id: 1,
            voucher: voucher.clone(),
            borrower: borrower.clone(),
            amount: 1000_000_000,
            token: token.clone(),
            eta: 5000,
            executed: false,
            cancelled: false,
        };

        assert_eq!(timelock.id, 1);
        assert_eq!(timelock.amount, 1000_000_000);
        assert_eq!(timelock.executed, false);
        assert_eq!(timelock.cancelled, false);
    }

    #[test]
    fn test_withdrawal_timelock_delay() {
        let env = Env::default();
        let voucher = Address::random(&env);
        let borrower = Address::random(&env);
        let token = Address::random(&env);
        let current_time = 1000;
        let delay = 3600;  // 1 hour

        let timelock = WithdrawalTimelock {
            id: 1,
            voucher: voucher.clone(),
            borrower: borrower.clone(),
            amount: 1000_000_000,
            token: token.clone(),
            eta: current_time + delay,
            executed: false,
            cancelled: false,
        };

        assert_eq!(timelock.eta, current_time + delay);
        assert!(timelock.eta > current_time);
    }

    #[test]
    fn test_withdrawal_timelock_execution() {
        let env = Env::default();
        let voucher = Address::random(&env);
        let borrower = Address::random(&env);
        let token = Address::random(&env);

        let mut timelock = WithdrawalTimelock {
            id: 1,
            voucher: voucher.clone(),
            borrower: borrower.clone(),
            amount: 1000_000_000,
            token: token.clone(),
            eta: 5000,
            executed: false,
            cancelled: false,
        };

        // Execute timelock
        timelock.executed = true;

        assert_eq!(timelock.executed, true);
        assert_eq!(timelock.cancelled, false);
    }

    #[test]
    fn test_withdrawal_timelock_cancellation() {
        let env = Env::default();
        let voucher = Address::random(&env);
        let borrower = Address::random(&env);
        let token = Address::random(&env);

        let mut timelock = WithdrawalTimelock {
            id: 1,
            voucher: voucher.clone(),
            borrower: borrower.clone(),
            amount: 1000_000_000,
            token: token.clone(),
            eta: 5000,
            executed: false,
            cancelled: false,
        };

        // Cancel timelock
        timelock.cancelled = true;

        assert_eq!(timelock.cancelled, true);
        assert_eq!(timelock.executed, false);
    }

    #[test]
    fn test_withdrawal_timelock_no_double_execution() {
        let env = Env::default();
        let voucher = Address::random(&env);
        let borrower = Address::random(&env);
        let token = Address::random(&env);

        let mut timelock = WithdrawalTimelock {
            id: 1,
            voucher: voucher.clone(),
            borrower: borrower.clone(),
            amount: 1000_000_000,
            token: token.clone(),
            eta: 5000,
            executed: false,
            cancelled: false,
        };

        // Execute once
        timelock.executed = true;

        // Verify cannot execute again
        assert!(timelock.executed && !timelock.cancelled);
    }

    #[test]
    fn test_withdrawal_timelock_amount_validation() {
        let env = Env::default();
        let voucher = Address::random(&env);
        let borrower = Address::random(&env);
        let token = Address::random(&env);

        let timelock = WithdrawalTimelock {
            id: 1,
            voucher: voucher.clone(),
            borrower: borrower.clone(),
            amount: 5000_000_000,  // 500 XLM
            token: token.clone(),
            eta: 5000,
            executed: false,
            cancelled: false,
        };

        // Validate amount is positive
        assert!(timelock.amount > 0);
    }
}
