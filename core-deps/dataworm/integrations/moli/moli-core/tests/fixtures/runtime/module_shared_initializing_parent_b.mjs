import { depValue } from "./module-shared-initializing-dep.mjs";

window.moduleSharedInitializingParentBValue = depValue;
window.moduleSharedInitializingParentBObservedDepEnd =
  window.moduleSharedInitializingOrder.includes("dep-end");
window.moduleSharedInitializingOrder.push(`b:${String(depValue)}`);
