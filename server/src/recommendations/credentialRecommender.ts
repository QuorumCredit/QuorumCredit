/**
 * Issue #1778: Credential recommendation engine.
 *
 * Recommends issuers and credential types a holder does not have yet, using
 * item-based collaborative filtering over the credential store:
 *
 *   - Items are `issuer:<name>` and `type:<type>` for each active credential.
 *   - Two items are similar when the same holders hold both (cosine over holder sets).
 *   - A candidate scores the sum of its similarity to the holder's own items.
 *   - A holder with no history falls back to popularity (number of holders with the item).
 *
 * Personalization comes from feedback. Dismissed items are removed for that holder,
 * and each accepted item raises the score of other items of the same kind. Every
 * list served counts as impressions. Feedback is accepted only for items actually
 * shown, so the acceptance rate measures real recommendations.
 *
 * The model is rebuilt from the store on each call, which suits the in-memory store.
 * Per-holder item sets are capped to bound the pairwise work.
 */

import { credentialStore, type Credential, type CredentialStore } from "../credentials/credentialStore.js";

export const MAX_ITEMS_PER_HOLDER = 50;
const DEFAULT_LIMIT = 10;
const MAX_LIMIT = 50;
const ACCEPTED_KIND_BOOST = 0.25;

export type RecommendationKind = "issuer" | "type";

export interface Recommendation {
  itemId: string;
  kind: RecommendationKind;
  value: string;
  score: number;
  reason: "similar_holders" | "popular";
  personalized: boolean;
}

export interface RecommendationList {
  holderId: string;
  recommendations: Recommendation[];
  generatedAt: number;
}

export interface RecommendationEffectiveness {
  impressions: number;
  accepted: number;
  dismissed: number;
  acceptanceRate: number;
}

type Source = Pick<CredentialStore, "getAllCredentials">;

function itemsFor(credential: Credential): string[] {
  return [`issuer:${credential.issuer}`, `type:${credential.type}`];
}

function kindOf(itemId: string): RecommendationKind {
  return itemId.startsWith("issuer:") ? "issuer" : "type";
}

function valueOf(itemId: string): string {
  return itemId.slice(itemId.indexOf(":") + 1);
}

export class CredentialRecommender {
  /** holderId -> items the holder has already been shown and what they did with them */
  private impressions = new Map<string, Set<string>>();
  private feedback = new Map<string, Map<string, "accepted" | "dismissed">>();
  private totals = { impressions: 0, accepted: 0, dismissed: 0 };

  constructor(private readonly source: Source) {}

  recommend(holderId: string, limit = DEFAULT_LIMIT, now = Math.floor(Date.now() / 1000)): RecommendationList {
    const size = Math.min(Math.max(limit, 1), MAX_LIMIT);
    const holders = this.holderItemSets();
    const popularity = this.popularity(holders);
    const ownItems = holders.get(holderId) ?? new Set<string>();
    const dismissed = this.dismissedFor(holderId);
    const accepted = this.acceptedFor(holderId);

    const scores = new Map<string, number>();
    if (ownItems.size > 0) {
      const similarity = this.similarityTo(ownItems, holders, popularity);
      for (const [candidate, score] of similarity) scores.set(candidate, score);
    } else {
      for (const [item, count] of popularity) scores.set(item, count);
    }

    const acceptedKinds = new Map<RecommendationKind, number>();
    for (const item of accepted) {
      const kind = kindOf(item);
      acceptedKinds.set(kind, (acceptedKinds.get(kind) ?? 0) + 1);
    }

    const ranked: Recommendation[] = [];
    for (const [itemId, rawScore] of scores) {
      if (ownItems.has(itemId) || dismissed.has(itemId)) continue;
      const kind = kindOf(itemId);
      const boost = 1 + ACCEPTED_KIND_BOOST * (acceptedKinds.get(kind) ?? 0);
      ranked.push({
        itemId,
        kind,
        value: valueOf(itemId),
        score: Math.round(rawScore * boost * 1e6) / 1e6,
        reason: ownItems.size > 0 ? "similar_holders" : "popular",
        personalized: boost > 1,
      });
    }
    ranked.sort((a, b) => b.score - a.score || a.itemId.localeCompare(b.itemId));
    const recommendations = ranked.slice(0, size);

    this.recordImpressions(holderId, recommendations.map((r) => r.itemId));
    return { holderId, recommendations, generatedAt: now };
  }

