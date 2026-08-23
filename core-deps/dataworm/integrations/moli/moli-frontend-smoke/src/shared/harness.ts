import type { ExpectedNetworkFailure, SmokeMeta, SmokeState } from "./types";

let activeMeta: SmokeMeta | undefined;
let readyStarted = false;
let pendingFrame:
  | {
      token: string;
      timer: ReturnType<typeof setTimeout>;
      resolve: () => void;
      reject: (error: Error) => void;
    }
  | undefined;

function errorText(value: unknown): string {
  if (value instanceof Error) {
    return `${value.name}: ${value.message}`;
  }
  return String(value);
}

function state(): SmokeState | undefined {
  return window.__MOLI_FRONTEND_SMOKE__;
}

function publish(phase: SmokeState["phase"]): void {
  const current = state();
  if (!current || !activeMeta) {
    return;
  }
  current.phase = phase;
  document.documentElement.dataset.frontendSmoke = phase;
  document.documentElement.dataset.frontendSmokeId = activeMeta.id;
  window.dispatchEvent(
    new CustomEvent("moli-frontend-smoke-ready", {
      detail: {
        id: current.id,
        framework: current.framework,
        phase,
      },
    }),
  );
}

export function beginCase(meta: SmokeMeta): void {
  activeMeta = meta;
  readyStarted = false;
  pendingFrame = undefined;
  window.__MOLI_FRONTEND_SMOKE__ = {
    id: meta.id,
    framework: meta.framework,
    phase: "booting",
    checkpoints: ["entry"],
    frames: [],
    errors: [],
  };
  window.__MOLI_FRONTEND_SMOKE_RESUME__ = (token) => {
    const current = state();
    if (!current || !pendingFrame || pendingFrame.token !== token) {
      return false;
    }
    clearTimeout(pendingFrame.timer);
    const resolve = pendingFrame.resolve;
    pendingFrame = undefined;
    delete current.pendingFrame;
    publish("booting");
    resolve();
    return true;
  };
  document.title = meta.title;
  document.documentElement.dataset.frontendSmoke = "booting";
  document.documentElement.dataset.frontendSmokeId = meta.id;

  window.addEventListener("error", (event) => {
    failCase(event.error ?? event.message);
  });
  window.addEventListener("unhandledrejection", (event) => {
    failCase(event.reason);
  });
}

export function checkpoint(name: string): void {
  const current = state();
  if (current && !current.checkpoints.includes(name)) {
    current.checkpoints.push(name);
  }
}

export function assertFixture(condition: unknown, message: string): asserts condition {
  if (!condition) {
    throw new Error(`fixture assertion failed: ${message}`);
  }
}

export function expectNetworkFailure(expectation: ExpectedNetworkFailure): void {
  const current = state();
  if (!current) {
    throw new Error("cannot register an expected network failure before beginCase");
  }
  const diagnostics = (current.expectedDiagnostics ??= { networkFailures: [] });
  if (diagnostics.networkFailures.some((item) => item.label === expectation.label)) {
    throw new Error(`duplicate expected network failure label: ${expectation.label}`);
  }
  diagnostics.networkFailures.push(expectation);
}

export function captureFrame(meta: SmokeMeta, name: string): Promise<void> {
  const current = state();
  if (!current || current.id !== meta.id) {
    return Promise.reject(new Error(`frame state identity mismatch for ${meta.id}`));
  }
  if (pendingFrame || current.pendingFrame) {
    return Promise.reject(new Error(`frame ${current.pendingFrame?.name ?? "unknown"} is pending`));
  }
  const index = current.frames.length;
  const token = `${meta.id}:${index}:${name}`;
  current.frames.push(name);
  current.pendingFrame = { index, name, token };
  checkpoint(name);
  return new Promise<void>((resolve, reject) => {
    const timer = setTimeout(() => {
      if (!pendingFrame || pendingFrame.token !== token) {
        return;
      }
      pendingFrame = undefined;
      delete current.pendingFrame;
      const error = new Error(`runner did not resume frame ${name}`);
      reject(error);
      failCase(error);
    }, 120_000);
    pendingFrame = { token, timer, resolve, reject };
    publish("checkpoint");
  });
}

export function markReady(meta: SmokeMeta, checkpoints: string[] = []): void {
  const current = state();
  if (!current || current.id !== meta.id) {
    throw new Error(`ready state identity mismatch for ${meta.id}`);
  }
  if (readyStarted) {
    return;
  }
  readyStarted = true;
  for (const name of checkpoints) {
    checkpoint(name);
  }
  if (current.errors.length > 0) {
    publish("error");
    return;
  }
  void captureFrame(meta, "ready")
    .then(() => {
      const latest = state();
      publish(latest && latest.errors.length > 0 ? "error" : "ready");
    })
    .catch(failCase);
}

export function failCase(error: unknown): void {
  const current = state();
  if (!current) {
    return;
  }
  const text = errorText(error);
  if (!current.errors.includes(text)) {
    current.errors.push(text);
  }
  if (pendingFrame) {
    clearTimeout(pendingFrame.timer);
    const reject = pendingFrame.reject;
    pendingFrame = undefined;
    delete current.pendingFrame;
    reject(new Error(text));
  }
  publish("error");
}

export async function microtaskTurns(count = 2): Promise<void> {
  for (let index = 0; index < count; index += 1) {
    await Promise.resolve();
  }
}
