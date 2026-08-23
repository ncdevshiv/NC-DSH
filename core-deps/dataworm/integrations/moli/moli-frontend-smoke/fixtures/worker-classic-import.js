importScripts("./worker-import-a.js", "./worker-import-b.js");

postMessage({
  phase: "loaded",
  sequence: globalThis.__workerImportSequence,
  importScriptsType: typeof importScripts,
  path: new URL(location.href).pathname,
  name: self.name,
});

addEventListener("message", (event) => {
  postMessage({
    phase: "computed",
    input: event.data.value,
    doubled: globalThis.__workerDouble(event.data.value),
    tripled: globalThis.__workerTriple(event.data.value),
    sequence: globalThis.__workerImportSequence,
  });
});
