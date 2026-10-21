/**
 * Issue #1776: Fraud risk model for credential verification.
 *
 * A logistic-regression scorer over six engineered features. Each feature is
 * normalized to [0, 1] and the score is sigmoid(bias + Σ weight·feature), so every
 * contribution to a score can be read back out. The default weights are hand-set
 * priors. `trainFraudModel` fits new weights from labelled history, and
 * `setActiveFraudModel` swaps the active model without a deploy.
 *
 * Scoring is advisory. Verification attaches the assessment to its result but
 * never changes a verification's status on its own.
 */

import type { Credential, CredentialStore } from "../credentials/credentialStore.js";

export const FRAUD_FEATURES = [
  "issuerVelocity",
  "holderVelocity",
  "revokedRatio",
  "metadataAnomaly",
  "documentReuse",
  "shortExpiry",
] as const;

export type FraudFeature = (typeof FRAUD_FEATURES)[number];
export type FraudFeatures = Record<FraudFeature, number>;

export interface FraudModel {
  version: string;
  bias: number;
  weights: FraudFeatures;
}

export type FraudRiskLevel = "low" | "medium" | "high";

export interface FraudAssessment {
  score: number;
  riskLevel: FraudRiskLevel;
  features: FraudFeatures;
  contributions: FraudFeatures;
  modelVersion: string;
  assessedAt: number;
}

/** Hand-set prior weights used until a model is trained and activated. */
export const DEFAULT_FRAUD_MODEL: FraudModel = {
  version: "logit-prior-v1",
  bias: -3,
  weights: {
    issuerVelocity: 2,
    holderVelocity: 2.5,
    revokedRatio: 2,
    metadataAnomaly: 1.5,
    documentReuse: 3,
    shortExpiry: 1,
  },
};

const MEDIUM_RISK_THRESHOLD = 0.4;
const HIGH_RISK_THRESHOLD = 0.7;
const DAY_SECONDS = 24 * 60 * 60;
const VELOCITY_WINDOW_SECONDS = DAY_SECONDS;
const ISSUER_VELOCITY_CAP = 20;
const HOLDER_VELOCITY_CAP = 5;
const METADATA_ANOMALY_CAP = 3;
const DOCUMENT_REUSE_CAP = 2;
const MAX_METADATA_VALUE_LENGTH = 512;
const SUSPICIOUS_KEY = /override|bypass|admin|backdoor/i;

/** Raw observations the features are derived from. Anything omitted counts as zero. */
export interface FraudSignals {
  now: number;
  expiresAt: number;
  issuerCredentialsLast24h: number;
  holderCredentialsLast24h: number;
  holderTotalCredentials: number;
  holderRevokedCredentials: number;
  metadata: Record<string, unknown>;
  /** Other credentials that carry the same document hash. */
  documentHashReuseCount: number;
}

export function extractFraudFeatures(signals: FraudSignals): FraudFeatures {
  const clamp = (value: number) => Math.min(1, Math.max(0, value));
  const total = signals.holderTotalCredentials;
  const remainingSeconds = signals.expiresAt - signals.now;

  return {
    issuerVelocity: clamp(signals.issuerCredentialsLast24h / ISSUER_VELOCITY_CAP),
    holderVelocity: clamp(signals.holderCredentialsLast24h / HOLDER_VELOCITY_CAP),
    revokedRatio: total > 0 ? clamp(signals.holderRevokedCredentials / total) : 0,
    metadataAnomaly: clamp(countMetadataAnomalies(signals.metadata) / METADATA_ANOMALY_CAP),
    documentReuse: clamp(signals.documentHashReuseCount / DOCUMENT_REUSE_CAP),
    shortExpiry: remainingSeconds < 7 * DAY_SECONDS ? 1 : remainingSeconds < 30 * DAY_SECONDS ? 0.5 : 0,
  };
}

/** Count metadata fields that look tampered with: empty values, oversized values, or override-style keys. */
export function countMetadataAnomalies(metadata: Record<string, unknown>, depth = 0): number {
  if (depth > 5) return 0;
  let anomalies = 0;
  for (const [key, value] of Object.entries(metadata ?? {})) {
    if (SUSPICIOUS_KEY.test(key)) anomalies++;
    if (typeof value === "string") {
      if (value.trim().length === 0 || value.length > MAX_METADATA_VALUE_LENGTH) anomalies++;
    } else if (value && typeof value === "object" && !Array.isArray(value)) {
      anomalies += countMetadataAnomalies(value as Record<string, unknown>, depth + 1);
    }
  }
  return anomalies;
}

function sigmoid(x: number): number {
  return 1 / (1 + Math.exp(-x));
}

