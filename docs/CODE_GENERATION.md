# API Client Code Generation

Issue #1587: Auto-generated client libraries for TypeScript and Python

## Overview

The QuorumCredit API provides auto-generated client libraries in TypeScript and Python, built from the OpenAPI specification (`openapi.yaml`).

## Generating Clients

### Prerequisites

```bash
cd server
npm install
```

### Generate Clients

```bash
npm run generate-clients
```

This generates:
- **TypeScript**: `generated/typescript-client/`
- **Python**: `generated/python-client/`

## TypeScript Client

### Installation

```bash
npm install @quorum-credit/client-typescript
```

### Usage

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

const results = await client.search({ category: "loan", limit: 50 });
```

## Python Client

### Installation

```bash
pip install quorum-credit-client
```

### Usage

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

results = client.search(category="loan", limit=50)
```

## Publishing

### TypeScript to npm

```bash
cd generated/typescript-client
npm publish
```

### Python to PyPI

```bash
cd generated/python-client
python -m build
twine upload dist/*
```

## Updating

When the OpenAPI spec changes:

1. Update `openapi.yaml`
2. Run `npm run generate-clients`
3. Review generated code for breaking changes
4. Bump version in generated clients' package.json/setup.py
5. Publish new versions

## Features

Both generated clients include:

- Full API endpoint coverage
- Type safety (TypeScript) and type hints (Python)
- Error handling
- Configurable timeout and headers
- Support for zero-trust authentication
- Built-in request signing

## Architecture

The code generation script (`scripts/generate-clients.ts`):

1. Reads the OpenAPI specification
2. Generates TypeScript client with full typing
3. Generates Python client with type hints
4. Packages each client with:
   - Package metadata (package.json/setup.py)
   - README with examples
   - Type definitions (.d.ts for TypeScript)

## Maintenance

- Keep OpenAPI spec in sync with actual API
- Test generated clients after generation
- Update client versions before publishing
- Document breaking changes in CHANGELOG
