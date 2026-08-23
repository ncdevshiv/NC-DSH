export type FrameworkName = "react" | "vue" | "angular";
export type Complexity = "simple" | "medium" | "complex";

export interface SmokeMeta {
  id: string;
  framework: FrameworkName;
  family: string;
  complexity: Complexity;
  title: string;
}

export interface CaseSpec {
  variant: number;
  seed: number;
  size: number;
  slug: string;
  title: string;
}

export interface SmokeState {
  id: string;
  framework: FrameworkName;
  phase: "booting" | "checkpoint" | "ready" | "error";
  checkpoints: string[];
  frames: string[];
  pendingFrame?: {
    index: number;
    name: string;
    token: string;
  };
  errors: string[];
  expectedDiagnostics?: {
    networkFailures: ExpectedNetworkFailure[];
  };
}

export interface ExpectedNetworkFailure {
  label: string;
  url: string;
  type: string;
  canceled: boolean;
}

declare global {
  interface Window {
    __MOLI_FRONTEND_SMOKE__?: SmokeState;
    __MOLI_FRONTEND_SMOKE_RESUME__?: (token: string) => boolean;
  }
}
