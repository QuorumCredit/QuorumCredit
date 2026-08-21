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
});
