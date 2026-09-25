# QuorumCredit API Schema Documentation

This document provides comprehensive documentation for all API response schemas, error codes, and examples.

## Table of Contents

- [Authentication](#authentication)
- [Error Responses](#error-responses)
- [Response Schemas](#response-schemas)
- [Status Codes](#status-codes)
- [Examples](#examples)

## Authentication

All API endpoints (except `/health`, `/metrics`, and `/webhook`) require a JWT token obtained from the `/api/auth/token` endpoint.

### Token Request/Response

**Endpoint:** `POST /api/auth/token`

**Request Schema:**
```json
{
  "apiKey": "string (required)",
  "borrower": "string (optional)"
}
```

**Response Schema (200 OK):**
```json
{
  "token": "string"
}
```

**Error Response (400/401):**
```json
{
  "error": "string"
}
```

---

## Error Responses

All error responses follow the standard `ErrorResponse` schema:

```json
{
  "error": "string"
}
```

### HTTP Status Codes

| Code | Name | Description |
|------|------|-------------|
| 200 | OK | Request successful |
| 201 | Created | Resource created successfully |
| 400 | Bad Request | Invalid request parameters |
| 401 | Unauthorized | Missing or invalid authentication token |
| 403 | Forbidden | Access denied |
| 404 | Not Found | Resource not found |
| 500 | Internal Server Error | Server error occurred |

### Error Messages

| Error | Status | Description | Solution |
|-------|--------|-------------|----------|
| `Invalid API key` | 401 | API key is invalid or missing | Check your API key configuration |
| `Token expired` | 401 | JWT token has expired | Request a new token |
| `Webhook not found` | 404 | Webhook with ID does not exist | Verify webhook ID |
| `Loan not found` | 404 | Loan with ID does not exist | Verify loan ID |
| `Invalid request body` | 400 | Request parameters failed validation | Check parameter types and required fields |
| `Failed to deliver webhook` | 500 | Webhook delivery failed | Check webhook URL and retry |
| `Forecast generation error` | 500 | Forecast calculation failed | Check loan parameters |

---

## Response Schemas

### Health Response

**Schema:**
```json
{
  "status": "string (enum: 'ok')"
}
```

**Example (200 OK):**
```json
{
  "status": "ok"
}
```

---

### Expense Schema

**Properties:**
| Property | Type | Description |
|----------|------|-------------|
| id | string | Unique expense identifier |
| loanId | string | Associated loan identifier |
| category | string | Category (business, education, healthcare, other) |
| amount | number | Amount in stroops |
| description | string | Optional description |
| createdAt | integer (int64) | Unix epoch milliseconds |

**Example:**
```json
{
  "id": "exp_123456",
  "loanId": "loan_789",
  "category": "education",
  "amount": 50000,
  "description": "Tuition payment",
  "createdAt": 1696156800000
}
```

---

### Recurring Payment Schedule

**Properties:**
| Property | Type | Description |
|----------|------|-------------|
| loanId | string | Associated loan identifier |
| amount | number | Payment amount in stroops |
| frequencySeconds | number | Frequency in seconds |
| startDate | integer (int64) | Start date as Unix epoch milliseconds |
| active | boolean | Whether schedule is active |
| successRateBps | integer | Success rate in basis points (0-10000) |

**Example (200 OK):**
```json
{
  "loanId": "loan_123",
  "amount": 100000,
  "frequencySeconds": 2592000,
  "startDate": 1696156800000,
  "active": true,
  "successRateBps": 9500
}
```

---

### Webhook Registration

**Properties:**
| Property | Type | Description | Notes |
|----------|------|-------------|-------|
| id | string | Webhook identifier | Returned only on creation |
| url | string (URI) | Webhook endpoint URL | |
| events | array[string] | Events to subscribe to | See webhook events list |
| secret | string | Signing secret | Only returned at registration |
| enabled | boolean | Whether webhook is active | Default: true |
| createdAt | integer (int64) | Creation timestamp | Unix epoch ms |

**Supported Events:**
- `loan.requested` - Loan request created
- `loan.disbursed` - Loan disbursed
- `loan.repaid` - Loan repaid
- `loan.defaulted` - Loan defaulted
- `vouch.created` - Vouch created
- `vouch.withdrawn` - Vouch withdrawn
- `slash.executed` - Slash executed
- `config.updated` - Configuration updated

**Example (201 Created):**
```json
{
  "id": "wh_abc123def456",
  "url": "https://example.com/webhook",
  "events": ["loan.disbursed", "loan.repaid"],
  "secret": "whsec_1234567890abcdef",
  "enabled": true,
  "createdAt": 1696156800000
}
```

---

### Webhook Delivery Statistics

**Properties:**
| Property | Type | Description |
|----------|------|-------------|
| webhookId | string | Webhook identifier |
| totalAttempts | integer | Total delivery attempts |
| successCount | integer | Successful deliveries |
| failureCount | integer | Failed deliveries |
| successRateBps | integer | Success rate in basis points (0-10000) |

**Example (200 OK):**
```json
{
  "webhookId": "wh_abc123def456",
  "totalAttempts": 150,
  "successCount": 147,
  "failureCount": 3,
  "successRateBps": 9800
}
```

---

### Loan Forecast Response

**Properties:**
| Property | Type | Description |
|----------|------|-------------|
| loanId | string | Loan identifier |
| principal | number | Loan principal in stroops |
| interestRateBps | number | Interest rate in basis points |
| termDays | number | Loan term in days |
| totalPayments | integer | Total number of payments |
| schedule | array[ForecastPayment] | Payment schedule array |
| accuracy | object | Historical accuracy metrics |

**Forecast Payment Object:**
| Property | Type | Description |
|----------|------|-------------|
| paymentNumber | integer | Payment sequence number |
| dueDate | integer (int64) | Due date as Unix epoch milliseconds |
| principalComponent | number | Principal portion of payment |
| interestComponent | number | Interest portion of payment |
| totalPayment | number | Total payment amount |
| remainingBalance | number | Remaining loan balance |

**Example (200 OK):**
```json
{
  "loanId": "loan_456",
  "principal": 1000000,
  "interestRateBps": 500,
  "termDays": 365,
  "totalPayments": 12,
  "schedule": [
    {
      "paymentNumber": 1,
      "dueDate": 1698748800000,
      "principalComponent": 83000,
      "interestComponent": 4167,
      "totalPayment": 87167,
      "remainingBalance": 917000
    }
  ],
  "accuracy": {
    "meanErrorBps": 50,
    "sampleCount": 5
  }
}
```

---

### Forecast Accuracy Response

**Properties:**
| Property | Type | Description |
|----------|------|-------------|
| loanId | string | Loan identifier |
| meanErrorBps | number | Mean error in basis points |
| sampleCount | integer | Number of samples |

**Example (201 Created):**
```json
{
  "loanId": "loan_456",
  "meanErrorBps": 45,
  "sampleCount": 6
}
```

---

### Webhook Delivery Attempt

**Properties:**
| Property | Type | Description |
|----------|------|-------------|
| webhookId | string | Webhook identifier |
| event | string | Event type |
| attemptedAt | integer (int64) | Attempt timestamp (Unix epoch ms) |
| statusCode | integer | HTTP status code from delivery |
| ok | boolean | Whether delivery succeeded |
| error | string | Error message if failed |

**Example:**
```json
{
  "webhookId": "wh_abc123def456",
  "event": "loan.disbursed",
  "attemptedAt": 1696243200000,
  "statusCode": 200,
  "ok": true,
  "error": null
}
```

---

## Common Request/Response Patterns

### Success Response Pattern
```
HTTP/1.1 200 OK
Content-Type: application/json

{
  "property1": "value1",
  "property2": "value2",
  ...
}
```

### Created Response Pattern
```
HTTP/1.1 201 Created
Content-Type: application/json
Location: /resource/{id}

{
  "id": "resource_id",
  ...
}
```

### Error Response Pattern
```
HTTP/1.1 4xx/5xx
Content-Type: application/json

{
  "error": "Error description"
}
```

---

## Validation Rules

### Amount Fields
- Must be positive numbers
- Cannot be zero
- Stored as integers (stroops)

### Category Fields (Expenses)
- Valid values: `business`, `education`, `healthcare`, `other`

### Webhook URLs
- Must be valid HTTPS URL
- Must be reachable and return 2xx status
- Will be retried up to 5 times on failure

### Frequency Fields
- Must be positive integers
- Measured in seconds
- Minimum: 1 second

### Date Fields
- Unix epoch milliseconds
- Required format: `int64`

---

## Rate Limiting

Currently there are no rate limits enforced on the API. This may change in future versions.

---

## Pagination

List endpoints (e.g., webhook deliveries) return all available results. Pagination may be added in future versions.

---

## Versioning

API Version: `2.0.0`

The API uses semantic versioning. Breaking changes will increment the major version.
