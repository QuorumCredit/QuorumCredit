# QuorumCredit Event Indexer Service

Event indexing service for QuorumCredit smart contract (#1084, #1366).

## Overview

This service indexes on-chain events emitted by the QuorumCredit contract and provides a REST API for querying events by type, timestamp, and participant.

## Features

- **Restart-Safe**: Persisted cursor ensures indexing resumes from the last processed ledger after restarts
- **Atomic Writes**: Write-to-temp-then-rename prevents database corruption on crashes
- **Event Deduplication**: Prevents duplicate events from re-processed ledgers
- **Corruption Detection**: Preserves corrupt databases for forensics and requires explicit flag to rebuild
- **Real-time Indexing**: Continuous indexing from Soroban RPC
- **REST API**: Query indexed events by type, timestamp, and participant
- **Event Statistics**: Analytics and monitoring endpoints
- **Health Monitoring**: Health check endpoint for production deployments

## Quick Start

### Prerequisites

- Node.js 18+
- npm or yarn

### Installation

```bash
cd services/indexer
npm install
```

### Configuration

1. Copy `.env.example` to `.env`:
   ```bash
   cp .env.example .env
   ```

2. Edit `.env` with your configuration:
   ```env
   RPC_URL=https://soroban-testnet.stellar.org:443
   CONTRACT_ID=YOUR_CONTRACT_ID_HERE
   NETWORK_PASSPHRASE=Test SDF Network ; September 2015
   PORT=3000
   ```

### Running the Service

```bash
# Development mode
npm run dev

# Production mode
npm start

# Start from a specific ledger
npm start -- --from-ledger=12345

# Start from genesis (ledger 1)
npm start -- --from-genesis

# Allow rebuild on corrupt database
npm start -- --allow-rebuild
```

## Command-Line Options

- `--from-ledger=<number>`: Start indexing from a specific ledger (overrides cursor)
- `--from-genesis`: Start indexing from ledger 1 (equivalent to `--from-ledger=1`)
- `--allow-rebuild`: Allow starting with an empty database if the existing database is corrupt

## Restart Safety & Crash Recovery

The indexer maintains a **cursor file** (`cursor.json`) that tracks the last successfully processed ledger. On restart:

1. If a cursor exists, indexing resumes from `lastProcessedLedger + 1`
2. If no cursor exists, indexing starts from `latest - 1000` ledgers
3. Explicit `--from-ledger` or `--from-genesis` flags override the cursor

This ensures no events are skipped on restart, even if the process is killed mid-batch.

### Crash Safety Guarantees

The indexer provides **crash-safe event indexing** through these mechanisms:

1. **Atomic Writes**: Database and cursor are written atomically using write-to-temp-then-rename
   - A crash before the rename leaves the previous file intact
   - Only a successful rename updates the file
   - No partial or corrupted writes are possible

2. **Deduplication on Restart**: When a ledger is re-processed after a crash:
   - Events are matched against the in-memory deduplication set
   - Events already in the database are skipped
   - No duplicate events are added

3. **Ledger Replay Safety**: If a crash occurs between database save and cursor update:
   - The cursor will be behind the actual database
   - On restart, that ledger will be re-processed
   - Deduplication ensures no duplicates are added
   - Result: correct state without data loss

**Example crash scenario:**
```
Process saves events from ledger 1000 → crashes → 
Process restarts, loads database with ledger 1000 events →
Process resumes cursor from 999 → 
Process re-fetches ledger 1000 events →
Deduplication skips already-stored events →
Result: All events correctly stored, no loss or duplicates
```

## Corruption Handling

If the database file (`events.db.json`) is corrupted:

1. The corrupt file is automatically preserved with a `.corrupt.<timestamp>` suffix
2. An error is logged with instructions
3. **The service will NOT start** unless `--allow-rebuild` is provided
4. This prevents silent data loss from unintended database resets

Example:
```bash
# If database is corrupt, this will fail:
npm start

# To explicitly rebuild:
npm start -- --allow-rebuild
```

## Atomic Writes

All writes to `events.db.json` and `cursor.json` use atomic write-to-temp-then-rename to prevent corruption:

```
1. Write data to <file>.tmp
2. Flush to disk
3. Atomically rename <file>.tmp to <file>
```

A crash during step 1 or 2 leaves the original file intact. Only a successful rename updates the file.

## Event Deduplication

Events are identified by a composite key: `<ledger>-<event_index>`. The indexer maintains an in-memory set of seen event IDs. If a ledger is re-processed (e.g., due to cursor recovery), duplicate events are automatically skipped.

## API Endpoints

### Health Check
```
GET /health
```

Returns service health status, database size, and indexing state.

### Query Events
```
GET /events
```

Query parameters:
- `type`: Filter by event type (e.g., `vouch/create`, `loan/repay`)
- `start_date`: Start timestamp in milliseconds
- `end_date`: End timestamp in milliseconds
- `participant`: Filter by participant address
- `contract_id`: Filter by contract ID
- `limit`: Maximum number of results (default: 100)
- `offset`: Pagination offset (default: 0)

Example:
```
GET /events?type=loan/slash&start_date=1672531200000&end_date=1672617600000&limit=10
```

### Statistics
```
GET /stats
```

Returns event statistics including:
- Total events indexed
- Events by type
- Events by day (last 30 days)
- Unique participants

## Event Types

The indexer recognizes the following QuorumCredit event types:

- `vouch/create`: New vouch created
- `vouch/increase`: Vouch stake increased
- `vouch/decrease`: Vouch stake decreased
- `vouch/withdraw`: Vouch withdrawn
- `loan/request`: Loan requested
- `loan/repay`: Loan repaid
- `loan/slash`: Loan slashed
- `admin/config`: Configuration updated
- `admin/pause`: Contract paused
- `admin/unpause`: Contract unpaused

## Database & Persistence

Events are stored in JSON files:
- `events.db.json`: All indexed events
- `cursor.json`: Last processed ledger cursor

Both files use atomic writes to prevent corruption.

### Persistence Architecture

The indexer uses a **JSON-based append model** for event storage with atomic writes:

1. **In-Memory Database**: Events are loaded into memory on startup
2. **Full Rewrite**: Each batch of new events triggers a full JSON file rewrite
3. **Atomic Guarantees**: Write-to-temp-then-rename ensures crash safety
4. **Deduplication Index**: In-memory Set for O(1) duplicate checking

### Scalability & Performance

**Current JSON-based approach:**
- Write latency increases linearly with database size (O(n))
- Load time on startup is O(n)
- Suitable for small to medium datasets (< 100k events)

**Performance characteristics:**
| Database Size | Save Time | Load Time |
|---------------|-----------|-----------|
| 1,000 events | ~10ms | ~5ms |
| 10,000 events | ~100ms | ~50ms |
| 100,000 events | ~1s | ~500ms |
| 1,000,000 events | ~10s | ~5s |

**Migration Path for Scale**

For production deployments handling millions of events, consider migrating to SQLite:

```javascript
// Planned replacement for JSON backend
// - Append-only log for write amplification reduction
// - Indexed queries for fast lookups
// - Automatic WAL mode for crash safety
// - Embedded database, no separate service needed
```

The Rust indexer (`tools/indexer`) already uses SQLite and can serve as a reference implementation.

For now, the JSON backend is suitable for:
- Development and testing
- Production systems with < 1M events
- Systems with infrequent startup/restart cycles

## Testing

```bash
# Run tests
npm test

# Run tests in watch mode
npm run test:watch

# Run tests with coverage
npm test -- --coverage
```

Tests verify:
- ✅ Restart safety: cursor persistence and resume
- ✅ Corruption detection: fails startup without `--allow-rebuild`
- ✅ Atomic writes: no `.tmp` files left after successful writes
- ✅ Event deduplication: duplicate events are skipped
- ✅ Database preservation: corrupt files are saved with `.corrupt` suffix

## Monitoring

The service logs indexing progress and errors to the console. Health checks are available via the `/health` endpoint.

Key metrics to monitor:
- Events indexed per minute
- Database size
- Cursor progression
- API response times

## Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Soroban RPC   │───▶│   Event Indexer  │───▶│   events.db.json │
└─────────────────┘    └─────────────────┘    └─────────────────┘
                              │                 ┌─────────────────┐
                              └────────────────▶│   cursor.json    │
                              │                 └─────────────────┘
                              ▼
                       ┌─────────────────┐
                       │   REST API      │
                       └─────────────────┘
```

## Development

### Building
```bash
npm run build
```

### Testing
```bash
npm test
```

### Linting
```bash
npm run lint  # (if configured)
```

## Deployment

### Docker
```bash
docker build -t quorumcredit-indexer .
docker run -p 3000:3000 \
  -e CONTRACT_ID=YOUR_CONTRACT_ID \
  -e RPC_URL=https://soroban-testnet.stellar.org:443 \
  quorumcredit-indexer
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `RPC_URL` | Soroban RPC URL | `https://soroban-testnet.stellar.org:443` |
| `CONTRACT_ID` | QuorumCredit contract ID | Required |
| `NETWORK_PASSPHRASE` | Network passphrase | `Test SDF Network ; September 2015` |
| `PORT` | API server port | `3000` |

## Troubleshooting

### Common Issues

1. **No events being indexed**
   - Verify contract ID is correct
   - Check RPC URL connectivity
   - Ensure contract is deployed and emitting events

2. **Database corrupt on startup**
   - Check `events.db.json.corrupt.<timestamp>` for forensics
   - Use `--allow-rebuild` to restart with empty database
   - Verify disk space and permissions

3. **Cursor not persisting**
   - Check file permissions on `cursor.json`
   - Verify disk space
   - Check logs for write errors

4. **Duplicate events after restart**
   - This should not happen; file a bug report with logs

5. **Indexer falling behind**
   - Check RPC response times
   - Reduce polling interval if needed
   - Monitor system resources

### Recovery Procedures

**Database corruption:**
```bash
# 1. Check preserved corrupt file
ls -la services/indexer/events.db.json.corrupt.*

# 2. Decide: restore or rebuild
# Option A: Restore from backup (if available)
cp backup/events.db.json services/indexer/

# Option B: Rebuild from scratch
npm start -- --allow-rebuild --from-genesis
```

**Cursor corruption:**
```bash
# Cursor corruption is non-critical - indexer will fall back to latest-1000
# If needed, manually reset cursor:
echo '{"lastProcessedLedger":12345,"lastUpdated":1234567890}' > services/indexer/cursor.json
```

**Large gap detection:**
```bash
# If indexer missed events (e.g., was down for >1000 ledgers):
# 1. Stop the indexer
# 2. Re-index from last known good ledger
npm start -- --from-ledger=<last_known_ledger>
```

### Logs

Check console output for:
- Indexing progress: `Indexed N new events from ledger X`
- Cursor updates: `Loaded cursor: last processed ledger X`
- Deduplication: `Skipping duplicate event: <id>`
- RPC connection errors
- Event parsing errors
- Database save errors
- Corruption warnings

## Migration from Old Indexer

If upgrading from the previous indexer (without cursor):

1. **Backup existing database:**
   ```bash
   cp services/indexer/events.db.json services/indexer/events.db.json.backup
   ```

2. **Start the new indexer:**
   ```bash
   npm start
   ```
   
   The new indexer will load existing events and create a cursor at the latest indexed ledger.

3. **Verify:**
   ```bash
   # Check cursor exists
   cat services/indexer/cursor.json
   
   # Check database loaded
   curl http://localhost:3000/health
   ```

## Indexer Architecture & Event Parsing (#1502)

### Two Implementations

This repository contains **two independent indexer implementations**:

1. **TypeScript/Node Indexer** (`services/indexer/`)
   - Purpose: Production REST API for event queries
   - Storage: JSON files (ephemeral)
   - Deployment: Docker container, scalable
   - Use case: Real-time event API, monitoring

2. **Rust Indexer** (`tools/indexer/`)
   - Purpose: Comprehensive on-chain data analysis
   - Storage: SQLite (persistent, queryable)
   - Deployment: Local development tool
   - Use case: Analytics, data science, auditing

### Authoritative Indexer

**For production monitoring**, use the **TypeScript indexer** (`services/indexer/`):
- ✅ REST API with low-latency queries
- ✅ Real-time event streaming
- ✅ Health checks and metrics
- ✅ Restart-safe cursor persistence
- ✅ Atomic write guarantees

The Rust indexer is for development and analysis, not production services.

### Event Parsing Consistency

Both indexers parse QuorumCredit events from the same on-chain format. To prevent divergence, both implement the same event type parsers:

**Supported Event Types:**
- `vouch/create`: New vouch pledge
- `vouch/increase`: Stake increase
- `vouch/decrease`: Stake decrease
- `loan/request`: Loan application
- `loan/repay`: Loan repayment
- `loan/slash`: Collateral slash
- `admin/*`: Administrative events

**Parsing Logic:**
- Events are identified by composite key: `<ledger>-<event_index>`
- Topic[0] = event type (string)
- Topic[1] = participant address
- Additional topics are event-type-specific data
- Both indexers use `scValToNative()` for topic deserialization

### Comparison

| Feature | TypeScript Indexer | Rust Indexer |
|---------|-------------------|--------------|
| **Use Case** | Production API service | Dev/analytics tool |
| **Storage** | JSON (in-memory loaded) | SQLite (persistent) |
| **Deduplication** | In-memory Set | `UNIQUE INDEX` |
| **Cursor** | `cursor.json` | SQLite `cursor` table |
| **Reorg Detection** | Not implemented | Full audit trail |
| **API** | REST (Express) | CLI only |
| **Deployment** | Docker container | Local binary |
| **Crash Safety** | Atomic writes + cursor | WAL mode + transactions |
| **Event Parsing** | Same implementation | Same implementation |

Both implement cursor persistence and restart safety (#1366).

### When to Use Each

**Use TypeScript Indexer when:**
- Building REST APIs that query events
- Need real-time event monitoring
- Running in containerized/cloud environments
- Want low-latency query responses
- Need health checks and metrics

**Use Rust Indexer when:**
- Analyzing historical on-chain patterns
- Building audit reports
- Need full reorg detection
- Doing data science / research
- Running locally with persistent storage

## License

MIT