/**
 * Webhook Signature Verification Module
 * 
 * #1082: Add Webhook Signature Verification
 * #1367: Replace Math.random() with crypto.randomBytes; migrate WebhookRegistry to persistent storage
 * 
 * This module implements HMAC-SHA256 signature verification for webhook requests
 * to prevent spoofing attacks.
 */

import { createHmac, timingSafeEqual, randomBytes } from 'node:crypto';
import type { Redis } from 'ioredis';

export interface WebhookRegistration {
  id: string;
  url: string;
  secret: string;
  createdAt: Date;
  lastUsed?: Date;
  events: string[];
  enabled: boolean;
}

export interface WebhookPayload {
  event: string;
  data: any;
  timestamp: number;
  webhookId: string;
}

export interface SignedWebhookRequest {
  url: string;
  payload: WebhookPayload;
  headers: Record<string, string>;
}

/**
 * Generate a new webhook secret (32-byte random hex string)
 * 
 * #1367: Uses crypto.randomBytes (CSPRNG) instead of Math.random().
 */
export function generateWebhookSecret(): string {
  return randomBytes(32).toString('hex'); // 64 hex chars = 32 bytes
}

/**
 * Sign a webhook payload with HMAC-SHA256
 */
export function signWebhookPayload(
  payload: WebhookPayload,
  secret: string
): string {
  const payloadString = JSON.stringify(payload);
  const hmac = createHmac('sha256', secret);
  hmac.update(payloadString);
  return hmac.digest('hex');
}

/**
 * Verify a webhook signature
 */
export function verifyWebhookSignature(
  payload: WebhookPayload,
  signature: string,
  secret: string
): boolean {
  try {
    const expectedSignature = signWebhookPayload(payload, secret);
    return timingSafeEqual(
      Buffer.from(signature, 'hex'),
      Buffer.from(expectedSignature, 'hex')
    );
  } catch (error) {
    return false;
  }
}

/**
 * Create a signed webhook request
 */
export function createSignedWebhookRequest(
  registration: WebhookRegistration,
  event: string,
  data: any
): SignedWebhookRequest {
  const payload: WebhookPayload = {
    event,
    data,
    timestamp: Date.now(),
    webhookId: registration.id,
  };

  const signature = signWebhookPayload(payload, registration.secret);

  return {
    url: registration.url,
    payload,
    headers: {
      'Content-Type': 'application/json',
      'X-Webhook-Event': event,
      'X-Webhook-Timestamp': payload.timestamp.toString(),
      'X-Webhook-Signature': signature,
      'X-Webhook-Signature-Version': 'hmac-sha256',
      'X-Webhook-Id': registration.id,
    },
  };
}

/**
 * Validate incoming webhook request signature
 */
export function validateIncomingWebhook(
  body: any,
  headers: Record<string, string | string[]>,
  secret: string
): { valid: boolean; payload?: WebhookPayload; error?: string } {
  try {
    // Extract signature from headers
    const signature = Array.isArray(headers['x-webhook-signature'])
      ? headers['x-webhook-signature'][0]
      : headers['x-webhook-signature'];

    const timestamp = Array.isArray(headers['x-webhook-timestamp'])
      ? headers['x-webhook-timestamp'][0]
      : headers['x-webhook-timestamp'];

    const webhookId = Array.isArray(headers['x-webhook-id'])
      ? headers['x-webhook-id'][0]
      : headers['x-webhook-id'];

    const event = Array.isArray(headers['x-webhook-event'])
      ? headers['x-webhook-event'][0]
      : headers['x-webhook-event'];

    if (!signature || !timestamp || !webhookId || !event) {
      return {
        valid: false,
        error: 'Missing required headers',
      };
    }

    // Parse timestamp
    const timestampNum = parseInt(timestamp, 10);
    if (isNaN(timestampNum)) {
      return {
        valid: false,
        error: 'Invalid timestamp',
      };
    }

    // Check timestamp freshness (reject requests older than 5 minutes)
    const now = Date.now();
    if (Math.abs(now - timestampNum) > 5 * 60 * 1000) {
      return {
        valid: false,
        error: 'Timestamp too old',
      };
    }

    // Reconstruct payload for verification
    const payload: WebhookPayload = {
      event,
      data: body,
      timestamp: timestampNum,
      webhookId,
    };

    // Verify signature
    if (!verifyWebhookSignature(payload, signature, secret)) {
      return {
        valid: false,
        error: 'Invalid signature',
      };
    }

    return {
      valid: true,
      payload,
    };
  } catch (error) {
    return {
      valid: false,
      error: error instanceof Error ? error.message : 'Unknown error',
    };
  }
}

/**
 * #1367: Persistent, multi-instance-safe webhook registry interface.
 * Follows the same LocalXxx/RedisXxx pattern used in jtiRevocationStore.ts.
 */
