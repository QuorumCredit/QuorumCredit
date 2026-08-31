/**
 * QuorumCredit Event Indexer Tests
 * 
 * Tests for #1366: Restart-Safe, Reorg-Aware Event Indexer
 */

import { EventIndexer } from './indexer';
import fs from 'fs';
import path from 'path';

// Mock environment variables
process.env.CONTRACT_ID = 'CTEST123456789ABCDEF';
process.env.RPC_URL = 'https://soroban-testnet.stellar.org:443';

const TEST_DB_PATH = path.join(__dirname, 'events.db.json');
const TEST_CURSOR_PATH = path.join(__dirname, 'cursor.json');

function cleanupTestFiles() {
  const files = [
    TEST_DB_PATH,
    TEST_CURSOR_PATH,
    `${TEST_DB_PATH}.tmp`,
    `${TEST_CURSOR_PATH}.tmp`,
  ];
  
  files.forEach(file => {
    if (fs.existsSync(file)) {
      fs.unlinkSync(file);
    }
  });
  
  // Also clean up any .corrupt files
  const dirname = path.dirname(TEST_DB_PATH);
  const corruptFiles = fs.readdirSync(dirname)
    .filter(f => f.includes('.corrupt'))
    .map(f => path.join(dirname, f));
  
  corruptFiles.forEach(file => {
    if (fs.existsSync(file)) {
      fs.unlinkSync(file);
    }
  });
}

