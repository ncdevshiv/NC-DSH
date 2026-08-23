import { assertFixture } from "./harness";

export interface PlatformBoundaryResult {
  status: "ready";
  facts: PlatformFact[];
}

export interface PlatformFact {
  name: string;
  value: string;
}

export type PlatformFrame = "platform-1" | "platform-2";
export type CapturePlatformFrame = (name: PlatformFrame) => Promise<void>;

export const PLATFORM_EVENT_TIMEOUT_MS = 20_000;

export function fact(name: string, value: unknown): PlatformFact {
  return { name, value: String(value) };
}

function platformLog(host: HTMLElement): HTMLOListElement {
  let log = host.querySelector("[data-platform-log]");
  if (!log) {
    log = document.createElement("ol");
    log.setAttribute("data-platform-log", "");
    host.append(log);
  }
  assertFixture(log instanceof HTMLOListElement, "platform log is an ordered list");
  return log;
}

export async function capturePlatformStep(
  host: HTMLElement,
  capture: CapturePlatformFrame,
  name: PlatformFrame,
  label: string,
  details: unknown[],
): Promise<void> {
  const item = document.createElement("li");
  item.dataset.platformStep = name;
  item.textContent = `${label}:${details.map(String).join("|")}`;
  platformLog(host).append(item);
  host.dataset.lastPlatformStep = name;
  await capture(name);
}

export function withEventTimeout<T>(promise: Promise<T>, label: string): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`timed out waiting for ${label}`)),
      PLATFORM_EVENT_TIMEOUT_MS,
    );
    promise.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (error: unknown) => {
        clearTimeout(timer);
        reject(error);
      },
    );
  });
}

export function errorName(value: unknown): string {
  return value instanceof Error || value instanceof DOMException
    ? value.name
    : Object.prototype.toString.call(value);
}
