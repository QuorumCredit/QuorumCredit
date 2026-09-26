# Troubleshooting Guide

## Quick Troubleshooting Flowchart

```
Problem Occurs
    ↓
[Check Error Code]
    ├─ 400-level? → Client Error (step 1)
    ├─ 401-level? → Auth Error (step 2)
    ├─ 404-level? → Not Found (step 3)
    ├─ 429-level? → Rate Limited (step 4)
    └─ 500-level? → Server Error (step 5)
```

## Step 1: Client Errors (400-449)

### Issue: "invalid request body"

**Symptoms**:
- Getting 400 error with "invalid request body"
- POST requests failing
- JSON is being sent

**Diagnosis**:
```bash
# Test JSON validity
echo '{"borrowerId": "user123"}' | jq .

# Check Content-Type header
curl -i -H "Content-Type: application/json" ...
```

**Solutions**:

1. **Check JSON Syntax**
   ```typescript
   // ❌ Bad - trailing comma
   { "borrowerId": "user123", }
   
   // ✅ Good
   { "borrowerId": "user123" }
   ```

2. **Verify Content-Type**
   ```typescript
   // Ensure header is set
   headers: {
     'Content-Type': 'application/json'
   }
   ```

3. **Validate Before Sending**
   ```typescript
   function validateBatch(data: unknown) {
     try {
       JSON.stringify(data);
       return true;
     } catch (e) {
       console.error('Invalid JSON:', e);
       return false;
     }
   }
   ```

---

### Issue: Missing Required Field

**Symptoms**:
- Error message like "borrowerId required"
- Field is present in request
- Another similar field works

**Diagnosis**:
```typescript
// Check field name spelling
const req = { borrowerId: "123" }; // ✅ Correct
const req = { borrower_id: "123" }; // ❌ Wrong (snake_case)
const req = { borrowerid: "123" };  // ❌ Wrong (lowercase)
```

**Solutions**:

1. **Case Sensitivity**
   ```typescript
   // Always use camelCase for JSON
   ✅ borrowerId, credentialIds, webhookUrl
   ❌ borrower_id, credential_ids, webhook_url
   ```

2. **Array vs Single Value**
   ```typescript
   // ❌ Wrong - string instead of array
   { credentialIds: "cred_123" }
   
   // ✅ Correct - array of strings
   { credentialIds: ["cred_123", "cred_456"] }
   ```

3. **Required vs Optional**
   ```typescript
   // Check API docs - which fields are actually required?
   // webhookUrl is optional, borrowerId is required
   const batch = {
     borrowerId: "user123",           // Required
     credentialIds: ["cred_1"],       // Required
     webhookUrl: "https://..."        // Optional
   };
   ```

---

### Issue: Invalid Parameter Value

**Symptoms**:
- Error like "amount must be a positive number"
- Value looks correct in JSON
- Type checking passes locally

**Diagnosis**:
```typescript
// Check parameter ranges
const amount = -100;      // ❌ Negative
const amount = 0;         // ❌ Zero
const amount = 100;       // ✅ Positive

const tenure = -3600;     // ❌ Negative
const tenure = 0;         // ❌ Zero
const tenure = 86400;     // ✅ Positive
```

**Solutions**:

1. **Validate Ranges Locally**
   ```typescript
   function validateAmount(amount: number) {
     if (!Number.isFinite(amount)) {
       throw new Error('Amount must be a finite number');
     }
     if (amount <= 0) {
       throw new Error('Amount must be positive');
     }
     return true;
   }
   ```

2. **Type Validation**
   ```typescript
   // Use Zod or similar
   const schema = z.object({
     amount: z.number().positive(),
     tenure: z.number().positive().int(),
   });
   ```

---

## Step 2: Authentication Errors (401)

### Issue: "too many failed attempts"

**Symptoms**:
- Getting 401 repeatedly
- API key appears correct
- Was working before, now failing
- Multiple requests failing from same IP

**Diagnosis**:
```bash
# Test API key validity
curl -X POST https://api.quorum.local/api/auth/token \
  -H "Content-Type: application/json" \
  -d '{"apiKey": "your-key-here"}'

# Check rate limiting
# Count failed attempts in last 60 seconds
grep "401" logfile | tail -10
```

