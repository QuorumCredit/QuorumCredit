import { describe, expect, it } from "vitest";
import type { EventStore } from "../src/bridge/eventStore.js";
import { CredentialStore } from "../src/credentials/credentialStore.js";
import {
  executeGraphqlRequest,
  MAX_GRAPHQL_COMPLEXITY,
} from "../src/graphql/api.js";

describe("GraphQL API", () => {
  it("resolves credential and verification data from the existing store", async () => {
    const credentials = new CredentialStore();
    const credential = credentials.issueCredential(
      "holder-graphql",
      "identity",
      "issuer",
      Math.floor(Date.now() / 1_000) + 60,
      { source: "test" }
    );
    const events = {
      getRecentEvents: () => [],
    } as unknown as EventStore;

    const result = await executeGraphqlRequest(
      {
        query: `query FindCredential($id: ID!) {
          credential(id: $id) { id holderId metadata }
        }`,
        variables: { id: credential.id },
      },
      { credentials, events }
    );

    expect(result).toEqual({
      data: {
        credential: {
          id: credential.id,
          holderId: "holder-graphql",
          metadata: { source: "test" },
        },
      },
    });
  });

  it("rejects operations over the configured query complexity ceiling", async () => {
    const credentials = new CredentialStore();
    const result = await executeGraphqlRequest(
      { query: "{ credentials(limit: 100) { id } }" },
      {
        credentials,
        events: { getRecentEvents: () => [] } as unknown as EventStore,
      }
    );

    expect(MAX_GRAPHQL_COMPLEXITY).toBe(100);
    expect(result.errors?.[0]?.message).toContain("exceeds limit 100");
  });

  it("resolves credential verification statistics and bounded indexer events", async () => {
    const credentials = new CredentialStore();
    const credential = credentials.issueCredential(
      "holder-data",
      "professional",
      "issuer",
      Math.floor(Date.now() / 1_000) + 60
    );
    credentials.recordVerification(
      credential.id,
      credential.holderId,
      "passport",
      credential.expiresAt,
      "verified"
    );
    const event = {
      id: 8,
      ledger: 2,
      ledgerClosedAt: "2026-01-01T00:00:00Z",
      txHash: "tx-hash",
      contractId: "contract",
      category: "loan",
      action: "request",
      value: { borrower: "holder-data" },
    };
    const events = {
      getRecentEvents: (limit: number) => [event].slice(0, limit),
    } as unknown as EventStore;

    const result = await executeGraphqlRequest(
      {
        query: `{
          credentials(holderId: "holder-data", limit: 1) { id verification { verificationStatus } }
          verificationStats(holderId: "holder-data") { total verified }
          events(limit: 1) { id category action }
        }`,
      },
      { credentials, events }
    );

    expect(result).toEqual({
      data: {
        credentials: [{ id: credential.id, verification: { verificationStatus: "verified" } }],
        verificationStats: { total: 1, verified: 1 },
        events: [{ id: 8, category: "loan", action: "request" }],
      },
    });
  });

  it("rejects list requests that exceed the complexity budget", async () => {
    const result = await executeGraphqlRequest(
      { query: "{ credentials(limit: 101) { id } }" },
      {
        credentials: new CredentialStore(),
        events: { getRecentEvents: () => [] } as unknown as EventStore,
      }
    );
    expect(result.errors?.[0]?.message).toContain("exceeds limit 100");
  });
});
