import math

# Constants
C_ADDR = 1.5  # XLM to stand up a Stellar account (minimum reserve + trustline)
XLM_TO_STROOP = 10_000_000
BPS_DENOMINATOR = 10_000

# Legacy parameters
LEGACY_MAX_VOUCHES = 20
LEGACY_REP_STEP_BPS = 500
LEGACY_MAX_REP_BPS = 10_000

# Redesigned parameters
SYBIL_MIN_STAKE_FOR_CREDIT = 1_000_000 / XLM_TO_STROOP  # 0.1 XLM
SYBIL_MIN_STAKE_FOR_REP = 1_000_000 / XLM_TO_STROOP     # 0.1 XLM
SYBIL_MIN_VOUCH_AGE_SECS = 24 * 60 * 60                  # 24 hours (1 day)
SYBIL_STAKE_TIME_SATURATION = 100
SYBIL_REP_SATURATION = 200
YIELD_BPS = 9000  # 90% of interest goes to vouchers

def simulate_vouching_score(target_score):
    """
    Calculates the minimum cost (in XLM) to achieve a target vouching score.
    """
    # ── Legacy Model ──
    # score = (voucher_count / 20) * 1000
    # To reach target_score, we need N = target_score * 20 / 1000 vouches.
    # No minimum stake, so stake = 1 stroop per address.
    legacy_count = math.ceil((target_score / 1000.0) * LEGACY_MAX_VOUCHES)
    legacy_stake = legacy_count * (1.0 / XLM_TO_STROOP)
    legacy_addr_cost = legacy_count * C_ADDR
    legacy_total_cost = legacy_addr_cost + legacy_stake

    # ── Redesigned Model ──
    # score = (total_weight / SATURATION) * 1000
    # total_weight = sum(sqrt(stake_deci_xlm * age_days))
    # Minimum stake is 0.1 XLM (1,000,000 stroops), meaning stake_deci_xlm >= 1.
    # Minimum age is 1 day, meaning age_days >= 1.
    # To minimize cost, the attacker can:
    # Option 1: Use 1 address, stake a large amount S for 1 day.
    #           contribution = sqrt(S_deci * 1) = S_deci^0.5.
    #           We need contribution = target_weight = target_score / 10.
    #           S_deci = target_weight^2.
    #           S_xlm = S_deci / 10.0.
    #           Cost Option 1 = 1 * C_ADDR + S_xlm.
    # Option 2: Use N addresses, stake minimum (0.1 XLM) for 1 day.
    #           Each contributes sqrt(1 * 1) = 1.
    #           We need N = target_weight = target_score / 10.
    #           Cost Option 2 = N * (C_ADDR + 0.1).
    #
    # Let's compute both options and take the minimum:
    target_weight = target_score / 10.0
    
    # Option 1: Single address with large stake
    opt1_stake_deci = target_weight ** 2
    opt1_stake_xlm = opt1_stake_deci / 10.0
    opt1_total_cost = C_ADDR + opt1_stake_xlm
    
    # Option 2: Multiple addresses with minimum stake
    opt2_count = math.ceil(target_weight)
    opt2_total_cost = opt2_count * (C_ADDR + 0.1)
    
    redesign_total_cost = min(opt1_total_cost, opt2_total_cost)
    redesign_count = 1 if opt1_total_cost < opt2_total_cost else opt2_count
    redesign_stake = opt1_stake_xlm if opt1_total_cost < opt2_total_cost else opt2_count * 0.1

    return {
        "legacy_count": legacy_count,
        "legacy_stake": legacy_stake,
        "legacy_total": legacy_total_cost,
        "redesign_count": redesign_count,
        "redesign_stake": redesign_stake,
        "redesign_total": redesign_total_cost,
    }

