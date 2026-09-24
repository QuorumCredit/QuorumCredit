import { describe, it, expect, vi, afterEach } from 'vitest';
import { startRelayServer } from '../src/pubsub/relayServer.js';
import type { RelayServerHandle } from '../src/pubsub/relayServer.js';
import { RelayBus } from '../src/pubsub/RelayBus.js';

// Helper: wait for a message on a channel
function waitForMessage(bus: RelayBus, channel: string): Promise<string> {
  return new Promise((resolve) => {
    const handler = (msg: string) => {
      resolve(msg);
    };
    void bus.subscribe(channel, handler);
  });
}

// Helper: collect N messages
function collectMessages(bus: RelayBus, channel: string, count: number): Promise<string[]> {
  return new Promise((resolve) => {
    const collected: string[] = [];
    const handler = (msg: string) => {
      collected.push(msg);
      if (collected.length >= count) resolve(collected);
    };
    void bus.subscribe(channel, handler);
  });
}

describe('RelayBus integration', () => {
  let server: RelayServerHandle;
  const clients: RelayBus[] = [];

  afterEach(async () => {
    for (const client of clients) {
      await client.close();
    }
    clients.length = 0;
    if (server) await server.close();
  });

  function makeClient(): RelayBus {
    const bus = new RelayBus('127.0.0.1', server.port);
    clients.push(bus);
    return bus;
  }

  it('delivers a published message to a subscriber on the same client', async () => {
    server = await startRelayServer();
    const bus = makeClient();
    const received = waitForMessage(bus, 'ch');
    await bus.publish('ch', 'hello');
    expect(await received).toBe('hello');
  });

  it('fans out a message to two clients subscribed to the same channel', async () => {
    server = await startRelayServer();
    const clientA = makeClient();
    const clientB = makeClient();

    const msgA = waitForMessage(clientA, 'broadcast');
    const msgB = waitForMessage(clientB, 'broadcast');

    // Give subscriptions time to register
    await new Promise(r => setTimeout(r, 20));

    await clientA.publish('broadcast', 'fan-out');

    const [a, b] = await Promise.all([msgA, msgB]);
    expect(a).toBe('fan-out');
    expect(b).toBe('fan-out');
  });

  it('does not deliver to clients subscribed to a different channel', async () => {
    server = await startRelayServer();
    const clientA = makeClient();
    const clientB = makeClient();

    const handler = vi.fn();
    await clientB.subscribe('other-channel', handler);
    await new Promise(r => setTimeout(r, 20));

    await clientA.publish('my-channel', 'targeted');
    await new Promise(r => setTimeout(r, 30));

    expect(handler).not.toHaveBeenCalled();
  });

  it('stops delivering after unsubscribe', async () => {
    server = await startRelayServer();
    const bus = makeClient();
    const handler = vi.fn();

    await bus.subscribe('ch', handler);
    await new Promise(r => setTimeout(r, 10));
    await bus.publish('ch', 'before-unsub');
    await new Promise(r => setTimeout(r, 20));
    expect(handler).toHaveBeenCalledTimes(1);

    await bus.unsubscribe('ch', handler);
    await new Promise(r => setTimeout(r, 10));
    await bus.publish('ch', 'after-unsub');
    await new Promise(r => setTimeout(r, 20));
    expect(handler).toHaveBeenCalledTimes(1); // no additional calls
  });

  it('reconnect behavior: new client connects after messages were published', async () => {
    server = await startRelayServer();
    const clientA = makeClient();

    // Publish before clientB connects
    await clientA.publish('ch', 'early');
    await new Promise(r => setTimeout(r, 10));

    // clientB connects after publish
    const clientB = makeClient();
    const received = waitForMessage(clientB, 'ch');

    await new Promise(r => setTimeout(r, 20));
    await clientA.publish('ch', 'late');

    expect(await received).toBe('late'); // only sees messages after subscription
  });

  it('handles connection drop by destroying socket and reconnecting via new client', async () => {
    server = await startRelayServer();
    const clientA = makeClient();

    // Force-close clientA (simulates network drop)
    await clientA.close();
    clients.splice(clients.indexOf(clientA), 1);

    // New client can still connect and use the server
    const clientB = makeClient();
    const received = waitForMessage(clientB, 'recovery');
    await new Promise(r => setTimeout(r, 20));
    await clientB.publish('recovery', 'ok');
    expect(await received).toBe('ok');
  });

  it('lock: grants to first requester, rejects second while held', async () => {
    server = await startRelayServer();
    const clientA = makeClient();
    const clientB = makeClient();

    const gotA = await clientA.tryAcquireLock('leader', 5000, 'a');
    const gotB = await clientB.tryAcquireLock('leader', 5000, 'b');
    expect(gotA).toBe(true);
    expect(gotB).toBe(false);
  });

  it('lock: released by holder allows another to acquire', async () => {
    server = await startRelayServer();
    const clientA = makeClient();
    const clientB = makeClient();

    await clientA.tryAcquireLock('leader', 5000, 'a');
    await clientA.releaseLock('leader', 'a');
    await new Promise(r => setTimeout(r, 10));
    const gotB = await clientB.tryAcquireLock('leader', 5000, 'b');
    expect(gotB).toBe(true);
  });

  it('preserves message ordering for a single publisher', async () => {
    server = await startRelayServer();
    const publisher = makeClient();
    const subscriber = makeClient();

    const messages = collectMessages(subscriber, 'ordered', 5);
    await new Promise(r => setTimeout(r, 20));

    for (let i = 0; i < 5; i++) {
      await publisher.publish('ordered', `msg-${i}`);
    }

    const received = await messages;
    expect(received).toEqual(['msg-0', 'msg-1', 'msg-2', 'msg-3', 'msg-4']);
  });
});
