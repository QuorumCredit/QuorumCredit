import { createHash, randomUUID } from "node:crypto";
import type { Credential, CredentialStore } from "../credentials/credentialStore.js";

export interface CredentialImportInput {
  holderId: string;
  type: Credential["type"];
  issuer: string;
  expiresAt: number;
  metadata: Record<string, unknown>;
}

export interface ParsedCredentialImportRow {
  row: number;
  input?: CredentialImportInput;
  errors: string[];
}

export interface CredentialImportRowResult {
  row: number;
  status: "pending" | "imported" | "failed";
  credentialId?: string;
  errors: string[];
}

export interface CredentialImportBatch {
  importId: string;
  status: "in_progress" | "completed";
  total: number;
  processed: number;
  imported: number;
  failed: number;
  progress: number;
  createdAt: number;
  completedAt?: number;
  results: CredentialImportRowResult[];
}

const MAX_BATCHES = 1_000;
const BATCH_TTL_MS = 24 * 60 * 60 * 1_000;
const CREDENTIAL_TYPES = new Set<Credential["type"]>([
  "identity",
  "education",
  "professional",
  "financial",
]);

export function parseCredentialJson(value: unknown): ParsedCredentialImportRow[] {
  if (!Array.isArray(value)) {
    throw new Error("JSON import body must be an array of credentials");
  }
  return value.map((item, index) => validateImportRow(item, index + 1));
}

export function parseCredentialCsv(csv: string): ParsedCredentialImportRow[] {
  const records = parseCsvRecords(csv.replace(/^\uFEFF/, ""));
  if (records.length === 0) throw new Error("CSV import must include a header row");
  const headers = records[0]!.map((header) => header.trim());
  const required = ["holderId", "type", "issuer", "expiresAt"];
  for (const header of required) {
    if (!headers.includes(header)) throw new Error(`CSV header must include ${header}`);
  }
  if (new Set(headers).size !== headers.length || headers.some((header) => !header)) {
    throw new Error("CSV headers must be non-empty and unique");
  }

  return records.slice(1).map((record, index) => {
    if (record.length !== headers.length) {
      return {
        row: index + 2,
        errors: [`expected ${headers.length} columns, received ${record.length}`],
      };
    }
    const values = Object.fromEntries(headers.map((header, i) => [header, record[i]]));
    let metadata: unknown = {};
    if (values.metadata) {
      try {
        metadata = JSON.parse(values.metadata);
      } catch {
        return { row: index + 2, errors: ["metadata must contain valid JSON"] };
      }
    }
    return validateImportRow(
      {
        holderId: values.holderId,
        type: values.type,
        issuer: values.issuer,
        expiresAt: Number(values.expiresAt),
        metadata,
      },
      index + 2
    );
  });
}

function validateImportRow(value: unknown, row: number): ParsedCredentialImportRow {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return { row, errors: ["credential must be an object"] };
  }
  const candidate = value as Record<string, unknown>;
  const errors: string[] = [];
  const holderId =
    typeof candidate.holderId === "string" ? candidate.holderId.trim() : "";
  const issuer = typeof candidate.issuer === "string" ? candidate.issuer.trim() : "";
  const type = candidate.type;
  const expiresAt =
    typeof candidate.expiresAt === "number"
      ? candidate.expiresAt
      : typeof candidate.expiresAt === "string"
        ? Number(candidate.expiresAt)
        : Number.NaN;
  const metadata = candidate.metadata ?? {};

  if (!holderId) errors.push("holderId is required");
  if (!issuer) errors.push("issuer is required");
  if (typeof type !== "string" || !CREDENTIAL_TYPES.has(type as Credential["type"])) {
    errors.push("type must be identity, education, professional, or financial");
  }
  if (!Number.isSafeInteger(expiresAt) || expiresAt <= Math.floor(Date.now() / 1_000)) {
    errors.push("expiresAt must be a future Unix timestamp in seconds");
  }
  if (typeof metadata !== "object" || metadata === null || Array.isArray(metadata)) {
    errors.push("metadata must be a JSON object");
  }

  if (errors.length > 0) return { row, errors };
  return {
    row,
    errors,
    input: {
      holderId,
      type: type as Credential["type"],
      issuer,
      expiresAt,
      metadata: metadata as Record<string, unknown>,
    },
  };
}

