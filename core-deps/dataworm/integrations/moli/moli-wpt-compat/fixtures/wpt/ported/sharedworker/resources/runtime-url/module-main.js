import value from "./module-dependency.js";

onconnect = function (event) {
  event.ports[0].postMessage({
    value: value,
    metaUrl: import.meta.url,
    importScriptsType: typeof importScripts,
  });
};
