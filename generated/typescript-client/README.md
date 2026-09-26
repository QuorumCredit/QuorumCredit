# QuorumCredit TypeScript Client

Auto-generated TypeScript client library for the QuorumCredit API.

## Installation

```bash
npm install @quorum-credit/client-typescript
```

## Usage

```typescript
import QuorumCreditClient from "@quorum-credit/client-typescript";

const client = new QuorumCreditClient({
  baseUrl: "https://api.quorumcredit.xyz",
  timeout: 30000,
  headers: {
    "X-Client-ID": "your-client-id",
    "X-Client-Signature": "your-signature",
    "X-Request-Timestamp": Date.now().toString(),
    "X-Request-Nonce": "your-nonce",
  },
});

// Search for events with facets
const results = await client.search({
  category: "loan",
  limit: 50,
});

// Get webhook stats
const stats = await client.getWebhookStats("wh_123456");

// Register a new webhook
const webhook = await client.registerWebhook({
  url: "https://example.com/webhook",
  events: ["loan_issued", "payment_received"],
});
```

## Features

- Full TypeScript typing
- Support for all QuorumCredit API endpoints
- Automatic request signing (when headers provided)
- Built-in error handling
- Configurable timeout and custom headers

## API Reference

### Search

- `search(query)` - Execute a faceted search with optional filters
- `getSearchStats()` - Get search statistics
- `getSearchPatterns(limit)` - Get top search patterns

### Webhooks

- `registerWebhook(payload)` - Register a new webhook
- `listWebhooks()` - List all webhooks
- `getWebhook(id)` - Get a specific webhook
- `deleteWebhook(id)` - Delete a webhook
- `getWebhookDeliveries(id)` - Get delivery attempts
- `getWebhookStats(id)` - Get delivery statistics

### System

- `getHealth()` - Get health status
- `getMetrics()` - Get Prometheus metrics

## Security

This client includes support for zero-trust authentication. Ensure you:

1. Provide valid client credentials
2. Keep your client secret secure
3. Include proper authentication headers
4. Verify SSL/TLS certificates in production

## License

MIT
