import { describe, it, expect, vi } from "vitest";
import {
  validateWebhookUrl,
  resolveAndValidateWebhookUrl,
  type DnsLookupFn,
  type WebhookUrlValidationOptions,
} from "../src/webhooks/signature.js";
import { createPinnedWebhookSender } from "../src/http/webhookRoutes.js";

const publicLookup: DnsLookupFn = async () => [
  { address: "93.184.216.34", family: 4 },
];

describe("validateWebhookUrl", () => {
  const cases: Array<{
    url: string;
    ok: boolean;
    options?: WebhookUrlValidationOptions;
    contains?: string;
  }> = [
    { url: "https://example.com/hook", ok: true, options: { lookup: publicLookup } },
    { url: "http://example.com/hook", ok: true, options: { lookup: publicLookup } },
    { url: "https://sub.example.com/hook", ok: true, options: { lookup: publicLookup } },
    { url: "ftp://example.com/hook", ok: false, contains: "unsupported scheme" },
    { url: "file:///etc/passwd", ok: false, contains: "unsupported scheme" },
    { url: "http://localhost/hook", ok: false, contains: "blocked host" },
    { url: "http://127.0.0.1/hook", ok: false, contains: "blocked host" },
    { url: "http://127.0.0.2/hook", ok: false, contains: "blocked IP range" },
    { url: "http://127.255.255.254/hook", ok: false, contains: "blocked IP range" },
    { url: "http://100.64.0.1/hook", ok: false, contains: "blocked IP range" },
    { url: "http://169.254.169.254/latest/meta-data", ok: false, contains: "blocked host" },
    { url: "http://10.0.0.1/hook", ok: false, contains: "private IP range" },
    { url: "http://172.16.0.1/hook", ok: false, contains: "private IP range" },
    { url: "http://192.168.1.1/hook", ok: false, contains: "private IP range" },
    { url: "http://169.254.1.1/hook", ok: false, contains: "private IP range" },
    { url: "http://0.0.0.0/hook", ok: false, contains: "blocked host" },
    { url: "http://metadata.google.internal/hook", ok: false, contains: "blocked host" },
    // Issue #2150: IPv6 loopback, IPv4-mapped, unique-local, link-local addresses
    { url: "http://[::1]/hook", ok: false, contains: "loopback" },
    { url: "http://[::ffff:127.0.0.1]/hook", ok: false, contains: "IPv4-mapped" },
    { url: "http://[fd00::1]/hook", ok: false, contains: "unique-local" },
    { url: "http://[fc00::1]/hook", ok: false, contains: "unique-local" },
    { url: "http://[fe80::1]/hook", ok: false, contains: "link-local" },
  ];

  for (const c of cases) {
    it(`url=${c.url} => ${c.ok ? "accept" : "reject"}`, () => {
      if (c.ok) {
        const result = validateWebhookUrl(c.url, c.options);
        expect(result.toString()).toBe(new URL(c.url).toString());
      } else {
        await expect(validateWebhookUrl(c.url, c.options)).rejects.toThrow(
          c.contains ?? "unsupported scheme"
        );
      }
    });
  }

  it("allows private hosts when allowPrivateHosts=true", async () => {
    await expect(
      validateWebhookUrl("http://localhost/hook", { allowPrivateHosts: true })
    ).resolves.toBeInstanceOf(URL);
    await expect(
      validateWebhookUrl("http://10.0.0.1/hook", { allowPrivateHosts: true })
    ).resolves.toBeInstanceOf(URL);
  });

  it("rejects a hostname that resolves to 127.0.0.1 (e.g. 127.0.0.1.nip.io)", async () => {
    const loopbackLookup: DnsLookupFn = async () => [
      { address: "127.0.0.1", family: 4 },
    ];

    await expect(
      validateWebhookUrl("https://127.0.0.1.nip.io/hook", { lookup: loopbackLookup })
    ).rejects.toThrow("loopback address not allowed");
  });

  it("checks every returned DNS address and rejects if any address is private", async () => {
    const mixedLookup: DnsLookupFn = async () => [
      { address: "93.184.216.34", family: 4 },
      { address: "10.0.0.5", family: 4 },
    ];

    await expect(
      validateWebhookUrl("https://dual.example.com/hook", { lookup: mixedLookup })
    ).rejects.toThrow("private IP range not allowed");
  });

  it("rejects hostnames that resolve to loopback or private IPv6 addresses", async () => {
    const ipv6Loopback: DnsLookupFn = async () => [{ address: "::1", family: 6 }];
    const ipv4MappedLoopback: DnsLookupFn = async () => [
      { address: "::ffff:127.0.0.1", family: 6 },
    ];
    const ipv6Private: DnsLookupFn = async () => [{ address: "fd12:3456::1", family: 6 }];

    await expect(
      validateWebhookUrl("https://v6-loopback.example.com/hook", { lookup: ipv6Loopback })
    ).rejects.toThrow("loopback address not allowed");
    await expect(
      validateWebhookUrl("https://v4mapped.example.com/hook", { lookup: ipv4MappedLoopback })
    ).rejects.toThrow("loopback address not allowed");
    await expect(
      validateWebhookUrl("https://v6-ula.example.com/hook", { lookup: ipv6Private })
    ).rejects.toThrow("private IP range not allowed");
  });

  it("returns the pinned resolved address for valid public hostnames", async () => {
    const target = await resolveAndValidateWebhookUrl("https://example.com/hook", {
      lookup: publicLookup,
    });

    expect(target.url.toString()).toBe("https://example.com/hook");
    expect(target.address).toBe("93.184.216.34");
    expect(target.family).toBe(4);
    expect(target.addresses).toEqual([{ address: "93.184.216.34", family: 4 }]);
  });
});

describe("createPinnedWebhookSender", () => {
  it("re-resolves DNS on every delivery and blocks DNS rebinding to 127.0.0.1", async () => {
    const lookup = vi
      .fn<DnsLookupFn>()
      .mockResolvedValueOnce([{ address: "127.0.0.1", family: 4 }]);

    const sender = createPinnedWebhookSender({ allowPrivateHosts: false, lookup });
    const result = await sender(
      "https://rebind.example.com/hook",
      { "content-type": "application/json" },
      { event: "loan_issued" }
    );

    expect(lookup).toHaveBeenCalledTimes(1);
    expect(lookup).toHaveBeenCalledWith("rebind.example.com");
    expect(result.ok).toBe(false);
    expect(result.error).toContain("loopback address not allowed");
  });
});
