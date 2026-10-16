import { afterEach, describe, expect, it } from "vitest";
import {
  DEFAULT_FRAUD_MODEL,
  FRAUD_FEATURES,
  assessFraudRisk,
  collectCredentialSignals,
  countMetadataAnomalies,
  extractFraudFeatures,
  getActiveFraudModel,
  scoreFraudRisk,
  setActiveFraudModel,
  trainFraudModel,
  type FraudFeatures,
  type FraudSignals,
  type LabelledSample,
} from "../src/verification/fraudModel.js";
import { identityVerificationService } from "../src/credentials/identityVerificationService.js";
import { CredentialStore } from "../src/credentials/credentialStore.js";

const NOW = 1_800_000_000;
const DAY = 86_400;

function zeroFeatures(): FraudFeatures {
  return Object.fromEntries(FRAUD_FEATURES.map((f) => [f, 0])) as FraudFeatures;
}

function signals(overrides: Partial<FraudSignals> = {}): FraudSignals {
  return {
    now: NOW,
    expiresAt: NOW + 365 * DAY,
    issuerCredentialsLast24h: 0,
    holderCredentialsLast24h: 0,
    holderTotalCredentials: 0,
    holderRevokedCredentials: 0,
    metadata: { course: "Rust" },
    documentHashReuseCount: 0,
    ...overrides,
  };
}

afterEach(() => {
  setActiveFraudModel(DEFAULT_FRAUD_MODEL);
});

describe("extractFraudFeatures", () => {
  it("is zero for a clean, long-lived credential with no suspicious activity", () => {
    expect(extractFraudFeatures(signals())).toEqual(zeroFeatures());
  });

  it("normalizes velocity, revocation, and reuse signals into [0, 1]", () => {
    const features = extractFraudFeatures(
      signals({
        issuerCredentialsLast24h: 10,
        holderCredentialsLast24h: 50,
        holderTotalCredentials: 4,
        holderRevokedCredentials: 1,
        documentHashReuseCount: 1,
      })
    );
    expect(features.issuerVelocity).toBeCloseTo(0.5);
    expect(features.holderVelocity).toBe(1);
    expect(features.revokedRatio).toBeCloseTo(0.25);
    expect(features.documentReuse).toBeCloseTo(0.5);
  });

  it("flags short remaining validity in two bands", () => {
    expect(extractFraudFeatures(signals({ expiresAt: NOW + 3 * DAY })).shortExpiry).toBe(1);
    expect(extractFraudFeatures(signals({ expiresAt: NOW + 20 * DAY })).shortExpiry).toBe(0.5);
    expect(extractFraudFeatures(signals({ expiresAt: NOW + 90 * DAY })).shortExpiry).toBe(0);
  });

  it("gives zero revocation ratio when the holder has no history", () => {
    expect(extractFraudFeatures(signals({ holderRevokedCredentials: 3 })).revokedRatio).toBe(0);
  });
});

describe("countMetadataAnomalies", () => {
  it("counts empty values, oversized values, and override-style keys, including nested ones", () => {
    expect(
      countMetadataAnomalies({
        name: "   ",
        notes: "x".repeat(600),
        admin_override: "yes",
        nested: { bypass: "ok" },
        fine: "value",
      })
    ).toBe(4);
  });

  it("does not recurse without bound", () => {
    let deep: Record<string, unknown> = { bypass: "x" };
    for (let i = 0; i < 20; i++) deep = { child: deep };
    expect(countMetadataAnomalies(deep)).toBeLessThanOrEqual(1);
  });
});

describe("scoreFraudRisk", () => {
  it("scores a clean credential as low risk", () => {
    const assessment = scoreFraudRisk(zeroFeatures(), DEFAULT_FRAUD_MODEL, NOW);
    expect(assessment.riskLevel).toBe("low");
    expect(assessment.score).toBeLessThan(0.1);
    expect(assessment.modelVersion).toBe(DEFAULT_FRAUD_MODEL.version);
  });

  it("scores a fully suspicious credential as high risk", () => {
    const all = Object.fromEntries(FRAUD_FEATURES.map((f) => [f, 1])) as FraudFeatures;
    const assessment = scoreFraudRisk(all, DEFAULT_FRAUD_MODEL, NOW);
    expect(assessment.riskLevel).toBe("high");
    expect(assessment.score).toBeGreaterThan(0.99);
  });

  it("explains the score through per-feature contributions", () => {
    const features = { ...zeroFeatures(), documentReuse: 1 };
    const assessment = scoreFraudRisk(features, DEFAULT_FRAUD_MODEL, NOW);
    expect(assessment.contributions.documentReuse).toBe(DEFAULT_FRAUD_MODEL.weights.documentReuse);
    expect(assessment.contributions.holderVelocity).toBe(0);
  });

  it("assesses signals end to end", () => {
    const assessment = assessFraudRisk(signals({ documentHashReuseCount: 2, holderCredentialsLast24h: 5 }));
    expect(assessment.riskLevel).not.toBe("low");
    expect(assessment.features.documentReuse).toBe(1);
  });
});

