import { middle } from "./worker-module-tla-middle.js";
import { reexportedSource } from "./worker-module-tla-reexport-list.js";
import { source as starSource } from "./worker-module-tla-reexport-star.js";

self.__httpTlaOrder.push("main-start");
const promisedNamespacePromise = import("./worker-module-tla-promised.js");
const promisedThenValuePromise = promisedNamespacePromise.then(function (namespace) {
  return namespace.promised;
});
const dynamicNamespace = await import("./worker-module-tla-dynamic.js");
const promisedNamespace = await promisedNamespacePromise;
const promisedThenValue = await promisedThenValuePromise;
self.__httpTlaOrder.push("main-after-dynamic");

postMessage({
  middle,
  dynamic: dynamicNamespace.dynamic,
  dynamicOrder: self.__httpTlaDynamicOrder.join("|"),
  promised: promisedNamespace.promised,
  promisedThenValue,
  promisedOrder: self.__httpTlaPromisedOrder.join("|"),
  reexportedSource,
  starSource,
  reexportOrder: self.__httpTlaReexportOrder.join("|"),
  order: self.__httpTlaOrder.join("|"),
  importScriptsType: typeof importScripts,
});
