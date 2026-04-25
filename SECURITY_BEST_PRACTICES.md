# Security Best Practices

## Key Management

- Use hardware wallets or secure key management services whenever possible.
- Never store private keys, secret phrases, or deployer keys in plaintext in repository files.
- Use environment variables or secret management systems for deployment credentials.
- Limit developer access to production private keys and rotate keys if a compromise is suspected.
- Protect signing machines with full-disk encryption and multi-factor authentication.
- Maintain an offline backup of keys in a separate secure location.

## Multisig Setup and Operation

- Configure all admin-sensitive contract actions behind a multisig threshold.
- Use at least 3-of-5 or equivalent quorum for production admin access.
- Keep signer membership well documented and review it regularly.
- Ensure all multisig participants are aware of recovery and signer removal procedures.
- Test multisig flows on testnet before using them in production.
- Use multisig for contract upgrades, `slash()` workflow decisions, and emergency pauses.

## Emergency Procedures

- Define and document allowed emergency actions before deployment.
- Keep an emergency contact list of core maintainers and multisig signers.
- Maintain a rollback or pause procedure for contracts that can be executed quickly.
- For Soroban contracts, verify emergency function signatures before broadcasting transactions.
- Record all emergency approvals and decisions for audit purposes.
- Practice incident drills on testnet to ensure the team can execute emergency actions under pressure.

## Monitoring and Alerting Recommendations

- Monitor contract activity, especially large vouch, loan requests, repayments, and slash events.
- Add alerts for unusual patterns, such as rapid increases in total loan volume or repeated defaults.
- Track contract health metrics, including gas usage, failure rates, and token balances.
- Use log aggregation and dashboarding tools to correlate events across off-chain tooling and on-chain activity.
- Alert on governance and multisig operations so maintainers can verify expected behavior.
- Monitor repository and dependency security advisories for new vulnerabilities.

## Incident Response Procedures

- Establish a clear incident response owner and communication channel for security events.
- Triage incidents by severity, verify the impact, and determine whether immediate action is required.
- Preserve evidence: transaction logs, signatures, multisig approvals, and deployment metadata.
- If an exploit is suspected, pause or restrict contract operations if possible.
- Notify affected stakeholders promptly and transparently.
- After resolution, perform a post-incident review and update procedures based on lessons learned.
