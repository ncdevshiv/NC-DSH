async function run() {
  const awaited = await import("./worker-module-json-config.json", { with: { type: "json" } });
  const promised = import("./worker-module-json-config.json", { with: { type: "json" } });
  const promisedNamespace = await promised;
  const dataNamespace = await import("data:application/json,%7B%22name%22%3A%22dynamic-data-json%22%7D", { with: { type: "json" } });
  const repeated = await import("./worker-module-json-config.json", { with: { type: "json" } });

  postMessage({
    awaitedName: awaited.default.name,
    awaitedNested: awaited.default.nested.value,
    awaitedItems: awaited.default.items.join("|"),
    promisedName: promisedNamespace.default.name,
    dataName: dataNamespace.default.name,
    promisedNamespaceObjectSame: promisedNamespace === awaited,
    repeatedNamespaceObjectSame: repeated === awaited,
    sameNamespaceDefault: repeated.default === awaited.default,
    importScriptsType: typeof importScripts,
  });
}

run().catch(function (error) {
  postMessage({ error: String(error && error.message || error) });
});
