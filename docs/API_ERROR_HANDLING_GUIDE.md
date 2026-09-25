# API Error Handling Guide

## Overview

This guide provides comprehensive documentation on QuorumCredit API error codes, handling strategies, and recovery procedures. Understanding these error codes will help you implement robust error handling in your applications.

## Error Code Reference

### Authentication & Authorization Errors (400-401 range)

#### 400 Bad Request
**When**: Invalid request syntax, missing required parameters, or malformed JSON.

**Example Response**:
```json
{
  "error": "invalid request body"
}
```

**Recovery Procedure**:
1. Validate JSON syntax using a JSON validator
2. Check that all required fields are present
3. Verify parameter types match the API specification
4. Retry with corrected parameters

**Related Endpoints**: All POST/PUT endpoints

---

#### 401 Unauthorized
**When**: API key is missing, invalid, or rate-limited.

**Example Response**:
```json
{
  "error": "too many failed attempts — try again later"
}
```

**Recovery Procedure**:
1. Verify API key is correct and not expired
2. Check rate limiting status (HTTP 429)
3. Implement exponential backoff for retries
4. Wait for rate limit window to reset (60 seconds default)
5. Contact support if key is invalid

**Related Endpoints**: `/api/auth/token`

---

### Validation Errors (400 range)

#### 400 Missing Required Parameter
**When**: A required query parameter or request body field is missing.

**Examples**:
- Missing `borrowerId` in batch creation
- Missing `credentialIds` array in verification request
- Missing `amount` in cart operations

**Recovery Procedure**:
1. Review API documentation for required fields
2. Inspect request payload before sending
3. Implement client-side validation before API calls
4. Add field presence checks in request builders

---

#### 400 Invalid Parameter Value
**When**: A parameter value is outside acceptable ranges or invalid.

**Examples**:
- `amount` must be positive: `amount <= 0`
- `tenureSeconds` must be greater than zero
- `batchId` doesn't match expected format

**Recovery Procedure**:
1. Validate parameter ranges before submission
2. Check parameter formatting (IDs, addresses, etc.)
3. Use regex validation for pattern-based fields
4. Implement type checking for numeric fields

---

### Not Found Errors (404)

#### 404 Batch Not Found
**When**: The requested batch ID doesn't exist or has expired.

**Recovery Procedure**:
1. Verify the batchId is correct
2. Check if batch was created in the current time window
3. Query `/batch-verification/borrower` to list valid batches
4. For expired batches (>24h), regenerate reports if needed

---

#### 404 Report Not Found
**When**: The requested report ID doesn't exist or has been purged.

**Recovery Procedure**:
1. Verify reportId is correct
2. Check report history with `/audit-report/history`
3. For archived reports, generate new reports for the desired period
4. Note: Reports are retained for 90 days by default

---

### Rate Limiting (429)

#### 429 Too Many Requests
**When**: Request rate limit exceeded (per-IP or per-key basis).

**Example Response**:
```json
{
  "error": "too many failed attempts — try again later"
}
```

**Recovery Procedure** (Exponential Backoff):
```
Attempt 1: Wait 1 second, retry
Attempt 2: Wait 2 seconds, retry
Attempt 3: Wait 4 seconds, retry
Attempt 4: Wait 8 seconds, retry
Maximum: Wait 60 seconds
```

**Best Practices**:
- Implement request queuing for batch operations
- Space out verification requests across multiple batches
- Use `/batch-verification/progress` polling instead of hammering status endpoints
- Consider using scheduled reports instead of on-demand generation

---

### Server Errors (500)

#### 500 Internal Server Error
**When**: Unexpected server error during processing.

**Common Causes**:
- Database connection failure
- Webhook delivery failure
- Report generation service failure
- Memory or resource exhaustion

**Recovery Procedure**:
1. Check service health at `/health`
2. Review error details in webhook payload (if applicable)
3. Implement retry logic with exponential backoff
4. Log full error context for debugging
5. Contact support with request ID if persists

---

## Error Handling Decision Tree

```
Is the request malformed?
├─ YES (400): Fix syntax/format → Retry
└─ NO: Continue

Is the authentication invalid?
├─ YES (401): Verify API key → Request new token → Retry
└─ NO: Continue

Is the rate limit exceeded?
├─ YES (429): Implement backoff → Retry
└─ NO: Continue

Is the resource not found?
├─ YES (404): Check resource exists → Query alternatives → Regenerate if needed
└─ NO: Continue

Is the server having issues?
├─ YES (5xx): Check health endpoint → Implement retry → Contact support
└─ NO: Unknown error → Check documentation → Contact support
```

## Error Handling Patterns

### Pattern 1: Immediate Retry (Safe Operations)

Use for idempotent operations (GET, HEAD, status checks):

```typescript
async function withRetry<T>(
  fn: () => Promise<T>,
  maxRetries = 3
): Promise<T> {
  for (let i = 0; i < maxRetries; i++) {
    try {
      return await fn();
    } catch (err: any) {
      if (i === maxRetries - 1) throw err;
      
      // Retry on 429 or 5xx errors
      if (err.status === 429 || err.status >= 500) {
        await new Promise(resolve => 
          setTimeout(resolve, Math.pow(2, i) * 1000)
        );
        continue;
      }
      throw err;
    }
  }
  throw new Error("Unreachable");
}

// Usage
const batch = await withRetry(() => 
  fetch(`/batch-verification/${batchId}`)
);
```