export interface WebhookRegistry {
  /**
   * Register a new webhook
   */
  registerWebhook(url: string, events: string[]): Promise<WebhookRegistration>;

  /**
   * Get webhook registration by ID
   */
  getWebhook(id: string): Promise<WebhookRegistration | undefined>;

  /**
   * Update webhook last used timestamp
   */
  updateLastUsed(id: string): Promise<void>;

  /**
   * Disable a webhook
   */
  disableWebhook(id: string): Promise<void>;

  /**
   * Enable a webhook
   */
  enableWebhook(id: string): Promise<void>;

  /**
   * Delete a webhook
   */
  deleteWebhook(id: string): Promise<boolean>;

  /**
   * List all webhooks
   */
  listWebhooks(): Promise<WebhookRegistration[]>;

  /**
   * Get webhooks for a specific event
   */
  getWebhooksForEvent(event: string): Promise<WebhookRegistration[]>;

  /**
   * Close / clean up underlying connections
   */
  close(): Promise<void>;
}

// ── Local (single-instance) implementation ─────────────────────────────────────

/**
 * In-process webhook registry backed by a plain Map.
 * NOT multi-instance-safe — for local dev and unit tests only.
 */
export class LocalWebhookRegistry implements WebhookRegistry {
  private registrations: Map<string, WebhookRegistration> = new Map();

  async registerWebhook(url: string, events: string[]): Promise<WebhookRegistration> {
    const id = `wh_${Date.now()}_${randomBytes(6).toString('hex')}`;
    const secret = generateWebhookSecret();
    
    const registration: WebhookRegistration = {
      id,
      url,
      secret,
      createdAt: new Date(),
      events,
      enabled: true,
    };

    this.registrations.set(id, registration);
    return registration;
  }

  async getWebhook(id: string): Promise<WebhookRegistration | undefined> {
    return this.registrations.get(id);
  }

  async updateLastUsed(id: string): Promise<void> {
    const registration = this.registrations.get(id);
    if (registration) {
      registration.lastUsed = new Date();
      this.registrations.set(id, registration);
    }
  }

  async disableWebhook(id: string): Promise<void> {
    const registration = this.registrations.get(id);
    if (registration) {
      registration.enabled = false;
      this.registrations.set(id, registration);
    }
  }

  async enableWebhook(id: string): Promise<void> {
    const registration = this.registrations.get(id);
    if (registration) {
      registration.enabled = true;
      this.registrations.set(id, registration);
    }
  }

  async deleteWebhook(id: string): Promise<boolean> {
    return this.registrations.delete(id);
  }

  async listWebhooks(): Promise<WebhookRegistration[]> {
    return Array.from(this.registrations.values());
  }

  async getWebhooksForEvent(event: string): Promise<WebhookRegistration[]> {
    return Array.from(this.registrations.values()).filter(
      (reg) => reg.enabled && reg.events.includes(event)
    );
  }

  async close(): Promise<void> {
    this.registrations.clear();
  }
}

// ── Redis (multi-instance) implementation ──────────────────────────────────────

const REDIS_WEBHOOK_PREFIX = 'qc:webhook:';
const REDIS_WEBHOOK_IDS_KEY = 'qc:webhook:ids';

/**
 * Redis-backed webhook registry for multi-instance deployments.
 * 
 * Storage schema:
 *   qc:webhook:{id} → JSON serialized WebhookRegistration
 *   qc:webhook:ids → SET of all webhook IDs
 */
export class RedisWebhookRegistry implements WebhookRegistry {
  constructor(private readonly redis: Redis) {}

  async registerWebhook(url: string, events: string[]): Promise<WebhookRegistration> {
    const id = `wh_${Date.now()}_${randomBytes(6).toString('hex')}`;
    const secret = generateWebhookSecret();
    
    const registration: WebhookRegistration = {
      id,
      url,
      secret,
      createdAt: new Date(),
      events,
      enabled: true,
    };

    await this.redis.set(
      `${REDIS_WEBHOOK_PREFIX}${id}`,
      JSON.stringify(registration)
    );
    await this.redis.sadd(REDIS_WEBHOOK_IDS_KEY, id);
    
    return registration;
  }

  async getWebhook(id: string): Promise<WebhookRegistration | undefined> {
    const raw = await this.redis.get(`${REDIS_WEBHOOK_PREFIX}${id}`);
    if (!raw) return undefined;
    
    const registration = JSON.parse(raw);
    // Parse Date fields back from ISO strings
    registration.createdAt = new Date(registration.createdAt);
    if (registration.lastUsed) {
      registration.lastUsed = new Date(registration.lastUsed);
    }
    return registration;
  }

