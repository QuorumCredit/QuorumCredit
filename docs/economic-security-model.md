# Economic Security Model: Sybil-Resistant Vouching & Governance

This document outlines the formal economic security model, cost-of-attack formulations, and agent-based simulation validation for the Sybil-resistant vouching and governance upgrades implemented in QuorumCredit.

---

## 1. Executive Summary

QuorumCredit relies on credit score calculations and voucher reputation weights to determine borrower eligibility, interest rates, and loan extension approvals. 

In the **Legacy Model**:
- Vouching scores and reputation multipliers rewarded raw counts.
- Any attacker could create a Sybil ring of $N$ addresses, vouch for one another with trivial stake ($1\text{ stroop}$), take micro-loans, and immediately repay them to farm high reputation multipliers and credit scores.
- Governance for loan term extensions (`approve_extension`) was based on raw voucher counts, enabling a Sybil ring to override large legitimate stakeholders at near-zero cost.

In the **Redesigned Model**:
- **Trivial-Stake Floors**: Vouches must meet a minimum stake floor of $0.1\text{ XLM}$ ($1,000,000\text{ stroops}$) to count towards credit scoring or reputation.
- **Vouch Age Cooldown**: Vouches must age at least $24\text{ hours}$ before contributing to credit scores.
- **Diminishing Returns (Stake-Time Weighting)**: Vouching score increases with the square root of stake-time weight ($\sqrt{\text{stake} \times \text{age}}$). Reputation multiplier increases with the square root of total yield earned ($\sqrt{\text{yield\_earned}}$).
- **Stake-Weighted Governance**: Extension approvals require a majority of reputation-weighted stake, matching the protocol's core stake-weighted model.

---

## 2. Threat Model & Cost-of-Attack Formulations

Let:
- $C_{\text{addr}} = 1.5\text{ XLM}$ be the Stellar account creation reserve cost (1.0 XLM minimum reserve + 0.5 XLM trustline/data reserve).
- $S_i$ be the stake committed by voucher address $i$ (in XLM).
- $A_i$ be the age of vouch $i$ (in days).
- $Y_v$ be the aggregate yield earned by voucher $v$ (in XLM).
- $R_{\text{target}}$ be the target reputation weight multiplier (range $1.0\times$ to $2.0\times$).
- $CS_{\text{target}}$ be the target vouching credit score component (range $0$ to $1000$).

### 2.1 Credit Score Farming Cost

To farm a vouching score of $CS_{\text{target}}$:

#### Legacy Model
$$CS_{\text{legacy}} = \min\left(\frac{N}{20} \times 1000, 1000\right)$$
An attacker needs $N = \lceil \frac{CS_{\text{target}}}{1000} \times 20 \rceil$ Sybil addresses. Since there is no stake floor, they stake $S_i = 1\text{ stroop} \approx 0$.
$$\text{Cost}_{\text{legacy}}(CS_{\text{target}}) = N \times C_{\text{addr}}$$

#### Redesigned Model
$$CS_{\text{redesign}} = \min\left(\frac{\sum \min(\sqrt{10 \times S_i \times A_i}, 100)}{100} \times 1000, 1000\right)$$
Under the floors, $S_i \ge 0.1\text{ XLM}$ and $A_i \ge 1\text{ day}$. To minimize capital outlay, the attacker has two main strategies:
1. **Capital-Efficient Strategy (Many cheap accounts)**: Field $N = \lceil \frac{CS_{\text{target}}}{10} \rceil$ accounts, each staking $0.1\text{ XLM}$ for $1\text{ day}$. 
   $$\text{Cost}_{\text{redesign, Cap-Eff}} = N \times (C_{\text{addr}} + 0.1)\text{ XLM}$$
2. **Account-Efficient Strategy (Single large stake)**: Field $1$ account, stake $S_{\text{xlm}} = \frac{1}{10} \left(\frac{CS_{\text{target}}}{10}\right)^2$ for $1\text{ day}$.
   $$\text{Cost}_{\text{redesign, Acc-Eff}} = C_{\text{addr}} + S_{\text{xlm}}$$

The attacker's optimal strategy is:
$$\text{Cost}_{\text{redesign}}(CS_{\text{target}}) = \min\left(\text{Cost}_{\text{redesign, Cap-Eff}}, \text{Cost}_{\text{redesign, Acc-Eff}}\right)$$

### 2.2 Reputation Multiplier Farming Cost

To farm a reputation multiplier $R_{\text{target}}$ ($R_{\text{target}} = 1.0 + \text{bonus\_bps} / 10000$):

#### Legacy Model
Each successful vouch (trivial micro-loan repayment cycle) adds $500\text{ bps}$ ($5\%$) to the multiplier, up to $10,000\text{ bps}$ ($100\%$ bonus or $2.0\times$ multiplier).
An attacker needs $N = \lceil \frac{\text{bonus\_bps}}{500} \rceil$ cycles. Since they can cycle trivial loans ($1\text{ stroop}$) through a single Sybil address immediately, there is zero fee/interest cost.
$$\text{Cost}_{\text{legacy}}(R_{\text{target}}) = C_{\text{addr}}$$

