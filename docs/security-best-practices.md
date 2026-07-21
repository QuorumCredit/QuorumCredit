# QuorumCredit Security Best Practices

This guide provides security best practices for operators, developers, and users of QuorumCredit.

## Table of Contents

- [Operator Security](#operator-security)
- [Key Management](#key-management)
- [Contract Deployment](#contract-deployment)
- [Access Control](#access-control)
- [Monitoring & Incident Response](#monitoring--incident-response)
- [User Security](#user-security)
  - [Wallet Security](#wallet-security)
  - [Key Management for Users](#key-management-for-users)
  - [Phishing Prevention](#phishing-prevention)
  - [Identifying Rug Pulls & Scams](#identifying-rug-pulls--scams)
  - [Safe Operation Checklist](#safe-operation-checklist)
  - [Emergency Contact Procedures](#emergency-contact-procedures)
- [Development Security](#development-security)

---

## Operator Security

### Admin Key Management

**Critical**: Admin keys control the entire protocol. Compromise of admin keys can lead to:
- Unauthorized contract upgrades
- Configuration changes
- Fund theft
- Protocol shutdown

**Best Practices**:

1. **Use Hardware Wallets**: Store admin keys on hardware wallets (Ledger, Trezor)
   ```bash
   # Never store keys in plaintext files
   # Use hardware wallet for signing
   stellar keys add admin-hw --hw
   ```

2. **Implement Multisig**: Require multiple signatures for admin functions
   ```bash
   # Initialize with 3-of-5 multisig
   stellar contract invoke --id $CONTRACT_ID --fn initialize \
     --source $DEPLOYER_SECRET_KEY -- \
     --admins '["'$ADMIN1'","'$ADMIN2'","'$ADMIN3'","'$ADMIN4'","'$ADMIN5'"]' \
     --admin_threshold 3
   ```

3. **Separate Roles**: Use different keys for different functions
   - **Deployer key**: Only for initial deployment and initialization
   - **Admin keys**: For ongoing governance
   - **Operator key**: For routine operations (pause/unpause)

4. **Rotate Keys Regularly**: Change admin keys periodically
   - Quarterly minimum
   - Immediately if compromise is suspected
   - After staff changes

5. **Secure Key Storage**:
   - Never commit keys to version control
   - Use `.env` files with `.gitignore`
   - Store in secure vaults (HashiCorp Vault, AWS Secrets Manager)
   - Encrypt at rest

### Admin Threshold Configuration

**Recommendation**: Use at least 2-of-3 multisig for production

| Scenario | Threshold | Rationale |
|----------|-----------|-----------|
| Testnet | 1-of-1 | Development only |
| Staging | 2-of-3 | Prevent single-key compromise |
| Mainnet | 3-of-5 | High security, operational flexibility |

### Pause Mechanism

**Use pause for**:
- Emergency response to security issues
- Contract upgrades
- Maintenance windows
- Investigating anomalies

**Pause procedure**:
```bash
# 1. Pause the contract
stellar contract invoke --id $CONTRACT_ID --fn pause --network mainnet \
  --source $ADMIN_SECRET_KEY -- --admin_signers '["'$ADMIN1'","'$ADMIN2'","'$ADMIN3'"]'

# 2. Investigate and fix
# ... perform investigation ...

# 3. Unpause when ready
stellar contract invoke --id $CONTRACT_ID --fn unpause --network mainnet \
  --source $ADMIN_SECRET_KEY -- --admin_signers '["'$ADMIN1'","'$ADMIN2'","'$ADMIN3'"]'
```

---

## Key Management

### Secret Key Security

**Never**:
- Commit secret keys to version control
- Share keys via email or chat
- Store keys in plaintext
- Use the same key for multiple purposes
- Reuse keys across networks (testnet/mainnet)

**Always**:
- Use environment variables for keys
- Rotate keys regularly
- Use hardware wallets for production
- Implement key access logging
- Backup keys securely

### Environment Variables

**Secure `.env` setup**:
```bash
# .env (NEVER commit this)
NETWORK=mainnet
DEPLOYER_SECRET_KEY="SB..."
ADMIN_SECRET_KEY_1="SB..."
ADMIN_SECRET_KEY_2="SB..."
ADMIN_SECRET_KEY_3="SB..."
TOKEN_CONTRACT="CA..."
```

**Protect `.env`**:
```bash
# Add to .gitignore
echo ".env" >> .gitignore
echo ".env.local" >> .gitignore

# Restrict file permissions
chmod 600 .env

# Use secure vaults in production
# Example: AWS Secrets Manager
aws secretsmanager get-secret-value --secret-id quorum-credit-keys
```

### Key Rotation

**Rotation schedule**:
- **Quarterly**: Routine rotation
- **Immediately**: If compromise suspected
- **After staff changes**: Remove departing team member's keys
- **After security incident**: Rotate all keys

**Rotation procedure**:
1. Generate new admin keys
2. Update contract configuration with new keys
3. Verify new keys work
4. Revoke old keys
5. Document rotation in audit log

---

## Contract Deployment

### Pre-Deployment Checklist

- [ ] All tests passing: `cargo test`
- [ ] No clippy warnings: `cargo clippy`
- [ ] Code reviewed by 2+ team members
- [ ] Security audit completed (for mainnet)
- [ ] Testnet deployment verified
- [ ] Deployment script tested
- [ ] Rollback plan documented
- [ ] Monitoring configured
- [ ] Communication plan ready

### Deployment Sequence

**Critical**: Follow this exact sequence to prevent front-running attacks

```bash
# Step 1: Build WASM
cargo build --target wasm32-unknown-unknown --release

# Step 2: Deploy contract (deployer signs)
CONTRACT_ID=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/quorum_credit.wasm \
  --network mainnet \
  --source $DEPLOYER_SECRET_KEY)

# Step 3: Initialize immediately (SAME deployer key)
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn initialize \
  --network mainnet \
  --source $DEPLOYER_SECRET_KEY \
  -- \
  --deployer $DEPLOYER_ADDRESS \
  --admins '["'$ADMIN1'","'$ADMIN2'","'$ADMIN3'"]' \
  --admin_threshold 2 \
  --token $TOKEN_CONTRACT
```

**Why this matters**: If initialization is delayed, an attacker could call `initialize` first with malicious parameters.

### Testnet Verification

Before mainnet deployment:

1. **Deploy to testnet**: Follow deployment sequence
2. **Run integration tests**: Test all critical paths
3. **Verify configuration**: Check `get_config()` returns expected values
4. **Test admin functions**: Verify multisig works
5. **Simulate incident**: Test pause/unpause
6. **Monitor for 24 hours**: Watch for anomalies

### Upgrade Safety

**Before upgrading**:
1. Pause the contract
2. Backup current state (if possible)
3. Test new WASM on testnet
4. Verify upgrade procedure
5. Prepare rollback plan

**Upgrade procedure**:
```bash
# 1. Build new WASM
cargo build --target wasm32-unknown-unknown --release

# 2. Pause contract
stellar contract invoke --id $CONTRACT_ID --fn pause --network mainnet \
  --source $ADMIN_SECRET_KEY -- --admin_signers '["'$ADMIN1'","'$ADMIN2'"]'

# 3. Install new WASM
NEW_WASM_HASH=$(stellar contract install \
  --wasm target/wasm32-unknown-unknown/release/quorum_credit.wasm \
  --network mainnet \
  --source $ADMIN_SECRET_KEY)

# 4. Upgrade contract
stellar contract invoke --id $CONTRACT_ID --fn upgrade --network mainnet \
  --source $ADMIN_SECRET_KEY -- \
  --admin_signers '["'$ADMIN1'","'$ADMIN2'"]' \
  --new_wasm_hash $NEW_WASM_HASH

# 5. Unpause contract
stellar contract invoke --id $CONTRACT_ID --fn unpause --network mainnet \
  --source $ADMIN_SECRET_KEY -- --admin_signers '["'$ADMIN1'","'$ADMIN2'"]'
```

---

## Access Control

### Role-Based Access

| Role | Functions | Requirements |
|------|-----------|--------------|
| **Deployer** | `initialize` | Must sign deployment tx |
| **Admin** | `pause`, `unpause`, `upgrade`, `set_config` | Must meet `admin_threshold` |
| **Voucher** | `vouch`, `increase_stake`, `decrease_stake`, `withdraw_vouch` | Must sign tx |
| **Borrower** | `request_loan`, `repay` | Must sign tx |
| **Anyone** | `get_config`, `get_loan`, `get_vouches`, `is_eligible` | Read-only |

### Authorization Checks

**Always verify**:
- Caller is authorized for the function
- Caller has required signatures (for multisig)
- Caller has sufficient balance (for token transfers)
- Caller is not blacklisted (for borrowers)

**Example**:
```rust
// Verify caller is the borrower
borrower.require_auth();

// Verify caller is an admin (multisig)
verify_admin_signatures(&env, &admin_signers, &admin_threshold)?;
```

### Blacklist Management

**Use blacklist for**:
- Repeat defaulters
- Fraudulent borrowers
- Sanctioned addresses
- Compromised accounts

**Blacklist procedure**:
```bash
# Add to blacklist
stellar contract invoke --id $CONTRACT_ID --fn add_to_blacklist \
  --network mainnet --source $ADMIN_SECRET_KEY -- \
  --admin_signers '["'$ADMIN1'","'$ADMIN2'"]' \
  --borrower $BORROWER_ADDRESS

# Remove from blacklist
stellar contract invoke --id $CONTRACT_ID --fn remove_from_blacklist \
  --network mainnet --source $ADMIN_SECRET_KEY -- \
  --admin_signers '["'$ADMIN1'","'$ADMIN2'"]' \
  --borrower $BORROWER_ADDRESS
```

---

## Monitoring & Incident Response

### Monitoring Setup

**Monitor these metrics**:
- Contract balance (should never go negative)
- Loan disbursements (unusual spikes)
- Default rate (should be < 5%)
- Yield distribution (should match calculations)
- Admin actions (all should be logged)
- Contract pause state (should be unpaused normally)

**Monitoring tools**:
- Stellar Horizon API for transaction history
- Soroban RPC for contract state
- Custom indexer for event tracking
- Alerting system (PagerDuty, Opsgenie)

### Incident Response Plan

**Incident severity levels**:

| Level | Impact | Response Time |
|-------|--------|----------------|
| **Critical** | Funds at risk, contract compromised | Immediate (< 5 min) |
| **High** | Significant functionality broken | 15 minutes |
| **Medium** | Minor functionality broken | 1 hour |
| **Low** | Documentation or UI issues | 24 hours |

**Critical incident response**:
1. **Pause contract** (< 1 minute)
2. **Notify stakeholders** (< 5 minutes)
3. **Investigate** (ongoing)
4. **Develop fix** (parallel)
5. **Test fix on testnet** (before deploying)
6. **Deploy fix** (multisig approval)
7. **Unpause contract** (after verification)
8. **Post-mortem** (within 24 hours)

### Logging & Auditing

**Log all**:
- Admin actions (pause, unpause, upgrade, config changes)
- Loan disbursements and repayments
- Slash events
- Authorization failures
- Configuration changes

**Audit trail**:
```bash
# Query contract events
stellar events --id $CONTRACT_ID --network mainnet

# Filter by event type
stellar events --id $CONTRACT_ID --network mainnet --type "admin/*"

# Export for analysis
stellar events --id $CONTRACT_ID --network mainnet --format json > audit.json
```

### Disaster Recovery

**Backup procedures**:
1. **Contract state**: Snapshot contract storage regularly
2. **Configuration**: Backup `get_config()` output
3. **Admin keys**: Secure backup of admin keys (encrypted)
4. **Documentation**: Keep deployment and configuration docs updated

**Recovery procedures**:
1. **Identify issue**: Determine what went wrong
2. **Pause contract**: Stop all operations
3. **Assess damage**: Determine scope of impact
4. **Develop fix**: Create patched WASM
5. **Deploy fix**: Use upgrade procedure
6. **Verify recovery**: Confirm state is correct
7. **Unpause**: Resume operations

---

## User Security

This section is for **borrowers and vouchers** — anyone interacting with QuorumCredit day-to-day, not just protocol operators. The single biggest cause of user fund loss is not a smart contract bug — it's a leaked secret key, a fake website, or a too-good-to-be-true "guaranteed yield" pool. Read this section before you sign your first transaction.

### For Borrowers

**Protect your account**:
- Use hardware wallet for loan requests
- Never share your secret key
- Verify contract address before interacting
- Check loan terms before accepting
- Set calendar reminders for repayment deadlines

**Repayment security**:
- Repay before deadline to avoid default
- Verify repayment amount before sending
- Keep proof of repayment (transaction hash)
- Confirm yield was received

### For Vouchers

**Protect your stake**:
- Use hardware wallet for vouching
- Never share your secret key
- Verify borrower identity before vouching
- Start with small stakes to test
- Diversify across multiple borrowers

**Vouch responsibly**:
- Only vouch for people you trust
- Understand the risks (50% slash on default)
- Monitor borrower's loan status
- Participate in slash votes
- Withdraw vouches when no longer comfortable

### Wallet Security

Your wallet is the only thing standing between your funds and an attacker. Treat wallet hygiene as seriously as the protocol treats admin key hygiene.

**Choosing a wallet**:
- Prefer a **hardware wallet** (Ledger, Trezor) for any meaningful stake or loan amount — keys never leave the device and transactions are signed on-screen.
- If using a software/browser wallet (Freighter, xBull, Lobstr, Rabet), keep it updated and only install extensions from official app store listings.
- Never use a wallet whose seed phrase was generated by, or entered into, a website, Discord bot, or "recovery tool" — legitimate wallets generate seeds locally on your device.

**Operating your wallet safely**:
- Read every transaction before signing — check the destination contract ID, function name, and amount. Soroban wallets show a simulation; don't blind-sign.
- Use a **dedicated wallet** for QuorumCredit interactions, separate from wallets holding your long-term savings.
- Lock your wallet/browser session when not in use, and never leave a hardware wallet plugged in and unlocked unattended.
- Disable auto-connect / "always allow" for dApp connections; approve each session explicitly.
- Keep the device and OS your wallet runs on patched — malware that reads clipboards or keystrokes can silently swap a pasted address or exfiltrate a seed phrase.

### Key Management for Users

The same principles that protect protocol admin keys apply to your personal keys — just at individual scale.

**Never**:
- Type your secret key (starts with `S...`) into any website, form, chat, or "support" bot — QuorumCredit staff will **never** ask for it.
- Store your secret key in plaintext notes, screenshots, cloud drives, email drafts, or password managers without encryption.
- Share your seed phrase or secret key with anyone claiming to need it for "verification," "unlocking a stuck transaction," or "syndication approval."
- Reuse the same key across testnet and mainnet, or across unrelated protocols.

**Always**:
- Write your seed phrase down on paper or a metal backup, store it offline, and never photograph or type it.
- Use a hardware wallet or a passphrase-protected key for anything beyond small test amounts.
- Verify you are on the correct, official domain before connecting a wallet or entering any key material.
- If you ever suspect a key has been exposed, move funds to a new address immediately and treat the old key as burned — do not "wait and see."

### Phishing Prevention

**Be aware of**:
- Fake contract addresses
- Phishing emails claiming to be from QuorumCredit
- Fake websites mimicking QuorumCredit
- Social engineering attacks
- Fake "support" accounts in Discord/Telegram DMs offering to "help" with a stuck transaction
- Malicious browser extensions or fake wallet-connect pop-ups that clone the real signing UI
- Airdrop/giveaway scams asking you to "verify your wallet" by connecting and signing a blank transaction

**Verify authenticity**:
- Check contract address on GitHub
- Use official website only
- Verify email sender domain
- Never click links in unsolicited emails
- Use hardware wallet for all transactions
- Bookmark the official site rather than following search-engine ads or links shared in chats
- Remember: **no legitimate team member will ever DM you first or ask for your secret key/seed phrase**

### Identifying Rug Pulls & Scams

QuorumCredit's social-collateral model reduces (but does not eliminate) rug-pull risk, since vouchers stake real XLM against borrowers. Watch for these warning signs before vouching, borrowing, or interacting with any "QuorumCredit-branded" contract or pool:

**Red flags on a contract or deployment**:
- Admin key is a single signer with **no multisig** (`admin_threshold` of 1) on a "mainnet" deployment — one compromised or malicious key can drain or reconfigure everything.
- No public audit report, or an audit report that doesn't match the deployed contract's WASM hash.
- Contract is **unpausable** or the pause function is missing/disabled — legitimate deployments always retain an emergency pause path.
- Upgrade authority held by an anonymous or unverifiable address with no timelock.
- Contract address does not match the one published in this repository or the official website/GitHub.

**Red flags on a "deal" or pool**:
- Guaranteed or unusually high yield with "no risk" — real yield in this protocol comes from repaid loans; if the return doesn't map to a loan and repayment schedule, question it.
- Pressure to vouch or deposit quickly ("limited slots," "price goes up in 1 hour").
- Anonymous team with no verifiable history, or a team that refuses to answer basic questions about the deployment's admin/multisig configuration.
- Requests to send funds to a wallet address directly instead of interacting with the published contract.
- Clone/fork projects using QuorumCredit's name or branding without a link back to this repository.

**Before you vouch or borrow, verify**:
- [ ] Contract ID matches the address published in this README/GitHub release
- [ ] `get_config()` shows a multisig `admin_threshold` ≥ 2 for any mainnet deployment
- [ ] The deployment has a public audit report, or you're knowingly using an unaudited testnet/beta
- [ ] The team/documentation is reachable through the [official channels](#emergency-contact-procedures) below, not only a Discord DM

### Safe Operation Checklist

Run through this before every session, not just the first time:

- [ ] I am on the official website/domain and have verified the contract address
- [ ] My wallet is a hardware wallet, or a software wallet I fully control the seed for
- [ ] I have never entered my secret key or seed phrase into a website, bot, or form
- [ ] I reviewed the transaction (destination, function, amount) before signing — I did not blind-sign
- [ ] I am vouching/borrowing only what I can afford to lose, given the slash risk
- [ ] I diversified vouches across multiple borrowers rather than concentrating stake in one
- [ ] I set a reminder for my loan/vouch deadlines (repayment, cooldown, withdrawal windows)
- [ ] I know how to reach [official support channels](#emergency-contact-procedures) if something looks wrong
- [ ] I have not connected my wallet to any unfamiliar dApp claiming to be QuorumCredit
- [ ] My device/browser and wallet software are up to date

### Emergency Contact Procedures

If you suspect your account is compromised, you've interacted with a phishing site, or you've spotted a likely scam/rug pull impersonating QuorumCredit, act immediately — speed matters more than perfect information.

**If you believe your key/seed phrase is exposed**:
1. **Move funds first, ask questions later.** If you can still access the wallet, immediately transfer remaining funds to a new address generated on a device you trust.
2. **Withdraw any active vouches or loan positions** you can still control, before the exposed key can be used against them.
3. **Stop using the exposed key entirely** — do not reuse it for anything, even after "cleaning" the device.
4. **Report it** so the team can watch for related on-chain activity and warn other users if it's part of a wider campaign.

**If you encounter a phishing site, fake support account, or suspected rug pull**:
1. Do **not** engage further, click links, or share any additional information.
2. Screenshot the phishing site/message/contract address for evidence.
3. Report it through the official channels below.
4. Warn others in official community channels only — do not amplify the scam link itself.

**Official contact channels** (use these, and only these, for security emergencies):

| Situation | Channel |
|---|---|
| Compromised key, active fund-loss risk | Email `security@quorumcredit.io` with subject `[URGENT] Account Compromise` |
| Phishing site / fake support / suspected rug pull impersonating QuorumCredit | Email `security@quorumcredit.io` or file a [GitHub Security Advisory](https://github.com/your-org/QuorumCredit/security/advisories/new) |
| Smart contract vulnerability | Follow [SECURITY.md](../SECURITY.md) — do **not** open a public GitHub issue |
| General questions / community support | [Stellar Developer Discord](https://discord.gg/stellardev) |

> QuorumCredit staff will never DM you first, never ask for your secret key or seed phrase, and never ask you to "verify" your wallet by signing a blind transaction. Any message that does is impersonation — report it using the channels above.

---

## Development Security

### Code Review

**All code changes require**:
- 2+ peer reviews
- Security review for sensitive code
- Automated testing (100% coverage for critical paths)
- Clippy checks (no warnings)
- Cargo audit (no vulnerabilities)

### Dependency Management

**Secure dependencies**:
```bash
# Check for vulnerabilities
cargo audit

# Update dependencies
cargo update

# Pin versions in Cargo.toml
soroban-sdk = "=20.0.0"  # Exact version
```

**Avoid**:
- Unvetted dependencies
- Dependencies with known vulnerabilities
- Outdated dependencies
- Typosquatting variants

### Testing

**Test coverage**:
- Unit tests: 100% for critical functions
- Integration tests: All user flows
- Property-based tests: Invariants
- Fuzz tests: Edge cases
- Security tests: Authorization, overflow, underflow

**Test execution**:
```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_vouch_and_loan_disbursed

# Generate coverage
cargo tarpaulin --out Html
```

### Security Audit

**Before mainnet deployment**:
1. **Internal audit**: Team review
2. **External audit**: Third-party security firm
3. **Formal verification**: Mathematical proof of correctness (optional)
4. **Bug bounty**: Incentivize community to find issues

**Audit checklist**:
- [ ] No integer overflow/underflow
- [ ] No reentrancy vulnerabilities
- [ ] Proper authorization checks
- [ ] Correct yield/slash calculations
- [ ] State consistency maintained
- [ ] Error handling complete
- [ ] No hardcoded values
- [ ] Proper event logging

---

## Vulnerability Disclosure

### Reporting Security Issues

**Do not**:
- Open public GitHub issues
- Post on social media
- Share with unauthorized parties
- Attempt to exploit vulnerabilities

**Do**:
- Report privately via [GitHub Security Advisories](https://github.com/QuorumCredit/QuorumCredit/security/advisories/new)
- Include detailed reproduction steps
- Allow time for fix before disclosure
- Follow responsible disclosure timeline

### Responsible Disclosure Timeline

1. **Day 0**: Report vulnerability
2. **Day 1**: Acknowledgment from team
3. **Day 7**: Initial assessment
4. **Day 30**: Fix developed and tested
5. **Day 45**: Fix deployed to mainnet
6. **Day 60**: Public disclosure (if appropriate)

---

## Security Checklist

### Pre-Deployment

- [ ] All tests passing
- [ ] No clippy warnings
- [ ] No cargo audit vulnerabilities
- [ ] Code reviewed by 2+ team members
- [ ] Security audit completed
- [ ] Testnet deployment verified
- [ ] Admin multisig configured
- [ ] Monitoring configured
- [ ] Incident response plan ready
- [ ] Communication plan ready

### Post-Deployment

- [ ] Monitor contract balance
- [ ] Monitor loan metrics
- [ ] Monitor admin actions
- [ ] Review audit logs daily
- [ ] Rotate admin keys quarterly
- [ ] Update security documentation
- [ ] Conduct security training
- [ ] Test incident response procedures

### Ongoing

- [ ] Keep dependencies updated
- [ ] Monitor security advisories
- [ ] Conduct regular security reviews
- [ ] Perform penetration testing
- [ ] Update threat model
- [ ] Review and update this guide

---

## Resources

- [SECURITY.md](../SECURITY.md) - Vulnerability disclosure policy
- [Deployment Guide](../docs/deployment-guide.md) - Deployment procedures
- [Monitoring Guide](../docs/monitoring-guide.md) - Monitoring setup
- [Threat Model](../docs/threat-model.md) - Security threat analysis
- [Stellar Security](https://developers.stellar.org/docs/learn/security) - Stellar security best practices
- [Soroban Security](https://soroban.stellar.org/docs/learn/security) - Soroban security best practices

---

## Questions?

For security questions or concerns:
- Email: security@quorumcredit.io
- GitHub: [Security Advisories](https://github.com/QuorumCredit/QuorumCredit/security/advisories)
- Discord: [Stellar Developer Discord](https://discord.gg/stellardev)
