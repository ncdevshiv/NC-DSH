import "@angular/compiler";

import {
  ApplicationRef,
  type Type,
  provideZonelessChangeDetection,
} from "@angular/core";
import { bootstrapApplication } from "@angular/platform-browser";

import { assertFixture, captureFrame, markReady } from "../shared/harness";
import type { SmokeMeta } from "../shared/types";

export async function settleAngular(application: ApplicationRef): Promise<void> {
  await application.whenStable();
  await Promise.resolve();
  await application.whenStable();
}

export async function bootstrapAngular<T>(
  component: Type<T>,
  meta: SmokeMeta,
  checkpoint: string,
  update: (instance: T, application: ApplicationRef) => void | Promise<void>,
): Promise<void> {
  const application = await bootstrapApplication(component, {
    providers: [provideZonelessChangeDetection()],
  });
  await settleAngular(application);
  const instance = application.components[0]?.instance as T | undefined;
  assertFixture(instance, "Angular root component exists");
  assertFixture(document.querySelector("[data-case-body]") !== null, "case body mounted");
  await captureFrame(meta, "mounted");
  await update(instance, application);
  await settleAngular(application);
  markReady(meta, ["mounted", checkpoint]);
}
