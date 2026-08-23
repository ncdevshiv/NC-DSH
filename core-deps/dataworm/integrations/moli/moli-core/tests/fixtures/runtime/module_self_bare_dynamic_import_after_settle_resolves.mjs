window.moduleSelfBareImportAfterSettleOrder.push("module-start");

setTimeout(() => {
  window.moduleSelfBareImportAfterSettleOrder.push("timeout-start");
  const selfImportPromise = import("/assets/module-self-bare-dynamic-import-after-settle-resolves.mjs");
  window.moduleSelfBareImportAfterSettleIsPromise =
    !!selfImportPromise && typeof selfImportPromise.then === "function";

  selfImportPromise.then((namespace) => {
    window.moduleSelfBareImportAfterSettleOrder.push(`resolved:${namespace.value}`);
    window.moduleSelfBareImportAfterSettleResolvedValue = namespace.value;
    window.moduleSelfBareImportAfterSettleFinalOrder =
      window.moduleSelfBareImportAfterSettleOrder.join(",");
  });
}, 0);

window.moduleSelfBareImportAfterSettleOrder.push("module-end");

export const value = "ready-after-settle";
