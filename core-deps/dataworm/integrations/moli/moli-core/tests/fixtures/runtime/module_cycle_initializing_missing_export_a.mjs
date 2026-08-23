import { readyFromB } from "/assets/module-cycle-initializing-missing-export-b.mjs";

export const readyFromA = "a";

window.moduleCycleInitializingMissingOrder.push(`a:${readyFromB}`);