  async updateLastUsed(id: string): Promise<void> {
    const registration = await this.getWebhook(id);
    if (registration) {
      registration.lastUsed = new Date();
      await this.redis.set(
        `${REDIS_WEBHOOK_PREFIX}${id}`,
        JSON.stringify(registration)
      );
    }
  }

  async disableWebhook(id: string): Promise<void> {
    const registration = await this.getWebhook(id);
    if (registration) {
      registration.enabled = false;
      await this.redis.set(
        `${REDIS_WEBHOOK_PREFIX}${id}`,
        JSON.stringify(registration)
      );
    }
  }

  async enableWebhook(id: string): Promise<void> {
    const registration = await this.getWebhook(id);
    if (registration) {
      registration.enabled = true;
      await this.redis.set(
        `${REDIS_WEBHOOK_PREFIX}${id}`,
        JSON.stringify(registration)
      );
    }
  }

  async deleteWebhook(id: string): Promise<boolean> {
    const deleted = await this.redis.del(`${REDIS_WEBHOOK_PREFIX}${id}`);
    await this.redis.srem(REDIS_WEBHOOK_IDS_KEY, id);
    return deleted > 0;
  }

  async listWebhooks(): Promise<WebhookRegistration[]> {
    const ids = await this.redis.smembers(REDIS_WEBHOOK_IDS_KEY);
    const webhooks: WebhookRegistration[] = [];
    
    for (const id of ids) {
      const webhook = await this.getWebhook(id);
      if (webhook) webhooks.push(webhook);
    }
    
    return webhooks;
  }

  async getWebhooksForEvent(event: string): Promise<WebhookRegistration[]> {
    const all = await this.listWebhooks();
    return all.filter(
      (reg) => reg.enabled && reg.events.includes(event)
    );
  }

  async close(): Promise<void> {
    await this.redis.quit();
  }
}

// ── Factory ────────────────────────────────────────────────────────────────────

/**
 * Build the appropriate webhook registry based on whether Redis is configured.
 * Mirrors the `buildRevocationStore` pattern in auth/jtiRevocationStore.ts.
 */
export function buildWebhookRegistry(redisUrl: string | undefined): WebhookRegistry {
  if (redisUrl) {
    // Lazily import ioredis so it is not required in local/test environments.
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { Redis } = require('ioredis') as typeof import('ioredis');
    const redis = new Redis(redisUrl, { lazyConnect: false });
    return new RedisWebhookRegistry(redis);
  }
  return new LocalWebhookRegistry();
}

/**
 * Singleton instance for convenience.
 * In production, call buildWebhookRegistry with your Redis URL instead.
 * 
 * #1367: This local singleton is preserved for backwards compatibility in tests
 * and local development, but production deployments MUST use buildWebhookRegistry
 * with a Redis URL to achieve multi-instance safety.
 */
export const webhookRegistry = new LocalWebhookRegistry();

// ── SSRF validation (issue #1486) ──────────────────────────────────────────────

const ALLOWED_SCHEMES = new Set(["http", "https"]);
const BLOCKED_HOSTNAMES = new Set([
  "localhost",
  "127.0.0.1",
  "::1",
  "0.0.0.0",
  "169.254.169.254",
  "metadata.google.internal",
  "metadata.internal",
]);

const PRIVATE_IPV4_RE = /^(10\.|172\.(1[6-9]|2\d|3[01])\.|192\.168\.)/;
const LINK_LOCAL_IPV4_RE = /^169\.254\./;

export interface WebhookUrlValidationOptions {
  /** When true, skip scheme/host validation. Intended for local/dev environments only. */
  allowPrivateHosts?: boolean;
}

/**
 * Validate a webhook registration URL to prevent SSRF attacks.
 *
 * Rejects:
 * - non-http(s) schemes
 * - loopback/link-local/private IPv4 ranges
 * - cloud metadata endpoints
 *
 * Returns the normalized URL when valid, or throws with a descriptive message.
 */
export function validateWebhookUrl(rawUrl: string, options: WebhookUrlValidationOptions = {}): URL {
  const { allowPrivateHosts = false } = options;

  const url = new URL(rawUrl);

  if (!ALLOWED_SCHEMES.has(url.protocol.replace(":", ""))) {
    throw new Error(`unsupported scheme: ${url.protocol}`);
  }

  if (allowPrivateHosts) return url;

  const hostname = url.hostname.toLowerCase();

  if (BLOCKED_HOSTNAMES.has(hostname)) {
    throw new Error(`blocked host: ${hostname}`);
  }

  if (PRIVATE_IPV4_RE.test(hostname) || LINK_LOCAL_IPV4_RE.test(hostname)) {
    throw new Error(`private IP range not allowed: ${hostname}`);
  }

  if (hostname === "::1" || hostname.startsWith("0.") || hostname === "0") {
    throw new Error(`loopback address not allowed: ${hostname}`);
  }

  return url;
}