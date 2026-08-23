import { createApp, type Component } from "vue";

import { assertFixture } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";

export function mountVue(component: Component, meta: SmokeMeta, spec: CaseSpec): void {
  const container = document.getElementById("app");
  assertFixture(container, "Vue root exists");
  createApp(component, { meta, spec }).mount(container);
}
