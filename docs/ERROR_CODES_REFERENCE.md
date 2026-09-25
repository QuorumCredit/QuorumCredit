# Error Codes Reference

## Quick Lookup Table

| Code | HTTP Status | Severity | Category | Retry |
|------|-------------|----------|----------|-------|
| INVALID_REQUEST | 400 | Low | Validation | No |
| MISSING_REQUIRED_FIELD | 400 | Low | Validation | No |
| INVALID_PARAMETER_VALUE | 400 | Low | Validation | No |
| UNAUTHORIZED | 401 | High | Authentication | Yes |
| RATE_LIMITED | 429 | Medium | Throttling | Yes |
| BATCH_NOT_FOUND | 404 | Medium | Not Found | No |
| REPORT_NOT_FOUND | 404 | Medium | Not Found | No |
| INTERNAL_ERROR | 500 | Critical | Server | Yes |
| SERVICE_UNAVAILABLE | 503 | Critical | Server | Yes |

## Detailed Error Codes

### 400 Series - Client Errors

#### INVALID_REQUEST (400)
- **Message**: "invalid request body"
- **Cause**: Malformed JSON or invalid request format
- **HTTP Status**: 400
- **Retry**: No
- **Example**:
  ```json
  {
    "error": "invalid request body"
  }
  ```
- **Resolution**:
  1. Validate JSON syntax
  2. Check Content-Type header is `application/json`
  3. Ensure request body is not empty
  4. Use JSON schema validator

#### MISSING_REQUIRED_FIELD (400)
- **Message**: "{fieldName} required" (e.g., "borrowerId required")
- **Cause**: Required parameter is missing from request
- **HTTP Status**: 400
- **Retry**: No
- **Example**:
  ```json
  {
    "error": "borrowerId required"
  }
  ```
- **Resolution**:
  1. Check API documentation for required fields
  2. Add missing field to request
  3. Validate before sending request
  4. Use TypeScript types/Zod for validation

#### INVALID_PARAMETER_VALUE (400)
- **Message**: "{fieldName} must be {description}"
- **Cause**: Parameter value is invalid (wrong type, out of range)
- **HTTP Status**: 400
- **Retry**: No
- **Examples**:
  ```json
  {
    "error": "amount must be a positive number"
  }
  ```
  ```json
  {
    "error": "tenureSeconds must be a positive number"
  }
  ```
- **Resolution**:
  1. Verify parameter type matches specification
  2. Check numeric values are within valid ranges
  3. Validate array items match expected types
  4. Use type guards in TypeScript

#### UNAUTHORIZED (401)
- **Message**: "too many failed attempts — try again later" or missing API key
- **Cause**: Authentication failure (invalid key, rate limited)
- **HTTP Status**: 401
- **Retry**: Yes (with backoff)
- **Example**:
  ```json
  {
    "error": "too many failed attempts — try again later"
  }
  ```
- **Resolution**:
  1. Verify API key is correct and valid
  2. Check key expiration date
  3. Implement exponential backoff
  4. Contact support if key is valid but rejected

### 400-404 Series - Not Found

#### BATCH_NOT_FOUND (404)
- **Message**: "batch not found"
- **Cause**: Referenced batch ID doesn't exist or has expired
- **HTTP Status**: 404
- **Retry**: No
- **Example**:
  ```json
  {
    "error": "batch not found"
  }
  ```
- **Resolution**:
  1. Verify batch ID is correctly spelled
  2. Query `/batch-verification/borrower` to list valid batches
  3. Create new batch if expired
  4. Check batch expiration (24-hour TTL)

#### REPORT_NOT_FOUND (404)
- **Message**: "report not found"
- **Cause**: Referenced report ID doesn't exist or has been purged
- **HTTP Status**: 404
- **Retry**: No
- **Example**:
  ```json
  {
    "error": "report not found"
  }
  ```
- **Resolution**:
  1. Verify report ID is correct
  2. Check report history: `GET /audit-report/history`
  3. Regenerate report for desired period
  4. Note: Reports older than 90 days are purged

### 429 - Rate Limiting

#### RATE_LIMITED (429)
- **Message**: "too many failed attempts — try again later"
- **Cause**: Request rate limit exceeded
- **HTTP Status**: 429
- **Retry**: Yes (with exponential backoff)
- **Example**:
  ```json
  {
    "error": "too many failed attempts — try again later"
  }
  ```
