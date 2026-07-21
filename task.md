Description
Priority: High

Description: calculate_vouching_score (src/credit_score.rs:66-93) and vouch_reputation_weight (src/vouch.rs, ~line 1492) both reward raw vouch counts and successful_vouches history with no cost-of-collusion analysis — a ring of mutually-controlled addresses can vouch for one another, take trivially small loans, repay instantly, and farm both credit score and voucher-reputation weight, which then multiplies stake weight up to 2x per vouch (vouch.rs, ~lines 1499-1506). Nothing in the codebase bounds what this actually costs an attacker to execute at scale, and approve_extension (loan.rs, ~line 1123) compounds the problem by requiring (total_vouchers / 2) + 1 raw voucher count for deadline-extension governance — ignoring stake size entirely, so a Sybil ring of many trivial vouchers can out-vote a single legitimate large-stake voucher on loan extensions, contradicting the stake-weighting model used everywhere else in the protocol.

Tasks:

Build a formal cost-of-attack model for Sybil-ring vouching farms (cost to stand up N colluding addresses, cost to cycle trivial loans through them, marginal reputation/score gain per cycle)
Validate the model with a Monte Carlo/agent-based simulation across realistic stake/threshold/cooldown configurations
Redesign vouch_reputation_weight's multiplier and calculate_vouching_score to be resistant to low-cost repeated small-stake cycling (e.g. diminishing returns, stake-time-weighted rather than count-weighted history, minimum-stake floors for reputation credit)
Fix approve_extension to use stake-weighted approval (matching vouch_reputation_weight's model) instead of raw voucher-count majority
Add a queryable view surfacing an estimated attack cost for a given borrower's current voucher configuration
Demonstrate before/after attack-cost measurements showing the changes meaningfully raise the cost of reputation/score farming
Confirm existing credit_score.rs and vouch.rs tests continue to pass or document intentional behavior changes; document methodology in docs/economic-security-model.md