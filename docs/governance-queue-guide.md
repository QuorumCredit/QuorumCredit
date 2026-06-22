# Admin Governance Queue with Multi-Signature Confirmation

## Overview

The Admin Governance Queue is a comprehensive system for managing administrative actions through a multi-signature confirmation process. This system ensures that sensitive administrative operations require approval from multiple administrators before execution, with built-in timelock delays for security.

## Features

- **Multi-Signature Confirmation**: Admin actions require approval from a configurable threshold of administrators
- **Timelock Protection**: All proposals have a mandatory delay before execution (default: 24 hours)
- **Execution Window**: Proposals must be executed within a configurable time window (default: 7 days)
- **Proposal Cancellation**: Proposals can be cancelled by the proposer or any admin
- **Comprehensive Action Types**: Support for all major administrative operations
- **Audit Trail**: All governance actions are logged with events for transparency

## Governance Actions

The following governance actions can be proposed:

- `Pause` - Pause the contract
- `Unpause` - Unpause the contract
- `Upgrade(BytesN<32>)` - Upgrade the contract to a new WASM hash
- `SetProtocolFee(u32)` - Set protocol fee in basis points
- `SetFeeTreasury(Address)` - Set fee treasury address
- `AddAllowedToken(Address)` - Add an allowed token
- `RemoveAllowedToken(Address)` - Remove an allowed token
- `SetMinStake(i128)` - Set minimum stake amount
- `SetMaxLoanAmount(i128)` - Set maximum loan amount
- `SetMinVouchers(u32)` - Set minimum vouchers required
- `SetMaxVouchersPerBorrower(u32)` - Set maximum vouchers per borrower
- `SetMaxLoanToStakeRatio(u32)` - Set max loan to stake ratio
- `SetGracePeriod(u64)` - Set grace period
- `SetYieldBps(i128)` - Set yield basis points
- `SetSlashBps(i128)` - Set slash basis points
- `SetAdminThreshold(u32)` - Set admin threshold
- `AddAdmin(Address)` - Add an admin
- `RemoveAdmin(Address)` - Remove an admin
- `RotateAdmin(Address, Address)` - Rotate an admin
- `SetReputationNft(Address)` - Set reputation NFT contract
- `SetWhitelistEnabled(bool)` - Set whitelist enabled
- `BlacklistBorrower(Address)` - Blacklist a borrower
- `SetPrepaymentPenaltyBps(u32)` - Set prepayment penalty basis points
- `SetDynamicSlashThreshold(bool)` - Set dynamic slash threshold enabled
- `SetLoanSizeSlashEnabled(bool)` - Set loan size slash enabled
- `SetLoanSizeSlashMaxBps(i128)` - Set loan size slash max basis points
- `SetSuccessorAdmin(Option<Address>)` - Set successor admin
- `SetConfirmationRequired(bool)` - Set confirmation required
- `SetAdminCompensationBps(u32)` - Set admin compensation basis points
- `SetRemovalVoteThreshold(u32)` - Set removal vote threshold
- `SetRateLimitConfig(RateLimitConfig)` - Set rate limit config

## Data Structures

### GovernanceProposal

```rust
pub struct GovernanceProposal {
    pub id: u64,                          // Unique proposal ID
    pub action: GovernanceAction,          // The governance action to execute
    pub proposer: Address,                 // Address that proposed the action
    pub approvals: Vec<Address>,            // Addresses that have approved
    pub rejections: Vec<Address>,          // Addresses that have rejected
    pub status: GovernanceProposalStatus,  // Current status
    pub created_at: u64,                  // Creation timestamp
    pub executable_at: u64,                // When it can be executed
    pub expires_at: u64,                  // When it expires
    pub description: String,              // Optional description
    pub executed_at: Option<u64>,         // Execution timestamp (if executed)
}
```

### GovernanceProposalStatus

```rust
pub enum GovernanceProposalStatus {
    Pending,   // Proposal is pending approval
    Approved,  // Proposal has been approved and can be executed
    Executed,  // Proposal has been executed
    Cancelled, // Proposal has been cancelled
    Expired,   // Proposal has expired
}
```

### GovernanceQueueConfig

