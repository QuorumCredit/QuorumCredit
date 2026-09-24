import { SorobanRpc, xdr, scValToNative } from '@stellar/stellar-sdk';

/**
 * Versioned event registry.
 *
 * Each recognized event shape is keyed by its topic[0] symbol and declares the
 * exact number of topics and the index of each field. This lets `parseEvent`
 * fail loudly (log + skip) when the on-chain contract emits an unrecognized
 * topic count/shape instead of silently mis-mapping fields.
 */
export interface EventSchema {
  /** Expected total number of topics (including topic[0]). */
  topicCount: number;
  /** Map of field name -> topic index. */
  topicFields: Record<string, number>;
  /** Map of field name -> data index (for non-topic payload fields). */
  dataFields: Record<string, number>;
}

export const EVENT_REGISTRY: Record<string, EventSchema> = {
  'vouch/create': {
    topicCount: 4,
    topicFields: { vouchId: 1, borrower: 2, threshold: 3 },
    dataFields: {},
  },
  'loan/request': {
    topicCount: 4,
    topicFields: { loanId: 1, borrower: 2, amount: 3 },
    dataFields: {},
  },
  'loan/repay': {
    topicCount: 4,
    topicFields: { loanId: 1, borrower: 2, amount: 3 },
    dataFields: {},
  },
  'loan/slash': {
    topicCount: 4,
    topicFields: { loanId: 1, borrower: 2, amount: 3 },
    dataFields: {},
  },
};

export interface ParsedEvent {
  type: string;
  fields: Record<string, unknown>;
}

/**
 * Parse a raw Soroban event into a typed record.
 *
 * Returns `null` (and logs an error) when the event's topic count/shape does
 * not match the registered schema for its type, so callers can skip it rather
 * than persist mis-labeled fields.
 */
export function parseEvent(event: SorobanRpc.Api.EventResponse): ParsedEvent | null {
  const topics = event.topic;
  if (!topics || topics.length === 0) {
    console.error('[indexer] parseEvent: event has no topics, skipping', event);
    return null;
  }

  const type = scValToNative(topics[0]) as string;
  const schema = EVENT_REGISTRY[type];

  if (!schema) {
    console.error(`[indexer] parseEvent: unrecognized event type "${type}", skipping`, event);
    return null;
  }

  if (topics.length !== schema.topicCount) {
    console.error(
      `[indexer] parseEvent: schema mismatch for "${type}": expected ${schema.topicCount} topics, got ${topics.length}, skipping`,
      event,
    );
    return null;
  }

  const fields: Record<string, unknown> = {};
  for (const [name, index] of Object.entries(schema.topicFields)) {
    if (index >= topics.length) {
      console.error(
        `[indexer] parseEvent: schema mismatch for "${type}": field "${name}" expects topic[${index}] but only ${topics.length} topics present, skipping`,
        event,
      );
      return null;
    }
    fields[name] = scValToNative(topics[index]);
  }

  const data = event.value ? (scValToNative(event.value) as unknown[]) : [];
  for (const [name, index] of Object.entries(schema.dataFields)) {
    if (!Array.isArray(data) || index >= data.length) {
      console.error(
        `[indexer] parseEvent: schema mismatch for "${type}": field "${name}" expects data[${index}] but data has ${Array.isArray(data) ? data.length : 0} entries, skipping`,
        event,
      );
      return null;
    }
    fields[name] = data[index];
  }

  return { type, fields };
}
