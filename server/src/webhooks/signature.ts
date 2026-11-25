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
import { lookup as dnsLookup } from 'node:dns/promises';
import { isIP } from 'node:net';
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

export interface ResolvedAddress {
  address: string;
  family: 4 | 6;
}

export type DnsLookupFn = (
  hostname: string
) => Promise<Array<{ address: string; family: number }>>;

export interface WebhookUrlValidationOptions {
  /** When true, skip private-host and DNS IP checks. Intended for local/dev environments only. */
  allowPrivateHosts?: boolean;
  /** Optional custom DNS lookup function (defaults to dns.promises.lookup with { all: true }). */
  lookup?: DnsLookupFn;
}

export interface ValidatedWebhookTarget {
  url: URL;
  /** Pinned IP address to connect to (undefined only when allowPrivateHosts=true). */
  address?: string;
  /** IP family (4 or 6) for the pinned address. */
  family?: 4 | 6;
  /** All resolved addresses that passed validation. */
  addresses: ResolvedAddress[];
}

function normalizeHostname(hostname: string): string {
  const lower = hostname.toLowerCase();
  if (lower.startsWith("[") && lower.endsWith("]")) {
    return lower.slice(1, -1);
  }
  return lower;
}

function parseIpv4Octets(ip: string): number[] | null {
  const parts = ip.split(".");
  if (parts.length !== 4) return null;
  const nums = parts.map((p) => Number(p));
  if (nums.some((n) => !Number.isInteger(n) || n < 0 || n > 255)) return null;
  return nums;
}

/**
 * Throws if an IPv4 or IPv6 address is loopback, private, link-local,
 * multicast, or otherwise non-public.
 */
export function assertPublicIpAddress(ip: string, hostname?: string): void {
  const normalized = normalizeHostname(ip);
  const label = hostname && hostname !== normalized ? `${hostname} (${normalized})` : normalized;
  const family = isIP(normalized);

  if (family === 4) {
    const octets = parseIpv4Octets(normalized);
    if (!octets) {
      throw new Error(`invalid IPv4 address: ${label}`);
    }
    const [a, b, c] = octets;

    if (a === 127 || a === 0) {
      throw new Error(`loopback address not allowed: ${label}`);
    }
    if (
      a === 10 ||
      (a === 172 && b >= 16 && b <= 31) ||
      (a === 192 && b === 168) ||
      (a === 169 && b === 254) ||
      (a === 100 && b >= 64 && b <= 127)
    ) {
      throw new Error(`private IP range not allowed: ${label}`);
    }
    if (
      (a === 192 && b === 0 && c === 0) ||
      (a === 192 && b === 0 && c === 2) ||
      (a === 198 && (b === 18 || b === 19)) ||
      (a === 198 && b === 51 && c === 100) ||
      (a === 203 && b === 0 && c === 113) ||
      a >= 224
    ) {
      throw new Error(`reserved IP range not allowed: ${label}`);
    }
    return;
  }

  if (family === 6) {
    // Handle IPv4-mapped IPv6 addresses (e.g. ::ffff:127.0.0.1 or ::ffff:7f00:1)
    if (normalized.startsWith("::ffff:")) {
      const mapped = normalized.slice("::ffff:".length);
      if (isIP(mapped) === 4) {
        assertPublicIpAddress(mapped, hostname);
        return;
      }
      const hexParts = mapped.split(":");
      if (hexParts.length === 2) {
        const hi = parseInt(hexParts[0], 16);
        const lo = parseInt(hexParts[1], 16);
        if (!Number.isNaN(hi) && !Number.isNaN(lo)) {
          const ipv4 = `${(hi >> 8) & 0xff}.${hi & 0xff}.${(lo >> 8) & 0xff}.${lo & 0xff}`;
          assertPublicIpAddress(ipv4, hostname);
          return;
        }
      }
    }

    if (normalized === "::1" || normalized === "::") {
      throw new Error(`loopback address not allowed: ${label}`);
    }
    // Unique local (fc00::/7), link-local (fe80::/10), multicast (ff00::/8)
    const firstHextet = parseInt(normalized.split(":")[0] || "0", 16);
    if (
      (firstHextet & 0xfe00) === 0xfc00 ||
      (firstHextet & 0xffc0) === 0xfe80 ||
      (firstHextet & 0xff00) === 0xff00
    ) {
      throw new Error(`private IP range not allowed: ${label}`);
    }
    return;
  }

  throw new Error(`invalid IP address: ${label}`);
}

const defaultDnsLookup: DnsLookupFn = (hostname) =>
  dnsLookup(hostname, { all: true });

/**
 * Validate a webhook URL and resolve its hostname via DNS, verifying that
 * every returned address is a public IP.
 *
 * Returns the validated URL along with the resolved IP address(es) so callers
 * can pin the connection to the validated address and prevent DNS rebinding.
 */
export async function resolveAndValidateWebhookUrl(
  rawUrl: string,
  options: WebhookUrlValidationOptions = {}
): Promise<ValidatedWebhookTarget> {
  const { allowPrivateHosts = false, lookup = defaultDnsLookup } = options;

  const url = new URL(rawUrl);

  if (!ALLOWED_SCHEMES.has(url.protocol.replace(":", ""))) {
    throw new Error(`unsupported scheme: ${url.protocol}`);
  }

  if (allowPrivateHosts) {
    return { url, addresses: [] };
  }

  const hostname = normalizeHostname(url.hostname);

  if (BLOCKED_HOSTNAMES.has(hostname)) {
    throw new Error(`blocked host: ${hostname}`);
  }

  const ipFamily = isIP(hostname);
  if (ipFamily === 4 || ipFamily === 6) {
    assertPublicIpAddress(hostname);
    const resolved: ResolvedAddress = { address: hostname, family: ipFamily };
    return {
      url,
      address: resolved.address,
      family: resolved.family,
      addresses: [resolved],
    };
  }

  if (hostname.startsWith("0.") || hostname === "0") {
    throw new Error(`loopback address not allowed: ${hostname}`);
  }

  const records = await lookup(hostname);
  if (!records || records.length === 0) {
    throw new Error(`DNS resolution returned no addresses for host: ${hostname}`);
  }

  const validatedAddresses: ResolvedAddress[] = [];
  for (const record of records) {
    assertPublicIpAddress(record.address, hostname);
    const family: 4 | 6 = record.family === 6 ? 6 : 4;
    validatedAddresses.push({ address: record.address, family });
  }

  return {
    url,
    address: validatedAddresses[0].address,
    family: validatedAddresses[0].family,
    addresses: validatedAddresses,
  };
}

/**
 * Validate a webhook registration URL to prevent SSRF attacks.
 *
 * Rejects:
 * - non-http(s) schemes
 * - loopback/link-local/private IPv4 and IPv6 ranges
 * - cloud metadata endpoints
 * - hostnames that resolve via DNS to any private/loopback/reserved IP
 *
 * Returns the normalized URL when valid, or throws with a descriptive message.
 */
export async function validateWebhookUrl(
  rawUrl: string,
  options: WebhookUrlValidationOptions = {}
): Promise<URL> {
  const { url } = await resolveAndValidateWebhookUrl(rawUrl, options);
  return url;
}