```rust
pub struct GovernanceQueueConfig {
    pub timelock_delay: u64,      // Minimum delay before execution (default: 24 hours)
    pub execution_window: u64,    // Time window for execution (default: 7 days)
    pub require_multisig: bool,   // Whether multi-sig is required (default: true)
}
```

## Contract Functions

### Configuration

#### `set_governance_queue_config`

Set the governance queue configuration parameters.

```rust
pub fn set_governance_queue_config(
    env: Env,
    admin_signers: Vec<Address>,
    config: GovernanceQueueConfig,
)
```

**Requirements**: Admin approval (multi-sig)

### Proposal Management

#### `propose_governance_action`

Propose a governance action to the queue.

```rust
pub fn propose_governance_action(
    env: Env,
    proposer: Address,
    action: GovernanceAction,
    description: String,
) -> Result<u64, ContractError>
```

**Requirements**: Caller must be an admin

**Returns**: Proposal ID

#### `approve_governance_action`

Approve a governance proposal.

```rust
pub fn approve_governance_action(
    env: Env,
    admin: Address,
    proposal_id: u64,
) -> Result<(), ContractError>
```

**Requirements**: Caller must be an admin, cannot vote twice

**Behavior**: When approval threshold is met, proposal status changes to `Approved`

#### `reject_governance_action`

Reject a governance proposal.

```rust
pub fn reject_governance_action(
    env: Env,
    admin: Address,
    proposal_id: u64,
) -> Result<(), ContractError>
```

**Requirements**: Caller must be an admin, cannot vote twice

**Behavior**: When rejection threshold is met, proposal status changes to `Cancelled`

#### `execute_governance_action`

Execute an approved governance proposal.

```rust
pub fn execute_governance_action(
    env: Env,
    proposal_id: u64,
) -> Result<(), ContractError>
```

**Requirements**: 
- Proposal must be in `Approved` status
- Timelock delay must have elapsed
- Execution window must not have passed

**Behavior**: Anyone can call this once conditions are met

#### `cancel_governance_action`

Cancel a governance proposal.

```rust
pub fn cancel_governance_action(
    env: Env,
    caller: Address,
    proposal_id: u64,
) -> Result<(), ContractError>
```

**Requirements**: Caller must be the proposer or an admin

### View Functions

#### `get_governance_proposal`

Get a governance proposal by ID.

```rust
pub fn get_governance_proposal(
    env: Env,
    proposal_id: u64,
) -> Option<GovernanceProposal>
```

#### `get_governance_queue_config_view`

Get the current governance queue configuration.

```rust
pub fn get_governance_queue_config_view(env: Env) -> GovernanceQueueConfig
```

#### `get_governance_proposal_count`

Get the total number of governance proposals created.

```rust
pub fn get_governance_proposal_count(env: Env) -> u64
```

## Workflow Example

### 1. Configure the Governance Queue

```rust
let config = GovernanceQueueConfig {
    timelock_delay: 24 * 60 * 60,      // 24 hours
    execution_window: 7 * 24 * 60 * 60,  // 7 days
    require_multisig: true,
};

contract.set_governance_queue_config(
    &[admin1, admin2],  // Requires admin approval
    &config,
);
```

### 2. Propose an Action

```rust
let proposal_id = contract.propose_governance_action(
    &admin1,
    &GovernanceAction::SetProtocolFee(500),  // 5%
    &String::from_str(&env, "Increase protocol fee to 5%"),
)?;
```

### 3. Approve the Proposal

```rust
// Admin 1 approves
contract.approve_governance_action(&admin1, proposal_id)?;

// Admin 2 approves (meets threshold)
contract.approve_governance_action(&admin2, proposal_id)?;
```

### 4. Wait for Timelock

The proposal must wait for the timelock delay (default: 24 hours) before execution.

### 5. Execute the Proposal

```rust
// Anyone can execute once timelock has elapsed
contract.execute_governance_action(proposal_id)?;
```

## Security Features

### Multi-Signature Protection

- All proposals require approval from a threshold of administrators
- The threshold is configurable via `admin_threshold` in the config
- Each admin can only vote once per proposal

### Timelock Protection

