/**
 * Issue #1780: Credential Holder Onboarding Flow
 *
 * Guides a credential holder through a fixed, ordered set of steps before
 * their onboarding can be completed. Each flow belongs to one holder; only
 * that holder can read or advance it.
 *
 * In-memory, like the other credential stores in this server.
 */

export const ONBOARDING_STEPS = ["profile", "identity", "documents", "consent"] as const;
export type OnboardingStep = (typeof ONBOARDING_STEPS)[number];

export type OnboardingStatus = "in_progress" | "completed";

export interface OnboardingFlow {
  id: string;
  holderId: string;
  status: OnboardingStatus;
  completedSteps: OnboardingStep[];
  createdAt: number;
  updatedAt: number;
  completedAt?: number;
}

export interface OnboardingProgress {
  completed: number;
  total: number;
  percent: number;
  nextStep: OnboardingStep | null;
  steps: Array<{ step: OnboardingStep; completed: boolean }>;
}

export type OnboardingErrorCode =
  | "unknown_step"
  | "step_out_of_order"
  | "flow_completed"
  | "steps_incomplete";

export class OnboardingError extends Error {
  constructor(
    readonly code: OnboardingErrorCode,
    message: string,
    readonly missingSteps?: OnboardingStep[]
  ) {
    super(message);
  }
}

export function isOnboardingStep(value: string): value is OnboardingStep {
  return (ONBOARDING_STEPS as readonly string[]).includes(value);
}

export class OnboardingStore {
  private flows = new Map<string, OnboardingFlow>();
  private counter = 0;

  /**
   * Start onboarding for a holder. If the holder already has an in-progress
   * flow, that flow is returned so repeated calls are safe.
   */
  start(holderId: string, now = Date.now()): OnboardingFlow {
    const existing = this.findActive(holderId);
    if (existing) return existing;

    const flow: OnboardingFlow = {
      id: `onb_${++this.counter}_${now}`,
      holderId,
      status: "in_progress",
      completedSteps: [],
      createdAt: now,
      updatedAt: now,
    };
    this.flows.set(flow.id, flow);
    return flow;
  }

  /** Fetch a flow, but only for its holder. Returns undefined otherwise. */
  getFlow(flowId: string, holderId: string): OnboardingFlow | undefined {
    const flow = this.flows.get(flowId);
    return flow && flow.holderId === holderId ? flow : undefined;
  }

  /**
   * Mark a step complete. Steps must be completed in order, so a holder
   * cannot skip ahead. Completing an already-done step is a no-op.
   */
  completeStep(
    flowId: string,
    holderId: string,
    step: string,
    now = Date.now()
  ): OnboardingFlow | undefined {
    const flow = this.getFlow(flowId, holderId);
    if (!flow) return undefined;
    if (!isOnboardingStep(step)) {
      throw new OnboardingError("unknown_step", `unknown step: ${step}`);
    }
    if (flow.status === "completed") {
      throw new OnboardingError("flow_completed", "onboarding is already completed");
    }
    if (flow.completedSteps.includes(step)) return flow;

    const expected = nextStepOf(flow);
    if (step !== expected) {
      throw new OnboardingError(
        "step_out_of_order",
        `step ${step} must wait for ${expected ?? "completion"}`
      );
    }
    flow.completedSteps.push(step);
    flow.updatedAt = now;
    return flow;
  }

  /**
   * Finish onboarding. Every step must be complete first.
   */
  complete(flowId: string, holderId: string, now = Date.now()): OnboardingFlow | undefined {
    const flow = this.getFlow(flowId, holderId);
    if (!flow) return undefined;
    if (flow.status === "completed") return flow;

    const missingSteps = ONBOARDING_STEPS.filter((s) => !flow.completedSteps.includes(s));
    if (missingSteps.length > 0) {
      throw new OnboardingError(
        "steps_incomplete",
        `onboarding is missing steps: ${missingSteps.join(", ")}`,
        [...missingSteps]
      );
    }
    flow.status = "completed";
    flow.completedAt = now;
    flow.updatedAt = now;
    return flow;
  }

  private findActive(holderId: string): OnboardingFlow | undefined {
    for (const flow of this.flows.values()) {
      if (flow.holderId === holderId && flow.status === "in_progress") return flow;
    }
    return undefined;
  }
}

export function progressOf(flow: OnboardingFlow): OnboardingProgress {
  const steps = ONBOARDING_STEPS.map((step) => ({
    step,
    completed: flow.completedSteps.includes(step),
  }));
  const completed = steps.filter((s) => s.completed).length;
  return {
    completed,
    total: ONBOARDING_STEPS.length,
    percent: Math.round((completed / ONBOARDING_STEPS.length) * 100),
    nextStep: nextStepOf(flow),
    steps,
  };
}

function nextStepOf(flow: OnboardingFlow): OnboardingStep | null {
  return ONBOARDING_STEPS.find((s) => !flow.completedSteps.includes(s)) ?? null;
}

export const onboardingStore = new OnboardingStore();