**Solutions**:

1. **Verify API Key**
   ```typescript
   // ✅ Correct format
   "sk_live_abc123def456..."
   
   // ❌ Wrong - incomplete key
   "sk_live_abc123"
   
   // ❌ Wrong - whitespace
   " sk_live_abc123def456... "
   ```

2. **Rate Limit Recovery**
   ```typescript
   // Wait 60 seconds before retry
   async function getTokenWithWait() {
     try {
       return await requestToken();
     } catch (err) {
       if (err.status === 401) {
         console.log('Rate limited, waiting 60s...');
         await sleep(60000);
         return await requestToken();
       }
       throw err;
     }
   }
   ```

3. **Rotate API Keys**
   - Generate new API key in dashboard
   - Update environment variable
   - Delete old key after confirmation
   - Verify new key works

---

### Issue: Token Expired or Invalid

**Symptoms**:
- 401 error on authenticated endpoint
- Token was working, now failing
- Different error than "too many attempts"

**Solutions**:

1. **Request New Token**
   ```typescript
   const token = await fetch('/api/auth/token', {
     method: 'POST',
     headers: { 'Content-Type': 'application/json' },
     body: JSON.stringify({ apiKey: process.env.API_KEY })
   });
   ```

2. **Token Caching**
   ```typescript
   class TokenManager {
     private token?: string;
     private expiresAt = 0;
     
     async getToken() {
       if (this.token && Date.now() < this.expiresAt) {
         return this.token;
       }
       // Request new token before expiry
       const newToken = await this.requestToken();
       this.token = newToken;
       this.expiresAt = Date.now() + 23 * 60 * 60 * 1000; // 23h
       return this.token;
     }
   }
   ```

---

## Step 3: Not Found Errors (404)

### Issue: "batch not found"

**Symptoms**:
- Can create batch but cannot retrieve it
- Batch ID looks correct
- Other recent batches work

**Diagnosis**:
```bash
# List available batches
curl "https://api.quorum.local/batch-verification/borrower?borrowerId=user123"

# Check batch was actually created
grep "batch_" logfile | grep "created"

# Verify batch ID spelling
echo "batch_1234567890" | wc -c  # Should be reasonable length
```

**Solutions**:

1. **Verify Batch Exists**
   ```typescript
   async function getBatchSafe(batchId: string) {
     try {
       const batch = await fetch(`/batch-verification/${batchId}`);
       if (batch.status === 404) {
         // List available batches
         const batches = await fetch(
           `/batch-verification/borrower?borrowerId=${borrowerId}`
         );
         console.log('Available batches:', batches);
       }
       return batch;
     } catch (err) {
       // Handle error
     }
   }
   ```

2. **Check Batch Expiration**
   ```typescript
   // Batches expire after 24 hours
   const createdTime = batch.createdAt;
   const now = Date.now();
   const ageHours = (now - createdTime) / (1000 * 60 * 60);
   
   if (ageHours > 24) {
     console.log('Batch has expired, create new one');
   }
   ```

3. **List Borrower's Batches**
   ```typescript
   // If you don't remember batch ID
   const response = await fetch(
     `/batch-verification/borrower?borrowerId=user123&limit=50`
   );
   const batches = await response.json();
   // Use most recent batch or recreate
   ```

---

### Issue: "report not found"

**Symptoms**:
- Just generated report but can't retrieve it
- Report ID looks correct
- Older reports work fine

**Diagnosis**:
```bash
# Check report history
curl "https://api.quorum.local/audit-report/history?limit=50"

# Verify report format
# Reports older than 90 days are purged
```

**Solutions**:

1. **Regenerate Report**
   ```typescript
   async function getOrGenerateReport(reportId: string) {
     try {
       return await fetch(`/audit-report/view?reportId=${reportId}`);
     } catch (err) {
       if (err.status === 404) {
         // Generate new report
         const newReport = await fetch('/audit-report/generate', {
           method: 'POST',
           body: JSON.stringify({ format: 'json' })
         });
         return newReport;
       }
       throw err;
     }
   }
   ```

