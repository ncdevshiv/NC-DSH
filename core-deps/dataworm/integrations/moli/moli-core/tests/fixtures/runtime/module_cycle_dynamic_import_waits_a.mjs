import "./module-cycle-dynamic-import-waits-b.mjs";

window.moduleCycleOrder.push("a-start");
await Promise.resolve();
window.moduleCycleOrder.push("a-end");
window.moduleCycleAfterAEndOrder = window.moduleCycleOrder.join(",");

export const aValue = "a-ready";
