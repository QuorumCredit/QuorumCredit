export interface EndpointRateLimitConfig {
  endpoint: string;
  requestsPerMinute: number;
  burst: number;
}

export interface QuorumSliceMember {
  attestor: string;
  reputation: number;
  stake: number;
  region?: string;
}

export interface QuorumDifficultyScore {
  memberCount: number;
  totalStake: number;
  averageReputation: number;
  regionDiversity: number;
  score: number;
}

export interface ReputationDecayConfig {
  decayPerDayBps: number;
  floor: number;
}

const DEFAULT_DECAY: ReputationDecayConfig = {
  decayPerDayBps: 50,
  floor: 100,
};

export class QuorumPolicyStore {
  private readonly rateLimits = new Map<string, EndpointRateLimitConfig>();

  setEndpointRateLimit(config: EndpointRateLimitConfig): EndpointRateLimitConfig {
    if (!config.endpoint.startsWith('/')) throw new Error('endpoint must start with /');
    if (config.requestsPerMinute <= 0) throw new Error('requestsPerMinute must be positive');
    if (config.burst < config.requestsPerMinute) throw new Error('burst must be at least requestsPerMinute');
    this.rateLimits.set(config.endpoint, config);
    return config;
  }

  listEndpointRateLimits(): EndpointRateLimitConfig[] {
    return Array.from(this.rateLimits.values()).sort((a, b) => a.endpoint.localeCompare(b.endpoint));
  }

  scoreSlice(members: QuorumSliceMember[]): QuorumDifficultyScore {
    if (members.length === 0) {
      return { memberCount: 0, totalStake: 0, averageReputation: 0, regionDiversity: 0, score: 0 };
    }

    const totalStake = members.reduce((sum, member) => sum + Math.max(0, member.stake), 0);
    const averageReputation = members.reduce((sum, member) => sum + Math.max(0, member.reputation), 0) / members.length;
    const regions = new Set(members.map((member) => member.region).filter(Boolean));
    const regionDiversity = regions.size / members.length;
    const stakeWeight = Math.min(totalStake / 1_000_000, 1);
    const reputationWeight = Math.min(averageReputation / 1_000, 1);
    const sizeWeight = Math.min(members.length / 7, 1);
    const score = Math.round((stakeWeight * 40 + reputationWeight * 40 + regionDiversity * 10 + sizeWeight * 10) * 100) / 100;

    return { memberCount: members.length, totalStake, averageReputation, regionDiversity, score };
  }

  applyReputationDecay(
    reputation: number,
    inactiveDays: number,
    config: ReputationDecayConfig = DEFAULT_DECAY,
  ): number {
    const decay = reputation * (config.decayPerDayBps / 10_000) * Math.max(0, inactiveDays);
    return Math.max(config.floor, Math.round(reputation - decay));
  }

  evolveComposition(
    current: QuorumSliceMember[],
    candidates: QuorumSliceMember[],
    targetSize = current.length,
  ): QuorumSliceMember[] {
    const byAttestor = new Map<string, QuorumSliceMember>();
    for (const member of [...current, ...candidates]) {
      const existing = byAttestor.get(member.attestor);
      if (!existing || member.reputation + member.stake > existing.reputation + existing.stake) {
        byAttestor.set(member.attestor, member);
      }
    }

    return Array.from(byAttestor.values())
      .sort((a, b) => (b.reputation + b.stake) - (a.reputation + a.stake))
      .slice(0, Math.max(1, targetSize));
  }
}

export const quorumPolicyStore = new QuorumPolicyStore();
