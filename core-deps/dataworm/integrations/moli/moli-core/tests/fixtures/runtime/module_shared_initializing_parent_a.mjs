import { depValue } from "./module-shared-initializing-dep.mjs";

window.moduleSharedInitializingParentAValue = depValue;
window.moduleSharedInitializingParentAObservedDepEnd =
  window.moduleSharedInitializingOrder.includes("dep-end");
window.moduleSharedInitializingOrder.push(`a:${String(depValue)}`);