### Pattern 2: Fallback Strategy (Degraded Service)

Use when full functionality is unavailable:

```typescript
async function getBatchWithFallback(batchId: string) {
  try {
    // Try primary endpoint
    return await fetchBatch(batchId);
  } catch (err: any) {
    if (err.status === 500) {
      // Fallback to cached data
      const cached = getCachedBatch(batchId);
      if (cached) {
        console.warn("Using cached batch data");
        return cached;
      }
    }
    throw err;
  }
}
```

### Pattern 3: Queue & Retry (Batch Operations)

Use for non-critical batch submissions:

```typescript
class VerificationQueue {
  private queue: PendingVerification[] = [];
  
  async enqueue(credentials: string[]) {
    this.queue.push({ credentials, retries: 0 });
    return this.processQueue();
  }
  
  private async processQueue() {
    while (this.queue.length > 0) {
      const item = this.queue.shift()!;
      try {
        await createBatch(item.credentials);
      } catch (err: any) {
        if (err.status === 429 && item.retries < 3) {
          item.retries++;
          const delay = Math.pow(2, item.retries) * 1000;
          await sleep(delay);
          this.queue.push(item);
        } else {
          this.handleFailure(item);
        }
      }
    }
  }
}
```

## Batch Operation Error Handling

### Partial Batch Failures

When a batch contains 100 credentials and 3 fail to verify:

1. **Expected Behavior**: Batch completes with mixed results
2. **Check Progress**: `GET /batch-verification/progress?batchId=...`
3. **Get Results**: `GET /batch-verification/results?batchId=...`
4. **Retry Failures**: Create new batch with only failed credentials
5. **Report Status**: Include success rate in audit logs

### Complete Batch Failures

When all credentials in a batch fail:

1. **Investigate**: Check webhook error messages
2. **Validate Input**: Verify credential IDs are correctly formatted
3. **Contact Support**: Provide batch ID for investigation
4. **Manual Review**: Export batch results for analysis

## Webhook Error Handling

Webhooks are delivered for batch completion events.

### Webhook Retry Policy

- **Automatic Retries**: 3 attempts over 24 hours
- **Exponential Backoff**: 1s → 10s → 1m delays
- **Verification**: All webhooks are signed with HMAC-SHA256

### Webhook Processing Best Practices

```typescript
async function handleWebhook(event: BatchCompletionEvent) {
  try {
    // 1. Verify webhook signature
    if (!verifySignature(event, webhookSecret)) {
      logger.error("Invalid webhook signature");
      return 401;
    }
    
    // 2. Idempotency: Check if already processed
    if (await isDuplicate(event.batchId)) {
      logger.info("Duplicate webhook, ignoring");
      return 200;
    }
    
    // 3. Process event
    await processBatchCompletion(event);
    
    // 4. Mark as processed
    await recordProcessed(event.batchId);
    
    return 200;
  } catch (err) {
    logger.error("Webhook processing failed", err);
    // Return 5xx to trigger retry
    return 503;
  }
}
```

## Troubleshooting Common Issues

### "Invalid request body"

**Diagnosis**:
- Check JSON syntax
- Verify parameter names match API spec
- Ensure types are correct (string vs number)

**Fix**:
```typescript
// Bad
{ batchId: 123 }  // batchId should be string

// Good
{ batchId: "batch_1234567890" }
```

### "Too many failed attempts"

**Diagnosis**:
- Multiple authentication failures from same IP
- Rate limit on `/api/auth/token` exceeded

**Fix**:
1. Verify API key is correct
2. Wait 60 seconds before retry
3. Implement local request caching
4. Use static tokens instead of requesting new ones

### "Batch not found"

**Diagnosis**:
- Batch ID is incorrect
- Batch has expired from cache (>24h)
- Batch was deleted or cancelled

**Fix**:
1. List available batches: `GET /batch-verification/borrower?borrowerId=...`
2. Create new batch if old one expired
3. Check batch status before assuming it exists

### "Report not found"

**Diagnosis**:
- Report ID is incorrect
- Report was purged (>90 days old)
- Report generation failed

**Fix**:
1. Check report history: `GET /audit-report/history`
2. Regenerate report for desired period
3. Verify format parameter is correct (json/csv/pdf)

## Support & Escalation

### When to Contact Support

- Persistent 5xx errors
- Webhooks not being delivered
- Unexpected behavior not covered in docs
- Rate limiting preventing normal operations
- Need for emergency API key rotation

### Providing Debug Information

When contacting support, include:
1. Request ID (from headers or webhook)
2. Timestamp of occurrence
3. Full error message and stack trace
4. Steps to reproduce
5. Expected vs actual behavior

## Additional Resources

- [API Reference](./openapi.yaml)
- [Batch Verification Guide](./batch-verification.md)
- [Audit Report Guide](./audit-reports.md)
- [Webhook Documentation](./webhook-signature-verification-guide.md)

---

*Last Updated: 2026-09-24*
*Version: 1.0.0*