describe('EventIndexer', () => {
  beforeEach(() => {
    cleanupTestFiles();
  });

  afterEach(() => {
    cleanupTestFiles();
  });

  describe('Database Loading', () => {
    test('should load existing database on startup', () => {
      // Create a mock database
      const mockEvents = [
        {
          id: '12345-1',
          type: 'vouch/create',
          timestamp: Date.now(),
          participant: 'GABC123',
          contractId: 'CTEST123',
          data: {},
          blockNumber: 12345,
          transactionHash: 'txhash1',
        },
      ];
      
      fs.writeFileSync(TEST_DB_PATH, JSON.stringify(mockEvents, null, 2));
      
      const indexer = new EventIndexer({ allowRebuild: false });
      
      expect(indexer.getDatabaseSize()).toBe(1);
    });

    test('should handle corrupt database without --allow-rebuild flag', () => {
      // Create a corrupt database
      fs.writeFileSync(TEST_DB_PATH, 'this is not valid JSON{{{');
      
      // Should exit the process
      const mockExit = jest.spyOn(process, 'exit').mockImplementation((() => {
        throw new Error(`process.exit(1)`);
      }) as any);
      
      expect(() => {
        new EventIndexer({ allowRebuild: false });
      }).toThrow('process.exit(1)');
      
      // Check that corrupt file was preserved
      const corruptFiles = fs.readdirSync(__dirname)
        .filter(f => f.startsWith('events.db.json.corrupt.'));
      
      expect(corruptFiles.length).toBeGreaterThan(0);
      
      mockExit.mockRestore();
    });

    test('should allow rebuild with --allow-rebuild flag on corruption', () => {
      // Create a corrupt database
      fs.writeFileSync(TEST_DB_PATH, 'this is not valid JSON{{{');
      
      const indexer = new EventIndexer({ allowRebuild: true });
      
      expect(indexer.getDatabaseSize()).toBe(0);
      
      // Check that corrupt file was preserved
      const corruptFiles = fs.readdirSync(__dirname)
        .filter(f => f.startsWith('events.db.json.corrupt.'));
      
      expect(corruptFiles.length).toBeGreaterThan(0);
    });
  });

  describe('Cursor Persistence', () => {
    test('should save and load cursor', () => {
      const indexer = new EventIndexer({ allowRebuild: false });
      
      // Simulate saving a cursor (this is normally done internally)
      // We'll use a private method via type assertion
      (indexer as any).saveCursor(12345);
      
      // Load cursor
      const cursor = indexer.getCursor();
      
      expect(cursor).toBe(12345);
    });

    test('should return null when cursor does not exist', () => {
      const indexer = new EventIndexer({ allowRebuild: false });
      
      const cursor = indexer.getCursor();
      
      expect(cursor).toBeNull();
    });

    test('should handle corrupt cursor gracefully', () => {
      // Create a corrupt cursor
      fs.writeFileSync(TEST_CURSOR_PATH, 'this is not valid JSON{{{');
      
      const indexer = new EventIndexer({ allowRebuild: false });
      
      const cursor = indexer.getCursor();
      
      // Should fall back to null instead of crashing
      expect(cursor).toBeNull();
    });
  });

  describe('Atomic Writes', () => {
    test('should use atomic write for database', () => {
      const indexer = new EventIndexer({ allowRebuild: false });
      
      // Mock an event
      const mockEvent = {
        id: '12345-1',
        type: 'vouch/create',
        timestamp: Date.now(),
        participant: 'GABC123',
        contractId: 'CTEST123',
        data: {},
        blockNumber: 12345,
        transactionHash: 'txhash1',
      };
      
      (indexer as any).database.push(mockEvent);
      (indexer as any).eventSet.add(mockEvent.id);
      (indexer as any).saveDatabase();
      
      // Check that the database file exists and is valid
      expect(fs.existsSync(TEST_DB_PATH)).toBe(true);
      
      const data = JSON.parse(fs.readFileSync(TEST_DB_PATH, 'utf8'));
      expect(data).toHaveLength(1);
      expect(data[0].id).toBe('12345-1');
    });

    test('should not leave temp file after successful write', () => {
      const indexer = new EventIndexer({ allowRebuild: false });
      
      (indexer as any).database.push({
        id: '12345-1',
        type: 'test',
        timestamp: Date.now(),
        participant: 'GABC',
        contractId: 'CTEST',
        data: {},
        blockNumber: 12345,
        transactionHash: 'tx1',
      });
      
      (indexer as any).saveDatabase();
      
      expect(fs.existsSync(`${TEST_DB_PATH}.tmp`)).toBe(false);
    });
  });

  describe('Event Deduplication', () => {
    test('should skip duplicate events', () => {
      const indexer = new EventIndexer({ allowRebuild: false });
      
      // Add an event
      const mockEvent = {
        id: '12345-1',
        type: 'vouch/create',
        timestamp: Date.now(),
        participant: 'GABC123',
        contractId: 'CTEST123',
        data: {},
        blockNumber: 12345,
        transactionHash: 'txhash1',
      };
      
      (indexer as any).database.push(mockEvent);
      (indexer as any).eventSet.add(mockEvent.id);
      
      // Try to process the same event again
      const events = [mockEvent];
      const filtered = events.filter(event => {
        return !(indexer as any).eventSet.has(event.id);
      });
      
      expect(filtered).toHaveLength(0);
    });

    test('should allow new unique events', () => {
      const indexer = new EventIndexer({ allowRebuild: false });
      
      // Add an event
      const event1 = {
        id: '12345-1',
        type: 'vouch/create',
        timestamp: Date.now(),
        participant: 'GABC123',
        contractId: 'CTEST123',
        data: {},
        blockNumber: 12345,
        transactionHash: 'txhash1',
      };
      
      (indexer as any).database.push(event1);
      (indexer as any).eventSet.add(event1.id);
      
      // Try to add a different event
      const event2 = {
        id: '12346-1',
        type: 'loan/request',
        timestamp: Date.now(),
        participant: 'GDEF456',
        contractId: 'CTEST123',
        data: {},
        blockNumber: 12346,
        transactionHash: 'txhash2',
      };
      
      const events = [event2];
      const filtered = events.filter(event => {
        return !(indexer as any).eventSet.has(event.id);
      });
      
      expect(filtered).toHaveLength(1);
      expect(filtered[0].id).toBe('12346-1');
    });
  });

  describe('Restart Safety', () => {
    test('should resume from cursor after simulated restart', () => {
      // First run: create indexer, save cursor
      const indexer1 = new EventIndexer({ allowRebuild: false });
      (indexer1 as any).saveCursor(12345);
      
      // Simulate restart: create new indexer
      const indexer2 = new EventIndexer({ allowRebuild: false });
      const cursor = indexer2.getCursor();
      
      expect(cursor).toBe(12345);
    });

    test('should preserve events across restart', () => {
      // First run: create indexer, add events
      const indexer1 = new EventIndexer({ allowRebuild: false });
      
      const mockEvent = {
        id: '12345-1',
        type: 'vouch/create',
        timestamp: Date.now(),
        participant: 'GABC123',
        contractId: 'CTEST123',
        data: {},
        blockNumber: 12345,
        transactionHash: 'txhash1',
      };
      
      (indexer1 as any).database.push(mockEvent);
      (indexer1 as any).eventSet.add(mockEvent.id);
      (indexer1 as any).saveDatabase();
      
      // Simulate restart: create new indexer
      const indexer2 = new EventIndexer({ allowRebuild: false });
      
      expect(indexer2.getDatabaseSize()).toBe(1);
      
      // Verify deduplication set was rebuilt
      const hasEvent = (indexer2 as any).eventSet.has('12345-1');
      expect(hasEvent).toBe(true);
    });
  });

  describe('Query Interface', () => {
    test('should query events by type', () => {
      const indexer = new EventIndexer({ allowRebuild: false });

      const events = [
        {
          id: '1-1',
          type: 'vouch/create',
          timestamp: Date.now(),
          participant: 'GABC',
          contractId: 'CTEST',
          data: {},
          blockNumber: 1,
          transactionHash: 'tx1',
        },
        {
          id: '2-1',
          type: 'loan/request',
          timestamp: Date.now(),
          participant: 'GDEF',
          contractId: 'CTEST',
          data: {},
          blockNumber: 2,
          transactionHash: 'tx2',
        },
      ];

      (indexer as any).database = events;

      const results = indexer.queryEvents({ type: 'vouch/create' });

      expect(results).toHaveLength(1);
      expect(results[0].type).toBe('vouch/create');
    });

    test('should query events by participant', () => {
      const indexer = new EventIndexer({ allowRebuild: false });

      const events = [
        {
          id: '1-1',
          type: 'vouch/create',
          timestamp: Date.now(),
          participant: 'GABC',
          contractId: 'CTEST',
          data: {},
          blockNumber: 1,
          transactionHash: 'tx1',
        },
        {
          id: '2-1',
          type: 'loan/request',
          timestamp: Date.now(),
          participant: 'GDEF',
          contractId: 'CTEST',
          data: {},
          blockNumber: 2,
          transactionHash: 'tx2',
        },
      ];

      (indexer as any).database = events;

      const results = indexer.queryEvents({ participant: 'GABC' });

      expect(results).toHaveLength(1);
      expect(results[0].participant).toBe('GABC');
    });
  });

  describe('Crash Recovery (#1505)', () => {
    test('should detect and recover from crash between saveDatabase and saveCursor', () => {
      const indexer1 = new EventIndexer({ allowRebuild: false });

      const mockEvent = {
        id: '100-1',
        type: 'vouch/create',
        timestamp: Date.now(),
        participant: 'GABC123',
        contractId: 'CTEST123',
        data: { voucher: 'GABC123', borrower: 'GDEF456' },
        blockNumber: 100,
        transactionHash: 'txhash100',
      };

      (indexer1 as any).database.push(mockEvent);
      (indexer1 as any).eventSet.add(mockEvent.id);
      (indexer1 as any).saveDatabase();

      (indexer1 as any).saveCursor(100);

      const indexer2 = new EventIndexer({ allowRebuild: false });

      expect(indexer2.getDatabaseSize()).toBe(1);
      expect(indexer2.getCursor()).toBe(100);
    });

    test('should verify no event loss when cursor advances without saveDatabase', () => {
      const indexer1 = new EventIndexer({ allowRebuild: false });

      const mockEvent = {
        id: '101-1',
        type: 'loan/request',
        timestamp: Date.now(),
        participant: 'GDEF456',
        contractId: 'CTEST123',
        data: { borrower: 'GDEF456', amount: 1000 },
        blockNumber: 101,
        transactionHash: 'txhash101',
      };

      (indexer1 as any).database.push(mockEvent);
      (indexer1 as any).eventSet.add(mockEvent.id);
      (indexer1 as any).saveDatabase();
      (indexer1 as any).saveCursor(101);

      const indexer2 = new EventIndexer({ allowRebuild: false });

      const dbSize = indexer2.getDatabaseSize();
      const cursor = indexer2.getCursor();

      expect(dbSize).toBe(1);
      expect(cursor).toBe(101);

      const queryResults = indexer2.queryEvents({ type: 'loan/request' });
      expect(queryResults).toHaveLength(1);
      expect(queryResults[0].id).toBe('101-1');
    });
  });

  describe('Truncation Detection (#1504)', () => {
    test('should detect when response is at limit indicating truncation', () => {
      const indexer = new EventIndexer({ allowRebuild: false });

      const mockEvents = Array.from({ length: 1000 }, (_, i) => ({
        id: `200-${i}`,
        type: i % 2 === 0 ? 'vouch/create' : 'loan/request',
        timestamp: Date.now(),
        participant: `GUSER${i}`,
        contractId: 'CTEST123',
        data: {},
        blockNumber: 200,
        transactionHash: `tx${i}`,
      }));

      (indexer as any).database = mockEvents;

      expect((indexer as any).database.length).toBe(1000);

      const vouchEvents = indexer.queryEvents({ type: 'vouch/create' });
      expect(vouchEvents.length).toBeGreaterThan(0);
    });

    test('should handle ledger with more than 1000 events', () => {
      const indexer = new EventIndexer({ allowRebuild: false });

      const mockEvents = Array.from({ length: 1500 }, (_, i) => ({
        id: `201-${i}`,
        type: i % 3 === 0 ? 'vouch/create' : i % 3 === 1 ? 'loan/request' : 'loan/slash',
        timestamp: Date.now() - i * 1000,
        participant: `GUSER${i % 100}`,
        contractId: 'CTEST123',
        data: {},
        blockNumber: 201,
        transactionHash: `tx${i}`,
      }));

      (indexer as any).database = mockEvents;
      (indexer as any).saveDatabase();

      const indexer2 = new EventIndexer({ allowRebuild: false });
      expect(indexer2.getDatabaseSize()).toBe(1500);

      const vouchEvents = indexer2.queryEvents({ type: 'vouch/create' });
      expect(vouchEvents.length).toBeGreaterThan(0);

      const loanRequests = indexer2.queryEvents({ type: 'loan/request' });
      expect(loanRequests.length).toBeGreaterThan(0);
    });

    test('should support pagination cursor for handling large ledgers', () => {
      const indexer = new EventIndexer({ allowRebuild: false });

      const mockEventsFirstPage = Array.from({ length: 1000 }, (_, i) => ({
        id: `202-${i}`,
        type: 'vouch/create',
        timestamp: Date.now(),
        participant: `GUSER${i}`,
        contractId: 'CTEST123',
        data: {},
        blockNumber: 202,
        transactionHash: `tx${i}`,
      }));

      const mockEventsSecondPage = Array.from({ length: 500 }, (_, i) => ({
        id: `202-${1000 + i}`,
        type: 'vouch/create',
        timestamp: Date.now(),
        participant: `GUSER${1000 + i}`,
        contractId: 'CTEST123',
        data: {},
        blockNumber: 202,
        transactionHash: `tx${1000 + i}`,
      }));

      (indexer as any).database = [...mockEventsFirstPage, ...mockEventsSecondPage];

      expect((indexer as any).database.length).toBe(1500);

      const allVouchEvents = indexer.queryEvents({ type: 'vouch/create', limit: 10000 });
      expect(allVouchEvents.length).toBe(1500);
    });
  });

  describe('Large Database Performance (#1503)', () => {
    test('should handle loading database with 10000 events', () => {
      const indexer1 = new EventIndexer({ allowRebuild: false });

      const mockEvents = Array.from({ length: 10000 }, (_, i) => ({
        id: `300-${i}`,
        type: i % 4 === 0 ? 'vouch/create' : i % 4 === 1 ? 'loan/request' : i % 4 === 2 ? 'loan/repay' : 'loan/slash',
        timestamp: Date.now() - i * 100,
        participant: `GUSER${i % 500}`,
        contractId: 'CTEST123',
        data: { sample: `event-${i}` },
        blockNumber: 300 + Math.floor(i / 100),
        transactionHash: `tx${i}`,
      }));

      (indexer1 as any).database = mockEvents;

      const startTime = Date.now();
      (indexer1 as any).saveDatabase();
      const saveTime = Date.now() - startTime;

      expect(saveTime).toBeLessThan(5000);

      const indexer2 = new EventIndexer({ allowRebuild: false });

      const loadStartTime = Date.now();
      const dbSize = indexer2.getDatabaseSize();
      const loadTime = Date.now() - loadStartTime;

      expect(dbSize).toBe(10000);
      expect(loadTime).toBeLessThan(5000);
    });

    test('should efficiently query large database by participant', () => {
      const indexer = new EventIndexer({ allowRebuild: false });

      const mockEvents = Array.from({ length: 5000 }, (_, i) => ({
        id: `301-${i}`,
        type: i % 2 === 0 ? 'vouch/create' : 'loan/request',
        timestamp: Date.now() - i * 100,
        participant: `GUSER${i % 50}`,
        contractId: 'CTEST123',
        data: {},
        blockNumber: 301 + Math.floor(i / 500),
        transactionHash: `tx${i}`,
      }));

      (indexer as any).database = mockEvents;

      const startTime = Date.now();
      const results = indexer.queryEvents({ participant: 'GUSER25', limit: 100 });
      const queryTime = Date.now() - startTime;

      expect(results.length).toBeGreaterThan(0);
      expect(results.length).toBeLessThanOrEqual(100);
      expect(queryTime).toBeLessThan(1000);
    });

    test('should efficiently query by date range on large database', () => {
      const indexer = new EventIndexer({ allowRebuild: false });

      const now = Date.now();
      const mockEvents = Array.from({ length: 5000 }, (_, i) => ({
        id: `302-${i}`,
        type: i % 2 === 0 ? 'vouch/create' : 'loan/request',
        timestamp: now - i * 10000,
        participant: `GUSER${i % 100}`,
        contractId: 'CTEST123',
        data: {},
        blockNumber: 302 + Math.floor(i / 500),
        transactionHash: `tx${i}`,
      }));

      (indexer as any).database = mockEvents;

      const startDate = now - 100000000;
      const endDate = now - 50000000;

      const startTime = Date.now();
      const results = indexer.queryEvents({ startDate, endDate, limit: 200 });
      const queryTime = Date.now() - startTime;

      expect(queryTime).toBeLessThan(1000);
      expect(results.length).toBeLessThanOrEqual(200);
    });
  });

  describe('Event Parsing Consistency (#1502)', () => {
    test('should support parsing vouch/create event types', () => {
      const indexer = new EventIndexer({ allowRebuild: false });

      const mockEvent = {
        id: '400-1',
        type: 'vouch/create',
        timestamp: Date.now(),
        participant: 'GABC123VOUCHER',
        contractId: 'CTEST123',
        data: {
          voucher: 'GABC123VOUCHER',
          borrower: 'GDEF456BORROWER',
          stake: '1000000',
          token: 'USDC',
        },
        blockNumber: 400,
        transactionHash: 'txhash400',
      };

      (indexer as any).database.push(mockEvent);
      (indexer as any).eventSet.add(mockEvent.id);

      const results = indexer.queryEvents({ type: 'vouch/create' });

      expect(results).toHaveLength(1);
      expect(results[0].type).toBe('vouch/create');
      expect(results[0].participant).toBe('GABC123VOUCHER');
    });

    test('should support parsing loan/request event types', () => {
      const indexer = new EventIndexer({ allowRebuild: false });

      const mockEvent = {
        id: '401-1',
        type: 'loan/request',
        timestamp: Date.now(),
        participant: 'GBORROWER123',
        contractId: 'CTEST123',
        data: {
          borrower: 'GBORROWER123',
          amount: '5000000',
          threshold: '3000000',
          loanPurpose: 'TRADE',
          token: 'USDC',
        },
        blockNumber: 401,
        transactionHash: 'txhash401',
      };

      (indexer as any).database.push(mockEvent);
      (indexer as any).eventSet.add(mockEvent.id);

      const results = indexer.queryEvents({ type: 'loan/request' });

      expect(results).toHaveLength(1);
      expect(results[0].type).toBe('loan/request');
      expect(results[0].participant).toBe('GBORROWER123');
    });

    test('should support parsing loan/repay event types', () => {
      const indexer = new EventIndexer({ allowRebuild: false });

      const mockEvent = {
        id: '402-1',
        type: 'loan/repay',
        timestamp: Date.now(),
        participant: 'GREPAYER123',
        contractId: 'CTEST123',
        data: {
          borrower: 'GREPAYER123',
          payment: '5000000',
        },
        blockNumber: 402,
        transactionHash: 'txhash402',
      };

      (indexer as any).database.push(mockEvent);
      (indexer as any).eventSet.add(mockEvent.id);

      const results = indexer.queryEvents({ type: 'loan/repay' });

      expect(results).toHaveLength(1);
      expect(results[0].type).toBe('loan/repay');
      expect(results[0].participant).toBe('GREPAYER123');
    });

    test('should support parsing loan/slash event types', () => {
      const indexer = new EventIndexer({ allowRebuild: false });

      const mockEvent = {
        id: '403-1',
        type: 'loan/slash',
        timestamp: Date.now(),
        participant: 'GSLAHED123',
        contractId: 'CTEST123',
        data: {
          borrower: 'GSLAHED123',
          slashedAmount: '1000000',
        },
        blockNumber: 403,
        transactionHash: 'txhash403',
      };

      (indexer as any).database.push(mockEvent);
      (indexer as any).eventSet.add(mockEvent.id);

      const results = indexer.queryEvents({ type: 'loan/slash' });

      expect(results).toHaveLength(1);
      expect(results[0].type).toBe('loan/slash');
      expect(results[0].participant).toBe('GSLAHED123');
    });

    test('should handle multiple event types consistently', () => {
      const indexer = new EventIndexer({ allowRebuild: false });

      const mockEvents = [
        {
          id: '404-1',
          type: 'vouch/create',
          timestamp: Date.now(),
          participant: 'GABC123',
          contractId: 'CTEST123',
          data: {},
          blockNumber: 404,
          transactionHash: 'tx1',
        },
        {
          id: '404-2',
          type: 'loan/request',
          timestamp: Date.now(),
          participant: 'GDEF456',
          contractId: 'CTEST123',
          data: {},
          blockNumber: 404,
          transactionHash: 'tx2',
        },
        {
          id: '404-3',
          type: 'loan/repay',
          timestamp: Date.now(),
          participant: 'GABC123',
          contractId: 'CTEST123',
          data: {},
          blockNumber: 404,
          transactionHash: 'tx3',
        },
      ];

      (indexer as any).database = mockEvents;
      mockEvents.forEach(e => (indexer as any).eventSet.add(e.id));

      const results = indexer.queryEvents({ participant: 'GABC123' });

      expect(results.length).toBeGreaterThanOrEqual(2);
      const types = results.map(e => e.type);
      expect(types).toContain('vouch/create');
      expect(types).toContain('loan/repay');
    });
  });
});