function parseCsvRecords(csv: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let quoted = false;
  for (let i = 0; i < csv.length; i += 1) {
    const character = csv[i]!;
    if (quoted) {
      if (character === '"' && csv[i + 1] === '"') {
        field += '"';
        i += 1;
      } else if (character === '"') {
        quoted = false;
      } else {
        field += character;
      }
    } else if (character === '"' && field.length === 0) {
      quoted = true;
    } else if (character === ",") {
      row.push(field);
      field = "";
    } else if (character === "\n" || character === "\r") {
      row.push(field);
      if (row.some((cell) => cell.trim())) rows.push(row);
      row = [];
      field = "";
      if (character === "\r" && csv[i + 1] === "\n") i += 1;
    } else {
      field += character;
    }
  }
  if (quoted) throw new Error("CSV contains an unterminated quoted field");
  row.push(field);
  if (row.some((cell) => cell.trim())) rows.push(row);
  return rows;
}

export class CredentialImportStore {
  private readonly batches = new Map<string, CredentialImportBatch>();
  private readonly owners = new Map<string, string>();

  startImport(
    rows: ParsedCredentialImportRow[],
    credentials: CredentialStore,
    owner: string
  ): CredentialImportBatch {
    this.cleanup();
    if (this.batches.size >= MAX_BATCHES) {
      const completed = [...this.batches.entries()]
        .filter(([, batch]) => batch.status === "completed")
        .sort(([, left], [, right]) => (left.completedAt ?? left.createdAt) - (right.completedAt ?? right.createdAt));
      while (this.batches.size >= MAX_BATCHES && completed.length > 0) {
        const [expiredId] = completed.shift()!;
        this.batches.delete(expiredId);
        this.owners.delete(expiredId);
      }
      if (this.batches.size >= MAX_BATCHES) {
        throw new Error("credential import tracking capacity is full");
      }
    }
    const now = Date.now();
    const batch: CredentialImportBatch = {
      importId: randomUUID(),
      status: "in_progress",
      total: rows.length,
      processed: 0,
      imported: 0,
      failed: 0,
      progress: rows.length === 0 ? 100 : 0,
      createdAt: now,
      results: rows.map(({ row, errors }) => ({
        row,
        status: errors.length > 0 ? "failed" : "pending",
        errors: [...errors],
      })),
    };
    batch.failed = batch.results.filter((result) => result.status === "failed").length;
    batch.processed = batch.failed;
    this.batches.set(batch.importId, batch);
    this.owners.set(
      batch.importId,
      createHash("sha256").update(owner).digest("hex")
    );

    void new Promise<void>((resolve) => setImmediate(resolve)).then(() =>
      this.processBatch(batch.importId, rows, credentials)
    );
    return snapshot(batch);
  }

  getBatch(importId: string, owner: string): CredentialImportBatch | undefined {
    const ownerHash = createHash("sha256").update(owner).digest("hex");
    if (this.owners.get(importId) !== ownerHash) return undefined;
    const batch = this.batches.get(importId);
    return batch ? snapshot(batch) : undefined;
  }

  private async processBatch(
    importId: string,
    rows: ParsedCredentialImportRow[],
    credentials: CredentialStore
  ): Promise<void> {
    const batch = this.batches.get(importId);
    if (!batch) return;
    for (const item of rows) {
      if (!item.input) continue;
      const result = batch.results.find((entry) => entry.row === item.row);
      if (!result) continue;
      try {
        const credential = credentials.issueCredential(
          item.input.holderId,
          item.input.type,
          item.input.issuer,
          item.input.expiresAt,
          item.input.metadata
        );
        result.status = "imported";
        result.credentialId = credential.id;
        batch.imported += 1;
      } catch (error) {
        result.status = "failed";
        result.errors.push(error instanceof Error ? error.message : "credential import failed");
        batch.failed += 1;
      }
      batch.processed += 1;
      batch.progress = batch.total === 0 ? 100 : Math.floor((batch.processed / batch.total) * 100);
      if (batch.processed % 25 === 0) {
        await new Promise<void>((resolve) => setImmediate(resolve));
      }
    }
    batch.status = "completed";
    batch.progress = 100;
    batch.completedAt = Date.now();
  }

  private cleanup(): void {
    const cutoff = Date.now() - BATCH_TTL_MS;
    for (const [id, batch] of this.batches) {
      if (
        batch.status === "completed" &&
        (batch.completedAt ?? batch.createdAt) < cutoff
      ) {
        this.batches.delete(id);
        this.owners.delete(id);
      }
    }
  }
}

function snapshot(batch: CredentialImportBatch): CredentialImportBatch {
  return { ...batch, results: batch.results.map((result) => ({ ...result, errors: [...result.errors] })) };
}

export const credentialImportStore = new CredentialImportStore();