export function scoreFraudRisk(
  features: FraudFeatures,
  model: FraudModel = activeFraudModel,
  now: number = Math.floor(Date.now() / 1000)
): FraudAssessment {
  const contributions = {} as FraudFeatures;
  let linear = model.bias;
  for (const feature of FRAUD_FEATURES) {
    const contribution = model.weights[feature] * features[feature];
    contributions[feature] = contribution;
    linear += contribution;
  }
  const score = sigmoid(linear);
  const riskLevel: FraudRiskLevel =
    score >= HIGH_RISK_THRESHOLD ? "high" : score >= MEDIUM_RISK_THRESHOLD ? "medium" : "low";
  return {
    score: Math.round(score * 10_000) / 10_000,
    riskLevel,
    features,
    contributions,
    modelVersion: model.version,
    assessedAt: now,
  };
}

export function assessFraudRisk(
  signals: FraudSignals,
  model: FraudModel = activeFraudModel
): FraudAssessment {
  return scoreFraudRisk(extractFraudFeatures(signals), model, signals.now);
}

/**
 * Gather the holder- and issuer-level signals for a credential from the store, so
 * callers only need to add per-document facts (such as hash reuse).
 */
export function collectCredentialSignals(
  store: Pick<CredentialStore, "getCredentialsForHolder" | "getAllCredentials">,
  credential: Credential,
  now: number = Math.floor(Date.now() / 1000)
): Omit<FraudSignals, "documentHashReuseCount"> {
  const windowStart = now - VELOCITY_WINDOW_SECONDS;
  const holderCredentials = store.getCredentialsForHolder(credential.holderId);
  const issuerCredentialsLast24h = store
    .getAllCredentials()
    .filter((c) => c.issuer === credential.issuer && c.issuedAt >= windowStart).length;
  return {
    now,
    expiresAt: credential.expiresAt,
    issuerCredentialsLast24h,
    holderCredentialsLast24h: holderCredentials.filter((c) => c.issuedAt >= windowStart).length,
    holderTotalCredentials: holderCredentials.length,
    holderRevokedCredentials: holderCredentials.filter((c) => c.status === "revoked").length,
    metadata: credential.metadata ?? {},
  };
}

export interface LabelledSample {
  features: FraudFeatures;
  fraudulent: boolean;
}

export interface TrainingOptions {
  epochs?: number;
  learningRate?: number;
  l2?: number;
  version?: string;
}

/**
 * Fit logistic-regression weights by batch gradient descent with L2 regularization.
 * Returns a model that can be activated with `setActiveFraudModel`.
 */
export function trainFraudModel(samples: LabelledSample[], options: TrainingOptions = {}): FraudModel {
  if (samples.length === 0) throw new RangeError("training requires at least one sample");
  const epochs = options.epochs ?? 500;
  const learningRate = options.learningRate ?? 0.5;
  const l2 = options.l2 ?? 0.001;

  const weights = {} as FraudFeatures;
  for (const feature of FRAUD_FEATURES) weights[feature] = 0;
  let bias = 0;

  for (let epoch = 0; epoch < epochs; epoch++) {
    const gradient = {} as FraudFeatures;
    for (const feature of FRAUD_FEATURES) gradient[feature] = 0;
    let biasGradient = 0;

    for (const sample of samples) {
      let linear = bias;
      for (const feature of FRAUD_FEATURES) linear += weights[feature] * sample.features[feature];
      const error = sigmoid(linear) - (sample.fraudulent ? 1 : 0);
      biasGradient += error;
      for (const feature of FRAUD_FEATURES) gradient[feature] += error * sample.features[feature];
    }

    const n = samples.length;
    bias -= learningRate * (biasGradient / n);
    for (const feature of FRAUD_FEATURES) {
      weights[feature] -= learningRate * (gradient[feature] / n + l2 * weights[feature]);
    }
  }

  return {
    version: options.version ?? `logit-trained-${samples.length}-${epochs}`,
    bias: Math.round(bias * 1e6) / 1e6,
    weights: roundFeatures(weights),
  };
}

function roundFeatures(features: FraudFeatures): FraudFeatures {
  const out = {} as FraudFeatures;
  for (const feature of FRAUD_FEATURES) out[feature] = Math.round(features[feature] * 1e6) / 1e6;
  return out;
}

/** Validate and install a model as the active scorer. Throws on malformed input. */
export function setActiveFraudModel(model: FraudModel): FraudModel {
  validateFraudModel(model);
  activeFraudModel = { version: model.version, bias: model.bias, weights: { ...model.weights } };
  return activeFraudModel;
}

export function getActiveFraudModel(): FraudModel {
  return activeFraudModel;
}

export function validateFraudModel(model: FraudModel): void {
  if (typeof model.version !== "string" || model.version.trim().length === 0) {
    throw new TypeError("model version must be a non-empty string");
  }
  if (!Number.isFinite(model.bias)) throw new TypeError("model bias must be a finite number");
  for (const feature of FRAUD_FEATURES) {
    const weight = model.weights?.[feature];
    if (typeof weight !== "number" || !Number.isFinite(weight)) {
      throw new TypeError(`weight for ${feature} must be a finite number`);
    }
  }
}

let activeFraudModel: FraudModel = DEFAULT_FRAUD_MODEL;