2. **Check Report History**
   ```typescript
   // Find available reports
   const history = await fetch('/audit-report/history?limit=10');
   const reports = await history.json();
   
   reports.forEach(report => {
     const ageMs = Date.now() - report.generatedAt;
     const ageDays = ageMs / (1000 * 60 * 60 * 24);
     console.log(`Report ${report.reportId}: ${ageDays.toFixed(1)} days old`);
   });
   ```

---

## Step 4: Rate Limiting (429)

### Issue: "too many requests"

**Symptoms**:
- Getting 429 errors suddenly
- Was making many requests in short time
- All requests failing with same error
- Other users not affected

**Diagnosis**:
```bash
# Count requests in last minute
curl -i https://api.quorum.local/batch-verification/create \
  2>&1 | grep "429"

# Check if requests are from same IP
netstat -an | grep ESTABLISHED | grep :443 | wc -l
```

**Solutions**:

1. **Implement Exponential Backoff**
   ```typescript
   async function withExponentialBackoff<T>(
     fn: () => Promise<T>,
     maxAttempts = 5
   ): Promise<T> {
     for (let attempt = 0; attempt < maxAttempts; attempt++) {
       try {
         return await fn();
       } catch (err: any) {
         if (err.status !== 429) throw err;
         if (attempt === maxAttempts - 1) throw err;
         
         const delayMs = Math.min(
           1000 * Math.pow(2, attempt),
           60000 // Max 60s
         );
         console.log(`Rate limited, waiting ${delayMs}ms...`);
         await new Promise(r => setTimeout(r, delayMs));
       }
     }
     throw new Error('Unreachable');
   }
   ```

2. **Batch Requests**
   ```typescript
   // ❌ Bad - 100 sequential requests
   for (const id of ids) {
     await createBatch(id);
   }
   
   // ✅ Good - batch with 10s delay
   for (let i = 0; i < ids.length; i++) {
     if (i > 0 && i % 10 === 0) {
       await sleep(10000);
     }
     await createBatch(ids[i]);
   }
   ```

3. **Use Scheduled Reports**
   ```typescript
   // Instead of on-demand generation
   const schedule = await fetch('/audit-report/schedule/create', {
     method: 'POST',
     body: JSON.stringify({
       format: 'json',
       frequency: 'daily',
       recipients: ['admin@company.com']
     })
   });
   ```

---

## Step 5: Server Errors (500+)

### Issue: "internal server error"

**Symptoms**:
- Getting 500 error sporadically
- Sometimes works, sometimes fails
- No clear pattern to failures
- Service health check shows "ok"

**Diagnosis**:
```bash
# Check service health
curl https://api.quorum.local/health

# Look for error patterns in logs
grep "500" /var/log/quorum/*.log | tail -20

# Check resource usage
free -h  # Memory
df -h    # Disk
ps aux | grep quorum
```

**Solutions**:

1. **Implement Retry Logic**
   ```typescript
   async function withRetry<T>(
     fn: () => Promise<T>,
     maxRetries = 3
   ): Promise<T> {
     for (let i = 0; i < maxRetries; i++) {
       try {
         return await fn();
       } catch (err: any) {
         // Only retry on 5xx errors
         if (err.status < 500) throw err;
         if (i === maxRetries - 1) throw err;
         
         const delay = Math.pow(2, i) * 1000;
         await new Promise(r => setTimeout(r, delay));
       }
     }
     throw new Error('Unreachable');
   }
   ```

2. **Use Fallback Data**
   ```typescript
   async function getBatchWithFallback(batchId: string) {
     try {
       return await getBatch(batchId);
     } catch (err: any) {
       if (err.status >= 500) {
         const cached = cache.get(batchId);
         if (cached) {
           console.warn('Using cached batch data');
           return cached;
         }
       }
       throw err;
     }
   }
   ```