  /**
   * Record what a holder did with a recommendation. Only items previously shown to
   * that holder are accepted, so the effectiveness totals reflect real impressions.
   */
  recordFeedback(
    holderId: string,
    itemId: string,
    outcome: "accepted" | "dismissed"
  ): { error: string; status: 400 | 409 } | { holderId: string; itemId: string; outcome: string } {
    if (!this.impressions.get(holderId)?.has(itemId)) {
      return { error: "feedback is only accepted for items recommended to this holder", status: 400 };
    }
    const holderFeedback = this.feedback.get(holderId) ?? new Map();
    this.feedback.set(holderId, holderFeedback);
    const previous = holderFeedback.get(itemId);
    if (previous) {
      return { error: `feedback already recorded as ${previous}`, status: 409 };
    }
    holderFeedback.set(itemId, outcome);
    if (outcome === "accepted") this.totals.accepted++;
    else this.totals.dismissed++;
    return { holderId, itemId, outcome };
  }

  effectiveness(): RecommendationEffectiveness {
    const { impressions, accepted, dismissed } = this.totals;
    return {
      impressions,
      accepted,
      dismissed,
      acceptanceRate: impressions === 0 ? 0 : Math.round((accepted / impressions) * 1e4) / 1e4,
    };
  }

  private holderItemSets(): Map<string, Set<string>> {
    const holders = new Map<string, Set<string>>();
    for (const credential of this.source.getAllCredentials()) {
      if (credential.status !== "active") continue;
      let items = holders.get(credential.holderId);
      if (!items) {
        items = new Set();
        holders.set(credential.holderId, items);
      }
      if (items.size >= MAX_ITEMS_PER_HOLDER) continue;
      for (const item of itemsFor(credential)) items.add(item);
    }
    return holders;
  }

  private popularity(holders: Map<string, Set<string>>): Map<string, number> {
    const counts = new Map<string, number>();
    for (const items of holders.values()) {
      for (const item of items) counts.set(item, (counts.get(item) ?? 0) + 1);
    }
    return counts;
  }

  /**
   * Score every item that co-occurs with the holder's items: the sum over the
   * holder's items of cosine(candidate, owned item).
   */
  private similarityTo(
    owned: Set<string>,
    holders: Map<string, Set<string>>,
    popularity: Map<string, number>
  ): Map<string, number> {
    const cooccurrence = new Map<string, Map<string, number>>();
    for (const items of holders.values()) {
      for (const a of items) {
        if (!owned.has(a)) continue;
        for (const b of items) {
          if (a === b) continue;
          let row = cooccurrence.get(b);
          if (!row) {
            row = new Map();
            cooccurrence.set(b, row);
          }
          row.set(a, (row.get(a) ?? 0) + 1);
        }
      }
    }

    const scores = new Map<string, number>();
    for (const [candidate, row] of cooccurrence) {
      if (owned.has(candidate)) continue;
      let score = 0;
      for (const [ownedItem, together] of row) {
        const cosine = together / Math.sqrt((popularity.get(candidate) ?? 1) * (popularity.get(ownedItem) ?? 1));
        score += cosine;
      }
      scores.set(candidate, score);
    }
    return scores;
  }

  private dismissedFor(holderId: string): Set<string> {
    const out = new Set<string>();
    for (const [item, outcome] of this.feedback.get(holderId) ?? []) {
      if (outcome === "dismissed") out.add(item);
    }
    return out;
  }

  private acceptedFor(holderId: string): Set<string> {
    const out = new Set<string>();
    for (const [item, outcome] of this.feedback.get(holderId) ?? []) {
      if (outcome === "accepted") out.add(item);
    }
    return out;
  }

  private recordImpressions(holderId: string, itemIds: string[]): void {
    let shown = this.impressions.get(holderId);
    if (!shown) {
      shown = new Set();
      this.impressions.set(holderId, shown);
    }
    for (const item of itemIds) {
      if (!shown.has(item)) {
        shown.add(item);
        this.totals.impressions++;
      }
    }
  }
}

export const credentialRecommender = new CredentialRecommender(credentialStore);