def simulate_reputation_multiplier(target_multiplier):
    """
    Calculates the minimum cost (in XLM) to achieve a target reputation multiplier.
    target_multiplier is in float (e.g. 1.5, 2.0)
    """
    target_bps_bonus = int((target_multiplier - 1.0) * BPS_DENOMINATOR)
    
    # ── Legacy Model ──
    # weight_bps = successful_vouches * 500 bps
    # To reach target_bps_bonus, we need N = target_bps_bonus / 500 successful vouches.
    # We can cycle 1-stroop loans through 1 sybil address (repaying instantly).
    # Since we can reuse the same address or do it with 1 address, we need 1 address.
    # Number of cycles = N.
    # Cost = 1 * C_ADDR + 1 stroop stake + N * fee (which is 0 because loan is 1 stroop).
    # Legacy total cost is just the cost of 1 address.
    legacy_count = 1
    legacy_stake = 1.0 / XLM_TO_STROOP
    legacy_fee = 0.0
    legacy_total = C_ADDR
    
    # ── Redesigned Model ──
    # weight_bps = (capped_sqrt * 10000 / SATURATION)
    # capped_sqrt = sqrt(yield_earned_stroops / 1000)
    # To get target_bps_bonus:
    # capped_sqrt = target_bps_bonus * SATURATION / 10000
    # yield_earned_stroops = 1000 * (capped_sqrt ^ 2)
    # yield_earned_xlm = yield_earned_stroops / XLM_TO_STROOP
    # Since yield_earned = interest_paid * 0.90, the interest paid by the attacker is:
    # interest_paid = yield_earned_xlm / 0.90
    # This interest is a direct sunk cost (fees paid to the protocol/vouchers).
    # Also requires the voucher to have earned at least SYBIL_MIN_STAKE_FOR_REP (0.1 XLM) in yield.
    # So effective_yield = max(yield_earned, 0.1 XLM).
    # Let's compute:
    capped_sqrt = (target_bps_bonus * SYBIL_REP_SATURATION) / 10000.0
    yield_earned_stroops = 1000.0 * (capped_sqrt ** 2)
    
    # Floor check
    min_yield_stroops = SYBIL_MIN_STAKE_FOR_REP * XLM_TO_STROOP
    if yield_earned_stroops < min_yield_stroops:
        yield_earned_stroops = min_yield_stroops
        
    yield_earned_xlm = yield_earned_stroops / XLM_TO_STROOP
    interest_paid = yield_earned_xlm / (YIELD_BPS / 10000.0)
    
    # Attacker needs 1 address, stakes >= 0.1 XLM, and pays 'interest_paid' XLM.
    # Note: the stake is recoverable, but the interest paid is a sunk cost.
    # However, to be conservative and realistic, we define the "sunk attack cost" as:
    # Account reserve + interest paid + stake capital cost (say 5% opportunity cost of stake,
    # but here we can just list the interest paid as a direct loss + account reserve).
    # Let's count the total cash out-of-pocket: Account reserve + interest paid.
    redesign_total = C_ADDR + interest_paid
    
    return {
        "legacy_total": legacy_total,
        "legacy_cycles": math.ceil(target_bps_bonus / 500.0),
        "redesign_total": redesign_total,
        "redesign_interest": interest_paid,
    }

def simulate_governance_override(real_stake_xlm):
    """
    Calculates the cost to override a legitimate voucher with real_stake_xlm in a loan extension.
    """
    # ── Legacy Model ──
    # Quorum is raw count: (total_vouchers / 2) + 1
    # If there is 1 real voucher, total_vouchers = 1. Majority = 1.
    # If attacker adds 2 sybil vouchers, total_vouchers = 3. Majority = 2.
    # Attacker can vote with 2 sybil vouchers and approve the extension.
    # Stake required: 2 stroops (essentially 0).
    # Accounts required: 2.
    # Cost = 2 * C_ADDR = 3.0 XLM.
    legacy_count = 2
    legacy_total = legacy_count * C_ADDR
    
    # ── Redesigned Model ──
    # Quorum is stake-weighted: approval_stake * 2 > total_stake
    # Real voucher has real_stake_xlm * 1.0 (assuming base weight 1.0x).
    # Attacker must field sybils whose weighted stake > real_stake.
    # Since new sybils have 0 history, their weight is 1.0x.
    # Attacker must stake > real_stake_xlm in aggregate.
    # Each sybil address must stake at least SYBIL_MIN_STAKE_FOR_CREDIT (0.1 XLM).
    # So attacker needs:
    # Stake = real_stake_xlm + epsilon.
    # Number of addresses = ceil(Stake / 0.1) to meet the floor per address.
    # Let's say epsilon is 0.1 XLM.
    redesign_stake = real_stake_xlm + 0.1
    redesign_count = math.ceil(redesign_stake / 0.1)
    # The attacker's capital requirement is redesign_stake.
    # The non-recoverable account creation cost is redesign_count * C_ADDR.
    # Total capital required (locked up) = redesign_stake + redesign_count * C_ADDR.
    redesign_total_capital = redesign_stake + (redesign_count * C_ADDR)
    
    return {
        "legacy_total_capital": legacy_total,
        "legacy_count": legacy_count,
        "redesign_stake": redesign_stake,
        "redesign_count": redesign_count,
        "redesign_total_capital": redesign_total_capital,
    }

