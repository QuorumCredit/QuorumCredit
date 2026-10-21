# QuorumCredit Python Client

Python client library for the QuorumCredit API.

> **Note:** This package is **generated** from the canonical
> [`openapi.yaml`](../../openapi.yaml) specification by the generator wired
> into CI. Do not edit the generated sources by hand — changes will be
> overwritten on the next generation run. To change the client, update
> `openapi.yaml` or the generator templates instead.
>
> The package is **not yet published to PyPI**. Until generation is wired into
> CI and a release is cut, `pip install quorum-credit-client` will not work.
> Install from a local checkout instead (see below).

## Installation

From a local checkout of this repository:

```bash
cd generated/python-client
pip install -e .
```

## Usage

```python
from quorum_credit_client import QuorumCreditClient, ClientConfig

client = QuorumCreditClient(
    config=ClientConfig(
        base_url="https://api.quorumcredit.xyz",
        timeout=30,
        headers={
            "X-Client-ID": "your-client-id",
            "X-Client-Signature": "your-signature",
            "X-Request-Timestamp": str(int(time.time() * 1000)),
            "X-Request-Nonce": "your-nonce",
        },
    )
)

# Search for events with facets
results = client.search(
    category="loan",
    action="issued",
    limit=50,
)

# Get webhook stats
stats = client.get_webhook_stats("wh_123456")

# Register a new webhook
webhook = client.register_webhook(
    url="https://example.com/webhook",
    events=["loan_issued", "payment_received"],
)
```

## Features

- Full type hints for better IDE support
- Support for all QuorumCredit API endpoints
- Automatic request signing support
- Built-in error handling
- Configurable timeout and custom headers

## Generation and Testing

The client is produced from `openapi.yaml` by the generator invoked in CI.
The generated package is validated with a smoke import test that ensures the
`quorum_credit_client` module and its public entry points (`QuorumCreditClient`,
`ClientConfig`) can be imported and instantiated. Run it locally with:

```bash
cd generated/python-client
python -m pytest tests/test_smoke_import.py
```

## API Reference

### Search

- `search(**query_params)` - Execute a faceted search with optional filters
- `get_search_stats()` - Get search statistics
- `get_search_patterns(limit)` - Get top search patterns

### Webhooks

- `register_webhook(url, events)` - Register a new webhook
- `list_webhooks()` - List all webhooks
- `get_webhook(webhook_id)` - Get a specific webhook
- `delete_webhook(webhook_id)` - Delete a webhook
- `get_webhook_deliveries(webhook_id)` - Get delivery attempts
- `get_webhook_stats(webhook_id)` - Get delivery statistics

### System

- `get_health()` - Get health status
- `get_metrics()` - Get Prometheus metrics

## Security

This client includes support for zero-trust authentication. Ensure you:

1. Provide valid client credentials
2. Keep your client secret secure
3. Include proper authentication headers
4. Verify SSL/TLS certificates in production

## License

MIT
