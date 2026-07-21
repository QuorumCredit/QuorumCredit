Description
Priority: High

Description: Three related authorization gaps sit in the same trust boundary. First, emergency_pause (admin.rs:1246-1263) requires only a single admin.require_auth() plus an is_admin check, while emergency_unpause (admin.rs:1265-1278) requires the full require_admin_approval multisig threshold — a single compromised admin key can unilaterally freeze the protocol, contradicting the "no single key can permanently affect protocol state" framing in docs/security-audit-checklist.md. Second, queue_slash and execute_queued_slashes (governance.rs:709-750) both call require_admin_approval(&env, &Vec::new(&env)) with a hardcoded empty signer vector — since require_admin_approval (helpers.rs:292-312) asserts admin_signers.len() >= cfg.admin_threshold, this means the call either always panics (bricking the feature whenever admin_threshold > 0) or, if admin_threshold is ever configured to 0, silently requires zero authorization and zero require_auth() calls at all — a fully unauthenticated path to queuing/executing slashes. Third, vote_slash (governance.rs:225-233) silently returns Ok(()) with no signal when a voucher's stake has been delegated ("Silently succeed — delegate will vote"), giving callers no way to distinguish "my vote counted" from "my vote was a no-op" on a fund-slashing action.

Tasks:

Make emergency_pause require the same multisig threshold as emergency_unpause, or, if single-key pause is an intentional circuit-breaker design, add a compensating control (e.g. auto-expiring pause, mandatory post-pause multisig ratification) and document the tradeoff explicitly
Fix queue_slash/execute_queued_slashes to pass a real, caller-supplied admin_signers vector and require actual signatures, matching every other fund-affecting admin path
Add a config-level invariant preventing admin_threshold from ever being set to 0 (or explicitly document and gate it as a deliberate "open" mode)
Change vote_slash's delegated-vote path to return a distinct, typed result (not bare Ok(())) so callers can tell a delegated no-op from a counted vote
Add tests proving emergency_pause cannot proceed with fewer than the configured threshold once fixed
Add tests proving queue_slash/execute_queued_slashes are unreachable without valid, non-empty, threshold-satisfying signatures
Add regression tests for the delegated-vote return value across single and chained delegation
Smart Contracts