- Mandatory delay between approval and execution
- Prevents rushed or malicious actions
- Gives time for community review and objection

### Execution Window

- Proposals must be executed within a time window
- Prevents stale proposals from being executed unexpectedly
- Expired proposals cannot be executed

### Cancellation Rights

- Proposer can cancel their own proposal
- Any admin can cancel a proposal
- Provides flexibility to withdraw proposals if needed

### Double-Voting Prevention

- Each admin can only vote once per proposal
- Cannot both approve and reject the same proposal
- Prevents manipulation of the voting process

## Error Codes

- `ProposalNotFound` - Proposal does not exist
- `ProposalAlreadyFinalized` - Proposal is already executed or cancelled
- `ProposalExpired` - Proposal has expired
- `ProposalAlreadyApproved` - Proposal is already approved
- `TimelockDelayNotElapsed` - Timelock delay has not passed
- `ExecutionWindowPassed` - Execution window has passed
- `InvalidGovernanceAction` - Action is invalid or not supported
- `UnauthorizedCaller` - Caller is not authorized
- `AlreadyVoted` - Admin has already voted

## Testing

Comprehensive tests are available in `src/governance_queue_test.rs`:

- `test_propose_governance_action` - Test proposal creation
- `test_propose_unauthorized` - Test unauthorized proposal attempt
- `test_approve_governance_action` - Test approval workflow
- `test_approve_unauthorized` - Test unauthorized approval attempt
- `test_double_approval` - Test double-voting prevention
- `test_reject_governance_action` - Test rejection workflow
- `test_execute_governance_action` - Test execution workflow
- `test_execute_before_timelock` - Test execution before timelock
- `test_execute_after_expiry` - Test execution after expiry
- `test_cancel_governance_action` - Test cancellation by proposer
- `test_cancel_by_admin` - Test cancellation by admin
- `test_cancel_unauthorized` - Test unauthorized cancellation
- `test_set_governance_queue_config` - Test configuration
- `test_governance_action_set_protocol_fee` - Test specific action execution
- `test_governance_action_add_admin` - Test admin addition
- `test_governance_proposal_count` - Test proposal counting

Run tests with:
```bash
cargo test governance_queue_test
```

## Event Topics

The following events are published by the governance queue:

- `("gov", "queue_cfg")` - Governance queue configuration updated
- `("gov", "propose")` - New proposal created
- `("gov", "approve")` - Proposal approved
- `("gov", "reject")` - Proposal rejected
- `("gov", "execute")` - Proposal executed
- `("gov", "cancel")` - Proposal cancelled

## Best Practices

1. **Always Use the Governance Queue**: For sensitive administrative operations, always use the governance queue instead of direct admin functions.

2. **Set Appropriate Timelocks**: Configure timelock delays based on the sensitivity of the action. Critical actions should have longer delays.

3. **Monitor Proposals**: Regularly check pending proposals and participate in the approval process.

4. **Use Descriptions**: Provide clear descriptions for proposals to help other admins understand the rationale.

5. **Test Before Proposing**: Test configuration changes on a testnet before proposing on mainnet.

6. **Keep Thresholds Reasonable**: Set admin thresholds that balance security with operational efficiency.

## Migration Path

### Phase 1: Deployment
- Deploy the updated contract with governance queue functionality
- Existing admin functions continue to work as before
- Governance queue is available but not required

### Phase 2: Gradual Adoption
- Start using the governance queue for non-critical operations
- Monitor the workflow and adjust configuration as needed
- Train admins on the new process

### Phase 3: Full Adoption
- Require governance queue for all sensitive operations
- Deprecate direct admin functions where appropriate
- Establish governance queue as the standard process

## Integration with Existing Systems

The governance queue integrates seamlessly with existing admin functions:

- **RBAC**: Respects role-based access control
- **Admin Whitelist/Blacklist**: Validates admin membership
- **Emergency Pause**: Can be used alongside governance queue
- **Config Updates**: Complements existing config update proposals

## Future Enhancements

Potential future improvements:

- Proposal batching for related actions
- Delegation of voting rights
- Proposal scheduling
- Voting power weighting
- Emergency override mechanisms
- Cross-chain governance support
