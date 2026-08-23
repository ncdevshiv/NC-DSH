window.moduleCycleOrder.push("b-start");

import("./module-cycle-dynamic-import-waits-a.mjs").then((namespace) => {
  window.moduleCycleSawAValue = namespace.aValue;
  window.moduleCycleOrder.push("b-dynamic-resolved");
  window.moduleCycleFinalOrder = window.moduleCycleOrder.join(",");
});

window.moduleCycleOrder.push("b-after-import");

export const bValue = "b-ready";