#### Redesigned Model
The bonus is proportional to $\sqrt{Y_v / 0.1}$.
$$\text{bonus\_bps} = \min\left(\sqrt{\frac{Y_v}{0.1}} \times \frac{10000}{200}, 10000\right)$$
To achieve $R_{\text{target}}$, the voucher must accumulate yield $Y_v$ (in XLM):
$$Y_v = 0.1 \times \left(\frac{\text{bonus\_bps} \times 200}{10000}\right)^2$$
Since $90\%$ of interest paid goes to vouchers as yield ($Y_v = \text{Interest\_Paid} \times 0.9$), the attacker must pay a direct, non-recoverable financial cost in interest:
$$\text{Interest\_Paid} = \frac{Y_v}{0.9}$$
The floor requires $Y_v \ge 0.1\text{ XLM}$. The total sunk cost is:
$$\text{Cost}_{\text{redesign}}(R_{\text{target}}) = C_{\text{addr}} + \text{Interest\_Paid}$$

### 2.3 Governance Override Cost

To override a legitimate voucher holding $S_{\text{real}}$ weighted stake in a deadline extension request:

#### Legacy Model
Approval is based on raw count: $\lfloor N_{\text{vouchers}} / 2 \rfloor + 1$. An attacker only needs $2$ Sybil accounts to outvote a single real voucher.
$$\text{Cost}_{\text{legacy}}(S_{\text{real}}) = 2 \times C_{\text{addr}} \approx 3.0\text{ XLM}$$

#### Redesigned Model
Approval is stake-weighted. Attacker must field sybils with total stake $S_{\text{attacker}} > S_{\text{real}}$. Since new sybils have no yield history, their weight is $1.0\times$ base.
Each address must stake $S_i \ge 0.1\text{ XLM}$. Thus, the attacker needs $N = \lceil \frac{S_{\text{real}} + 0.1}{0.1} \rceil$ addresses.
$$\text{Capital}_{\text{redesign}}(S_{\text{real}}) = S_{\text{attacker}} + N \times C_{\text{addr}}$$

---

## 3. Simulation & Validation Results

The simulation script (`scripts/sybil_simulation.py`) was run to calculate attack costs across realistic targets:

### 3.1 Credit Score Vouching Component Simulation
| Target Score | Legacy Cost (XLM) | Legacy Sybils | Redesign Cost (XLM) | Redesign Sybils | Cost Increase |
|--------------|-------------------|---------------|---------------------|-----------------|---------------|
| 500 (Medium) | 15.0000 XLM       | 10            | 80.0000 XLM         | 50              | **5.3x**      |
| 1000 (Max)   | 30.0000 XLM       | 20            | 160.0000 XLM        | 100             | **5.3x**      |

*Additionally, the redesign requires a mandatory 24-hour lockup period, preventing instant script-based score generation.*

### 3.2 Reputation Multiplier Simulation
| Target Multiplier | Legacy Cost (Sunk XLM) | Legacy Cycles | Redesign Cost (Sunk XLM) | Redesign Interest Paid | Cost Increase |
|-------------------|------------------------|---------------|--------------------------|------------------------|---------------|
| 1.5x (+50% bonus) | 1.5000 XLM             | 10            | 2.6111 XLM               | 1.1111 XLM             | **1.7x**      |
| 2.0x (+100% max)  | 1.5000 XLM             | 20            | 5.9444 XLM               | 4.4444 XLM             | **4.0x**      |

*In the redesign, the cost is a direct sunk cost of interest payments, meaning the attacker cannot recover this capital.*

### 3.3 Governance Extension Override Simulation
| Real Stake | Legacy Cost (XLM) | Legacy Sybils | Redesign Capital (XLM) | Redesign Sybils | Cost Increase (Capital) |
|------------|-------------------|---------------|------------------------|-----------------|-------------------------|
| 10 XLM     | 3.0000 XLM        | 2             | 161.6000 XLM           | 101             | **53.9x**               |
| 50 XLM     | 3.0000 XLM        | 2             | 801.6000 XLM           | 501             | **267.2x**              |
| 200 XLM    | 3.0000 XLM        | 2             | 3201.6000 XLM          | 2001            | **1067.2x**             |

*By shifting governance to stake-weighted voting, overriding a legitimate voucher becomes exponentially more expensive as their stake grows, aligning with the security assumptions of the rest of the protocol.*

---

## 4. Conclusion

The Sybil-resistant vouching and governance upgrades successfully raise the barrier of entry for attackers:
1. **Farming Credit Score** now requires either locking significant capital or managing hundreds of active addresses, raising costs by **5.3x** and imposing a **24-hour time barrier**.
2. **Reputation Farming** forces the attacker to pay real protocol fees and interest, turning a free attack into a **sunk financial cost** up to **4.0x** higher.
3. **Governance Security** scales linearly with legitimate stake, raising the attack capital cost by **up to 1,000x** against large stakeholders, rendering Sybil-ring takeover economically unviable.
