/**
 * Faceted Search Service for QuorumCredit
 *
 * #1588: Advanced search with facet aggregation, filtering, and pattern tracking.
 *
 * Provides real-time faceted search over indexed events with support for
 * multi-dimensional filtering (by category, action, date range) and search
 * pattern analytics.
 */

import type { IndexedEvent } from "../types.js";
import type { EventStore } from "../bridge/eventStore.js";

export interface FacetValue {
  value: string;
  count: number;
}

export interface SearchFacet {
  name: string;
  values: FacetValue[];
}

export interface FacetedSearchResult {
  total: number;
  items: IndexedEvent[];
  facets: SearchFacet[];
  searchId: string;
  executedAt: number;
}

export interface SearchQuery {
  q?: string;
  category?: string;
  action?: string;
  startLedger?: number;
  endLedger?: number;
  limit?: number;
  offset?: number;
}

export interface SearchPattern {
  query: string;
  count: number;
  lastUsedAt: number;
}

/**
 * Faceted search service backed by an event store.
 */
export class FacetedSearchService {
  private readonly eventStore: EventStore;
  private readonly searchPatterns = new Map<string, SearchPattern>();

  constructor(eventStore: EventStore) {
    this.eventStore = eventStore;
  }

  /**
   * Execute a faceted search query.
   *
   * Applies category, action, and ledger range filters, then aggregates results
   * by category and action to produce facets. Tracks the search pattern for analytics.
   */
  search(query: SearchQuery): FacetedSearchResult {
    const searchId = `search_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const executedAt = Date.now();

    this.trackSearchPattern(query);

    const events = this.eventStore.getEventsSince(0);
    const filtered = this.applyFilters(events, query);

    const facets = this.aggregateFacets(filtered);
    const sliced = filtered.slice(query.offset ?? 0, (query.offset ?? 0) + (query.limit ?? 100));

    return {
      total: filtered.length,
      items: sliced,
      facets,
      searchId,
      executedAt,
    };
  }

  /**
   * Get the most popular search patterns.
   */
  getTopSearchPatterns(limit: number = 10): SearchPattern[] {
    return Array.from(this.searchPatterns.values())
      .sort((a, b) => b.count - a.count)
      .slice(0, limit);
  }

  /**
   * Get search pattern stats.
   */
  getSearchStats(): {
    totalUniqueQueries: number;
    totalSearches: number;
    topPatterns: SearchPattern[];
  } {
    const patterns = Array.from(this.searchPatterns.values());
    const totalSearches = patterns.reduce((sum, p) => sum + p.count, 0);

    return {
      totalUniqueQueries: patterns.length,
      totalSearches,
      topPatterns: patterns.sort((a, b) => b.count - a.count).slice(0, 5),
    };
  }

  private applyFilters(events: IndexedEvent[], query: SearchQuery): IndexedEvent[] {
    let filtered = events;

    if (query.category) {
      filtered = filtered.filter((e) => e.category === query.category);
    }

    if (query.action) {
      filtered = filtered.filter((e) => e.action === query.action);
    }

    if (query.startLedger !== undefined) {
      filtered = filtered.filter((e) => e.ledger >= query.startLedger!);
    }

    if (query.endLedger !== undefined) {
      filtered = filtered.filter((e) => e.ledger <= query.endLedger!);
    }

    if (query.q) {
      const q = query.q.toLowerCase();
      filtered = filtered.filter(
        (e) =>
          e.category.toLowerCase().includes(q) ||
          e.action.toLowerCase().includes(q) ||
          JSON.stringify(e.value).toLowerCase().includes(q)
      );
    }

    return filtered;
  }

  private aggregateFacets(events: IndexedEvent[]): SearchFacet[] {
    const categoryFacet = new Map<string, number>();
    const actionFacet = new Map<string, number>();

    for (const event of events) {
      categoryFacet.set(event.category, (categoryFacet.get(event.category) ?? 0) + 1);
      actionFacet.set(event.action, (actionFacet.get(event.action) ?? 0) + 1);
    }

    return [
      {
        name: "category",
        values: Array.from(categoryFacet.entries())
          .map(([value, count]) => ({ value, count }))
          .sort((a, b) => b.count - a.count),
      },
      {
        name: "action",
        values: Array.from(actionFacet.entries())
          .map(([value, count]) => ({ value, count }))
          .sort((a, b) => b.count - a.count),
      },
    ];
  }

  private trackSearchPattern(query: SearchQuery): void {
    const pattern = JSON.stringify(query);
    const existing = this.searchPatterns.get(pattern);

    if (existing) {
      existing.count++;
      existing.lastUsedAt = Date.now();
    } else {
      this.searchPatterns.set(pattern, {
        query: pattern,
        count: 1,
        lastUsedAt: Date.now(),
      });
    }
  }
}