describe("trainFraudModel", () => {
  it("learns to separate fraudulent from legitimate feature patterns", () => {
    const samples: LabelledSample[] = [];
    for (let i = 0; i < 30; i++) {
      samples.push({
        features: { ...zeroFeatures(), documentReuse: 1, holderVelocity: 0.8 },
        fraudulent: true,
      });
      samples.push({
        features: { ...zeroFeatures(), metadataAnomaly: 0.1 * (i % 3) },
        fraudulent: false,
      });
    }

    const model = trainFraudModel(samples, { epochs: 800, learningRate: 1 });
    expect(model.version).toMatch(/^logit-trained-60-800$/);

    const correct = samples.filter((s) => {
      const predicted = scoreFraudRisk(s.features, model).score >= 0.5;
      return predicted === s.fraudulent;
    }).length;
    expect(correct / samples.length).toBeGreaterThanOrEqual(0.95);
  });

  it("rejects an empty training set", () => {
    expect(() => trainFraudModel([])).toThrow(RangeError);
  });
});

describe("setActiveFraudModel", () => {
  it("installs a valid model and rejects malformed ones", () => {
    const next = { version: "custom-1", bias: -1, weights: { ...DEFAULT_FRAUD_MODEL.weights } };
    setActiveFraudModel(next);
    expect(getActiveFraudModel().version).toBe("custom-1");

    expect(() => setActiveFraudModel({ ...next, version: "  " })).toThrow(TypeError);
    expect(() => setActiveFraudModel({ ...next, bias: Number.NaN })).toThrow(TypeError);
    expect(() =>
      setActiveFraudModel({ ...next, weights: { ...next.weights, documentReuse: Infinity } })
    ).toThrow(TypeError);
    expect(getActiveFraudModel().version).toBe("custom-1");
  });
});

describe("collectCredentialSignals", () => {
  it("counts holder and issuer activity from the credential store", () => {
    const store = new CredentialStore();
    const first = store.issueCredential("holder-x", "education", "Issuer A", NOW + 365 * DAY, { c: "a" });
    store.issueCredential("holder-x", "education", "Issuer A", NOW + 365 * DAY, { c: "b" });
    store.revokeCredential(first.id);

    const collected = collectCredentialSignals(store, first, Math.floor(Date.now() / 1000));
    expect(collected.holderTotalCredentials).toBe(2);
    expect(collected.holderRevokedCredentials).toBe(1);
    expect(collected.issuerCredentialsLast24h).toBe(2);
    expect(collected.holderCredentialsLast24h).toBe(2);
  });
});

describe("identityVerificationService fraud scoring", () => {
  it("attaches an advisory assessment to each document verification", () => {
    const result = identityVerificationService.verifyDocument(
      "fraud-clean-cred",
      "passport",
      "hash-unique-fraud-clean",
      NOW + 365 * DAY,
      { country: "NG" }
    );
    expect(result.fraudRisk.riskLevel).toBe("low");
    expect(result.fraudRisk.modelVersion).toBe(getActiveFraudModel().version);
    expect(result.documentHash).toBe("hash-unique-fraud-clean");
  });

  it("raises document-reuse risk when the same hash is verified for another credential", () => {
    identityVerificationService.verifyDocument(
      "fraud-reuse-a",
      "national_id",
      "hash-shared-fraud-test",
      NOW + 365 * DAY
    );
    const second = identityVerificationService.verifyDocument(
      "fraud-reuse-b",
      "national_id",
      "hash-shared-fraud-test",
      NOW + 365 * DAY
    );
    expect(second.fraudRisk.features.documentReuse).toBe(0.5);
    expect(second.fraudRisk.contributions.documentReuse).toBeGreaterThan(0);
  });

  it("surfaces the assessment in the verification report with a review flag", () => {
    identityVerificationService.verifyDocument("fraud-report-cred", "passport", "hash-report-fraud", NOW + 365 * DAY);
    const report = identityVerificationService.getVerificationReport("fraud-report-cred");
    expect(report.fraudRisk?.riskLevel).toBe("low");
    expect(report.requiresReview).toBe(false);
    expect(identityVerificationService.getVerificationReport("fraud-never-verified").fraudRisk).toBeNull();
  });
});