- **Resolution**:
  1. Implement exponential backoff (1s, 2s, 4s, 8s, 60s max)
  2. Queue requests for batch processing
  3. Use polling instead of hammering status endpoints
  4. Implement per-IP rate limiting client-side
  5. Contact support if legitimate traffic is being blocked

**Rate Limits**:
- Auth endpoint: 10 attempts per IP per minute
- Status endpoints: 100 requests per minute
- Batch creation: 50 batches per minute per borrower

### 500 Series - Server Errors

#### INTERNAL_ERROR (500)
- **Message**: "internal server error"
- **Cause**: Unexpected server-side error
- **HTTP Status**: 500
- **Retry**: Yes (with backoff)
- **Example**:
  ```json
  {
    "error": "internal server error"
  }
  ```
- **Resolution**:
  1. Check service health at `GET /health`
  2. Implement retry logic (max 3 attempts)
  3. Use exponential backoff
  4. Contact support with request ID and timestamp

#### SERVICE_UNAVAILABLE (503)
- **Message**: "service unavailable"
- **Cause**: Server is temporarily unable to process requests
- **HTTP Status**: 503
- **Retry**: Yes (with backoff)
- **Example**:
  ```json
  {
    "error": "service unavailable"
  }
  ```
- **Resolution**:
  1. Check service status page
  2. Implement backoff strategy
  3. Use fallback mechanisms
  4. Queue requests for retry when service recovers

## Error Handling by Operation Type

### Batch Verification Operations

| Scenario | Error | Status | Action |
|----------|-------|--------|--------|
| Invalid credential IDs | INVALID_PARAMETER_VALUE | 400 | Validate IDs before submission |
| Batch expired | BATCH_NOT_FOUND | 404 | Create new batch |
| Rate limited | RATE_LIMITED | 429 | Exponential backoff |
| Service down | SERVICE_UNAVAILABLE | 503 | Retry with backoff |

### Audit Report Operations

| Scenario | Error | Status | Action |
|----------|-------|--------|--------|
| Invalid format | INVALID_PARAMETER_VALUE | 400 | Use json/csv/pdf |
| Report expired | REPORT_NOT_FOUND | 404 | Regenerate report |
| Generation failed | INTERNAL_ERROR | 500 | Retry creation |
| Too many requests | RATE_LIMITED | 429 | Use scheduler instead |

## Error Context & Debugging

### Request ID Tracking

All errors include context in response headers:
```
X-Request-ID: req_1234567890abcdef
X-Trace-ID: trace_abcdef1234567890
```

### Debug Mode

Enable debug logging for more details:
```typescript
// Client-side
const response = await fetch('/api/endpoint', {
  headers: {
    'X-Debug': 'true'
  }
});

// Server-side
process.env.DEBUG = 'quorum-credit:*';
```

### Error Payload Structure

Complete error response format:
```json
{
  "error": "human-readable message",
  "code": "ERROR_CODE",
  "details": {
    "field": "fieldName",
    "value": "invalid value",
    "expected": "expected format"
  },
  "timestamp": 1234567890000,
  "requestId": "req_123456"
}
```

## Preventing Errors

### Input Validation

```typescript
import { z } from 'zod';

const BatchCreateSchema = z.object({
  borrowerId: z.string().min(1),
  credentialIds: z.array(z.string()).min(1),
  webhookUrl: z.string().url().optional(),
});

const validated = BatchCreateSchema.parse(input);
```

### Type Safety

```typescript
interface BatchVerificationRequest {
  borrowerId: string;
  credentialIds: string[];
  webhookUrl?: string;
}

async function createBatch(req: BatchVerificationRequest) {
  // TypeScript ensures all required fields are present
}
```

### Defensive Programming

```typescript
async function safeBatchCheck(batchId: string) {
  try {
    const batch = await getBatch(batchId);
    if (!batch) {
      throw new Error(`Batch ${batchId} not found`);
    }
    return batch;
  } catch (err) {
    if (err instanceof NotFoundError) {
      // Handle gracefully
      return null;
    }
    throw err;
  }
}
```

---

*Last Updated: 2026-09-24*
*Version: 1.0.0*
