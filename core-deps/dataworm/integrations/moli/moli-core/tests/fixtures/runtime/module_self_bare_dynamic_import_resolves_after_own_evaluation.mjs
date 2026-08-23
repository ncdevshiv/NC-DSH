window.moduleSelfBareImportOrder.push("module-start");

const selfImportPromise = import("/assets/module-self-bare-dynamic-import-resolves-after-own-evaluation.mjs");
window.moduleSelfBareImportIsPromise =
  !!selfImportPromise && typeof selfImportPromise.then === "function";

selfImportPromise.then((namespace) => {
  window.moduleSelfBareImportOrder.push(`resolved:${namespace.value}`);
  window.moduleSelfBareImportResolvedValue = namespace.value;
  window.moduleSelfBareImportFinalOrder =
    window.moduleSelfBareImportOrder.join(",");
});

window.moduleSelfBareImportOrder.push("module-end");

export const value = "ready";