3. **Queue Requests**
   ```typescript
   class RequestQueue {
     private queue: Array<() => Promise<any>> = [];
     private processing = false;
     
     async enqueue<T>(fn: () => Promise<T>): Promise<T> {
       return new Promise((resolve, reject) => {
         this.queue.push(async () => {
           try {
             resolve(await fn());
           } catch (err) {
             reject(err);
           }
         });
         this.process();
       });
     }
     
     private async process() {
       if (this.processing || this.queue.length === 0) return;
       this.processing = true;
       
       while (this.queue.length > 0) {
         const fn = this.queue.shift()!;
         try {
           await fn();
         } catch (err) {
           console.error('Queue processing error:', err);
         }
         await new Promise(r => setTimeout(r, 1000));
       }
       
       this.processing = false;
     }
   }
   ```

---

### Issue: "service unavailable"

**Symptoms**:
- Getting 503 errors
- Service shows as down
- Errors appeared suddenly
- Other services affected

**Solutions**:

1. **Check Service Status**
   - Visit status page: https://status.quorum.local
   - Check maintenance announcements
   - Join status channel for updates

2. **Implement Graceful Degradation**
   ```typescript
   async function safeOperation() {
     try {
       return await normalPath();
     } catch (err: any) {
       if (err.status === 503) {
         console.log('Service unavailable, using degraded mode');
         return await degradedPath();
       }
       throw err;
     }
   }
   ```

3. **Queue for Retry**
   ```typescript
   // Queue requests until service recovers
   async function enqueueUntilAvailable(fn: () => Promise<any>) {
     let attempts = 0;
     while (attempts < 10) {
       try {
         return await fn();
       } catch (err: any) {
         if (err.status === 503) {
           attempts++;
           const delay = Math.pow(2, attempts) * 5000;
           console.log(`Service unavailable, retrying in ${delay}ms`);
           await new Promise(r => setTimeout(r, delay));
           continue;
         }
         throw err;
       }
     }
     throw new Error('Service recovery timeout');
   }
   ```

---

## Common Patterns & Best Practices

### Pattern 1: Complete Error Handler

```typescript
async function apiCall<T>(
  endpoint: string,
  options?: RequestInit
): Promise<T> {
  const maxRetries = 3;
  
  for (let attempt = 0; attempt < maxRetries; attempt++) {
    try {
      const response = await fetch(endpoint, {
        ...options,
        headers: {
          'Content-Type': 'application/json',
          ...options?.headers,
        },
      });
      
      if (!response.ok) {
        const error = await response.json();
        throw {
          status: response.status,
          message: error.error,
        };
      }
      
      return await response.json();
    } catch (err: any) {
      // Determine if retry is possible
      const shouldRetry =
        (err.status >= 500 || err.status === 429) &&
        attempt < maxRetries - 1;
      
      if (shouldRetry) {
        const delay = Math.pow(2, attempt) * (
          err.status === 429 ? 5000 : 1000
        );
        await new Promise(r => setTimeout(r, delay));
        continue;
      }
      
      throw err;
    }
  }
  
  throw new Error('Max retries exceeded');
}
```

### Pattern 2: Validation Before Submit

```typescript
async function submitBatch(data: unknown) {
  // 1. Validate structure
  const schema = z.object({
    borrowerId: z.string().min(1),
    credentialIds: z.array(z.string()).min(1),
  });
  
  const validated = schema.parse(data);
  
  // 2. Pre-flight checks
  if (validated.credentialIds.length > 1000) {
    throw new Error('Batch too large (max 1000 credentials)');
  }
  
  // 3. Submit with error handling
  return await apiCall('/batch-verification/create', {
    method: 'POST',
    body: JSON.stringify(validated),
  });
}
```

---

## Getting Help

If troubleshooting doesn't resolve the issue:

1. **Gather Information**
   - Request ID (from error response)
   - Timestamp of occurrence
   - Full error message
   - Steps to reproduce
   - Environment (test/prod)

2. **Contact Support**
   - Email: support@quorum.credit
   - Slack: #api-support
   - Portal: https://support.quorum.credit

3. **Share Debug Info**
   ```bash
   # Export logs
   curl https://api.quorum.local/health > health.json
   
   # Share request details
   curl -v https://api.quorum.local/endpoint > request.log 2>&1
   ```

---

*Last Updated: 2026-09-24*
*Version: 1.0.0*