# Run simulations
vouch_500 = simulate_vouching_score(500)
vouch_1000 = simulate_vouching_score(1000)

rep_1_5 = simulate_reputation_multiplier(1.5)
rep_2_0 = simulate_reputation_multiplier(2.0)

gov_10 = simulate_governance_override(10)
gov_50 = simulate_governance_override(50)
gov_200 = simulate_governance_override(200)

# Output results in Markdown
report = f"""# Economic Security Model & Sybil Simulation Results

## 1. Credit Score Vouching Component Simulation (Target Score)
| Target Score | Legacy Cost (XLM) | Legacy Sybils | Redesign Cost (XLM) | Redesign Sybils | Cost Increase |
|--------------|-------------------|---------------|---------------------|-----------------|---------------|
| 500 (Medium) | {vouch_500['legacy_total']:.4f} XLM | {vouch_500['legacy_count']} | {vouch_500['redesign_total']:.4f} XLM | {vouch_500['redesign_count']} | {vouch_500['redesign_total']/vouch_500['legacy_total']:.1f}x |
| 1000 (Max)   | {vouch_1000['legacy_total']:.4f} XLM | {vouch_1000['legacy_count']} | {vouch_1000['redesign_total']:.4f} XLM | {vouch_1000['redesign_count']} | {vouch_1000['redesign_total']/vouch_1000['legacy_total']:.1f}x |

## 2. Reputation Multiplier Simulation (Target Bonus)
| Target Multiplier | Legacy Cost (Sunk XLM) | Legacy Cycles | Redesign Cost (Sunk XLM) | Redesign Interest Paid | Cost Increase |
|-------------------|-----------------------|---------------|--------------------------|------------------------|---------------|
| 1.5x (+50% bonus) | {rep_1_5['legacy_total']:.4f} XLM | {rep_1_5['legacy_cycles']} | {rep_1_5['redesign_total']:.4f} XLM | {rep_1_5['redesign_interest']:.4f} XLM | {rep_1_5['redesign_total']/rep_1_5['legacy_total']:.1f}x |
| 2.0x (+100% max)  | {rep_2_0['legacy_total']:.4f} XLM | {rep_2_0['legacy_cycles']} | {rep_2_0['redesign_total']:.4f} XLM | {rep_2_0['redesign_interest']:.4f} XLM | {rep_2_0['redesign_total']/rep_2_0['legacy_total']:.1f}x |

## 3. Governance Extension Override Simulation (vs Legitimate Stake)
| Real Stake | Legacy Cost (XLM) | Legacy Sybils | Redesign Capital (XLM) | Redesign Sybils | Cost Increase (Capital) |
|------------|-------------------|---------------|------------------------|-----------------|-------------------------|
| 10 XLM     | {gov_10['legacy_total_capital']:.4f} XLM | {gov_10['legacy_count']} | {gov_10['redesign_total_capital']:.4f} XLM | {gov_10['redesign_count']} | {gov_10['redesign_total_capital']/gov_10['legacy_total_capital']:.1f}x |
| 50 XLM     | {gov_50['legacy_total_capital']:.4f} XLM | {gov_50['legacy_count']} | {gov_50['redesign_total_capital']:.4f} XLM | {gov_50['redesign_count']} | {gov_50['redesign_total_capital']/gov_50['legacy_total_capital']:.1f}x |
| 200 XLM    | {gov_200['legacy_total_capital']:.4f} XLM | {gov_200['legacy_count']} | {gov_200['redesign_total_capital']:.4f} XLM | {gov_200['redesign_count']} | {gov_200['redesign_total_capital']/gov_200['legacy_total_capital']:.1f}x |

Note: Redesign Capital includes both the required stake (locked capital) and account reservation costs.
"""

print(report)
with open("docs/economic_security_model.md", "w") as f:
    f.write(report)
