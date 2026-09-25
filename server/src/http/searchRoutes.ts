/**
 * Search Routes for QuorumCredit
 *
 * Exposes faceted search endpoints for querying indexed events.
 */

import type { IncomingMessage, ServerResponse } from "node:http";
import type { FacetedSearchService, SearchQuery } from "../search/facetedSearch.js";

export interface SearchRoutesContext {
  searchService: FacetedSearchService;
}

/**
 * Handle search-related HTTP requests.
 */
export function handleSearchRequest(
  req: IncomingMessage,
  res: ServerResponse,
  ctx: SearchRoutesContext
): void {
  const url = new URL(req.url ?? "", "http://internal");

  if (req.method === "GET" && url.pathname === "/api/search") {
    void handleSearch(req, res, ctx);
    return;
  }

  if (req.method === "GET" && url.pathname === "/api/search/stats") {
    void handleSearchStats(req, res, ctx);
    return;
  }

  if (req.method === "GET" && url.pathname === "/api/search/patterns") {
    void handleSearchPatterns(req, res, ctx);
    return;
  }

  res.writeHead(404, { "content-type": "application/json" });
  res.end(JSON.stringify({ error: "not found" }));
}

/**
 * Execute a faceted search.
 *
 * Query parameters:
 * - q: Free text search (optional)
 * - category: Filter by event category (optional)
 * - action: Filter by event action (optional)
 * - startLedger: Filter by start ledger (optional)
 * - endLedger: Filter by end ledger (optional)
 * - limit: Result limit (default: 100)
 * - offset: Result offset (default: 0)
 */
async function handleSearch(
  req: IncomingMessage,
  res: ServerResponse,
  ctx: SearchRoutesContext
): Promise<void> {
  try {
    const url = new URL(req.url ?? "", "http://internal");
    const params = url.searchParams;

    const query: SearchQuery = {
      q: params.get("q") ?? undefined,
      category: params.get("category") ?? undefined,
      action: params.get("action") ?? undefined,
      startLedger: params.get("startLedger") ? parseInt(params.get("startLedger")!, 10) : undefined,
      endLedger: params.get("endLedger") ? parseInt(params.get("endLedger")!, 10) : undefined,
      limit: params.get("limit") ? parseInt(params.get("limit")!, 10) : 100,
      offset: params.get("offset") ? parseInt(params.get("offset")!, 10) : 0,
    };

    if (query.limit && query.limit > 1000) query.limit = 1000;
    if (query.offset && query.offset < 0) query.offset = 0;

    const result = ctx.searchService.search(query);

    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(result));
  } catch (error) {
    console.error("Error executing search:", error);
    res.writeHead(500, { "content-type": "application/json" });
    res.end(JSON.stringify({ error: "internal server error" }));
  }
}

/**
 * Get search statistics (unique queries, total searches, top patterns).
 */
async function handleSearchStats(
  _req: IncomingMessage,
  res: ServerResponse,
  ctx: SearchRoutesContext
): Promise<void> {
  try {
    const stats = ctx.searchService.getSearchStats();

    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(stats));
  } catch (error) {
    console.error("Error computing search stats:", error);
    res.writeHead(500, { "content-type": "application/json" });
    res.end(JSON.stringify({ error: "internal server error" }));
  }
}

/**
 * Get the most popular search patterns.
 */
async function handleSearchPatterns(
  req: IncomingMessage,
  res: ServerResponse,
  ctx: SearchRoutesContext
): Promise<void> {
  try {
    const url = new URL(req.url ?? "", "http://internal");
    const limitParam = url.searchParams.get("limit");
    const limit = limitParam ? Math.min(parseInt(limitParam, 10), 100) : 10;

    const patterns = ctx.searchService.getTopSearchPatterns(limit);

    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ patterns, count: patterns.length }));
  } catch (error) {
    console.error("Error fetching search patterns:", error);
    res.writeHead(500, { "content-type": "application/json" });
    res.end(JSON.stringify({ error: "internal server error" }));
  }
}
