import { base, describe } from "./worker-module-dependency.js";

postMessage({
  phase: "module-loaded",
  base,
  description: describe(2),
  importScriptsType: typeof importScripts,
  path: new URL(import.meta.url).pathname,
  name: self.name,
});

addEventListener("message", async (event) => {
  const dependency = await import("./worker-module-dependency.js");
  postMessage({
    phase: "module-dynamic",
    sameBase: dependency.base === base,
    description: dependency.describe(event.data.value),
    keys: Object.keys(dependency).sort(),
  });
});
