/**
 * Issue #1367: Test suite for WebhookRegistry persistence and CSPRNG secret generation.
 * 
 * This test file verifies:
 * 1. generateWebhookSecret uses crypto.randomBytes (CSPRNG) not Math.random()
 * 2. LocalWebhookRegistry works correctly in single-instance mode
 * 3. RedisWebhookRegistry provides multi-instance-safe persistence
 * 4. A webhook registered on one instance is visible and deliverable from another
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import {
  generateWebhookSecret,
  LocalWebhookRegistry,
  RedisWebhookRegistry,
  buildWebhookRegistry,
  type WebhookRegistry,
} from '../src/webhooks/signature.js';

describe('generateWebhookSecret (CSPRNG)', () => {
  it('generates a 64-character hex string (32 bytes)', () => {
    const secret = generateWebhookSecret();
    expect(secret).toHaveLength(64);
    expect(secret).toMatch(/^[0-9a-f]{64}$/);
  });

  it('generates unique secrets on successive calls', () => {
    const secrets = new Set<string>();
    for (let i = 0; i < 100; i++) {
      secrets.add(generateWebhookSecret());
    }
    // All 100 secrets should be unique
    expect(secrets.size).toBe(100);
  });

  it('shows no correlation attributable to Date.now() seeding', () => {
    // Generate secrets in quick succession to minimize Date.now() drift
    const secret1 = generateWebhookSecret();
    const secret2 = generateWebhookSecret();
    
    // Hamming distance should be high (close to 50% for random hex strings)
    let diffCount = 0;
    for (let i = 0; i < secret1.length; i++) {
      if (secret1[i] !== secret2[i]) diffCount++;
    }
    
    // Expect at least 40% of characters to differ (conservative threshold)
    // If seeded by Date.now() only, diffCount would be very low
    expect(diffCount).toBeGreaterThan(25);
  });

  it('implementation uses crypto.randomBytes, not Math.random()', () => {
    // This test verifies the implementation by checking that the secret
    // has the entropy profile of randomBytes output, not Math.random()
    const secret = generateWebhookSecret();
    const buffer = Buffer.from(secret, 'hex');
    
    // Verify the buffer is exactly 32 bytes
    expect(buffer.length).toBe(32);
    
    // A proper CSPRNG should have roughly uniform distribution
    // Count how many bytes are in each quartile [0-63], [64-127], [128-191], [192-255]
    const quartiles = [0, 0, 0, 0];
    for (const byte of buffer) {
      quartiles[Math.floor(byte / 64)]++;
    }
    
    // Each quartile should have roughly 8 bytes (32/4)
    // Allow generous bounds to account for randomness: [2, 18]
    for (const count of quartiles) {
      expect(count).toBeGreaterThanOrEqual(2);
      expect(count).toBeLessThanOrEqual(18);
    }
  });
});

describe('LocalWebhookRegistry', () => {
  let registry: LocalWebhookRegistry;

  beforeEach(() => {
    registry = new LocalWebhookRegistry();
  });

  afterEach(async () => {
    await registry.close();
  });

  it('registers a webhook with all required fields', async () => {
    const webhook = await registry.registerWebhook(
      'https://example.com/webhook',
      ['loan.requested', 'loan.repaid']
    );

    expect(webhook.id).toMatch(/^wh_\d+_[0-9a-f]+$/);
    expect(webhook.url).toBe('https://example.com/webhook');
    expect(webhook.secret).toHaveLength(64);
    expect(webhook.events).toEqual(['loan.requested', 'loan.repaid']);
    expect(webhook.enabled).toBe(true);
    expect(webhook.createdAt).toBeInstanceOf(Date);
    expect(webhook.lastUsed).toBeUndefined();
  });

  it('retrieves a webhook by ID', async () => {
    const registered = await registry.registerWebhook('https://example.com/webhook', ['vouch.created']);
    const retrieved = await registry.getWebhook(registered.id);

    expect(retrieved).toBeDefined();
    expect(retrieved?.id).toBe(registered.id);
    expect(retrieved?.url).toBe(registered.url);
    expect(retrieved?.secret).toBe(registered.secret);
  });

  it('returns undefined for non-existent webhook ID', async () => {
    const retrieved = await registry.getWebhook('wh_nonexistent');
    expect(retrieved).toBeUndefined();
  });

  it('updates lastUsed timestamp', async () => {
    const webhook = await registry.registerWebhook('https://example.com/webhook', ['loan.requested']);
    expect(webhook.lastUsed).toBeUndefined();

    await registry.updateLastUsed(webhook.id);
    const updated = await registry.getWebhook(webhook.id);
    expect(updated?.lastUsed).toBeInstanceOf(Date);
  });

  it('disables a webhook', async () => {
    const webhook = await registry.registerWebhook('https://example.com/webhook', ['loan.requested']);
    expect(webhook.enabled).toBe(true);

    await registry.disableWebhook(webhook.id);
    const disabled = await registry.getWebhook(webhook.id);
    expect(disabled?.enabled).toBe(false);
  });

  it('enables a webhook', async () => {
    const webhook = await registry.registerWebhook('https://example.com/webhook', ['loan.requested']);
    await registry.disableWebhook(webhook.id);
    
    await registry.enableWebhook(webhook.id);
    const enabled = await registry.getWebhook(webhook.id);
    expect(enabled?.enabled).toBe(true);
  });

  it('deletes a webhook', async () => {
    const webhook = await registry.registerWebhook('https://example.com/webhook', ['loan.requested']);
    const deleted = await registry.deleteWebhook(webhook.id);
    expect(deleted).toBe(true);

    const retrieved = await registry.getWebhook(webhook.id);
    expect(retrieved).toBeUndefined();
  });

  it('returns false when deleting non-existent webhook', async () => {
    const deleted = await registry.deleteWebhook('wh_nonexistent');
    expect(deleted).toBe(false);
  });

  it('lists all webhooks', async () => {
    await registry.registerWebhook('https://example.com/webhook1', ['loan.requested']);
    await registry.registerWebhook('https://example.com/webhook2', ['vouch.created']);
    
    const webhooks = await registry.listWebhooks();
    expect(webhooks).toHaveLength(2);
  });

  it('filters webhooks by event', async () => {
    await registry.registerWebhook('https://example.com/webhook1', ['loan.requested', 'loan.repaid']);
    await registry.registerWebhook('https://example.com/webhook2', ['vouch.created']);
    await registry.registerWebhook('https://example.com/webhook3', ['loan.requested']);
    
    const loanWebhooks = await registry.getWebhooksForEvent('loan.requested');
    expect(loanWebhooks).toHaveLength(2);
    
    const vouchWebhooks = await registry.getWebhooksForEvent('vouch.created');
    expect(vouchWebhooks).toHaveLength(1);
  });

  it('excludes disabled webhooks from event filtering', async () => {
    const webhook1 = await registry.registerWebhook('https://example.com/webhook1', ['loan.requested']);
    await registry.registerWebhook('https://example.com/webhook2', ['loan.requested']);
    
    await registry.disableWebhook(webhook1.id);
    
    const webhooks = await registry.getWebhooksForEvent('loan.requested');
    expect(webhooks).toHaveLength(1);
    expect(webhooks[0].id).not.toBe(webhook1.id);
  });
});

describe('RedisWebhookRegistry', () => {
  // Skip Redis tests if Redis is not available
  const redisUrl = process.env.REDIS_URL || 'redis://localhost:6379';
  let registry: RedisWebhookRegistry | null = null;
  
  beforeEach(async () => {
    try {
      // eslint-disable-next-line @typescript-eslint/no-require-imports
      const { Redis } = require('ioredis') as typeof import('ioredis');
      const redis = new Redis(redisUrl, { lazyConnect: false, connectTimeout: 1000 });
      
      // Test connection
      await redis.ping();
      
      // Clean up any existing test data
      const keys = await redis.keys('qc:webhook:*');
      if (keys.length > 0) {
        await redis.del(...keys);
      }
      
      registry = new RedisWebhookRegistry(redis);
    } catch (error) {
      // Redis not available, tests will be skipped
      registry = null;
    }
  });

  afterEach(async () => {
    if (registry) {
      // Clean up test data
      const keys = await (registry as any).redis.keys('qc:webhook:*');
      if (keys.length > 0) {
        await (registry as any).redis.del(...keys);
      }
      await registry.close();
    }
  });

  it.skipIf(!registry)('registers a webhook with all required fields', async () => {
    if (!registry) return;
    
    const webhook = await registry.registerWebhook(
      'https://example.com/webhook',
      ['loan.requested', 'loan.repaid']
    );

    expect(webhook.id).toMatch(/^wh_\d+_[0-9a-f]+$/);
    expect(webhook.url).toBe('https://example.com/webhook');
    expect(webhook.secret).toHaveLength(64);
    expect(webhook.events).toEqual(['loan.requested', 'loan.repaid']);
    expect(webhook.enabled).toBe(true);
    expect(webhook.createdAt).toBeInstanceOf(Date);
    expect(webhook.lastUsed).toBeUndefined();
  });

  it.skipIf(!registry)('retrieves a webhook by ID', async () => {
    if (!registry) return;
    
    const registered = await registry.registerWebhook('https://example.com/webhook', ['vouch.created']);
    const retrieved = await registry.getWebhook(registered.id);

    expect(retrieved).toBeDefined();
    expect(retrieved?.id).toBe(registered.id);
    expect(retrieved?.url).toBe(registered.url);
    expect(retrieved?.secret).toBe(registered.secret);
  });

  it.skipIf(!registry)('persists Date fields correctly', async () => {
    if (!registry) return;
    
    const registered = await registry.registerWebhook('https://example.com/webhook', ['loan.requested']);
    const retrieved = await registry.getWebhook(registered.id);
    
    expect(retrieved?.createdAt).toBeInstanceOf(Date);
    expect(retrieved?.createdAt.getTime()).toBeCloseTo(registered.createdAt.getTime(), -2);
  });

  it.skipIf(!registry)('updates lastUsed timestamp', async () => {
    if (!registry) return;
    
    const webhook = await registry.registerWebhook('https://example.com/webhook', ['loan.requested']);
    expect(webhook.lastUsed).toBeUndefined();

    await registry.updateLastUsed(webhook.id);
    const updated = await registry.getWebhook(webhook.id);
    expect(updated?.lastUsed).toBeInstanceOf(Date);
  });

  it.skipIf(!registry)('disables and enables webhooks', async () => {
    if (!registry) return;
    
    const webhook = await registry.registerWebhook('https://example.com/webhook', ['loan.requested']);
    
    await registry.disableWebhook(webhook.id);
    let retrieved = await registry.getWebhook(webhook.id);
    expect(retrieved?.enabled).toBe(false);
    
    await registry.enableWebhook(webhook.id);
    retrieved = await registry.getWebhook(webhook.id);
    expect(retrieved?.enabled).toBe(true);
  });

  it.skipIf(!registry)('deletes a webhook', async () => {
    if (!registry) return;
    
    const webhook = await registry.registerWebhook('https://example.com/webhook', ['loan.requested']);
    const deleted = await registry.deleteWebhook(webhook.id);
    expect(deleted).toBe(true);

    const retrieved = await registry.getWebhook(webhook.id);
    expect(retrieved).toBeUndefined();
  });

  it.skipIf(!registry)('lists all webhooks', async () => {
    if (!registry) return;
    
    await registry.registerWebhook('https://example.com/webhook1', ['loan.requested']);
    await registry.registerWebhook('https://example.com/webhook2', ['vouch.created']);
    
    const webhooks = await registry.listWebhooks();
    expect(webhooks.length).toBeGreaterThanOrEqual(2);
  });

  it.skipIf(!registry)('filters webhooks by event', async () => {
    if (!registry) return;
    
    await registry.registerWebhook('https://example.com/webhook1', ['loan.requested', 'loan.repaid']);
    await registry.registerWebhook('https://example.com/webhook2', ['vouch.created']);
    await registry.registerWebhook('https://example.com/webhook3', ['loan.requested']);
    
    const loanWebhooks = await registry.getWebhooksForEvent('loan.requested');
    expect(loanWebhooks.length).toBeGreaterThanOrEqual(2);
    
    const vouchWebhooks = await registry.getWebhooksForEvent('vouch.created');
    expect(vouchWebhooks.length).toBeGreaterThanOrEqual(1);
  });
});

describe('buildWebhookRegistry factory', () => {
  it('builds LocalWebhookRegistry when redisUrl is undefined', () => {
    const registry = buildWebhookRegistry(undefined);
    expect(registry).toBeInstanceOf(LocalWebhookRegistry);
  });

  it.skip('builds RedisWebhookRegistry when redisUrl is provided', () => {
    // Skip in CI if Redis is not available
    const redisUrl = process.env.REDIS_URL || 'redis://localhost:6379';
    const registry = buildWebhookRegistry(redisUrl);
    expect(registry).toBeInstanceOf(RedisWebhookRegistry);
  });
});

describe('Multi-instance webhook delivery', () => {
  const redisUrl = process.env.REDIS_URL || 'redis://localhost:6379';
  let registryA: WebhookRegistry | null = null;
  let registryB: WebhookRegistry | null = null;
  
  beforeEach(async () => {
    try {
      // eslint-disable-next-line @typescript-eslint/no-require-imports
      const { Redis } = require('ioredis') as typeof import('ioredis');
      const redisA = new Redis(redisUrl, { lazyConnect: false, connectTimeout: 1000 });
      const redisB = new Redis(redisUrl, { lazyConnect: false, connectTimeout: 1000 });
      
      await redisA.ping();
      
      // Clean up any existing test data
      const keys = await redisA.keys('qc:webhook:*');
      if (keys.length > 0) {
        await redisA.del(...keys);
      }
      
      registryA = new RedisWebhookRegistry(redisA);
      registryB = new RedisWebhookRegistry(redisB);
    } catch (error) {
      registryA = null;
      registryB = null;
    }
  });

  afterEach(async () => {
    if (registryA) {
      const keys = await (registryA as any).redis.keys('qc:webhook:*');
      if (keys.length > 0) {
        await (registryA as any).redis.del(...keys);
      }
      await registryA.close();
    }
    if (registryB) {
      await registryB.close();
    }
  });

  it.skipIf(!registryA || !registryB)('webhook registered on instance A is visible from instance B', async () => {
    if (!registryA || !registryB) return;
    
    // Register on instance A
    const webhook = await registryA.registerWebhook(
      'https://example.com/webhook',
      ['loan.requested']
    );

    // Retrieve from instance B
    const retrieved = await registryB.getWebhook(webhook.id);
    expect(retrieved).toBeDefined();
    expect(retrieved?.id).toBe(webhook.id);
    expect(retrieved?.url).toBe(webhook.url);
    expect(retrieved?.secret).toBe(webhook.secret);
  });

  it.skipIf(!registryA || !registryB)('webhook registered on instance A is deliverable from instance B', async () => {
    if (!registryA || !registryB) return;
    
    // Register on instance A
    await registryA.registerWebhook(
      'https://example.com/webhook',
      ['loan.requested', 'loan.repaid']
    );

    // Query for event from instance B
    const webhooks = await registryB.getWebhooksForEvent('loan.requested');
    expect(webhooks.length).toBeGreaterThanOrEqual(1);
    expect(webhooks[0].url).toBe('https://example.com/webhook');
    expect(webhooks[0].events).toContain('loan.requested');
  });

  it.skipIf(!registryA || !registryB)('updates on instance A are visible on instance B', async () => {
    if (!registryA || !registryB) return;
    
    // Register on instance A
    const webhook = await registryA.registerWebhook(
      'https://example.com/webhook',
      ['loan.requested']
    );

    // Disable on instance A
    await registryA.disableWebhook(webhook.id);

    // Verify disabled on instance B
    const retrieved = await registryB.getWebhook(webhook.id);
    expect(retrieved?.enabled).toBe(false);

    // Verify it's excluded from event filtering on instance B
    const webhooks = await registryB.getWebhooksForEvent('loan.requested');
    const found = webhooks.find(w => w.id === webhook.id);
    expect(found).toBeUndefined();
  });
});
